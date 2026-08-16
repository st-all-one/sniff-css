//! CLI-level tests for the `sniffCSS-check` binary: the `--input`/`--uniform`/
//! `--rules`/`--tolerance` flags exercised end-to-end on a real snapshot file.

use std::path::PathBuf;

const SNAPSHOT: &str = r##"{"id":1,"tag":"DIV","selector":"div.card:nth-child(1)","depth":0,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"rect":{"x":0,"y":0,"width":300,"height":120},"aria":{"focusable":false,"has_text":true},"styles":{"box_model":{"width":"300px","height":"120px"},"visual":{"color":"#212529","background-color":"#ffffff","background-image":"none"},"typography":{"font-size":"16px","font-weight":"400"}},"children":[]}
{"id":2,"tag":"DIV","selector":"div.card:nth-child(2)","depth":0,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"rect":{"x":0,"y":120,"width":300,"height":120},"aria":{"focusable":false,"has_text":true},"styles":{"box_model":{"width":"300px","height":"120px"},"visual":{"color":"#212529","background-color":"#ffffff","background-image":"none"},"typography":{"font-size":"16px","font-weight":"400"}},"children":[]}
{"id":3,"tag":"DIV","selector":"div.card:nth-child(3)","depth":0,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"rect":{"x":0,"y":240,"width":300,"height":80},"aria":{"focusable":false,"has_text":true},"styles":{"box_model":{"width":"300px","height":"80px"},"visual":{"color":"#212529","background-color":"#ffffff","background-image":"none"},"typography":{"font-size":"16px","font-weight":"400"}},"children":[]}
"##;

fn write_snapshot(name: &str, content: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sniffcss-check-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

fn run_cli(args: &[&str]) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_sniffCSS-check");
    let out = std::process::Command::new(bin)
        .args(args)
        .output()
        .expect("run sniffCSS-check");
    assert!(
        out.status.success(),
        "sniffCSS-check {args:?} failed: {}",
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
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
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
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
