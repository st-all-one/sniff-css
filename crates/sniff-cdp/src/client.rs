//! Chrome DevTools Protocol client.
//!
//! [`CdpClient`] is a thin, CDP-flavoured wrapper over the protocol-agnostic
//! [`JsonRpcClient`] (`crate::jsonrpc`): it adds endpoint resolution and the
//! `sessionId` multiplexing that the flattened CDP protocol needs. All
//! transport/routing lives in the shared client.

use crate::jsonrpc::{JsonRpcClient, JsonRpcError};
use crate::protocol::CdpEvent;
use serde_json::Value;
use thiserror::Error;

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

impl From<JsonRpcError> for CdpError {
    fn from(e: JsonRpcError) -> Self {
        match e {
            JsonRpcError::Connect(m) => CdpError::Connect(m),
            JsonRpcError::Transport(m) => CdpError::Transport(m),
            JsonRpcError::Protocol { method, message } => CdpError::Protocol { method, message },
            JsonRpcError::Timeout(m) => CdpError::Timeout(m),
            JsonRpcError::Closed => CdpError::Closed,
            JsonRpcError::Io(e) => CdpError::Io(e),
        }
    }
}

/// Result alias for the CDP client layer.
pub type Result<T> = std::result::Result<T, CdpError>;

/// A connection to one DevTools WebSocket endpoint.
///
/// Commands carry an optional `session_id` and responses/events are
/// routed by id/session, so a single connection can multiplex many
/// targets (see [`crate::session::CdpSession`]).
#[derive(Clone)]
pub struct CdpClient {
    inner: JsonRpcClient,
}

impl std::fmt::Debug for CdpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CdpClient")
            .field("inner", &self.inner)
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
        let inner = JsonRpcClient::connect(&ws_url).await?;
        Ok(Self { inner })
    }

    /// Subscribe to a stream of inbound CDP events.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<CdpEvent> {
        self.inner.subscribe()
    }

    /// Send a CDP command and wait for its response.
    pub async fn call(
        &self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<Value> {
        Ok(self.inner.call(method, params, session_id).await?)
    }

    /// Send a CDP command with no parameters.
    pub async fn call_no_params(&self, method: &str, session_id: Option<&str>) -> Result<Value> {
        Ok(self.inner.call_no_params(method, session_id).await?)
    }

    /// Close the underlying connection.
    pub async fn close(&self) {
        self.inner.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The CDP layer must keep resolving HTTP origins (regression guard for
    /// the JsonRpcClient generalization).
    #[tokio::test]
    async fn connect_resolves_http_origin() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut req = Vec::new();
            loop {
                let mut chunk = [0u8; 1024];
                match sock.read(&mut chunk).await.unwrap() {
                    0 => break,
                    n => {
                        req.extend_from_slice(&chunk[..n]);
                        if req.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                }
            }
            let body = format!(
                "{{\"webSocketDebuggerUrl\":\"ws://127.0.0.1:{}/devtools/browser/abc\"}}",
                addr.port()
            );
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.write_all(resp.as_bytes()).await;
        });

        // Resolution succeeds; the ws handshake on the returned URL will fail
        // (nothing listens there), which proves resolution ran.
        let err = CdpClient::connect(&format!("http://{addr}"))
            .await
            .unwrap_err();
        assert!(
            matches!(err, CdpError::Connect(_)),
            "connect to resolved-but-dead ws endpoint must surface Connect, got {err:?}"
        );
        server.await.unwrap();
    }
}
