//! `sniff-check` binary: deterministic UI checks over a snapshot.
//!
//! Offline (no browser): reads a `sniff-computed-style` JSONL snapshot and
//! runs uniformity (odd-card-out among sibling instances) and/or derived
//! rule checks (contrast, target size, focus, hidden focusables, alt).
//!
//! ```text
//! sniff-check --input snap.jsonl --uniform --rules
//! ```

use std::io::BufWriter;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use sniff_check::rules::{self, summarize, write_checks};
use sniff_check::uniformity::check_uniformity;
use sniff_diff::load_file;
use std::io::Write;

#[derive(Debug, Parser)]
#[command(
    name = "sniff-check",
    version,
    about = "Deterministic UI checks over sniff-computed-style JSONL snapshots",
    long_about = None
)]
struct Cli {
    /// Snapshot JSONL file (output of sniff-computed-style).
    #[arg(long, short = 'i')]
    input: PathBuf,

    /// Run the uniformity check (odd card out among sibling instances).
    /// Defaults to on when --rules is not given.
    #[arg(long, default_value_t = false)]
    uniform: bool,

    /// Run the derived rule checks (contrast, target size, focus, alt).
    /// Defaults to on when --uniform is not given.
    #[arg(long, default_value_t = false)]
    rules: bool,

    /// Tolerance for numeric deviations (same unit).
    #[arg(long, default_value_t = 0.5)]
    tolerance: f64,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let nodes = load_file(&cli.input).context("loading snapshot")?;

    let run_uniform = cli.uniform || !cli.rules;
    let run_rules = cli.rules || !cli.uniform;

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let mut rule_count = 0usize;
    let mut uniformity_instances = 0usize;
    let mut uniformity_outliers = 0usize;

    if run_rules {
        let lines = rules::run_rules(&nodes);
        rule_count = lines.len();
        let (pass, warn, fail) = summarize(&lines);
        write_checks(&mut out, &lines)?;
        eprintln!("rules: {pass} pass | {warn} warn | {fail} fail");
    }

    if run_uniform {
        let report = check_uniformity(&nodes, cli.tolerance);
        uniformity_instances = report.instances;
        uniformity_outliers = report.outliers.len();
        if report.instances < 2 {
            eprintln!(
                "note: uniformity needs 2+ instances of the same selector (got {})",
                report.instances
            );
        }
        for outlier in &report.outliers {
            let evidence = outlier
                .deviations
                .iter()
                .map(|d| match (d.norm.as_deref(), d.magnitude) {
                    (Some(norm), Some(mag)) => {
                        format!("{}: {} (norm {norm} ±{mag:0.2})", d.property, d.value)
                    }
                    (Some(norm), None) => format!("{}: {} (norm {norm})", d.property, d.value),
                    (None, _) => format!("{}: {}", d.property, d.value),
                })
                .collect::<Vec<_>>()
                .join("; ");
            serde_json::to_writer(
                &mut out,
                &serde_json::json!({
                    "check": "uniformity",
                    "selector": outlier.selector,
                    "status": "fail",
                    "evidence": format!(
                        "deviates from the {}/{} group norm: {evidence}",
                        report.instances, report.instances
                    ),
                }),
            )?;
            writeln!(out)?;
        }
    }

    let summary = serde_json::json!({
        "__check_summary": {
            "uniformity_instances": uniformity_instances,
            "uniformity_outliers": uniformity_outliers,
            "rules": rule_count,
        }
    });
    serde_json::to_writer(&mut out, &summary)?;
    writeln!(out)?;
    out.flush()?;
    Ok(())
}
