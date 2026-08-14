//! `flutter run --machine` / `flutter attach --machine` integration.
//!
//! In `--machine` mode, `flutter` prints one JSON object per line:
//!
//! ```text
//! {"event":"app.start",             "params":{...}}
//! {"event":"app.debugService",      "params":{"wsUri":"http://127.0.0.1:PORT/AUTH/ws", ...}}
//! {"event":"app.started",           "params":{...}}
//! ```
//!
//! The interesting event is `app.debugService` whose `params.wsUri` is the
//! Dart VM Service HTTP endpoint (the auth token lives in the URL path). We
//! convert it to a `ws://` URI and hand it to [`crate::vm::VmService`].

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Errors from driving `flutter run/attach --machine`.
#[derive(Debug, Error)]
pub enum MachineError {
    #[error("flutter not found on PATH")]
    FlutterNotFound,
    #[error("flutter exited with {status} before reporting the VM Service")]
    FlutterExited { status: String },
    #[error("timed out waiting for the VM Service (is the app built in debug mode?)")]
    Timeout,
    #[error("no app.debugService event in flutter output")]
    NoVmService,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// A decoded line of `flutter --machine` output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineEvent {
    /// The event name, e.g. `app.start`, `app.debugService`.
    pub event: String,
    /// The event's raw `params` object.
    pub params: serde_json::Map<String, serde_json::Value>,
}

/// Parse a single `flutter --machine` output line.
///
/// Accepts both a bare event object (`{"event":...}`) and the array-wrapped
/// form newer Flutter tools emit (`[{"event":...}]`).
pub fn parse_machine_line(line: &str) -> Option<MachineEvent> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let value = match value {
        serde_json::Value::Array(mut arr) => {
            if arr.len() == 1 {
                arr.pop().expect("single-element array")
            } else {
                return None;
            }
        }
        v => v,
    };
    let event = value.get("event")?.as_str()?.to_string();
    let params = value
        .get("params")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    Some(MachineEvent { event, params })
}

/// A live `flutter run --machine` (or `flutter attach --machine`) child.
pub struct FlutterMachine {
    child: Option<Child>,
}

impl std::fmt::Debug for FlutterMachine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlutterMachine").finish()
    }
}

impl FlutterMachine {
    /// Run a Flutter app on a device and wait for its VM Service URI.
    ///
    /// `project_dir` is the app root (containing `pubspec.yaml`); `target`
    /// is the app entry, e.g. `lib/main.dart`; `device_id` is an `adb` serial
    /// (e.g. `emulator-5554`) or `flutter-tester`.
    pub async fn run(
        project_dir: &Path,
        target: &str,
        device_id: &str,
    ) -> Result<Self, MachineError> {
        let child = Command::new("flutter")
            .arg("run")
            .arg("--machine")
            .arg("--device-id")
            .arg(device_id)
            .arg("--target")
            .arg(target)
            .current_dir(project_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| MachineError::FlutterNotFound)?;
        Ok(Self { child: Some(child) })
    }

    /// Attach to an already-running debug app on a device.
    ///
    /// `project_dir` is the app root (containing `pubspec.yaml`); `flutter
    /// attach` needs it to resolve the target entry.
    pub async fn attach(project_dir: &Path, device_id: &str) -> Result<Self, MachineError> {
        let child = Command::new("flutter")
            .arg("attach")
            .arg("--machine")
            .arg("--device-id")
            .arg(device_id)
            .current_dir(project_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| MachineError::FlutterNotFound)?;
        Ok(Self { child: Some(child) })
    }

    /// Read `--machine` output until `app.debugService` reports `wsUri`,
    /// then return the `ws://` VM Service endpoint.
    pub async fn wait_for_vm_service(&mut self, timeout: Duration) -> Result<String, MachineError> {
        let Some(child) = &mut self.child else {
            return Err(MachineError::FlutterExited {
                status: "already dropped".into(),
            });
        };
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| MachineError::Io(std::io::Error::other("no stdout pipe")))?;
        let mut lines = BufReader::new(stdout).lines();

        let deadline = tokio::time::Instant::now() + timeout;
        if let Some(status) = child.try_wait()? {
            return Err(MachineError::FlutterExited {
                status: status.to_string(),
            });
        }
        match scan_machine_output(&mut lines, deadline).await? {
            Some(uri) => Ok(uri),
            None => Err(MachineError::NoVmService),
        }
    }
}

/// Scan a stream of `flutter --machine` lines for the VM Service URI.
///
/// Keeps reading until it finds `app.debugService`/`app.debugPort`
/// (`Ok(Some(ws_uri))`), hits EOF (`Ok(None)`) or passes `deadline`
/// (`Err(Timeout)`). Extracted from [`FlutterMachine::wait_for_vm_service`] so
/// the scanner is testable against canned streams.
async fn scan_machine_output<R: AsyncBufRead + Unpin>(
    lines: &mut tokio::io::Lines<R>,
    deadline: tokio::time::Instant,
) -> Result<Option<String>, MachineError> {
    loop {
        let line = tokio::time::timeout(
            deadline.saturating_duration_since(tokio::time::Instant::now()),
            lines.next_line(),
        )
        .await
        .map_err(|_| MachineError::Timeout)??;
        let Some(line) = line else {
            return Ok(None);
        };
        let Some(ev) = parse_machine_line(&line) else {
            tracing::debug!(target: "sniff_flutter::machine", "ignored: {line}");
            continue;
        };
        tracing::debug!(target: "sniff_flutter::machine", event = %ev.event, "machine event");
        let vm_service = matches!(ev.event.as_str(), "app.debugService" | "app.debugPort")
            && ev
                .params
                .get("wsUri")
                .and_then(serde_json::Value::as_str)
                .is_some();
        if vm_service
            && let Some(ws_uri) = ev.params.get("wsUri").and_then(serde_json::Value::as_str)
        {
            return Ok(Some(crate::vm::to_ws_uri(ws_uri)));
        }
    }
}

