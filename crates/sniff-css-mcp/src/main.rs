//! `sniff-mcp` — Model Context Protocol server for computed-style sniffing.
//!
//! Launches one headless Chrome and serves MCP tools over stdio, so AI
//! agents can capture real computed styles and diff snapshots directly.

use sniff_cdp::protocol::LaunchOptions;
use sniff_css_mcp::{ChromePool, SniffMcpServer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let pool = match sniff_core::env::var("SNIFF_CONNECT") {
        Some(endpoint) if !endpoint.is_empty() => {
            tracing::info!(endpoint = %endpoint, "connecting to existing browser");
            let pool = ChromePool::connect(&endpoint).await?;
            pool.set_ignore_certificate_errors(env_flag("SNIFF_IGNORE_CERTIFICATE_ERRORS"))
                .await;
            pool
        }
        _ => {
            let opts = LaunchOptions {
                headless: true,
                ignore_certificate_errors: env_flag("SNIFF_IGNORE_CERTIFICATE_ERRORS"),
                ..Default::default()
            };
            ChromePool::launch(&opts).await?
        }
    };
    let service = SniffMcpServer::new(pool);

    tracing::info!(
        "sniff-mcp ready: sniff_page, sniff_diff, sniff_check, sniff_snapshots, sniff_categories over stdio"
    );
    let running = Box::pin(rmcp::serve_server(service, rmcp::transport::stdio())).await?;
    running.waiting().await?;
    Ok(())
}

/// Read a boolean-ish environment variable (`1`, `true`, `yes`, `on`);
/// absent/empty/other values are `false`.
fn env_flag(name: &str) -> bool {
    matches!(
        sniff_core::env::var(name)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // A global subscriber may already be installed; that is not fatal.
    if let Err(e) = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init()
    {
        sniff_core::diagnostic::write(format_args!("tracing subscriber not installed: {e}"));
    }
}
