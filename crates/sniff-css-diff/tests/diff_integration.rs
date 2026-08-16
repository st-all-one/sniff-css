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

fn run_cli(args: &[&str]) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_sniffCSS-diff");
    let out = std::process::Command::new(bin)
        .args(args)
        .output()
        .expect("run sniffCSS-diff");
    assert!(
        out.status.success(),
        "sniffCSS-diff {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn cli_tolerance_swallows_small_drift() {
    // tolerance 10 absorbs the widget width 44 -> 40 change, leaving only the
    // toggle color change and the added hint (2 CHANGED+ADDED lines total).
    let (stdout, stderr) = run_cli(&[
        "--tolerance",
        "10",
        fixture("base.jsonl").to_str().unwrap(),
        fixture("head.jsonl").to_str().unwrap(),
    ]);
    assert_eq!(stdout.lines().count(), 2);
    assert!(stdout.contains("\"status\":\"CHANGED\""));
    assert!(stderr.contains("changed: 1"));
    assert!(stderr.contains("added: 1"));
}

#[test]
fn cli_ignore_props_hides_volatile_background() {
    // --ignore-props background-color removes the toggle's only change.
    let (stdout, stderr) = run_cli(&[
        "--ignore-props",
        "background-color",
        fixture("base.jsonl").to_str().unwrap(),
        fixture("head.jsonl").to_str().unwrap(),
    ]);
    assert!(
        stdout.contains("\"status\":\"CHANGED\""),
        "width change stays"
    );
    assert!(
        stderr.contains("changed: 1"),
        "toggle change ignored: {stderr}"
    );
}

#[test]
fn cli_no_structural_suppresses_added() {
    let (stdout, stderr) = run_cli(&[
        "--no-structural",
        fixture("base.jsonl").to_str().unwrap(),
        fixture("head.jsonl").to_str().unwrap(),
    ]);
    assert!(!stdout.contains("\"status\":\"ADDED\""), "added suppressed");
    assert!(stdout.contains("\"status\":\"CHANGED\""));
    assert!(stderr.contains("added: 0"));
}

#[test]
fn cli_actions_flag_is_on_by_default_and_no_actions_disables() {
    let base = "{\"id\":1,\"tag\":\"DIV\",\"selector\":\"div\",\"depth\":0,\"styles\":{\"box_model\":{\"width\":\"44px\"}},\"children\":[]}\n{\"__actions\":[{\"index\":0,\"action\":\"click\",\"selector\":\"#open\",\"effect\":\"revealed\",\"appeared\":[{\"tag\":\"TABLE\",\"path\":\"body > table\"}],\"removed\":[],\"changed\":[]}]}\n";
    let head = "{\"id\":1,\"tag\":\"DIV\",\"selector\":\"div\",\"depth\":0,\"styles\":{\"box_model\":{\"width\":\"44px\"}},\"children\":[]}\n{\"__actions\":[{\"index\":0,\"action\":\"click\",\"selector\":\"#open\",\"effect\":\"no_effect\",\"appeared\":[],\"removed\":[],\"changed\":[]}]}\n";
    let dir = std::env::temp_dir().join(format!("sniffcss-diff-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let b = dir.join("base.jsonl");
    let h = dir.join("head.jsonl");
    std::fs::write(&b, base).unwrap();
    std::fs::write(&h, head).unwrap();

    // Default: __actions are diffed -> ACTION_CHANGED present.
    let (stdout, stderr) = run_cli(&[b.to_str().unwrap(), h.to_str().unwrap()]);
    assert!(
        stdout.contains("\"status\":\"ACTION_CHANGED\""),
        "actions compared by default: {stdout}"
    );
    assert!(stderr.contains("actions_changed: 1"));

    // --no-actions: only the node tree is diffed -> no action delta.
    let (stdout, stderr) = run_cli(&["--no-actions", b.to_str().unwrap(), h.to_str().unwrap()]);
    assert!(
        !stdout.contains("ACTION_CHANGED"),
        "actions skipped with --no-actions: {stdout}"
    );
    assert!(stderr.contains("actions_changed: 0"));

    let _ = std::fs::remove_dir_all(&dir);
}
