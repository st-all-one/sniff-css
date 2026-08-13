//! Minimal, dependency-free CDP wire types.
//!
//! We deliberately avoid generating a full CDP type surface. Commands
//! are serialized from `serde_json::Value` params, responses are read
//! as `serde_json::Value`, and only the small set of fields we need is
//! typed.

use serde::Serialize;
use serde_json::{Value, json};

/// An inbound CDP event delivered to subscribers.
///
/// Re-exported from the shared JSON-RPC layer: the CDP event wire shape
/// (`method` + `params` + optional `sessionId`) is identical to the generic
/// one, so CDP subscribes to the same broadcast channel without conversion.
pub use crate::jsonrpc::JsonRpcEvent as CdpEvent;

/// Outbound command frame.
#[derive(Debug, Serialize)]
pub struct Command<'a> {
    pub id: u64,
    pub method: &'a str,
    pub params: Value,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sessionId")]
    pub session_id: Option<&'a str>,
}

impl<'a> Command<'a> {
    /// Build a command with no params.
    pub fn no_params(method: &'a str, id: u64, session_id: Option<&'a str>) -> Command<'a> {
        Command {
            id,
            method,
            params: json!({}),
            session_id,
        }
    }
}

/// A remote debugging endpoint (the `ws://...` URL Chrome prints).
pub type WebSocketUrl = String;

/// Browser-level params used when launching Chromium.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOptions {
    /// Path to the Chrome/Chromium binary. Defaults to an auto-detected one.
    pub executable: Option<String>,
    /// User data directory; a fresh temp dir is used when `None`.
    pub user_data_dir: Option<String>,
    /// Run headless (default true for a sniffing tool).
    pub headless: bool,
    /// Extra command-line flags.
    pub extra_args: Vec<String>,
    /// How long to wait for the DevTools endpoint to appear.
    pub launch_timeout_ms: u64,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            executable: None,
            user_data_dir: None,
            headless: true,
            extra_args: Vec::new(),
            launch_timeout_ms: 15_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_serializes_without_session() {
        let cmd = Command::no_params("Page.enable", 1, None);
        let v = serde_json::to_value(&cmd).unwrap();
        assert_eq!(v["id"], 1);
        assert_eq!(v["method"], "Page.enable");
        assert!(!v.as_object().unwrap().contains_key("session_id"));
    }

    #[test]
    fn command_serializes_with_session() {
        let cmd = Command::no_params("Runtime.evaluate", 2, Some("abc"));
        let v = serde_json::to_value(&cmd).unwrap();
        assert_eq!(v["sessionId"], "abc");
    }

    #[test]
    fn event_accessors() {
        let ev = CdpEvent {
            method: "Network.loadingFinished".into(),
            params: json!({"requestId": "r1", "encodedDataLength": 2048})
                .as_object()
                .unwrap()
                .clone(),
            session_id: Some("s1".into()),
        };
        assert_eq!(ev.param_str("requestId"), Some("r1"));
        assert_eq!(ev.param_i64("encodedDataLength"), Some(2048));
    }
}
