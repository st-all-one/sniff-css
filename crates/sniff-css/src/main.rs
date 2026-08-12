//! `sniffCSS` binary.

mod args;

use anyhow::Context;
use args::Cli;
use clap::Parser;
use sniff_cdp::protocol::LaunchOptions;
use sniff_core::snapshot::{ensure_gitignored, snapshot_file_name, snapshot_root};
use sniff_engine::{Sniffer, write_output};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = Cli::parse();
    let chrome_path = cli.chrome.clone();
    let connect = cli.connect.clone();
    let screenshot_path = cli.screenshot.clone();
    let persist = cli.persist;
    let config = cli.into_config().context("invalid configuration")?;

    let outcome = if let Some(endpoint) = &connect {
        let sniffer = Sniffer::connect(endpoint)
            .await
            .context("connecting to browser")?;
        sniffer
            .sniff(&config)
            .await
            .context("sniffing via existing browser")?
    } else {
        let opts = LaunchOptions {
            executable: chrome_path,
            headless: true,
            ..Default::default()
        };
        let sniffer = Sniffer::launch(&opts).await.context("launching browser")?;
        sniffer.sniff(&config).await.context("sniffing page")?
    };

    // Capture the serialized output once, then stream it to stdout and (when
    // requested) persist it to the shared snapshot tree.
    let mut buf = Vec::new();
    {
        let mut out = BufWriter::new(&mut buf);
        write_output(&mut out, &outcome, &config.output)?;
        out.flush()?;
    }

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    out.write_all(&buf)?;
    out.flush()?;

    if persist {
        let path = persist_snapshot(&config, &buf)?;
        eprintln!("snapshot saved to {}", path.display());
    }

    if let Some(path) = &screenshot_path
        && let Some(bytes) = &outcome.screenshot
    {
        std::fs::write(path, bytes).with_context(|| format!("writing screenshot to {path}"))?;
        eprintln!("screenshot saved to {path}");
    }
    Ok(())
}

/// Persist the serialized output to the same sortable tree the MCP store
/// uses: `sniffCSS/[domain]/[UTC]-[path]-[selector].<ext>` (root from
/// `SNIFF_SNAPSHOT_DIR` or the CWD), with the tree auto-ignored by git.
/// Returns the root-relative path (matching MCP `__sniff` semantics).
fn persist_snapshot(config: &sniff_core::SniffConfig, bytes: &[u8]) -> anyhow::Result<PathBuf> {
    let root = snapshot_root();
    ensure_gitignored(&root)
        .with_context(|| format!("preparing snapshot root {}", root.display()))?;

    let ext = match config.output.format {
        sniff_core::OutputFormat::Json => "json",
        _ => "jsonl",
    };
    let domain = sniff_core::snapshot::domain_of(&config.url);
    let name = snapshot_file_name(&config.url, &config.selector, ext);
    let rel = PathBuf::from(&domain).join(&name);
    let abs = root.join(&rel);

    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(&abs, bytes)
        .with_context(|| format!("writing snapshot to {}", abs.display()))?;
    Ok(rel)
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
