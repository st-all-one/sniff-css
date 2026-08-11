//! Execution of wait/readiness strategies against a CDP session.

use sniff_cdp::session::CdpSession;
use sniff_core::{ReadyCondition, SniffConfig, SniffError, SniffResult, WaitStrategy};
use std::time::Duration;

/// Poll interval shared by all polling strategies.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Executes the ordered list of wait strategies from a config.
pub async fn wait_for_page(session: &CdpSession, config: &SniffConfig) -> SniffResult<()> {
    for strategy in &config.wait {
        execute_strategy(session, strategy).await?;
    }
    Ok(())
}

async fn execute_strategy(session: &CdpSession, strategy: &WaitStrategy) -> SniffResult<()> {
    match strategy {
        WaitStrategy::Selector {
            selector,
            timeout_ms,
        } => {
            wait_until(
                session,
                *timeout_ms,
                format!("selector `{selector}` to appear"),
                selector_exists_expr(selector),
            )
            .await
        }
        WaitStrategy::NetworkIdle {
            idle_ms,
            timeout_ms,
        } => wait_network_idle(session, *idle_ms, *timeout_ms).await,
        WaitStrategy::ElementReady {
            selector,
            conditions,
            timeout_ms,
        } => {
            wait_until(
                session,
                *timeout_ms,
                format!("element `{selector}` to be ready"),
                element_ready_expr(selector, conditions),
            )
            .await
        }
        WaitStrategy::FontsLoaded { timeout_ms } => {
            let expr = "document.fonts ? document.fonts.ready.then(() => true) : true";
            tokio::time::timeout(
                Duration::from_millis(*timeout_ms),
                session.evaluate(expr, true),
            )
            .await
            .map_err(|_| SniffError::Timeout("fonts to load".into()))?
            .map_err(|e| SniffError::Cdp(e.to_string()))?;
            Ok(())
        }
        WaitStrategy::AppFlag { flag, timeout_ms } => {
            let expr = format!(
                "window[{}] === true",
                serde_json::to_string(flag).map_err(SniffError::from)?
            );
            wait_until(
                session,
                *timeout_ms,
                format!("flag `{flag}` to be set"),
                expr,
            )
            .await
        }
        WaitStrategy::Delay { ms } => {
            tokio::time::sleep(Duration::from_millis(*ms)).await;
            Ok(())
        }
    }
}

/// Poll a boolean expression until it evaluates to `true` or timeout.
async fn wait_until(
    session: &CdpSession,
    timeout_ms: u64,
    what: String,
    expr: String,
) -> SniffResult<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let ready = session
            .evaluate(&expr, false)
            .await
            .map_err(|e| SniffError::Cdp(e.to_string()))?;
        if ready.as_bool().unwrap_or(false) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(SniffError::Timeout(what));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn selector_exists_expr(selector: &str) -> String {
    let sel = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".into());
    format!(
        "(() => {{ try {{ return document.querySelector({sel}) !== null; }} catch (e) {{ return false; }} }})()"
    )
}

fn element_ready_expr(selector: &str, conditions: &[ReadyCondition]) -> String {
    let sel = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".into());
    let checks: Vec<String> = conditions.iter().map(ready_check).collect();
    let joined = checks.join(" && ");
    format!(
        "(() => {{ const el = document.querySelector({sel}); if (!el) return false; const cs = getComputedStyle(el); const r = el.getBoundingClientRect(); return {joined}; }})()"
    )
}

fn ready_check(condition: &ReadyCondition) -> String {
    match condition {
        ReadyCondition::Visible => {
            "cs.display !== 'none' && cs.visibility !== 'hidden'".to_string()
        }
        ReadyCondition::HasSize => "r.width > 0 && r.height > 0".to_string(),
        ReadyCondition::Opacity(threshold) => {
            format!("parseFloat(cs.opacity) >= {threshold}")
        }
    }
}

/// Wait until no network requests are in flight for `idle_ms`.
///
/// In-flight counting is deliberately conservative: `requestWillBeSent`
/// starts a request and `loadingFinished`/`loadingFailed` ends it. If a
/// long-lived stream (SSE/WebSocket) keeps the counter above zero for
/// `idle_grace_ms`, we treat the page as idle once no *new* network
/// activity has been observed for `idle_ms`.
async fn wait_network_idle(session: &CdpSession, idle_ms: u64, timeout_ms: u64) -> SniffResult<()> {
    const IDLE_GRACE_MS: u64 = 2000;
    let mut rx = session.subscribe();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    let mut last_active = tokio::time::Instant::now();
    let mut in_flight: u64 = 0;
    let mut seen_any = false;

    loop {
        let no_new_activity = last_active.elapsed() >= Duration::from_millis(idle_ms);
        let grace_passed =
            seen_any && last_active.elapsed() >= Duration::from_millis(idle_ms + IDLE_GRACE_MS);
        if (in_flight == 0 && no_new_activity) || grace_passed {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(SniffError::Timeout("network to become idle".into()));
        }

        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(ev) if ev.session_id.as_deref() == Some(session.id()) => {
                        match ev.method.as_str() {
                            "Network.requestWillBeSent" => {
                                in_flight += 1;
                                seen_any = true;
                                last_active = tokio::time::Instant::now();
                            }
                            "Network.loadingFinished" | "Network.loadingFailed" => {
                                in_flight = in_flight.saturating_sub(1);
                                last_active = tokio::time::Instant::now();
                            }
                            _ => {}
                        }
                    }
                    Ok(_) => {}
                    Err(_) => return Err(SniffError::Cdp("event stream closed".into())),
                }
            }
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_exists_expr_is_self_contained() {
        let expr = selector_exists_expr(".card");
        assert!(expr.contains("document.querySelector"));
        assert!(expr.contains("\".card\""));
    }

    #[test]
    fn ready_expr_embeds_conditions() {
        let expr = element_ready_expr(
            ".card",
            &[ReadyCondition::Visible, ReadyCondition::Opacity(0.5)],
        );
        assert!(expr.contains("cs.display !== 'none'"));
        assert!(expr.contains("parseFloat(cs.opacity) >= 0.5"));
        assert!(expr.contains("document.querySelector"));
    }

    #[test]
    fn ready_expr_escapes_selector() {
        let expr = element_ready_expr(".a\\\"b", &[ReadyCondition::Visible]);
        assert!(expr.contains("\\\""));
    }
}
