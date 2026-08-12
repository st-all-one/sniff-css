//! Execution of user-interaction actions (click / hover / type) against a
//! CDP session.
//!
//! Actions are used to reveal elements that only exist after an interaction
//! (modals, dropdowns, hover menus, type-ahead suggestions). Each action is
//! split into two phases so the engine can capture a UI-effect snapshot
//! between them:
//!
//! - [`prepare`]: wait for the target (visible + sized), scroll it into view
//!   and return its path/rect/center.
//! - [`perform`]: dispatch the real trusted input event (`Input` domain) and
//!   settle.
//!
//! Chained actions work because each `prepare` waits for *its own* target,
//! which may only exist after the previous action (open modal → open
//! mini-modal → type into an input).

use serde_json::Value;
use sniff_cdp::session::CdpSession;
use sniff_core::{Action, SniffError, SniffResult};
use std::time::Duration;

/// Poll interval while waiting for an action target to appear.
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// Layout settle after `scrollIntoView` before reading coordinates.
const SCROLL_SETTLE: Duration = Duration::from_millis(50);

/// A resolved interaction target: where the action will be dispatched.
#[derive(Debug, Clone)]
pub struct ActionTarget {
    /// Stable path/key of the target element (id-anchored when possible).
    pub path: String,
    /// Bounding rect `(x, y, width, height)` in viewport coordinates.
    pub rect: (f64, f64, f64, f64),
    /// Center point `(x, y)` in viewport coordinates.
    pub center: (f64, f64),
}

/// Run a full action (prepare + perform). Convenience wrapper; the engine
/// uses the two phases directly so it can snapshot the page in between.
pub async fn execute(session: &CdpSession, action: &Action) -> SniffResult<()> {
    let target = prepare(session, action).await?;
    perform(session, action, &target).await
}

/// Wait for the action's target, scroll it into view and resolve its
/// path/rect/center. `Click`/`Hover`/`Type` require the target to be visible
/// and sized; `Upload` only needs it to exist, since file inputs are usually
/// visually hidden (its rect/center are reported as zeroes).
pub async fn prepare(session: &CdpSession, action: &Action) -> SniffResult<ActionTarget> {
    let selector = selector_of(action);
    let timeout_ms = timeout_of(action);
    let require_visible = !matches!(action, Action::Upload { .. });
    wait_for_target(session, selector, timeout_ms, require_visible).await?;
    session
        .evaluate(&scroll_expr(selector), false)
        .await
        .map_err(cdp_err)?;
    // Let the scroll settle before reading the (now current) rect.
    tokio::time::sleep(SCROLL_SETTLE).await;
    let value = session
        .evaluate(&target_expr(selector), false)
        .await
        .map_err(cdp_err)?;
    let rect = (
        value.get("x").and_then(Value::as_f64),
        value.get("y").and_then(Value::as_f64),
        value.get("w").and_then(Value::as_f64),
        value.get("h").and_then(Value::as_f64),
    );
    match rect {
        (Some(x), Some(y), Some(w), Some(h)) if w > 0.0 && h > 0.0 => Ok(ActionTarget {
            path: value
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or(selector)
                .to_string(),
            rect: (x, y, w, h),
            center: (x + w / 2.0, y + h / 2.0),
        }),
        // Upload targets may legitimately be hidden (display:none file
        // inputs); there is no click point to resolve, report zeroes.
        _ if matches!(action, Action::Upload { .. }) => Ok(ActionTarget {
            path: selector.to_string(),
            rect: (0.0, 0.0, 0.0, 0.0),
            center: (0.0, 0.0),
        }),
        _ => Err(SniffError::NoMatch {
            selector: selector.to_string(),
        }),
    }
}
pub async fn perform(
    session: &CdpSession,
    action: &Action,
    target: &ActionTarget,
) -> SniffResult<()> {
    match action {
        Action::Click { settle_ms, .. } => {
            session
                .input_click(target.center.0, target.center.1)
                .await
                .map_err(cdp_err)?;
            sleep(*settle_ms).await;
        }
        Action::Hover { settle_ms, .. } => {
            session
                .input_hover(target.center.0, target.center.1)
                .await
                .map_err(cdp_err)?;
            sleep(*settle_ms).await;
        }
        Action::Type {
            selector,
            text,
            settle_ms,
            ..
        } => {
            session
                .evaluate(&focus_expr(selector), false)
                .await
                .map_err(cdp_err)?;
            session.input_insert_text(text).await.map_err(cdp_err)?;
            sleep(*settle_ms).await;
        }
        Action::Upload {
            selector,
            files,
            settle_ms,
            ..
        } => {
            session
                .set_file_input(selector, files)
                .await
                .map_err(cdp_err)?;
            sleep(*settle_ms).await;
        }
    }
    Ok(())
}

