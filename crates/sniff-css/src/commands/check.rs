//! Handler de `sniff check`.

use std::io::Write as _;
use std::path::Path;

use serde_json::json;

use crate::cli::CheckArgs;
use crate::output::{AppError, Output};
use sniff_css_check::rules::{self, summarize, write_checks};
use sniff_css_check::uniformity::check_uniformity;

/// Checks determinísticos (uniformidade + regras derivadas) sobre um snapshot.
pub(crate) fn run(args: &CheckArgs) -> Result<Output, AppError> {
    let mut base = args.clone();
    if base.input.is_none()
        && base.input_legacy.is_none()
        && args.params.is_none()
        && args.batch.is_none()
        && sniff_core::input::stdin_is_piped()
    {
        base.input = Some(std::path::PathBuf::from("-"));
    }
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

/// Roda as regras/uniformidade de um item.
fn compute(args: &CheckArgs) -> Result<Output, AppError> {
    let input = args.input.as_deref().or(args.input_legacy.as_deref());
    let doc = load_doc(input)?;
    let nodes = doc.nodes;

    let run_uniform = args.uniform || !args.rules;
    let run_rules = args.rules || !args.uniform;

    let mut buf: Vec<u8> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut rule_count = 0usize;
    let mut uniformity_instances = 0usize;
    let mut uniformity_outliers = 0usize;

    if run_rules {
        let lines = rules::run_rules(&nodes, doc.viewport);
        rule_count = lines.len();
        let (pass, warn, fail) = summarize(&lines);
        write_checks(&mut buf, &lines).map_err(AppError::from)?;
        notes.push(format!("rules: {pass} pass | {warn} warn | {fail} fail"));
    }

    if run_uniform {
        let report = check_uniformity(&nodes, args.tolerance);
        uniformity_instances = report.instances;
        uniformity_outliers = report.outliers.len();
        if report.instances < 2 {
            notes.push(format!(
                "note: uniformity needs 2+ instances of the same selector (got {})",
                report.instances
            ));
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
                &mut buf,
                &serde_json::json!({
                    "check": "uniformity",
                    "selector": outlier.selector,
                    "status": "fail",
                    "evidence": format!(
                        "deviates from the {}/{} group norm: {evidence}",
                        report.instances, report.instances
                    ),
                }),
            )
            .map_err(|e| AppError::internal(e.to_string()))?;
            writeln!(buf).map_err(|e| AppError::internal(e.to_string()))?;
        }
    }

    let summary = serde_json::json!({
        "__check_summary": {
            "uniformity_instances": uniformity_instances,
            "uniformity_outliers": uniformity_outliers,
            "rules": rule_count,
        }
    });
    serde_json::to_writer(&mut buf, &summary).map_err(|e| AppError::internal(e.to_string()))?;
    writeln!(buf).map_err(|e| AppError::internal(e.to_string()))?;

    let data = serde_json::json!({
        "uniformity_instances": uniformity_instances,
        "uniformity_outliers": uniformity_outliers,
        "rules": rule_count,
    });
    let mut output = Output::new(String::from_utf8_lossy(&buf).into_owned(), data);
    output.warnings = notes;
    Ok(output)
}

/// Lê um snapshot de um arquivo, de stdin (`-`) ou do pipe implícito.
fn load_doc(input: Option<&Path>) -> Result<sniff_css_diff::DiffDocument, AppError> {
    let path = input.map(Path::to_path_buf);
    match path.as_deref() {
        Some(p) if !sniff_core::input::is_stdin(p) => {
            sniff_css_diff::load_file_doc(p).map_err(AppError::from)
        }
        Some(_) => {
            let text = sniff_core::input::read_stdin().map_err(AppError::from)?;
            sniff_css_diff::load_str_doc(&text).map_err(AppError::from)
        }
        None => {
            if !sniff_core::input::stdin_is_piped() {
                return Err(AppError::invalid(
                    "missing <SNAP>: pass a file, `-`, `--params`/`--batch`, or a pipe",
                ));
            }
            let text = sniff_core::input::read_stdin().map_err(AppError::from)?;
            sniff_css_diff::load_str_doc(&text).map_err(AppError::from)
        }
    }
}
