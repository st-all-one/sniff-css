//! Handler de `sniff diff`.

use std::path::Path;

use serde_json::json;

use crate::cli::DiffArgs;
use crate::output::{AppError, Output};

/// Diff determinístico entre dois snapshots JSONL.
pub(crate) fn run(args: &DiffArgs) -> Result<Output, AppError> {
    let base = args.clone();
    let items = crate::input::requests(&base, args.params.as_deref(), args.batch.as_deref())?;

    if args.dry_run {
        return Ok(crate::input::dry_run_report(&items));
    }

    if args.batch.is_some() {
        let mut lines = String::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut failures = 0_usize;
        for (index, item) in items.iter().enumerate() {
            let result = match item {
                Ok(item) => compute(item),
                Err(error) => Err(AppError::invalid(error.message.clone())),
            };
            match result {
                Ok(output) => {
                    let line = json!({
                        "index": index,
                        "ok": true,
                        "data": output.data,
                        "report": output.text,
                    });
                    lines.push_str(&line.to_string());
                    lines.push('\n');
                    warnings.extend(output.warnings);
                }
                Err(error) => {
                    failures = failures.saturating_add(1);
                    warnings.push(format!(
                        "item {}: {}",
                        index.saturating_add(1),
                        error.message
                    ));
                    lines.push_str(
                        &json!({"index": index, "ok": false, "error": error.message}).to_string(),
                    );
                    lines.push('\n');
                }
            }
        }
        let data = json!({"count": items.len(), "failed": failures});
        let mut output = Output::new(lines, data);
        output.warnings = warnings;
        return Ok(output);
    }

    let item = items
        .into_iter()
        .next()
        .ok_or_else(|| AppError::invalid("sem operação"))??;
    compute(&item)
}

/// Diff de um item.
fn compute(args: &DiffArgs) -> Result<Output, AppError> {
    let base_path = args.base.as_deref();
    let head_path = args.head.as_deref();
    if base_path.is_some_and(sniff_core::input::is_stdin)
        && head_path.is_some_and(sniff_core::input::is_stdin)
    {
        return Err(AppError::invalid(
            "apenas um de <BASE>/<HEAD> pode ser `-` (stdin)",
        ));
    }
    let base = load_doc(base_path, "BASE")?;
    let head = load_doc(head_path, "HEAD")?;

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

/// Lê um snapshot de um arquivo, de stdin (`-`) ou do pipe implícito.
fn load_doc(input: Option<&Path>, name: &str) -> Result<sniff_css_diff::DiffDocument, AppError> {
    let path = input.map(Path::to_path_buf);
    match path.as_deref() {
        Some(p) if !sniff_core::input::is_stdin(p) => {
            sniff_css_diff::load_file_doc(p).map_err(AppError::from)
        }
        Some(_) => {
            let text = sniff_core::input::read_stdin().map_err(AppError::from)?;
            sniff_css_diff::load_str_doc(&text).map_err(AppError::from)
        }
        None => Err(AppError::invalid(format!(
            "missing <{name}>: pass a file, `-`, `--params`/`--batch`, or a pipe"
        ))),
    }
}
