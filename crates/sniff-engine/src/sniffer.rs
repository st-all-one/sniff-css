//! Orchestration: browser lifecycle, page sessions and the sniffing flow.

use crate::action;
use crate::ax;
use crate::effects;
use crate::extractor::{self, SniffOutcome};
use crate::waiter;
use sniff_cdp::browser::BrowserProcess;
use sniff_cdp::client::CdpClient;
use sniff_cdp::protocol::LaunchOptions;
use sniff_cdp::session::CdpSession;
use sniff_core::contrast;
use sniff_core::{Action, SniffConfig, SniffError, SniffResult};
use std::time::Duration;

/// A coarse phase of the sniffing pipeline, used to report progress to
/// long-running consumers (e.g. an MCP server) without coupling the engine
/// to any transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Navigating,
    /// Performing user interactions (click/hover/type) that reveal
    /// elements only present after an action.
    Interacting,
    Waiting,
    Extracting,
    /// Capturing the accessibility tree (only when requested).
    Accessibility,
    Formatting {
        nodes: usize,
    },
}

impl Phase {
    /// A rough 0..=1.0 progress estimate for the phase.
    pub fn progress(&self) -> f64 {
        match self {
            Phase::Navigating => 0.2,
            Phase::Interacting => 0.35,
            Phase::Waiting => 0.4,
            Phase::Extracting => 0.7,
            Phase::Accessibility => 0.8,
            Phase::Formatting { .. } => 0.9,
        }
    }
}

/// De-animation pass executed before waits when `config.stabilize` is set:
/// emulates `prefers-reduced-motion: reduce`, cancels running animations
/// and injects an override that disables future animations/transitions,
/// so the captured state is deterministic across runs.
const STABILIZE_JS: &str = r#"
(() => {
  const id = '__sniff_stabilize';
  if (!document.getElementById(id)) {
    const st = document.createElement('style');
    st.id = id;
    st.textContent = '*,*::before,*::after{animation:none!important;transition:none!important;scroll-behavior:auto!important;animation-delay:0s!important;transition-delay:0s!important}';
    document.head.appendChild(st);
  }
  document.getAnimations().forEach((a) => { try { a.cancel(); } catch (e) {} });
  return true;
})()
"#;

/// A reusable sniffer: owns one browser process + CDP connection and
/// serves any number of sequential sniffing runs (each gets a fresh
/// page target, avoiding the cold-start cost of relaunching Chrome).
pub struct Sniffer {
    client: CdpClient,
    _process: BrowserProcess,
}

impl std::fmt::Debug for Sniffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sniffer")
            .field("endpoint", &self._process.ws_endpoint)
            .finish()
    }
}

impl Sniffer {
    /// Launch a fresh headless browser and connect to its DevTools.
    pub async fn launch(opts: &LaunchOptions) -> SniffResult<Self> {
        let process = BrowserProcess::launch(opts)
            .await
            .map_err(|e| SniffError::Browser(e.to_string()))?;
        let client = process
            .connect()
            .await
            .map_err(|e| SniffError::Cdp(e.to_string()))?;
        Ok(Self {
            client,
            _process: process,
        })
    }

    /// Connect to an already-running browser (e.g. your dev server).
    pub async fn connect(endpoint: &str) -> SniffResult<Self> {
        let client = CdpClient::connect(endpoint)
            .await
            .map_err(|e| SniffError::Cdp(e.to_string()))?;
        // A placeholder process that only keeps the endpoint string; it
        // will not be killed on drop since it does not own the child.
        let process = BrowserProcess::placeholder(endpoint);
        Ok(Self {
            client,
            _process: process,
        })
    }

    /// Open a fresh page target on the shared browser connection. The
    /// caller is responsible for closing the returned session.
    pub async fn new_session(&self) -> SniffResult<CdpSession> {
        CdpSession::new_page(&self.client, "about:blank")
            .await
            .map_err(|e| SniffError::Cdp(e.to_string()))
    }

    /// Run a full sniffing pipeline and return the outcome.
    pub async fn sniff(&self, config: &SniffConfig) -> SniffResult<SniffOutcome> {
        let session = self.new_session().await?;
        let result = sniff_session(&session, config).await;
        let _ = session.close().await;
        result
    }
}

/// Run the pipeline on an existing session.
pub async fn sniff_session(
    session: &CdpSession,
    config: &SniffConfig,
) -> SniffResult<SniffOutcome> {
    sniff_session_with_progress(session, config, |_| async {}).await
}

