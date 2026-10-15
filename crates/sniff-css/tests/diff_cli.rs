//! CLI-level tests for `sniff diff` (end-to-end over realistic fixtures).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "arquivo de teste: `.unwrap()`/`.expect()` é o mecanismo de falha, o panic aponta a linha"
)]
use std::path::PathBuf;

/// Fixtures live in the (now library-only) `sniff-css-diff` crate.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../sniff-css-diff/tests/fixtures")
        .join(name)
}

fn run_cli(args: &[&str]) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_sniff");
    let out = std::process::Command::new(bin)
        .arg("diff")
        .args(args)
        .output()
        .expect("run sniff diff");
    assert!(
        out.status.success(),
        "sniff diff {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn cli_reports_delta_and_stats() {
    let (stdout, stderr) = run_cli(&[
        fixture("base.jsonl").to_str().unwrap(),
        fixture("head.jsonl").to_str().unwrap(),
    ]);
    assert_eq!(stdout.lines().count(), 3);
    assert!(stdout.contains("\"status\":\"CHANGED\""));
    assert!(stderr.contains("changed: 2"));
    assert!(stderr.contains("added: 1"));
}

#[test]
fn cli_stats_only_suppresses_delta() {
    let (stdout, _stderr) = run_cli(&[
        "--stats-only",
        fixture("base.jsonl").to_str().unwrap(),
        fixture("head.jsonl").to_str().unwrap(),
    ]);
    assert!(stdout.is_empty());
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
    let dir = std::env::temp_dir().join(format!("sniff-diff-cli-{}", std::process::id()));
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

    drop(std::fs::remove_dir_all(&dir));
}