/// Poll until `selector` matches an element (that is visible and sized when
/// `require_visible` is set), up to `timeout_ms`. Existence alone is not
/// enough for click targets: a chained target injected hidden (e.g. inside a
/// closed tab) would otherwise be clicked on top of whatever covers it.
async fn wait_for_target(
    session: &CdpSession,
    selector: &str,
    timeout_ms: u64,
    require_visible: bool,
) -> SniffResult<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let ready = session
            .evaluate(&ready_expr(selector), false)
            .await
            .map_err(cdp_err)?;
        let exists = ready
            .get("exists")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let visible = ready
            .get("visible")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if exists && (!require_visible || visible) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(SniffError::NoMatch {
                selector: selector.to_string(),
            });
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn ready_expr(selector: &str) -> String {
    let sel = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".into());
    format!(
        "(() => {{ try {{ const el = document.querySelector({sel}); if (!el) return {{ exists: false, visible: false }}; \
         const cs = getComputedStyle(el); const r = el.getBoundingClientRect(); \
         return {{ exists: true, visible: cs.display !== 'none' && cs.visibility !== 'hidden' \
         && r.width > 0 && r.height > 0 }}; }} catch (e) {{ return {{ exists: false, visible: false }}; }} }})()"
    )
}

fn scroll_expr(selector: &str) -> String {
    let sel = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".into());
    format!(
        "(() => {{ const el = document.querySelector({sel}); if (!el) return false; \
         el.scrollIntoView({{ block: 'center', inline: 'center', behavior: 'instant' }}); \
         return true; }})()"
    )
}

fn target_expr(selector: &str) -> String {
    let sel = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".into());
    format!(
        "(() => {{ const el = document.querySelector({sel}); if (!el) return null; \
         const r = el.getBoundingClientRect(); if (r.width === 0 && r.height === 0) return null; \
         const esc = (v) => String(v).replace(/\\\\/g, '\\\\\\\\').replace(/\"/g, '\\\\\"'); \
         const anchor = el.id ? '#' + el.id : null; \
         const cls = el.classList; \
         const tok = anchor ? el.tagName.toLowerCase() + anchor \
           : (cls && cls.length) ? el.tagName.toLowerCase() + '.' + cls[0] : el.tagName.toLowerCase(); \
         return {{ x: r.x, y: r.y, w: r.width, h: r.height, path: tok }}; }})()"
    )
}

fn focus_expr(selector: &str) -> String {
    let sel = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".into());
    format!(
        "(() => {{ const el = document.querySelector({sel}); if (!el) return false; \
         el.focus(); return document.activeElement === el; }})()"
    )
}

fn selector_of(action: &Action) -> &str {
    match action {
        Action::Click { selector, .. }
        | Action::Hover { selector, .. }
        | Action::Type { selector, .. }
        | Action::Upload { selector, .. } => selector,
    }
}

fn timeout_of(action: &Action) -> u64 {
    match action {
        Action::Click { timeout_ms, .. }
        | Action::Hover { timeout_ms, .. }
        | Action::Type { timeout_ms, .. }
        | Action::Upload { timeout_ms, .. } => *timeout_ms,
    }
}

fn cdp_err(e: sniff_cdp::session::CdpSessionError) -> SniffError {
    SniffError::Cdp(e.to_string())
}

async fn sleep(ms: u64) {
    if ms > 0 {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_expr_checks_visibility_and_size() {
        let expr = ready_expr("#open-modal");
        assert!(expr.contains("document.querySelector"));
        assert!(expr.contains("cs.display !== 'none'"));
        assert!(expr.contains("r.width > 0 && r.height > 0"));
        assert!(expr.contains("exists"));
    }

    #[test]
    fn target_expr_returns_corner_and_dimensions() {
        let expr = target_expr(".btn");
        assert!(expr.contains("r.width === 0 && r.height === 0"));
        // Returns the top-left corner + size; the engine derives the center.
        assert!(expr.contains("x: r.x, y: r.y, w: r.width, h: r.height"));
        assert!(expr.contains("path: tok"));
    }

    #[test]
    fn scroll_expr_is_instant() {
        let expr = scroll_expr(".btn");
        assert!(expr.contains("scrollIntoView"));
        assert!(expr.contains("behavior: 'instant'"));
    }

    #[test]
    fn focus_expr_checks_active_element() {
        let expr = focus_expr("#q");
        assert!(expr.contains("el.focus()"));
        assert!(expr.contains("document.activeElement === el"));
    }

    #[test]
    fn expressions_escape_selectors() {
        let expr = ready_expr(".a\\\"b");
        assert!(expr.contains("\\\""));
    }

    #[test]
    fn accessors_read_action_fields() {
        let click = Action::Click {
            selector: "#a".into(),
            timeout_ms: 5000,
            settle_ms: 200,
        };
        assert_eq!(selector_of(&click), "#a");
        assert_eq!(timeout_of(&click), 5000);
        let ty = Action::Type {
            selector: "#q".into(),
            text: "x".into(),
            timeout_ms: 7000,
            settle_ms: 100,
        };
        assert_eq!(selector_of(&ty), "#q");
        assert_eq!(timeout_of(&ty), 7000);
        let up = Action::Upload {
            selector: "#file".into(),
            files: vec!["/tmp/x.jpg".into()],
            timeout_ms: 9000,
            settle_ms: 50,
        };
        assert_eq!(selector_of(&up), "#file");
        assert_eq!(timeout_of(&up), 9000);
    }
}
