//! Shared Chrome browser pool backing the MCP tools.
//!
//! One headless Chrome process is launched once and reused across tool
//! calls. Concurrency is bounded by a semaphore; a crashed browser is
//! relaunched transparently (best effort) on the next call.

use std::sync::Arc;

use sniff_cdp::protocol::LaunchOptions;
use sniff_core::{SniffConfig, SniffError, SniffResult};
use sniff_engine::extractor::SniffOutcome;
use sniff_engine::{Phase, Sniffer, sniff_session_with_progress};
use tokio::sync::{RwLock, Semaphore};

#[derive(Debug)]
struct Inner {
    sniffer: RwLock<Arc<Sniffer>>,
    semaphore: Arc<Semaphore>,
    launch_opts: LaunchOptions,
}

/// Cloneable handle to a shared browser pool.
#[derive(Debug, Clone)]
pub struct ChromePool {
    inner: Arc<Inner>,
}

impl ChromePool {
    /// Launch the browser (or connect to an existing one) and build the pool.
    pub async fn launch(opts: &LaunchOptions) -> SniffResult<Self> {
        let sniffer = Sniffer::launch(opts).await?;
        Ok(Self {
            inner: Arc::new(Inner {
                sniffer: RwLock::new(Arc::new(sniffer)),
                semaphore: Arc::new(Semaphore::new(DEFAULT_CONCURRENCY)),
                launch_opts: opts.clone(),
            }),
        })
    }

    /// Connect to an already-running browser instead of launching one.
    ///
    /// Used when the MCP server is pointed at a shared Chromium (e.g. the
    /// GUI instance in a container) via the `SNIFF_CONNECT` environment
    /// variable or for programmatic use with remote debugging enabled.
    pub async fn connect(endpoint: &str) -> SniffResult<Self> {
        let sniffer = Sniffer::connect(endpoint).await?;
        Ok(Self {
            inner: Arc::new(Inner {
                sniffer: RwLock::new(Arc::new(sniffer)),
                semaphore: Arc::new(Semaphore::new(DEFAULT_CONCURRENCY)),
                launch_opts: LaunchOptions::default(),
            }),
        })
    }

    /// Run a sniffing pipeline, reporting each [`Phase`] via `on_progress`.
    ///
    /// Bounded by the pool semaphore; concurrent calls run on independent
    /// page targets multiplexed over the single CDP connection.
    pub async fn sniff_with<F, Fut>(
        &self,
        config: &SniffConfig,
        on_progress: F,
    ) -> SniffResult<SniffOutcome>
    where
        F: FnMut(Phase) -> Fut + Clone,
        Fut: std::future::Future<Output = ()>,
    {
        let _permit = self
            .inner
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| SniffError::Other("browser pool closed".into()))?;

        let sniffer = self.inner.sniffer.read().await.clone();
        match self.run(&sniffer, config, on_progress.clone()).await {
            Ok(outcome) => Ok(outcome),
            Err(e) if is_transport_error(&e) => {
                tracing::warn!("browser connection lost ({e}); relaunching");
                self.relaunch().await;
                let sniffer = self.inner.sniffer.read().await.clone();
                self.run(&sniffer, config, on_progress).await
            }
            Err(e) => Err(e),
        }
    }

    async fn run<F, Fut>(
        &self,
        sniffer: &Sniffer,
        config: &SniffConfig,
        on_progress: F,
    ) -> SniffResult<SniffOutcome>
    where
        F: FnMut(Phase) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let session = sniffer.new_session().await?;
        let result = sniff_session_with_progress(&session, config, on_progress).await;
        let _ = session.close().await;
        result
    }

    /// Swap the current browser for a fresh one (best effort).
    async fn relaunch(&self) {
        match Sniffer::launch(&self.inner.launch_opts).await {
            Ok(sniffer) => {
                *self.inner.sniffer.write().await = Arc::new(sniffer);
                tracing::info!("browser relaunched");
            }
            Err(e) => tracing::warn!("browser relaunch failed: {e}"),
        }
    }
}

/// A `Cdp`/`Browser` error that most likely means the browser or its
/// transport died, so a relaunch is worthwhile.
fn is_transport_error(e: &SniffError) -> bool {
    let message = e.to_string().to_ascii_lowercase();
    (matches!(e, SniffError::Cdp(_) | SniffError::Browser(_)))
        && [
            "websocket",
            "connection closed",
            "connection lost",
            "closed",
            "eof",
        ]
        .iter()
        .any(|needle| message.contains(needle))
}

/// Default number of concurrent sniffing calls over the single browser.
const DEFAULT_CONCURRENCY: usize = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_errors_are_detected() {
        assert!(is_transport_error(&SniffError::Cdp(
            "websocket connection closed".into()
        )));
        assert!(is_transport_error(&SniffError::Browser(
            "connection lost during launch".into()
        )));
        // Non-transport errors must not trigger a relaunch.
        assert!(!is_transport_error(&SniffError::NoMatch {
            selector: "x".into()
        }));
        assert!(!is_transport_error(&SniffError::Timeout("page".into())));
        assert!(!is_transport_error(&SniffError::Cdp(
            "invalid params".into()
        )));
    }
}
