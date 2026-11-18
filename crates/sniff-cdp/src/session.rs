//! High-level operations bound to a single CDP target session.

use crate::client::{CdpClient, CdpError, Result};
use crate::protocol::CdpEvent;
use base64::Engine as _;
use serde_json::{Value, json};
use std::time::Duration;

/// Convenience alias for session-level errors.
pub type CdpSessionError = CdpError;

/// Build the params object for `Emulation.setEmulatedMedia` from `(name, value)` pairs.
fn emulated_media_params(features: &[(String, String)]) -> Value {
    let entries: Vec<Value> = features
        .iter()
        .map(|(name, value)| json!({"name": name, "value": value}))
        .collect();
    json!({"features": entries})
}

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
            .finish_non_exhaustive()
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
        self.set_viewport_with_mobile(width, height, false).await
    }

    /// Override the emulated viewport, optionally in mobile/touch mode so
    /// `pointer: coarse` media queries and touch handlers observe it.
    pub async fn set_viewport_with_mobile(
        &self,
        width: u32,
        height: u32,
        mobile: bool,
    ) -> Result<()> {
        self.call(
            "Emulation.setDeviceMetricsOverride",
            json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": 1.0,
                "mobile": mobile,
            }),
        )
        .await?;
        if mobile {
            self.call(
                "Emulation.setTouchEmulationEnabled",
                json!({"enabled": true, "maxTouchPoints": 5}),
            )
            .await?;
        }
        Ok(())
    }

    /// Apply emulated media features via `Emulation.setEmulatedMedia`.
    ///
    /// Each pair is a CDP media feature name/value (e.g.
    /// `("prefers-reduced-motion", "reduce")`). Passing an empty slice
    /// clears all emulated features, restoring the browser defaults.
    pub async fn set_emulated_media(&self, features: &[(String, String)]) -> Result<()> {
        tracing::debug!(session_id = %self.session_id, count = features.len(), "set emulated media");
        self.call(
            "Emulation.setEmulatedMedia",
            emulated_media_params(features),
        )
        .await?;
        Ok(())
    }

    /// Stabilization wrapper: emulate `prefers-reduced-motion: reduce`.
    ///
    /// Kept as a dedicated wrapper so the stabilization path never has to
    /// build the feature vector itself; delegates to [`Self::set_emulated_media`].
    pub async fn emulate_reduced_motion(&self) -> Result<()> {
        self.set_emulated_media(&[("prefers-reduced-motion".to_string(), "reduce".to_string())])
            .await
    }

    /// Capture the current page as a PNG (`Page.captureScreenshot`).
    ///
    /// `full_page` captures the entire scrollable document instead of just
    /// the visible viewport. Returns the decoded PNG bytes.
    pub async fn capture_screenshot(&self, full_page: bool) -> Result<Vec<u8>> {
        let res = self
            .call(
                "Page.captureScreenshot",
                json!({
                    "format": "png",
                    "fromSurface": true,
                    "captureBeyondViewport": full_page,
                }),
            )
            .await?;
        let data = res
            .get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| CdpError::Protocol {
                method: "Page.captureScreenshot".into(),
                message: "response missing `data`".into(),
            })?;
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| CdpError::Protocol {
                method: "Page.captureScreenshot".into(),
                message: format!("invalid base64: {e}"),
            })
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
                    Ok(_) => {}
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

    /// Dispatch a trusted keyboard event (`Input.dispatchKeyEvent`) to the
    /// focused element. Used by `key:` actions (e.g. Escape on a dialog).
    pub async fn input_key(&self, name: &str) -> Result<()> {
        let (key, code, vk, text): (String, String, u64, Option<String>) = match name {
            "Escape" | "Esc" => ("Escape".into(), "Escape".into(), 27, None),
            "Enter" => ("Enter".into(), "Enter".into(), 13, Some("\r".into())),
            "Tab" => ("Tab".into(), "Tab".into(), 9, None),
            "Space" | " " => (" ".into(), "Space".into(), 32, Some(" ".into())),
            "Backspace" => ("Backspace".into(), "Backspace".into(), 8, None),
            other if other.chars().count() == 1 => {
                let ch = other.chars().next().unwrap_or(' ');
                let (code, vk) = if ch.is_ascii_alphabetic() {
                    let upper = u64::from(u32::from(ch.to_ascii_uppercase()));
                    (format!("Key{}", ch.to_ascii_uppercase()), upper)
                } else if ch.is_ascii_digit() {
                    let digit = u64::from(u32::from(ch))
                        .saturating_sub(u64::from(u32::from('0')))
                        .saturating_add(48);
                    (format!("Digit{ch}"), digit)
                } else {
                    (other.to_string(), 0)
                };
                (other.to_string(), code, vk, Some(other.to_string()))
            }
            other => (other.to_string(), other.to_string(), 0, None),
        };
        for kind in ["keyDown", "keyUp"] {
            let base = serde_json::json!({
                "type": kind,
                "key": key,
                "code": code,
                "windowsVirtualKeyCode": vk,
                "nativeVirtualKeyCode": vk,
            });
            let params = if kind == "keyDown"
                && let Some(text) = &text
            {
                serde_json::json!({
                    "type": kind,
                    "key": key,
                    "code": code,
                    "windowsVirtualKeyCode": vk,
                    "nativeVirtualKeyCode": vk,
                    "text": text,
                })
            } else {
                base
            };
            self.call("Input.dispatchKeyEvent", params).await?;
        }
        Ok(())
    }

    /// Insert text into the currently focused editable element
    /// (`Input.insertText`).
    pub async fn input_insert_text(&self, text: &str) -> Result<()> {
        self.call("Input.insertText", json!({ "text": text }))
            .await?;
        Ok(())
    }

    /// Attach local files to an `<input type=file>` matched by `selector`
    /// (`DOM.setFileInputFiles`). The browser fires the `change` event itself,
    /// so upload handlers (crop previews, etc.) run for real. `files` are
    /// resolved by the browser process — in a container they must exist inside
    /// it. Works even for visually hidden inputs.
    pub async fn set_file_input(&self, selector: &str, files: &[String]) -> Result<()> {
        let doc = self.call("DOM.getDocument", json!({ "depth": 1 })).await?;
        let root = doc
            .get("root")
            .and_then(|r| r.get("nodeId"))
            .and_then(Value::as_i64)
            .ok_or_else(|| CdpError::Protocol {
                method: "DOM.getDocument".into(),
                message: "response missing root nodeId".into(),
            })?;
        let node = self
            .call(
                "DOM.querySelector",
                json!({ "nodeId": root, "selector": selector }),
            )
            .await?;
        let node_id =
            node.get("nodeId")
                .and_then(Value::as_i64)
                .ok_or_else(|| CdpError::Protocol {
                    method: "DOM.querySelector".into(),
                    message: "response missing nodeId".into(),
                })?;
        if node_id == 0 {
            return Err(CdpError::Protocol {
                method: "DOM.setFileInputFiles".into(),
                message: format!("selector `{selector}` matched no element"),
            });
        }
        self.call(
            "DOM.setFileInputFiles",
            json!({ "nodeId": node_id, "files": files }),
        )
        .await?;
        Ok(())
    }

    /// Set extra HTTP headers applied to every request of this session
    /// (`Network.setExtraHTTPHeaders`), e.g. a `X-CMS-AI-Token` for a
    /// stateless CMS AI middleware. Must be called before navigation.
    pub async fn set_extra_headers(&self, headers: &[(String, String)]) -> Result<()> {
        if headers.is_empty() {
            return Ok(());
        }
        let mut map = serde_json::Map::new();
        for (k, v) in headers {
            map.insert(k.clone(), Value::String(v.clone()));
        }
        self.call(
            "Network.setExtraHTTPHeaders",
            json!({ "headers": Value::Object(map) }),
        )
        .await?;
        Ok(())
    }

    /// Read every cookie in the browser jar (`Network.getAllCookies`).
    pub async fn get_all_cookies(&self) -> Result<Vec<Value>> {
        let res = self.call_no_params("Network.getAllCookies").await?;
        Ok(res
            .get("cookies")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    /// Set cookies into the browser jar (`Network.setCookies`). Each entry
    /// must be a CDP `CookieParam` object (see
    /// [`crate::storage::StorageState::to_cdp_cookies`]).
    pub async fn set_cookies(&self, cookies: &[Value]) -> Result<()> {
        if cookies.is_empty() {
            return Ok(());
        }
        self.call("Network.setCookies", json!({ "cookies": cookies }))
            .await?;
        Ok(())
    }

    /// Register a script that runs on every future document of this session
    /// before the page's own scripts (`Page.addScriptToEvaluateOnNewDocument`)
    /// — used to restore `localStorage` before navigation.
    pub async fn add_init_script(&self, source: &str) -> Result<()> {
        self.call(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": source }),
        )
        .await?;
        Ok(())
    }

    /// Close this session's target.
    pub async fn close(&self) -> Result<()> {
        self.call_no_params("Page.close").await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::emulated_media_params;
    use serde_json::json;

    #[test]
    fn emulated_media_params_preserves_input_order() {
        let features = vec![
            ("prefers-reduced-motion".to_string(), "reduce".to_string()),
            ("prefers-color-scheme".to_string(), "dark".to_string()),
        ];
        assert_eq!(
            emulated_media_params(&features),
            json!({"features": [
                {"name": "prefers-reduced-motion", "value": "reduce"},
                {"name": "prefers-color-scheme", "value": "dark"},
            ]})
        );
    }

    #[test]
    fn emulated_media_params_empty_clears_features() {
        assert_eq!(emulated_media_params(&[]), json!({"features": []}));
    }
}
