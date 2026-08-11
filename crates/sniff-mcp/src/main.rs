//! `sniff-mcp` — Model Context Protocol server for computed-style sniffing.
//!
//! Launches one headless Chrome and serves MCP tools over stdio, so AI
//! agents can capture real computed styles and diff snapshots directly.

use sniff_cdp::protocol::LaunchOptions;
use sniff_mcp::{ChromePool, SniffMcpServer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let opts = LaunchOptions {
        headless: true,
        ..Default::default()
    };
    let pool = ChromePool::launch(&opts).await?;
    let service = SniffMcpServer::new(pool);

    tracing::info!(
        "sniff-mcp ready: sniff_page, diff_snapshots, run_checks, list_categories over stdio"
    );
    let running = rmcp::serve_server(service, rmcp::transport::stdio()).await?;
    running.waiting().await?;
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
