//! `sniffCSS-diff` binary: diff two `sniffCSS` JSONL snapshots.

use std::io::BufWriter;
use std::path::PathBuf;

use clap::Parser;
use sniff_css_diff::{DiffOptions, diff_actions, diff_trees, load_file_doc, write_delta};

#[derive(Debug, Parser)]
#[command(
    name = "sniffCSS-diff",
    version,
    about = "Deterministic tree diff over sniffCSS JSONL snapshots",
    long_about = None
)]
struct Cli {
    /// Base snapshot file (JSONL output of sniffCSS).
    base: PathBuf,
    /// Head snapshot file (JSONL output of sniffCSS).
    head: PathBuf,
    /// Ignore value changes smaller than this in the same unit
    /// (e.g. 0.5 absorbs 16px -> 16.2px subpixel jitter). 0 disables.
    #[arg(long, default_value_t = 0.5)]
    tolerance: f64,
    /// Property names whose changes never mark a node as changed
    /// (volatile/animated props), comma-separated.
    #[arg(long, value_delimiter = ',')]
    ignore_props: Vec<String>,
    /// Suppress ADDED/REMOVED lines (report only CHANGED) — for lists
    /// whose item count varies by design.
    #[arg(long, default_value_t = false)]
    no_structural: bool,
    /// Only print summary statistics, not the delta lines.
    #[arg(long, default_value_t = false)]
    stats_only: bool,

    /// Also compare the `__actions` UI-effect maps (what/where each
    /// interaction revealed) when both snapshots carry them. ON by default;
    /// use --no-actions to diff only the node tree.
    #[arg(long, default_value_t = true)]
    actions: bool,

    /// Disable `__actions` UI-effect comparison.
    #[arg(long = "no-actions", default_value_t = false)]
    no_actions: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let base = load_file_doc(&cli.base)?;
    let head = load_file_doc(&cli.head)?;

    let opts = DiffOptions {
        tolerance: cli.tolerance,
        ignore_props: cli.ignore_props,
        ignore_structural: cli.no_structural,
    };
    let (mut deltas, mut stats) = diff_trees(&base.nodes, &head.nodes, &opts);
    if cli.actions && !cli.no_actions {
        diff_actions(&base.actions, &head.actions, &opts, &mut deltas, &mut stats);
    }

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    if !cli.stats_only {
        write_delta(&mut out, &deltas)?;
    }
    eprintln!(
        "nodes: {} -> {} | changed: {} | added: {} | removed: {} | actions_changed: {}",
        stats.base_nodes,
        stats.head_nodes,
        stats.changed,
        stats.added,
        stats.removed,
        stats.actions_changed
    );
    Ok(())
}
