//! End-to-end diff over realistic fixture snapshots.

use std::path::PathBuf;

use sniff_css_diff::{DiffOptions, diff_trees, load_file, write_delta};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn diffs_realistic_fixtures() {
    let base = load_file(&fixture("base.jsonl")).unwrap();
    let head = load_file(&fixture("head.jsonl")).unwrap();
    let (deltas, stats) = diff_trees(&base, &head, &DiffOptions::default());

    assert_eq!(stats.base_nodes, 3);
    assert_eq!(stats.head_nodes, 4);

    // widget width 44 -> 40 (real change, beyond tolerance)
    // toggle background-color change
    // size subpixel 16.1 -> 16.2 (ignored by tolerance)
    // hint span added
    let statuses: Vec<&str> = deltas.iter().map(|d| d.status).collect();
    assert_eq!(statuses, vec!["CHANGED", "CHANGED", "ADDED"]);
    assert_eq!(stats.changed, 2);
    assert_eq!(stats.added, 1);
    assert_eq!(stats.removed, 0);

    assert_eq!(deltas[0].selector, "div.widget[data-testid=\"widget\"]");
    let changes = deltas[0].changes.as_ref().unwrap();
    assert_eq!(changes["styles"]["box_model"]["width"]["before"], "44px");
    assert_eq!(changes["styles"]["box_model"]["width"]["after"], "40px");
    assert_eq!(changes["rect"]["before"]["width"], 44.0);
    assert_eq!(changes["rect"]["after"]["width"], 40.0);

    assert_eq!(deltas[1].selector, "button[data-testid=\"toggle\"]");
    let changes = deltas[1].changes.as_ref().unwrap();
    assert_eq!(
        changes["styles"]["visual"]["background-color"]["after"],
        "#16a34a"
    );

    assert_eq!(deltas[2].status, "ADDED");
    assert_eq!(deltas[2].selector, "span[data-testid=\"hint\"]");
    assert!(deltas[2].snapshot.is_some());
}

#[test]
fn identical_files_produce_no_delta() {
    let a = load_file(&fixture("base.jsonl")).unwrap();
    let (deltas, stats) = diff_trees(&a, &a, &DiffOptions::default());
    assert!(deltas.is_empty());
    assert_eq!(stats.changed, 0);
    assert_eq!(stats.added, 0);
    assert_eq!(stats.removed, 0);
}

#[test]
fn tolerance_swallows_small_structural_drift() {
    let base = load_file(&fixture("base.jsonl")).unwrap();
    let head = load_file(&fixture("head.jsonl")).unwrap();
    // Tolerance 10px absorbs the widget width 44 -> 40 drift entirely,
    // leaving only the toggle color change and the added hint.
    let (deltas, stats) = diff_trees(
        &base,
        &head,
        &DiffOptions {
            tolerance: 10.0,
            ..DiffOptions::default()
        },
    );
    assert_eq!(stats.changed, 1);
    assert_eq!(deltas[0].selector, "button[data-testid=\"toggle\"]");
    assert_eq!(deltas[1].status, "ADDED");
}

#[test]
fn delta_is_valid_json_lines() {
    let base = load_file(&fixture("base.jsonl")).unwrap();
    let head = load_file(&fixture("head.jsonl")).unwrap();
    let (deltas, _) = diff_trees(&base, &head, &DiffOptions::default());
    let mut buf = Vec::new();
    write_delta(&mut buf, &deltas).unwrap();
    let text = String::from_utf8(buf).unwrap();
    for line in text.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(v.get("status").is_some());
        assert!(v.get("selector").is_some());
    }
}

#[test]
fn cli_reports_delta_and_stats() {
    let bin = env!("CARGO_BIN_EXE_sniffCSS-diff");
    let out = std::process::Command::new(bin)
        .arg(fixture("base.jsonl"))
        .arg(fixture("head.jsonl"))
        .output()
        .expect("run sniffCSS-diff");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(stdout.lines().count(), 3);
    assert!(stdout.contains("\"status\":\"CHANGED\""));
    assert!(stderr.contains("changed: 2"));
    assert!(stderr.contains("added: 1"));
}

#[test]
fn cli_stats_only_suppresses_delta() {
    let bin = env!("CARGO_BIN_EXE_sniffCSS-diff");
    let out = std::process::Command::new(bin)
        .arg("--stats-only")
        .arg(fixture("base.jsonl"))
        .arg(fixture("head.jsonl"))
        .output()
        .expect("run sniffCSS-diff");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.is_empty());
}
