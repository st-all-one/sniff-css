//! Handler de `sniff check`.

use std::io::Write as _;

use crate::cli::CheckArgs;
use crate::output::{AppError, Output};
use sniff_css_check::rules::{self, summarize, write_checks};
use sniff_css_check::uniformity::check_uniformity;

/// Checks determinísticos (uniformidade + regras derivadas) sobre um snapshot.
pub(crate) fn run(args: &CheckArgs) -> Result<Output, AppError> {
    let doc = sniff_css_diff::load_file_doc(&args.input)?;
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
