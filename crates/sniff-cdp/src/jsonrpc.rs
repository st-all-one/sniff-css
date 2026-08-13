//! Protocol-agnostic JSON-RPC client over a WebSocket.
//!
//! Both the Chrome DevTools Protocol and the Dart VM Service Protocol are
//! JSON-RPC-over-WebSocket with an identical wire shape:
//!
//! ```text
//! -> {"id": N, "method": "X", "params": {...}}
//! <- {"id": N, "result": {...}}  |  {"id": N, "error": {"message": "..."}}
//! <- {"method": "X.event", "params": {...}}   (unsolicited event)
//! ```
//!
//! [`JsonRpcClient`] owns the transport (tokio-tungstenite) and the
//! `id -> oneshot` routing, and is consumed by `sniff-cdp` (CDP) and
//! `sniff-flutter` (Dart VM Service). The optional `session_id` field is
//! passed through verbatim: CDP uses it for flatten target multiplexing,
//! the VM Service always leaves it `None`.

use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// Error type for the JSON-RPC transport layer.
#[derive(Debug, Error)]
pub enum JsonRpcError {
    #[error("websocket connect failed: {0}")]
    Connect(String),
    #[error("websocket transport: {0}")]
    Transport(String),
    #[error("protocol returned error for `{method}`: {message}")]
    Protocol { method: String, message: String },
    #[error("timed out waiting for `{0}`")]
    Timeout(String),
    #[error("connection closed")]
    Closed,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Result alias for the JSON-RPC client.
pub type Result<T> = std::result::Result<T, JsonRpcError>;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = SplitSink<WsStream, WsMessage>;

/// An inbound unsolicited event delivered to subscribers.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonRpcEvent {
    pub method: String,
    pub params: Map<String, Value>,
    /// Passthrough of the `sessionId` frame field (CDP only; `None` for the
    /// Dart VM Service).
    pub session_id: Option<String>,
}

impl JsonRpcEvent {
    /// Convenience accessor for a nested string field.
    pub fn param_str(&self, key: &str) -> Option<&str> {
        self.params.get(key)?.as_str()
    }

    /// Convenience accessor for a nested integer field.
    pub fn param_i64(&self, key: &str) -> Option<i64> {
        self.params.get(key)?.as_i64()
    }
}

/// Outbound request frame.
#[derive(Debug, Serialize)]
pub struct Request<'a> {
    pub id: u64,
    pub method: &'a str,
    pub params: Value,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sessionId")]
    pub session_id: Option<&'a str>,
}

/// A WebSocket JSON-RPC connection to one remote endpoint.
///
/// Commands are matched to responses by `id`; unsolicited frames with a
/// `method` are broadcast as events, so a single connection can multiplex
/// many logical sessions (CDP targets).
#[derive(Clone)]
pub struct JsonRpcClient {
    next_id: Arc<AtomicU64>,
    pending: Arc<StdMutex<HashMap<u64, oneshot::Sender<Value>>>>,
    events: tokio::sync::broadcast::Sender<JsonRpcEvent>,
    sink: Arc<tokio::sync::Mutex<WsSink>>,
    _reader: Arc<tokio::task::JoinHandle<()>>,
}

impl std::fmt::Debug for JsonRpcClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonRpcClient")
            .field("next_id", &self.next_id)
            .finish()
    }
}

impl JsonRpcClient {
    /// Connect to a `ws://`/`wss://` JSON-RPC endpoint.
    pub async fn connect(ws_url: &str) -> Result<Self> {
        let (ws, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .map_err(|e| JsonRpcError::Connect(e.to_string()))?;
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
        events: &tokio::sync::broadcast::Sender<JsonRpcEvent>,
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
            let event = JsonRpcEvent {
                method: method.to_string(),
                params,
                session_id,
            };
            let _ = events.send(event);
        }
    }

    /// Subscribe to a stream of inbound events.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<JsonRpcEvent> {
        self.events.subscribe()
    }

    /// Send a request and wait for its response.
    pub async fn call(
        &self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().ok().and_then(|mut m| m.insert(id, tx));

        let request = Request {
            id,
            method,
            params,
            session_id,
        };
        let frame =
            serde_json::to_string(&request).map_err(|e| JsonRpcError::Transport(e.to_string()))?;
        tracing::debug!(method, id, session = session_id, "sending request");

        {
            let mut sink = self.sink.lock().await;
            sink.send(WsMessage::Text(frame.into()))
                .await
                .map_err(|e| JsonRpcError::Transport(e.to_string()))?;
        }

        let response = rx.await.map_err(|_| JsonRpcError::Closed)?;
        self.pending.lock().ok().and_then(|mut m| m.remove(&id));
        tracing::debug!(method, id, response = %response, "request response");

        if let Some(err) = response.get("error") {
            let message = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown protocol error")
                .to_string();
            return Err(JsonRpcError::Protocol {
                method: method.to_string(),
                message,
            });
        }

        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Send a request with no parameters.
    pub async fn call_no_params(&self, method: &str, session_id: Option<&str>) -> Result<Value> {
        self.call(method, Value::Object(Map::new()), session_id)
            .await
    }

    /// Convenience for callers that only parse the response as a typed value.
    pub async fn call_typed<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<T> {
        let raw = self.call(method, params, session_id).await?;
        serde_json::from_value(raw).map_err(|e| JsonRpcError::Transport(e.to_string()))
    }

    /// Close the underlying connection.
    pub async fn close(&self) {
        let _ = self.sink.lock().await.send(WsMessage::Close(None)).await;
    }

    /// Build a request frame (exposed for tests).
    #[cfg(test)]
    fn build_frame(id: u64, method: &str, params: Value, session_id: Option<&str>) -> String {
        serde_json::to_string(&Request {
            id,
            method,
            params,
            session_id,
        })
        .expect("request serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    type TestPending = std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>;

    #[test]
    fn dispatch_routes_response_by_id() {
        let pending: Arc<TestPending> = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (events_tx, _) = tokio::sync::broadcast::channel(16);
        let (tx, mut rx) = oneshot::channel();
        pending.lock().unwrap().insert(7, tx);
        let msg: Value = serde_json::json!({"id": 7, "result": {"ok": true}});
        JsonRpcClient::dispatch(msg, &pending, &events_tx);
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
        JsonRpcClient::dispatch(msg, &pending, &events_tx);
        let event = rx.try_recv().expect("event must be broadcast");
        assert_eq!(event.method, "Network.loadingFinished");
        assert_eq!(event.session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn dispatch_ignores_unknown_frames() {
        let pending: Arc<TestPending> = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let (events_tx, _) = tokio::sync::broadcast::channel(16);
        JsonRpcClient::dispatch(Value::Null, &pending, &events_tx);
        assert!(pending.lock().unwrap().is_empty());
    }

    #[test]
    fn frame_serializes_session_and_params() {
        let frame = JsonRpcClient::build_frame(
            3,
            "ext.flutter.inspector.getRootWidgetSummaryTree",
            json!({"objectId": "root"}),
            None,
        );
        let v: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(v["id"], 3);
        assert_eq!(
            v["method"],
            "ext.flutter.inspector.getRootWidgetSummaryTree"
        );
        assert_eq!(v["params"]["objectId"], "root");
        assert!(!v.as_object().unwrap().contains_key("sessionId"));
    }

    #[test]
    fn frame_includes_session_id_when_present() {
        let frame = JsonRpcClient::build_frame(1, "Page.enable", json!({}), Some("abc"));
        let v: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(v["sessionId"], "abc");
    }
}
