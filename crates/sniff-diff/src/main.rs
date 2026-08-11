//! `sniff-diff` binary: diff two `sniff-computed-style` JSONL snapshots.

use std::io::BufWriter;
use std::path::PathBuf;

use clap::Parser;
use sniff_diff::{DiffOptions, diff_trees, load_file, write_delta};

#[derive(Debug, Parser)]
#[command(
    name = "sniff-diff",
    version,
    about = "Deterministic tree diff over sniff-computed-style JSONL snapshots",
    long_about = None
)]
struct Cli {
    /// Base snapshot file (JSONL output of sniff-computed-style).
    base: PathBuf,
    /// Head snapshot file (JSONL output of sniff-computed-style).
    head: PathBuf,
    /// Ignore value changes smaller than this in the same unit
    /// (e.g. 0.5 absorbs 16px -> 16.2px subpixel jitter). 0 disables.
    #[arg(long, default_value_t = 0.5)]
    tolerance: f64,
    /// Only print summary statistics, not the delta lines.
    #[arg(long, default_value_t = false)]
    stats_only: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let base = load_file(&cli.base)?;
    let head = load_file(&cli.head)?;

    let opts = DiffOptions {
        tolerance: cli.tolerance,
    };
    let (deltas, stats) = diff_trees(&base, &head, &opts);

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    if !cli.stats_only {
        write_delta(&mut out, &deltas)?;
    }
    eprintln!(
        "nodes: {} -> {} | changed: {} | added: {} | removed: {}",
        stats.base_nodes, stats.head_nodes, stats.changed, stats.added, stats.removed
    );
    Ok(())
}
