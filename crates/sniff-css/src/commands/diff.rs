//! Handler de `sniff diff`.

use crate::cli::DiffArgs;
use crate::output::{AppError, Output};

/// Diff determinístico entre dois snapshots JSONL.
pub(crate) fn run(args: &DiffArgs) -> Result<Output, AppError> {
    let base = sniff_css_diff::load_file_doc(&args.base)?;
    let head = sniff_css_diff::load_file_doc(&args.head)?;

    let opts = sniff_css_diff::DiffOptions {
        tolerance: args.tolerance,
        ignore_props: args.ignore_props.clone(),
        ignore_structural: args.no_structural,
    };
    let (mut deltas, mut stats) = sniff_css_diff::diff_trees(&base.nodes, &head.nodes, &opts);
    if !args.no_actions {
        sniff_css_diff::diff_actions(&base.actions, &head.actions, &opts, &mut deltas, &mut stats);
    }

    let mut buf: Vec<u8> = Vec::new();
    if !args.stats_only {
        sniff_css_diff::write_delta(&mut buf, &deltas)?;
    }
    let text = String::from_utf8_lossy(&buf).into_owned();

    let summary = format!(
        "nodes: {} -> {} | changed: {} | added: {} | removed: {} | actions_changed: {}",
        stats.base_nodes,
        stats.head_nodes,
        stats.changed,
        stats.added,
        stats.removed,
        stats.actions_changed
    );
    let data = serde_json::json!({
        "base_nodes": stats.base_nodes,
        "head_nodes": stats.head_nodes,
        "changed": stats.changed,
        "added": stats.added,
        "removed": stats.removed,
        "actions_changed": stats.actions_changed,
    });

    let mut output = Output::new(text, data);
    output.warnings.push(summary);
    Ok(output)
}
