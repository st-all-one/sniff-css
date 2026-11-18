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
use sniff_core::modern::MatrixTag;
use sniff_core::storage::StorageState;
use sniff_core::types::ElementSnapshot;
use sniff_core::{Action, SniffConfig, SniffError, SniffResult};
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// A coarse phase of the sniffing pipeline, used to report progress to
/// long-running consumers (e.g. an MCP server) without coupling the engine
/// to any transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Navigating to the target URL.
    Navigating,
    /// Performing user interactions (click/hover/type) that reveal
    /// elements only present after an action.
    Interacting,
    /// Running the configured wait strategies.
    Waiting,
    /// Extracting computed styles from the page.
    Extracting,
    /// Capturing the accessibility tree (only when requested).
    Accessibility,
    /// Serializing the snapshot into the output stream.
    Formatting {
        /// Number of nodes being formatted.
        nodes: usize,
    },
}

impl Phase {
    /// A rough 0..=1.0 progress estimate for the phase.
    pub fn progress(&self) -> f64 {
        match self {
            Self::Navigating => 0.2,
            Self::Interacting => 0.35,
            Self::Waiting => 0.4,
            Self::Extracting => 0.7,
            Self::Accessibility => 0.8,
            Self::Formatting { .. } => 0.9,
        }
    }
}

/// De-animation pass executed before waits when `config.stabilize` is set:
/// emulates `prefers-reduced-motion: reduce`, cancels running animations
/// and injects an override that disables future animations/transitions,
/// so the captured state is deterministic across runs.
const STABILIZE_JS: &str = r"
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
";

/// A reusable sniffer: owns one browser process + CDP connection and
/// serves any number of sequential sniffing runs (each gets a fresh
/// page target, avoiding the cold-start cost of relaunching Chrome).
pub struct Sniffer {
    client: CdpClient,
    process: BrowserProcess,
    /// When set, every freshly created page session ignores TLS/HTTPS
    /// certificate errors (`Security.setIgnoreCertificateErrors`), so
    /// local dev servers behind a self-signed certificate load instead of
    /// the browser interstitial. Also applied at launch time via
    /// [`LaunchOptions::ignore_certificate_errors`].
    ignore_certificate_errors: AtomicBool,
}

