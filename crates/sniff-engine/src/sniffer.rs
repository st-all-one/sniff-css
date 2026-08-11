//! Orchestration: browser lifecycle, page sessions and the sniffing flow.

use crate::extractor::{self, SniffOutcome};
use crate::waiter;
use sniff_cdp::browser::BrowserProcess;
use sniff_cdp::client::CdpClient;
use sniff_cdp::protocol::LaunchOptions;
use sniff_cdp::session::CdpSession;
use sniff_core::{SniffConfig, SniffError, SniffResult};

/// A coarse phase of the sniffing pipeline, used to report progress to
/// long-running consumers (e.g. an MCP server) without coupling the engine
/// to any transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Navigating,
    Waiting,
    Extracting,
    Formatting { nodes: usize },
}

impl Phase {
    /// A rough 0..=1.0 progress estimate for the phase.
    pub fn progress(&self) -> f64 {
        match self {
            Phase::Navigating => 0.2,
            Phase::Waiting => 0.4,
            Phase::Extracting => 0.7,
            Phase::Formatting { .. } => 0.9,
        }
    }
}

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
    on_progress(Phase::Waiting).await;
    waiter::wait_for_page(session, config).await?;
    on_progress(Phase::Extracting).await;
    let outcome = extractor::extract(session, config).await?;
    if outcome.snapshots.is_empty() {
        return Err(SniffError::NoMatch {
            selector: config.selector.clone(),
        });
    }
    let nodes: usize = outcome.snapshots.iter().map(|s| s.node_count()).sum();
    on_progress(Phase::Formatting { nodes }).await;
    Ok(outcome)
}