/// Like [`sniff_session`], but invokes `on_progress(phase)` at each stage
/// so consumers can stream progress without blocking the pipeline.
pub async fn sniff_session_with_progress<F, Fut>(
    session: &CdpSession,
    config: &SniffConfig,
    mut on_progress: F,
) -> SniffResult<SniffOutcome>
where
    F: FnMut(Phase) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    if let Some(vp) = config.viewport {
        session
            .set_viewport(vp.width, vp.height)
            .await
            .map_err(|e| SniffError::Cdp(e.to_string()))?;
    }
    on_progress(Phase::Navigating).await;
    session
        .navigate(&config.url)
        .await
        .map_err(|e| SniffError::Cdp(e.to_string()))?;
    if config.stabilize {
        session
            .call(
                "Emulation.setEmulatedMedia",
                serde_json::json!({
                    "features": [{ "name": "prefers-reduced-motion", "value": "reduce" }]
                }),
            )
            .await
            .map_err(|e| SniffError::Cdp(e.to_string()))?;
        session
            .evaluate(STABILIZE_JS, false)
            .await
            .map_err(|e| SniffError::Cdp(e.to_string()))?;
        // Let the layout settle after animations are cancelled.
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    on_progress(Phase::Waiting).await;
    let mut action_reports: Vec<serde_json::Value> = Vec::new();
    if config.actions.is_empty() {
        waiter::wait_for_page(session, config).await?;
    } else {
        // Reveal elements that only exist after an interaction: each
        // action waits for its own target and performs the interaction;
        // the wait pipeline then targets the post-interaction DOM (e.g. a
        // `.modal` the click just opened).
        on_progress(Phase::Interacting).await;
        for (index, act) in config.actions.iter().enumerate() {
            let target = action::prepare(session, act)
                .await
                .map_err(|e| chain_error(index, act, &config.actions, e))?;
            let before = if config.effects {
                Some(effects::capture(session, config.stable_key.as_deref()).await?)
            } else {
                None
            };
            action::perform(session, act, &target)
                .await
                .map_err(|e| chain_error(index, act, &config.actions, e))?;
            let after = if config.effects {
                Some(effects::capture(session, config.stable_key.as_deref()).await?)
            } else {
                None
            };
            if let (Some(before), Some(after)) = (before, after) {
                action_reports.push(effects::diff(
                    &before,
                    &after,
                    &target,
                    act,
                    index,
                    config.effects_limit,
                ));
            }
        }
        // Freeze animations/transitions started by the interaction so the
        // captured state stays deterministic across runs. The injected
        // `animation/transition: none !important` style already covers
        // elements the actions created; this also cancels running ones.
        if config.stabilize {
            session
                .evaluate(STABILIZE_JS, false)
                .await
                .map_err(|e| SniffError::Cdp(e.to_string()))?;
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        on_progress(Phase::Waiting).await;
        waiter::wait_for_page(session, config).await?;
    }
    on_progress(Phase::Extracting).await;
    let mut outcome = extractor::extract(session, config).await?;
    if !action_reports.is_empty() {
        outcome.actions = Some(serde_json::Value::Array(action_reports));
    }
    if outcome.snapshots.is_empty() {
        return Err(SniffError::NoMatch {
            selector: config.selector.clone(),
        });
    }

    if config.output.include_contrast {
        contrast::apply_contrast_all(&mut outcome.snapshots);
    }

    if config.output.include_ax || config.ax_tree {
        on_progress(Phase::Accessibility).await;
        let capture = ax::capture(
            session,
            &outcome.snapshots,
            config.output.include_ax,
            config.ax_tree,
        )
        .await?;
        if let Some(facets) = capture.facets {
            ax::attach_ax(&mut outcome.snapshots, &facets);
        }
        outcome.ax_tree = capture.tree;
    }

    if config.screenshot {
        // Capture the final page state (post-stabilize/post-actions) as a
        // PNG; the decoded bytes are surfaced to the caller to persist.
        outcome.screenshot = Some(
            session
                .capture_screenshot(config.screenshot_full_page)
                .await
                .map_err(|e| SniffError::Cdp(e.to_string()))?,
        );
    }

    let nodes: usize = outcome.snapshots.iter().map(|s| s.node_count()).sum();
    on_progress(Phase::Formatting { nodes }).await;
    Ok(outcome)
}

/// Enrich an action failure with chain context so a broken sequence names
/// the exact step and its predecessors (e.g. "the mini-modal trigger never
/// appeared after the modal opened").
fn chain_error(index: usize, failed: &Action, chain: &[Action], cause: SniffError) -> SniffError {
    let describe = |a: &Action| match a {
        Action::Click { selector, .. } => format!("click:{selector}"),
        Action::Hover { selector, .. } => format!("hover:{selector}"),
        Action::Type { selector, text, .. } => format!("type:{selector}:{text}"),
    };
    let prior = chain[..index]
        .iter()
        .map(describe)
        .collect::<Vec<_>>()
        .join(" → ");
    SniffError::Other(format!(
        "action #{index} ({}) failed: {cause}. Prior steps: {} — if the target depends on the \
         previous step, raise its timeout_ms (wait for target) and settle_ms (post-action render); \
         verify the previous step actually revealed it.",
        describe(failed),
        if prior.is_empty() {
            "(none, first step)".to_string()
        } else {
            prior
        },
    ))
}
