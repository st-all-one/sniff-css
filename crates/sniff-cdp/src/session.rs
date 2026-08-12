//! High-level operations bound to a single CDP target session.

use crate::client::{CdpClient, CdpError, Result};
use crate::protocol::CdpEvent;
use serde_json::{Value, json};
use std::time::Duration;

/// Convenience alias for session-level errors.
pub type CdpSessionError = CdpError;

/// A bound session to a Chrome target (page).
///
/// Wraps a shared [`CdpClient`] with the `session_id` that Chrome
/// assigned when the target was attached. All commands are sent through
/// the shared connection but scoped to this session.
#[derive(Clone)]
pub struct CdpSession {
    client: CdpClient,
    session_id: String,
}

impl std::fmt::Debug for CdpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CdpSession")
            .field("session_id", &self.session_id)
            .finish()
    }
}

impl CdpSession {
    /// Create a new page target and attach to it, returning the session.
    pub async fn new_page(client: &CdpClient, url: &str) -> Result<Self> {
        let target = client
            .call("Target.createTarget", json!({"url": url}), None)
            .await?;
        let target_id = target
            .get("targetId")
            .and_then(Value::as_str)
            .ok_or_else(|| CdpError::Protocol {
                method: "Target.createTarget".into(),
                message: "missing targetId".into(),
            })?
            .to_string();

        let attach = client
            .call(
                "Target.attachToTarget",
                json!({"targetId": target_id, "flatten": true}),
                None,
            )
            .await?;
        let session_id = attach
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| CdpError::Protocol {
                method: "Target.attachToTarget".into(),
                message: "missing sessionId".into(),
            })?
            .to_string();

        let session = Self {
            client: client.clone(),
            session_id,
        };

        // Enable the domains the engine relies on.
        session.enable_domains().await?;

        Ok(session)
    }

    /// Enable Page, Runtime, DOM and Network domains for this session.
    pub async fn enable_domains(&self) -> Result<()> {
        for domain in ["Page", "Runtime", "DOM", "Network"] {
            let method = format!("{domain}.enable");
            self.client
                .call_no_params(&method, Some(&self.session_id))
                .await?;
        }
        Ok(())
    }

    /// The session id assigned by Chrome.
    pub fn id(&self) -> &str {
        &self.session_id
    }

    /// Send an arbitrary command scoped to this session.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.client
            .call(method, params, Some(&self.session_id))
            .await
    }

    /// Send a no-param command scoped to this session.
    pub async fn call_no_params(&self, method: &str) -> Result<Value> {
        self.client
            .call_no_params(method, Some(&self.session_id))
            .await
    }

    /// Subscribe to events, filtered to this session by the caller.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<CdpEvent> {
        self.client.subscribe()
    }

    /// Navigate the current page and wait for the load event.
    pub async fn navigate(&self, url: &str) -> Result<()> {
        self.call("Page.navigate", json!({"url": url})).await?;
        self.wait_for_event("Page.loadEventFired", Duration::from_secs(60))
            .await?;
        Ok(())
    }

    /// Override the emulated viewport size (affects `%`, `vh`, `rem`
    /// derivations and media queries).
    pub async fn set_viewport(&self, width: u32, height: u32) -> Result<()> {
        self.call(
            "Emulation.setDeviceMetricsOverride",
            json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": 1.0,
                "mobile": false,
            }),
        )
        .await?;
        Ok(())
    }

    /// Navigate without waiting (caller is responsible for readiness).
    pub async fn navigate_no_wait(&self, url: &str) -> Result<()> {
        self.call("Page.navigate", json!({"url": url})).await?;
        Ok(())
    }

    /// Wait for a specific event on this session, ignoring others.
    pub async fn wait_for_event(&self, method: &str, timeout: Duration) -> Result<CdpEvent> {
        let mut rx = self.subscribe();
        tokio::time::timeout(timeout, async {
            loop {
                match rx.recv().await {
                    Ok(ev)
                        if ev.method == method
                            && ev.session_id.as_deref() == Some(&self.session_id) =>
                    {
                        return Ok(ev);
                    }
                    Ok(_) => continue,
                    Err(_) => return Err(CdpError::Closed),
                }
            }
        })
        .await
        .map_err(|_| CdpError::Timeout(method.to_string()))?
    }

    /// Evaluate a JavaScript expression in the page context.
    ///
    /// `await_promise` makes `Runtime.evaluate` wait for promises to
    /// settle (used for `document.fonts.ready` and async snippets).
    pub async fn evaluate(&self, expression: &str, await_promise: bool) -> Result<Value> {
        let res = self
            .call(
                "Runtime.evaluate",
                json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": await_promise,
                    "userGesture": true,
                }),
            )
            .await?;

        // Surface JS exceptions as errors for easier debugging.
        if let Some(details) = res.get("exceptionDetails") {
            let text = details
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("JS exception");
            let description = details
                .get("exception")
                .and_then(|e| e.get("description"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            return Err(CdpError::Protocol {
                method: "Runtime.evaluate".into(),
                message: format!("{text}: {description}").trim().to_string(),
            });
        }

        Ok(res
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    /// Evaluate and require a JSON-serializable value back.
    pub async fn evaluate_json(&self, expression: &str) -> Result<Value> {
        self.evaluate(expression, false).await
    }

    /// Dispatch a real trusted mouse click at viewport coordinates
    /// (`Input.dispatchMouseEvent`), triggering the full `pointer`/
    /// `mouse`/`click` chain and `:active` state. Coordinates are
    /// relative to the top-left of the visible viewport.
    pub async fn input_click(&self, x: f64, y: f64) -> Result<()> {
        self.input_hover(x, y).await?;
        self.call(
            "Input.dispatchMouseEvent",
            json!({
                "type": "mousePressed",
                "x": x,
                "y": y,
                "button": "left",
                "buttons": 1,
                "clickCount": 1,
            }),
        )
        .await?;
        self.call(
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseReleased",
                "x": x,
                "y": y,
                "button": "left",
                "buttons": 0,
                "clickCount": 1,
            }),
        )
        .await?;
        Ok(())
    }

    /// Move the pointer to viewport coordinates (`Input.dispatchMouseEvent`
    /// `mouseMoved`), revealing CSS `:hover` menus and tooltips.
    pub async fn input_hover(&self, x: f64, y: f64) -> Result<()> {
        self.call(
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseMoved",
                "x": x,
                "y": y,
                "buttons": 0,
            }),
        )
        .await?;
        Ok(())
    }

    /// Insert text into the currently focused editable element
    /// (`Input.insertText`).
    pub async fn input_insert_text(&self, text: &str) -> Result<()> {
        self.call("Input.insertText", json!({ "text": text }))
            .await?;
        Ok(())
    }

    /// Close this session's target.
    pub async fn close(&self) -> Result<()> {
        self.call_no_params("Page.close").await?;
        Ok(())
    }
}
