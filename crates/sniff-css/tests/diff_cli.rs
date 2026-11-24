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

/// Runs `sniff diff` capturing the exit code (needed for invalid-input tests).
fn run_cli_status(args: &[&str], stdin: Option<&str>) -> (i32, String, String) {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let bin = env!("CARGO_BIN_EXE_sniff");
    let mut child = Command::new(bin)
        .arg("diff")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sniff diff");
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(text.as_bytes())
            .expect("write stdin");
    }
    let out = child.wait_with_output().expect("wait sniff diff");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn write_snapshot(name: &str, content: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("sniff-diff-cli-summary-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

const SUMMARY_DIGEST: &str = concat!(
    r##"{"__meta":{"style_defaults":{"visual":{"color":"#212529"}},"viewport":{"width":1366,"height":768}}}"##,
    "\n",
    r##"{"id":1,"tag":"DIV","selector":"div.card","depth":0,"visible":true,"grade":"AAA","css":{"width":"300px","color":"#212529"}}"##,
    "\n",
);

const FULL_V1_SNAPSHOT: &str = concat!(
    r#"{"__meta":{"viewport":{"width":1366,"height":768}}}"#,
    "\n",
    r#"{"id":1,"tag":"DIV","selector":"div","depth":0,"styles":{"box_model":{"width":"44px"}},"children":[]}"#,
    "\n",
);

const FULL_V2_SNAPSHOT: &str = concat!(
    r#"{"__meta":{"schema":2,"viewport":{"width":1366,"height":768}}}"#,
    "\n",
    r#"{"id":1,"tag":"DIV","selector":"div","depth":0,"styles":{"box_model":{"width":"40px"}},"children":[]}"#,
    "\n",
);

fn assert_summary_rejected(code: i32, stderr: &str, stdout: &str) {
    assert_eq!(code, 2, "summary digest must be invalid input: {stderr}");
    let message = format!("{stderr}{stdout}");
    assert!(
        message.contains("summary snapshot is not valid input for check/diff")
            && message.contains("--output jsonl"),
        "actionable message expected: {message}"
    );
}

#[test]
fn cli_diff_rejects_summary_snapshot_as_base() {
    let summary = write_snapshot("summary.jsonl", SUMMARY_DIGEST);
    let (code, stdout, stderr) = run_cli_status(
        &[
            summary.to_str().unwrap(),
            fixture("head.jsonl").to_str().unwrap(),
        ],
        None,
    );
    assert_summary_rejected(code, &stderr, &stdout);
    drop(std::fs::remove_dir_all(summary.parent().unwrap()));
}

#[test]
fn cli_diff_rejects_summary_snapshot_as_head() {
    let summary = write_snapshot("summary.jsonl", SUMMARY_DIGEST);
    let (code, stdout, stderr) = run_cli_status(
        &[
            fixture("base.jsonl").to_str().unwrap(),
            summary.to_str().unwrap(),
        ],
        None,
    );
    assert_summary_rejected(code, &stderr, &stdout);
    drop(std::fs::remove_dir_all(summary.parent().unwrap()));
}

#[test]
fn cli_diff_rejects_summary_snapshot_from_stdin() {
    let (code, stdout, stderr) = run_cli_status(
        &["-", fixture("head.jsonl").to_str().unwrap()],
        Some(SUMMARY_DIGEST),
    );
    assert_summary_rejected(code, &stderr, &stdout);
}

#[test]
fn cli_diff_accepts_full_v1_and_v2_snapshots() {
    let v1 = write_snapshot("v1.jsonl", FULL_V1_SNAPSHOT);
    let v2 = write_snapshot("v2.jsonl", FULL_V2_SNAPSHOT);
    let (code, stdout, stderr) =
        run_cli_status(&[v1.to_str().unwrap(), v2.to_str().unwrap()], None);
    assert_eq!(code, 0, "v1 vs v2 diff must work: {stderr}");
    assert!(stdout.contains("CHANGED"), "width drift expected: {stdout}");
    drop(std::fs::remove_dir_all(v1.parent().unwrap()));
    drop(std::fs::remove_dir_all(v2.parent().unwrap()));
}

#[test]
fn cli_diff_accepts_full_snapshot_from_stdin() {
    let (code, stdout, stderr) = run_cli_status(
        &["-", fixture("head.jsonl").to_str().unwrap()],
        Some(FULL_V2_SNAPSHOT),
    );
    assert_eq!(code, 0, "piped full snapshot must work: {stderr}");
    assert!(
        stdout.contains("\"status\""),
        "delta lines expected: {stdout}"
    );
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