impl std::fmt::Debug for Sniffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sniffer")
            .field("endpoint", &self.process.ws_endpoint)
            .finish_non_exhaustive()
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
            process,
            ignore_certificate_errors: AtomicBool::new(opts.ignore_certificate_errors),
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
            process,
            ignore_certificate_errors: AtomicBool::new(false),
        })
    }

    /// Make every subsequent page session ignore TLS/HTTPS certificate
    /// errors. Useful with [`Sniffer::connect`], where no launch flags can
    /// be injected into the external browser; the setting is applied over
    /// CDP (the browser-level `Security.setIgnoreCertificateErrors` before
    /// each navigation).
    pub fn set_ignore_certificate_errors(&self, ignore: bool) {
        self.ignore_certificate_errors
            .store(ignore, Ordering::Relaxed);
    }

    /// Open a fresh page target on the shared browser connection. The
    /// caller is responsible for closing the returned session.
    pub async fn new_session(&self) -> SniffResult<CdpSession> {
        let session = CdpSession::new_page(&self.client, "about:blank")
            .await
            .map_err(|e| SniffError::Cdp(e.to_string()))?;
        if self.ignore_certificate_errors.load(Ordering::Relaxed) {
            session
                .call(
                    "Security.setIgnoreCertificateErrors",
                    serde_json::json!({ "ignore": true }),
                )
                .await
                .map_err(|e| SniffError::Cdp(e.to_string()))?;
        }
        Ok(session)
    }

    /// Run a full sniffing pipeline and return the outcome.
    pub async fn sniff(&self, config: &SniffConfig) -> SniffResult<SniffOutcome> {
        let session = self.new_session().await?;
        let result = sniff_session(&session, config).await;
        drop(session.close().await);
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
    Fut: Future<Output = ()>,
{
    let coarse_pointer = ["pointer", "any-pointer"].iter().any(|key| {
        config
            .emulate
            .get(*key)
            .is_some_and(|value| value == "coarse")
    });
    if let Some(vp) = config.viewport {
        session
            .set_viewport_with_mobile(vp.width, vp.height, coarse_pointer)
            .await
            .map_err(|e| SniffError::Cdp(e.to_string()))?;
    }
    // Session identity: extra headers (e.g. X-CMS-AI-Token) and a persisted
    // session state (cookies + localStorage) must land *before* navigation so
    // the page and its scripts see them from the first request.
    session
        .set_extra_headers(&config.headers)
        .await
        .map_err(|e| SniffError::Cdp(e.to_string()))?;
    if let Some(path) = &config.storage_state_path {
        apply_storage_state(session, path).await?;
    }
    // Explicit `--emulate` features win over the stabilization default;
    // applying them before navigation makes page scripts observe them.
    let mut features: Vec<serde_json::Value> = Vec::new();
    if config.stabilize {
        features.push(serde_json::json!({
            "name": "prefers-reduced-motion", "value": "reduce"
        }));
    }
    for (name, value) in &config.emulate {
        if name == "pointer" || name == "any-pointer" {
            continue;
        }
        let entry = serde_json::json!({"name": name, "value": value});
        if let Some(slot) = features
            .iter_mut()
            .find(|item| item.get("name") == Some(&serde_json::Value::String(name.clone())))
        {
            *slot = entry;
        } else {
            features.push(entry);
        }
    }
    if !features.is_empty() {
        session
            .call(
                "Emulation.setEmulatedMedia",
                serde_json::json!({"features": features}),
            )
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
            .evaluate(STABILIZE_JS, false)
            .await
            .map_err(|e| SniffError::Cdp(e.to_string()))?;
        // Let the layout settle after animations are cancelled.
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    on_progress(Phase::Waiting).await;
    let mut action_reports: Vec<serde_json::Value> = Vec::new();
    if !config.actions.is_empty() {
        // Reveal elements that only exist after an interaction: each
        // action waits for its own target and performs the interaction;
        // the wait pipeline then targets the post-interaction DOM (e.g. a
        // `.modal` the click just opened).
        on_progress(Phase::Interacting).await;
        for (index, act) in config.actions.iter().enumerate() {
            let target = action::prepare(session, act)
                .await
                .map_err(|e| chain_error(index, act, &config.actions, &e))?;
            let before = if config.effects {
                Some(effects::capture(session, config.stable_key.as_deref()).await?)
            } else {
                None
            };
            action::perform(session, act, &target)
                .await
                .map_err(|e| chain_error(index, act, &config.actions, &e))?;
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
    }
    waiter::wait_for_page(session, config).await?;
    on_progress(Phase::Extracting).await;
    let mut outcome = extractor::extract(session, config).await?;
    if config.effects {
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
        let output = match (config.output.include_ax, config.ax_tree) {
            (true, true) => ax::AxOutput::Both,
            (true, false) => ax::AxOutput::Facets,
            (false, true) => ax::AxOutput::Tree,
            (false, false) => ax::AxOutput::None,
        };
        let capture = ax::capture(session, &outcome.snapshots, output).await?;
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

    // Persist the session state at the very end — after any login performed
    // through `actions` — so the next capture can pass `storage_state_path`.
    if let Some(path) = &config.save_storage_state {
        save_storage_state(session, path).await?;
    }

    let nodes: usize = outcome
        .snapshots
        .iter()
        .map(ElementSnapshot::node_count)
        .sum();
    on_progress(Phase::Formatting { nodes }).await;
    Ok(outcome)
}

/// Captures one state per viewport/emulation profile and folds them into a
/// single schema-2 outcome with `__meta.matrix` tags. Node ids are rebased so
/// consumers can slice states by contiguous chunks without collisions.
pub async fn sniff_matrix_with_progress<F, Fut>(
    sniffer: &Sniffer,
    config: &SniffConfig,
    mut on_progress: F,
) -> SniffResult<SniffOutcome>
where
    F: FnMut(Phase) -> Fut,
    Fut: Future<Output = ()>,
{
    let viewports: Vec<_> = config
        .viewport
        .into_iter()
        .chain(config.matrix_viewports.iter().copied())
        .collect();
    let profiles = if config.matrix_emulations.is_empty() {
        vec![config.emulate.clone()]
    } else {
        config.matrix_emulations.clone()
    };
    let total = viewports.len().saturating_mul(profiles.len());
    if total <= 1 {
        let session = sniffer.new_session().await?;
        let result = sniff_session_with_progress(&session, config, on_progress).await;
        drop(session.close().await);
        return result;
    }

    let mut combined = SniffOutcome::default();
    let mut offset = 0_u64;
    let mut index = 0_usize;
    for viewport in &viewports {
        for profile in &profiles {
            let mut state = config.clone();
            state.viewport = Some(*viewport);
            state.emulate = profile.clone();
            let session = sniffer.new_session().await?;
            let result = sniff_session_with_progress(&session, &state, &mut on_progress).await;
            drop(session.close().await);
            let mut outcome = result?;
            let node_id_start = offset.saturating_add(1);
            let last = rebase_ids(&mut outcome.snapshots, offset);
            // Empty states must not rewind the id counter; non-empty ones end
            // exactly at `last` so the next state starts at `last + 1`.
            if last >= node_id_start {
                offset = last;
            }
            combined.matrix.push(MatrixTag {
                index,
                total,
                viewport: Some(*viewport),
                emulation: profile.clone(),
                node_id_start: Some(node_id_start),
                node_id_end: Some(last),
            });
            if index == 0 {
                combined.document_facet = outcome.document_facet.take();
                combined.env_facet = outcome.env_facet.take();
                combined.declared_facet = outcome.declared_facet.take();
                combined.global_css_variables = outcome.global_css_variables.take();
                combined.shadow_roots = outcome.shadow_roots;
                combined.actions = outcome.actions.take();
                combined.screenshot = outcome.screenshot.take();
                combined.ax_tree = outcome.ax_tree.take();
            }
            combined.snapshots.append(&mut outcome.snapshots);
            index = index.saturating_add(1);
        }
    }
    Ok(combined)
}

/// Adds `offset` to every `id`/`parent_id` in the forest, returning the largest
/// resulting id so the next state starts above it.
fn rebase_ids(nodes: &mut [ElementSnapshot], offset: u64) -> u64 {
    let mut largest = 0_u64;
    for node in nodes {
        node.id = node.id.saturating_add(offset);
        largest = largest.max(node.id);
        if let Some(parent) = node.parent_id {
            node.parent_id = Some(parent.saturating_add(offset));
        }
        largest = largest.max(rebase_ids(&mut node.children, offset));
    }
    largest
}

/// Restore a persisted session state into the browser before navigation:
/// cookies via `Network.setCookies` and `localStorage` via an init script
/// that runs before the page's own scripts on every document.
async fn apply_storage_state(session: &CdpSession, path: &str) -> SniffResult<()> {
    let state = StorageState::from_file(path)?;
    if !state.cookies.is_empty() {
        session
            .set_cookies(&state.to_cdp_cookies())
            .await
            .map_err(|e| SniffError::Cdp(e.to_string()))?;
    }
    if let Some(script) = state.to_init_script() {
        session
            .add_init_script(&script)
            .await
            .map_err(|e| SniffError::Cdp(e.to_string()))?;
    }
    Ok(())
}

/// Export the current session state (all cookies + the page's `localStorage`)
/// to `path`. `localStorage` is only captured from the current page's origin;
/// sub-origins (e.g. a CDN) are not represented.
async fn save_storage_state(session: &CdpSession, path: &str) -> SniffResult<()> {
    let raw_cookies = session
        .get_all_cookies()
        .await
        .map_err(|e| SniffError::Cdp(e.to_string()))?;
    // `localStorage` throws on opaque origins (about:blank); capture is best
    // effort — cookies alone still persist the login.
    let page_state = session
        .evaluate(
            r"(() => { try { return { origin: location.origin,
                items: Object.entries(localStorage).map(([name, value]) => ({ name, value })) }; }
              catch (e) { return null; } })()",
            false,
        )
        .await
        .ok()
        .filter(|v| !v.is_null());
    let state = StorageState::from_cdp(&raw_cookies, page_state);
    state.write_file(path)
}

/// Enrich an action failure with chain context so a broken sequence names
/// the exact step and its predecessors (e.g. "the mini-modal trigger never
/// appeared after the modal opened").
fn chain_error(index: usize, failed: &Action, chain: &[Action], cause: &SniffError) -> SniffError {
    let describe = |a: &Action| match a {
        Action::Click { selector, .. } => format!("click:{selector}"),
        Action::Hover { selector, .. } => format!("hover:{selector}"),
        Action::Type { selector, text, .. } => format!("type:{selector}:{text}"),
        Action::Upload {
            selector, files, ..
        } => format!("upload:{selector}:{}", files.join(",")),
        Action::Key { key, .. } => format!("key:{key}"),
    };
    let prior = chain
        .get(..index)
        .unwrap_or_default()
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
