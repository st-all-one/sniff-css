//! Handler de `sniff check`.

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::Path;

use serde_json::json;

use crate::cli::CheckArgs;
use crate::output::{AppError, Output};
use sniff_css_check::modern::{
    ModernOptions, ModernProfile, compute_dom_stats, run_modern_checks_for,
};
use sniff_css_check::rules::{self, summarize, write_checks};
use sniff_css_check::uniformity::check_uniformity;
use sniff_css_diff::{DiffDocument, DiffNode, baseline_document};

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

/// Constrói os filtros da família `modern` a partir das flags da CLI.
fn modern_options(args: &CheckArgs) -> ModernOptions {
    let profile = match args.profile.as_deref() {
        Some("strict") => ModernProfile::Strict,
        _ => ModernProfile::Default,
    };
    ModernOptions {
        profile,
        ignore: args.ignore.iter().cloned().collect::<BTreeSet<_>>(),
        guide: args.guide.clone(),
        responsive: args.responsive && !args.no_responsive,
    }
}

/// Flatten pré-order reaproveitado pelas estatísticas de DOM.
fn flatten_nodes<'a>(nodes: &'a [DiffNode], out: &mut Vec<&'a DiffNode>) {
    for node in nodes {
        out.push(node);
        flatten_nodes(&node.children, out);
    }
}

/// Roda as regras/uniformidade e/ou as oportunidades de plataforma.
fn compute(args: &CheckArgs) -> Result<Output, AppError> {
    let input = args.input.as_deref().or(args.input_legacy.as_deref());
    let doc = load_doc(input)?;
    let baseline_doc = baseline_document(&doc);
    let nodes = &baseline_doc.nodes;
    let opts = modern_options(args);

    // Contrato v2 (B2): default = tudo; `--rules` = só quebras; `--modern` =
    // só oportunidades; `--no-modern` = perfil pré-v0.6.
    let run_uniform = args.uniform || (!args.rules && !args.modern);
    let run_rules = args.rules || !args.modern;
    let run_modern = args.modern || (!args.rules && !args.no_modern);

    let mut buf: Vec<u8> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut rule_count = 0usize;
    let mut modern_count = 0usize;
    let mut uniformity_instances = 0usize;
    let mut uniformity_outliers = 0usize;

    if run_rules {
        let lines = rules::run_rules(nodes, doc.viewport);
        rule_count = lines.len();
        let (pass, warn, fail) = summarize(&lines);
        write_checks(&mut buf, &lines).map_err(AppError::from)?;
        notes.push(format!("rules: {pass} pass | {warn} warn | {fail} fail"));
    }

    if run_uniform {
        let report = check_uniformity(nodes, args.tolerance);
        uniformity_instances = report.instances;
        uniformity_outliers = report.outliers.len();
        if report.instances < 2 {
            notes.push(format!(
                "note: uniformity needs 2+ instances of the same selector (got {})",
                report.instances
            ));
        }
        for outlier in &report.outliers {
            let detail = outlier
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
            let text = format!(
                "deviates from the {}/{} group norm: {detail}",
                report.instances, report.instances
            );
            let line = serde_json::json!({
                "check": "uniformity",
                "selector": outlier.selector,
                "status": "fail",
                "class": "bug",
                "severity": "fail",
                "evidence": {
                    "text": text,
                    "source": "uniformity"
                },
            });
            serde_json::to_writer(&mut buf, &line)
                .map_err(|e| AppError::internal(e.to_string()))?;
            writeln!(buf).map_err(|e| AppError::internal(e.to_string()))?;
        }
    }
    if run_modern {
        let lines = run_modern_checks_for(&doc, &baseline_doc, &opts);
        modern_count = lines.len();
        let (pass, warn, fail) = summarize(&lines);
        write_checks(&mut buf, &lines).map_err(AppError::from)?;
        notes.push(format!("modern: {pass} pass | {warn} warn | {fail} fail"));
    }

    let mut flat = Vec::new();
    flatten_nodes(nodes, &mut flat);
    let dom_stats = compute_dom_stats(&flat);
    let matrix_incomplete = doc.matrix.iter().any(|tag| tag.total > doc.matrix.len());
    let modern_profile = match opts.profile {
        ModernProfile::Strict => "strict",
        ModernProfile::Default => "default",
    };

    let summary = json!({
        "__check_summary": {
            "uniformity_instances": uniformity_instances,
            "uniformity_outliers": uniformity_outliers,
            "rules": rule_count,
            "modern": modern_count,
            "class": {
                "bug": rule_count.saturating_add(uniformity_outliers),
                "opportunity": modern_count,
            },
            "opportunities_by_class": {
                "opportunity": modern_count,
            },
            "modern_profile": modern_profile,
        }
    });
    let summary_value = summary
        .get("__check_summary")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    serde_json::to_writer(&mut buf, &summary).map_err(|e| AppError::internal(e.to_string()))?;
    writeln!(buf).map_err(|e| AppError::internal(e.to_string()))?;

    let report = String::from_utf8_lossy(&buf).into_owned();
    let checks: Vec<serde_json::Value> = report
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let data = json!({
        "uniformity_instances": uniformity_instances,
        "uniformity_outliers": uniformity_outliers,
        "rules": rule_count,
        "modern": modern_count,
        "dom_stats": dom_stats,
        "checks": checks,
        "__check_summary": summary_value,
    });
    let mut output = Output::new(String::from_utf8_lossy(&buf).into_owned(), data);
    output.warnings = notes;
    if matrix_incomplete {
        output.warnings.push(
            "matrix is incomplete: some states were not captured; comparative checks skipped"
                .to_string(),
        );
    }
    Ok(output)
}

/// Lê um snapshot de um arquivo, de stdin (`-`) ou do pipe implícito.
/// O digest `summary` é entrada inválida (exit 2), não erro de I/O (exit 5).
fn load_doc(input: Option<&Path>) -> Result<DiffDocument, AppError> {
    let path = input.map(Path::to_path_buf);
    let result = match path.as_deref() {
        Some(p) if !sniff_core::input::is_stdin(p) => sniff_css_diff::load_file_doc(p),
        Some(_) => {
            let text = sniff_core::input::read_stdin().map_err(AppError::from)?;
            sniff_css_diff::load_str_doc(&text)
        }
        None => {
            if !sniff_core::input::stdin_is_piped() {
                return Err(AppError::invalid(
                    "missing <SNAP>: pass a file, `-`, `--params`/`--batch`, or a pipe",
                ));
            }
            let text = sniff_core::input::read_stdin().map_err(AppError::from)?;
            sniff_css_diff::load_str_doc(&text)
        }
    };
    result.map_err(|error| match error {
        sniff_css_diff::DiffError::SummaryDigest => AppError::invalid(error.to_string()),
        other => AppError::from(other),
    })
}
