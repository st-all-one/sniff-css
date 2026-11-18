//! CLI-level tests for `sniff check`: the `--input`/`--uniform`/`--rules`/
//! `--tolerance` flags exercised end-to-end on a real snapshot file.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "arquivo de teste: `.unwrap()`/`.expect()` é o mecanismo de falha, o panic aponta a linha"
)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

const SNAPSHOT: &str = r##"{"id":1,"tag":"DIV","selector":"div.card:nth-child(1)","depth":0,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"rect":{"x":0,"y":0,"width":300,"height":120},"aria":{"focusable":false,"has_text":true},"styles":{"box_model":{"width":"300px","height":"120px"},"visual":{"color":"#212529","background-color":"#ffffff","background-image":"none"},"typography":{"font-size":"16px","font-weight":"400"}},"children":[]}
{"id":2,"tag":"DIV","selector":"div.card:nth-child(2)","depth":0,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"rect":{"x":0,"y":120,"width":300,"height":120},"aria":{"focusable":false,"has_text":true},"styles":{"box_model":{"width":"300px","height":"120px"},"visual":{"color":"#212529","background-color":"#ffffff","background-image":"none"},"typography":{"font-size":"16px","font-weight":"400"}},"children":[]}
{"id":3,"tag":"DIV","selector":"div.card:nth-child(3)","depth":0,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"rect":{"x":0,"y":240,"width":300,"height":80},"aria":{"focusable":false,"has_text":true},"styles":{"box_model":{"width":"300px","height":"80px"},"visual":{"color":"#212529","background-color":"#ffffff","background-image":"none"},"typography":{"font-size":"16px","font-weight":"400"}},"children":[]}
"##;

fn write_snapshot(name: &str, content: &str) -> PathBuf {
    // Unique dir per call: both tests write `snap.jsonl` and remove their
    // dir afterwards, so sharing a pid-based path races under the parallel
    // test runner.
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("sniff-check-cli-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

fn run_cli(args: &[&str]) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_sniff");
    let out = std::process::Command::new(bin)
        .arg("check")
        .args(args)
        .output()
        .expect("run sniff check");
    assert!(
        out.status.success(),
        "sniff check {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn cli_rules_and_uniform_default_on_without_flags() {
    let path = write_snapshot("snap.jsonl", SNAPSHOT);
    let (stdout, stderr) = run_cli(&["--input", path.to_str().unwrap()]);

    // Default: both rules and uniformity run (rules pass, outlier detected).
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.iter().any(|l| l.contains("\"check\":\"uniformity\"")),
        "uniformity outlier expected: {stdout}"
    );
    assert!(
        lines.iter().any(|l| l.contains("\"__check_summary\"")),
        "summary line expected: {stdout}"
    );
    assert!(
        stderr.contains("rules: 6 pass"),
        "3 text nodes x (AA + AAA) pass contrast: {stderr}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("\"uniformity_outliers\":1")),
        "outlier counted in summary: {stdout}"
    );
    drop(std::fs::remove_dir_all(path.parent().unwrap()));
}

#[test]
fn cli_rules_only_flag_skips_uniformity() {
    let path = write_snapshot("snap.jsonl", SNAPSHOT);
    let (stdout, stderr) = run_cli(&["--input", path.to_str().unwrap(), "--rules"]);
    // `--rules` alone: rules run, uniformity is gated off (per the CLI logic
    // `run_uniform = uniform || !rules`), so no `"check":"uniformity"` line.
    assert!(
        !stdout
            .lines()
            .any(|l| l.contains("\"check\":\"uniformity\"")),
        "uniformity must be skipped with --rules only: {stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|l| l.contains("\"check\":\"contrast-aa\"")),
        "{stdout}"
    );
    assert!(
        stderr.contains("rules:"),
        "rules summary expected: {stderr}"
    );
    assert_eq!(
        stderr.matches('|').count(),
        2,
        "pass | warn | fail: {stderr}"
    );
    drop(std::fs::remove_dir_all(path.parent().unwrap()));
}

const MODERN_SNAPSHOT: &str = r#"{"id":1,"tag":"DIV","selector":"div.fake-checkbox","depth":0,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"rect":{"x":0,"y":0,"width":24,"height":24},"attrs":{"role":"checkbox"},"aria":{"role":"checkbox","aria_checked":"false","focusable":true},"styles":{"box_model":{"width":"24px","height":"24px"}},"children":[]}
"#;

#[test]
fn cli_v2_default_runs_modern_and_flags_are_exclusive() {
    let path = write_snapshot("modern.jsonl", MODERN_SNAPSHOT);
    let arg = path.to_str().unwrap();

    let (stdout, _) = run_cli(&["--input", arg]);
    assert!(
        stdout.contains("\"check\":\"native-checkbox-reimplemented\""),
        "v2 default must emit modern opportunities: {stdout}"
    );
    assert!(
        stdout.contains("\"modern\":1") || stdout.contains("\"modern\":2"),
        "modern count in summary: {stdout}"
    );

    let (stdout, _) = run_cli(&["--input", arg, "--no-modern"]);
    assert!(
        !stdout.contains("native-checkbox-reimplemented"),
        "--no-modern must silence opportunities: {stdout}"
    );
    assert!(
        stdout.contains("\"opportunity\":0"),
        "opportunity class must be zero: {stdout}"
    );

    let (stdout, _) = run_cli(&["--input", arg, "--rules"]);
    assert!(
        !stdout.contains("native-checkbox-reimplemented"),
        "--rules must silence opportunities: {stdout}"
    );

    let (stdout, _) = run_cli(&["--input", arg, "--modern"]);
    assert!(
        stdout.contains("native-checkbox-reimplemented"),
        "--modern must emit opportunities: {stdout}"
    );
    assert!(
        !stdout.contains("\"check\":\"contrast-aa\""),
        "--modern must silence derived rules: {stdout}"
    );
    drop(std::fs::remove_dir_all(path.parent().unwrap()));
}

#[test]
fn cli_matrix_snapshot_runs_node_rules_only_on_baseline_state() {
    use std::fmt::Write as _;

    let mut second_state = String::new();
    for (id, y) in [(4, 0), (5, 120), (6, 240)] {
        writeln!(
            second_state,
            "{{\"id\":{id},\"tag\":\"DIV\",\"selector\":\"div.card:nth-child({})\",\"depth\":0,\"rect\":{{\"x\":0,\"y\":{y},\"width\":300,\"height\":120}},\"styles\":{{\"typography\":{{\"font-size\":\"16px\"}}}},\"children\":[]}}",
            id - 3
        )
        .unwrap();
    }
    let content = format!(
        "{{\"__meta\":{{\"schema\":2,\"matrix\":[{{\"index\":0,\"total\":2,\"emulation\":{{}},\"node_id_start\":1,\"node_id_end\":3}},{{\"index\":1,\"total\":2,\"emulation\":{{}},\"node_id_start\":4,\"node_id_end\":6}}]}}}}\n{SNAPSHOT}{second_state}"
    );
    let path = write_snapshot("matrix.jsonl", &content);
    let (_, stderr) = run_cli(&["--input", path.to_str().unwrap(), "--rules"]);
    assert!(
        stderr.contains("rules: 6 pass"),
        "3 baseline nodes x 2 contrast rules expected (not 12 across states): {stderr}"
    );
    drop(std::fs::remove_dir_all(path.parent().unwrap()));
}
