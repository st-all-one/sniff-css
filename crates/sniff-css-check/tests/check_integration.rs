//! Integration tests: run checks over inline snapshot JSONL (no browser).

use sniff_css_check::modern::{ModernOptions, run_modern_checks};
use sniff_css_check::rules::{RuleStatus, run_rules, summarize};
use sniff_css_check::uniformity::check_uniformity;
use sniff_css_diff::{load_str, load_str_doc};

/// Two identical cards + one odd card (different height), with a low-contrast
const SNAPSHOT: &str = r##"{"id":1,"tag":"DIV","selector":"div.card:nth-child(1)","depth":0,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"rect":{"x":0,"y":0,"width":300,"height":120},"aria":{"focusable":false,"has_text":true},"styles":{"box_model":{"width":"300px","height":"120px"},"visual":{"color":"#212529","background-color":"#ffffff","background-image":"none"},"typography":{"font-size":"16px","font-weight":"400"}},"children":[]}
{"id":2,"tag":"DIV","selector":"div.card:nth-child(2)","depth":0,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"rect":{"x":0,"y":120,"width":300,"height":120},"aria":{"focusable":false,"has_text":true},"styles":{"box_model":{"width":"300px","height":"120px"},"visual":{"color":"#212529","background-color":"#ffffff","background-image":"none"},"typography":{"font-size":"16px","font-weight":"400"}},"children":[]}
{"id":3,"tag":"DIV","selector":"div.card:nth-child(3)","depth":0,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"rect":{"x":0,"y":240,"width":300,"height":80},"aria":{"focusable":false,"has_text":true},"styles":{"box_model":{"width":"300px","height":"80px"},"visual":{"color":"#212529","background-color":"#ffffff","background-image":"none"},"typography":{"font-size":"16px","font-weight":"400"}},"children":[]}
"##;

#[test]
fn uniformity_and_rules_over_inline_snapshot() {
    let nodes = load_str(SNAPSHOT).unwrap();
    assert_eq!(nodes.len(), 3);

    let report = check_uniformity(&nodes, 0.5);
    assert_eq!(report.instances, 3);
    assert_eq!(report.outliers.len(), 1);
    let outlier = &report.outliers[0];
    assert_eq!(outlier.selector, "div.card:nth-child(3)");
    assert!(
        outlier
            .deviations
            .iter()
            .any(|d| d.property == "box_model.height" && d.value == "80px")
    );

    // #212529 on #ffffff is ~14.5:1 -> AA passes. No fail lines.
    let lines = run_rules(&nodes, None);
    let aa_fails = lines.iter().filter(|l| l.status == RuleStatus::Fail);
    assert_eq!(aa_fails.count(), 0, "high-contrast snapshot must not fail");
    let (_, _, fail) = summarize(&lines);
    assert_eq!(fail, 0);
}

#[test]
fn low_contrast_snapshot_fails_contrast_aa() {
    let nodes = load_str(SNAPSHOT).unwrap();
    // Override the third card's colors to dark-on-dark.
    let mut third = nodes[2].clone();
    third.styles.as_mut().unwrap().insert(
        "visual".to_string(),
        serde_json::json!({
            "color": "#212529",
            "background-color": "#020842",
            "background-image": "none"
        }),
    );
    let lines = run_rules(&[third], None);
    let aa = lines.iter().find(|l| l.check == "contrast-aa").unwrap();
    assert_eq!(aa.status, RuleStatus::Fail);
    assert!(aa.evidence.text.contains("ratio"));
}

/// Curated checks over a real-shape v2 matrix: off-screen reflow must not be
/// reported as a breakpoint jump, while an in-viewport jump still emits.
const MATRIX_SNAPSHOT: &str = r#"{"__meta":{"schema":2,"viewport":{"width":1366,"height":768},"matrix":[{"index":0,"total":2,"viewport":{"width":1366,"height":768},"node_id_start":1,"node_id_end":2},{"index":1,"total":2,"viewport":{"width":360,"height":800},"node_id_start":3,"node_id_end":4}]}}
{"id":1,"tag":"IMG","selector":"a.logo img","path":"html/body/a/img","rect":{"x":12,"y":198,"width":200,"height":183},"is_user_noticeable":{"display_visible":true}}
{"id":2,"tag":"FOOTER","selector":"footer.deep","path":"html/body/footer","rect":{"x":0,"y":18806,"width":1366,"height":400},"is_user_noticeable":{"display_visible":true}}
{"id":3,"tag":"IMG","selector":"a.logo img","path":"html/body/a/img","rect":{"x":3,"y":45,"width":57,"height":52},"is_user_noticeable":{"display_visible":true}}
{"id":4,"tag":"FOOTER","selector":"footer.deep","path":"html/body/footer","rect":{"x":0,"y":16000,"width":360,"height":400},"is_user_noticeable":{"display_visible":true}}
"#;

#[test]
fn curated_breakpoint_jump_ignores_offscreen_and_proportional_reflow() {
    let document = load_str_doc(MATRIX_SNAPSHOT).expect("matrix snapshot");
    let opts = ModernOptions::default();
    let lines = run_modern_checks(&document, &opts);
    let jumps: Vec<_> = lines
        .iter()
        .filter(|l| l.check == "breakpoint-jump")
        .collect();
    assert!(
        jumps.is_empty(),
        "logo reflow and far footer are not jumps: {jumps:?}"
    );
}