impl Drop for FlutterMachine {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn scan(text: &str) -> Result<Option<String>, MachineError> {
        let stream = Cursor::new(text.to_string());
        let mut lines = stream.lines();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(scan_machine_output(&mut lines, deadline))
    }

    #[test]
    fn parses_debug_service_event() {
        let ev = parse_machine_line(
            r#"{"event":"app.debugService","params":{"appId":"x","deviceId":"emulator-5554","wsUri":"http://127.0.0.1:54321/kL0g/ws"}}"#,
        )
        .expect("event");
        assert_eq!(ev.event, "app.debugService");
        assert_eq!(ev.params["wsUri"], "http://127.0.0.1:54321/kL0g/ws");
    }

    #[test]
    fn parses_debug_port_event() {
        let ev = parse_machine_line(
            r#"{"event":"app.debugPort","params":{"appId":"x","port":41005,"wsUri":"ws://127.0.0.1:41005/2rvqRdXfSUc=/ws","baseUri":"file:///data/app/"}}"#,
        )
        .expect("event");
        assert_eq!(ev.event, "app.debugPort");
        assert_eq!(ev.params["wsUri"], "ws://127.0.0.1:41005/2rvqRdXfSUc=/ws");
    }

    #[test]
    fn ignores_non_json_lines() {
        assert!(parse_machine_line("").is_none());
        assert!(parse_machine_line("not json").is_none());
        assert!(parse_machine_line("Launching lib/main.dart on emulator-5554...").is_none());
    }

    #[test]
    fn ignores_missing_params() {
        let ev = parse_machine_line(r#"{"event":"app.started"}"#).expect("event");
        assert_eq!(ev.event, "app.started");
        assert!(ev.params.is_empty());
    }

    #[test]
    fn parses_array_wrapped_events() {
        let ev = parse_machine_line(
            r#"[{"event":"app.debugPort","params":{"port":41005,"wsUri":"ws://127.0.0.1:41005/2rvqRdXfSUc=/ws"}}]"#,
        )
        .expect("event");
        assert_eq!(ev.event, "app.debugPort");
        assert_eq!(ev.params["port"], 41005);
    }

    #[test]
    fn rejects_empty_or_multi_element_arrays() {
        assert!(parse_machine_line("[]").is_none());
        assert!(parse_machine_line("[{},{}]").is_none());
    }

    #[test]
    fn scanner_accepts_array_wrapped_debug_port_event() {
        let uri = scan(
            "Launching lib/main.dart on emulator-5554...\n\
             [{\"event\":\"app.start\",\"params\":{}}]\n\
             [{\"event\":\"app.debugPort\",\"params\":{\"port\":41005,\"wsUri\":\"ws://127.0.0.1:41005/2rvqRdXfSUc=/ws\"}}]\n",
        )
        .expect("scan")
        .expect("uri");
        assert_eq!(uri, "ws://127.0.0.1:41005/2rvqRdXfSUc=/ws");
    }

    #[test]
    fn uri_conversion() {
        assert_eq!(
            crate::vm::to_ws_uri("http://127.0.0.1:54321/kL0g/ws"),
            "ws://127.0.0.1:54321/kL0g/ws"
        );
        assert_eq!(
            crate::vm::to_ws_uri("ws://127.0.0.1:54321/kL0g/ws"),
            "ws://127.0.0.1:54321/kL0g/ws"
        );
        assert_eq!(
            crate::vm::to_ws_uri("https://host:8080/a/ws"),
            "wss://host:8080/a/ws"
        );
    }

    #[test]
    fn scanner_returns_vm_service_uri_after_noise_lines() {
        let uri = scan(
            "Launching lib/main.dart on emulator-5554...\n\
             {\"event\":\"app.start\",\"params\":{}}\n\
             {\"event\":\"app.debugService\",\"params\":{\"wsUri\":\"http://127.0.0.1:54321/kL0g/ws\"}}\n",
        )
        .expect("scan")
        .expect("uri");
        assert_eq!(uri, "ws://127.0.0.1:54321/kL0g/ws");
    }

    #[test]
    fn scanner_accepts_new_debug_port_event() {
        let uri = scan(
            "Launching lib/main.dart on emulator-5554...\n\
             {\"event\":\"app.start\",\"params\":{}}\n\
             {\"event\":\"app.debugPort\",\"params\":{\"port\":41005,\"wsUri\":\"ws://127.0.0.1:41005/2rvqRdXfSUc=/ws\"}}\n",
        )
        .expect("scan")
        .expect("uri");
        assert_eq!(uri, "ws://127.0.0.1:41005/2rvqRdXfSUc=/ws");
    }

    #[test]
    fn scanner_ignores_debug_port_without_ws_uri() {
        let uri =
            scan("{\"event\":\"app.debugPort\",\"params\":{\"port\":41005}}\n").expect("scan");
        assert!(uri.is_none());
    }

    #[test]
    fn scanner_returns_none_on_eof_without_event() {
        let uri = scan("{\"event\":\"app.started\",\"params\":{}}\n").expect("scan");
        assert!(uri.is_none());
    }
}
