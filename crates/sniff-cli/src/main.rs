//! `sniff-computed-style` binary.

mod args;

use anyhow::Context;
use args::Cli;
use clap::Parser;
use sniff_cdp::protocol::LaunchOptions;
use sniff_engine::{Sniffer, write_output};
use std::io::{BufWriter, Write};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = Cli::parse();
    let chrome_path = cli.chrome.clone();
    let connect = cli.connect.clone();
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

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    write_output(&mut out, &outcome, &config.output)?;
    out.flush()?;
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
