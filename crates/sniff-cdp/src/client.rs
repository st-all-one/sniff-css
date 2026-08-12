//! WebSocket-based Chrome DevTools Protocol client.

use crate::protocol::{CdpEvent, Command};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// Error type for the CDP client layer.
#[derive(Debug, Error)]
pub enum CdpError {
    #[error("websocket connect failed: {0}")]
    Connect(String),
    #[error("websocket transport: {0}")]
    Transport(String),
    #[error("CDP returned error for `{method}`: {message}")]
    Protocol { method: String, message: String },
    #[error("timed out waiting for `{0}`")]
    Timeout(String),
    #[error("connection closed")]
    Closed,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Result alias for the CDP client.
pub type Result<T> = std::result::Result<T, CdpError>;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = SplitSink<WsStream, WsMessage>;

/// A connection to one DevTools WebSocket endpoint.
///
/// Commands carry an optional `session_id` and responses/events are
/// routed by id/session, so a single connection can multiplex many
/// targets (see [`crate::session::CdpSession`]).
#[derive(Clone)]
pub struct CdpClient {
    next_id: Arc<AtomicU64>,
    pending: Arc<StdMutex<HashMap<u64, oneshot::Sender<Value>>>>,
    events: tokio::sync::broadcast::Sender<CdpEvent>,
    sink: Arc<tokio::sync::Mutex<WsSink>>,
    _reader: Arc<tokio::task::JoinHandle<()>>,
}

impl std::fmt::Debug for CdpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CdpClient")
            .field("next_id", &self.next_id)
            .finish()
    }
}

impl CdpClient {
    /// Connect to a DevTools WebSocket endpoint.
    ///
    /// The `url` may be a `ws://`/`wss://` endpoint directly, or an HTTP
    /// origin (`http://127.0.0.1:9222` or `127.0.0.1:9222`) which is resolved
    /// through `/json/version` to the browser's WebSocket URL.
    pub async fn connect(url: &str) -> Result<Self> {
        let ws_url = crate::endpoint::resolve_endpoint(url).await?;
        let (ws, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(|e| CdpError::Connect(e.to_string()))?;
        let (sink, mut stream) = ws.split();

        let next_id = Arc::new(AtomicU64::new(1));
        let pending = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (events_tx, _) = tokio::sync::broadcast::channel(256);

        let pending_reader = pending.clone();
        let events_reader = events_tx.clone();
        let reader = tokio::spawn(async move {
            while let Some(msg) = stream.next().await {
                match msg {
                    Ok(WsMessage::Text(text)) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            Self::dispatch(value, &pending_reader, &events_reader);
                        }
                    }
                    Ok(WsMessage::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
        });

        Ok(Self {
            next_id,
            pending,
            events: events_tx,
            sink: Arc::new(tokio::sync::Mutex::new(sink)),
            _reader: Arc::new(reader),
        })
    }

    /// Route a decoded message to the pending map or the event bus.
    fn dispatch(
        value: Value,
        pending: &StdMutex<HashMap<u64, oneshot::Sender<Value>>>,
        events: &tokio::sync::broadcast::Sender<CdpEvent>,
    ) {
        if let Some(id) = value.get("id").and_then(Value::as_u64) {
            if let Some(tx) = pending.lock().ok().and_then(|mut m| m.remove(&id)) {
                let _ = tx.send(value);
            }
            return;
        }
        if let Some(method) = value.get("method").and_then(Value::as_str) {
            let session_id = value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(String::from);
            let params = value
                .get("params")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let event = CdpEvent {
                method: method.to_string(),
                params,
                session_id,
            };
            let _ = events.send(event);
        }
    }

    /// Subscribe to a stream of inbound events.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<CdpEvent> {
        self.events.subscribe()
    }

    /// Send a command and wait for its response.
    pub async fn call(
        &self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().ok().and_then(|mut m| m.insert(id, tx));

        let cmd = Command {
            id,
            method,
            params,
            session_id,
        };
        let frame = serde_json::to_string(&cmd).map_err(|e| CdpError::Transport(e.to_string()))?;
        tracing::debug!(method, id, session = session_id, "sending command");

        {
            let mut sink = self.sink.lock().await;
            sink.send(WsMessage::Text(frame.into()))
                .await
                .map_err(|e| CdpError::Transport(e.to_string()))?;
        }

        let response = rx.await.map_err(|_| CdpError::Closed)?;
        self.pending.lock().ok().and_then(|mut m| m.remove(&id));
        tracing::debug!(method, id, response = %response, "command response");

        if let Some(err) = response.get("error") {
            let message = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown CDP error")
                .to_string();
            return Err(CdpError::Protocol {
                method: method.to_string(),
                message,
            });
        }

        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Send a command with no parameters.
    pub async fn call_no_params(&self, method: &str, session_id: Option<&str>) -> Result<Value> {
        self.call(method, Value::Object(Map::new()), session_id)
            .await
    }

    /// Close the underlying connection.
    pub async fn close(&self) {
        let _ = self.sink.lock().await.send(WsMessage::Close(None)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestPending = std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>;

    #[test]
    fn dispatch_routes_response_by_id() {
        let pending: Arc<TestPending> = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (events_tx, _) = tokio::sync::broadcast::channel(16);
        let (tx, mut rx) = oneshot::channel();
        pending.lock().unwrap().insert(7, tx);
        let msg: Value = serde_json::json!({"id": 7, "result": {"ok": true}});
        CdpClient::dispatch(msg, &pending, &events_tx);
        // The sender must have been removed and the value delivered.
        assert!(pending.lock().unwrap().get(&7).is_none());
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn dispatch_routes_event_with_session() {
        let pending: Arc<TestPending> = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (events_tx, mut rx) = tokio::sync::broadcast::channel(16);
        let msg: Value = serde_json::json!({
            "method": "Network.loadingFinished",
            "params": {"requestId": "r1"},
            "sessionId": "s1"
        });
        CdpClient::dispatch(msg, &pending, &events_tx);
        let event = rx.try_recv().expect("event must be broadcast");
        assert_eq!(event.method, "Network.loadingFinished");
        assert_eq!(event.session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn dispatch_ignores_unknown_frames() {
        let pending: Arc<TestPending> = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (events_tx, _) = tokio::sync::broadcast::channel(16);
        CdpClient::dispatch(Value::Null, &pending, &events_tx);
        assert!(pending.lock().unwrap().is_empty());
    }
}
