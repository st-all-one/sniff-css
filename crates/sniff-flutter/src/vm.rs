//! Dart VM Service Protocol client.
//!
//! The VM Service speaks the same JSON-RPC-over-WebSocket wire as CDP, so
//! [`VmService`] wraps the shared [`JsonRpcClient`](sniff_cdp::jsonrpc::JsonRpcClient)
//! and adds VM Service conveniences. The Flutter widget/render trees are
//! fetched through the `ext.flutter.inspector.*` service extensions in
//! `crate::extractor` (T4/T5).

use serde_json::Value;
use sniff_cdp::jsonrpc::JsonRpcClient;
use sniff_cdp::jsonrpc::JsonRpcError;

/// Result alias for the VM Service layer.
pub type Result<T> = std::result::Result<T, VmError>;

/// Errors from VM Service interactions.
#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error("vm service websocket: {0}")]
    Transport(#[from] JsonRpcError),
    #[error("vm service call `{0}` failed: {1}")]
    Call(String, String),
    #[error("{0}")]
    Other(String),
}

/// Normalize an `http(s)://` VM Service URI to a `ws(s)://` one.
///
/// `flutter --machine` reports `params.wsUri` as `http://127.0.0.1:PORT/AUTH/ws`
/// (the auth token lives in the URL path); the wire transport needs the
/// `ws://` scheme.
pub fn to_ws_uri(uri: &str) -> String {
    if let Some(rest) = uri.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if let Some(rest) = uri.strip_prefix("https://") {
        format!("wss://{rest}")
    } else {
        uri.to_string()
    }
}

/// A connection to one Dart VM Service endpoint.
#[derive(Clone)]
pub struct VmService {
    client: JsonRpcClient,
}

impl std::fmt::Debug for VmService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VmService").finish()
    }
}

impl VmService {
    /// Connect to a VM Service endpoint (`ws://host:port/AUTH/ws`).
    pub async fn connect(ws_uri: &str) -> Result<Self> {
        let client = JsonRpcClient::connect(ws_uri).await?;
        Ok(Self { client })
    }

    /// Call any VM Service method (e.g. `getVM`, `ext.flutter.inspector.*`).
    pub async fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let raw = self
            .client
            .call(method, params, None)
            .await
            .map_err(|e| VmError::Call(method.to_string(), e.to_string()))?;
        Ok(unwrap_extension_result(method, raw))
    }

    /// Call a VM Service method with no params.
    pub async fn call_no_params(&self, method: &str) -> Result<serde_json::Value> {
        self.client
            .call_no_params(method, None)
            .await
            .map_err(|e| VmError::Call(method.to_string(), e.to_string()))
    }

    /// Basic liveness probe: `getVM` returns the isolate list.
    pub async fn get_vm(&self) -> Result<serde_json::Value> {
        self.call_no_params("getVM").await
    }

    /// Close the connection.
    pub async fn close(&self) {
        self.client.close().await;
    }
}

/// Service-extension responses (`ext.*`) carry a second envelope:
///
/// ```json
/// {"result": {"result": <payload>, "type": "_extensionType", "method": "..."}}
/// ```
///
/// The shared client already unwraps the outer `result`; this removes the
/// `_extensionType` layer so callers see the payload directly (the same shape
/// `getVM` and friends return). Non-extension methods are returned unchanged.
fn unwrap_extension_result(method: &str, raw: serde_json::Value) -> serde_json::Value {
    let Value::Object(obj) = &raw else {
        return raw;
    };
    if !method.starts_with("ext.flutter.")
        || obj.get("type").and_then(Value::as_str) != Some("_extensionType")
    {
        return raw;
    }
    obj.get("result").cloned().unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_uri_normalization() {
        assert_eq!(
            to_ws_uri("http://127.0.0.1:54321/abc/ws"),
            "ws://127.0.0.1:54321/abc/ws"
        );
        assert_eq!(to_ws_uri("https://x:1/y/ws"), "wss://x:1/y/ws");
        assert_eq!(to_ws_uri("ws://h:1/a/ws"), "ws://h:1/a/ws");
    }

    #[test]
    fn unwraps_extension_type_envelope() {
        let raw = serde_json::json!({
            "result": {"valueId": "inspector-0", "children": []},
            "type": "_extensionType",
            "method": "ext.flutter.inspector.getRootWidgetSummaryTree"
        });
        let out = unwrap_extension_result("ext.flutter.inspector.getRootWidgetSummaryTree", raw);
        assert_eq!(out["valueId"], "inspector-0");
    }

    #[test]
    fn leaves_regular_methods_and_errors_untouched() {
        let raw = serde_json::json!({"isolates": []});
        assert_eq!(
            unwrap_extension_result("getVM", raw.clone()),
            raw,
            "non-extension method result passes through"
        );
        let raw2 = serde_json::json!({
            "result": 1,
            "type": "not_an_extension",
            "method": "ext.flutter.foo"
        });
        assert_eq!(
            unwrap_extension_result("ext.flutter.foo", raw2.clone()),
            raw2
        );
    }
}
