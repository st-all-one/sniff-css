//! Testes da superfície v5 (API): operandos posicionais, stdin/heredoc,
//! `--params`, `--batch` e `--dry-run`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "arquivo de teste: `.unwrap()`/`.expect()` é o mecanismo de falha, o panic aponta a linha"
)]

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const SNAP_A: &str = concat!(
    r#"{"__meta":{"viewport":{"width":1366,"height":768}}}"#,
    "\n",
    r#"{"id":1,"tag":"DIV","selector":"div","rect":{"x":0,"y":0,"width":10,"height":10}}"#,
    "\n",
);
const SNAP_B: &str = concat!(
    r#"{"__meta":{"viewport":{"width":1366,"height":768}}}"#,
    "\n",
    r#"{"id":1,"tag":"DIV","selector":"div","rect":{"x":0,"y":0,"width":20,"height":10}}"#,
    "\n",
);

/// Roda o binário com `args` e um stdin opcional; devolve (código, stdout, stderr).
fn run(args: &[&str], stdin: Option<&str>) -> (i32, String, String) {
    let bin = env!("CARGO_BIN_EXE_sniff");
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn sniff");
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(text.as_bytes())
            .expect("write stdin");
    }
    let out = child.wait_with_output().expect("wait sniff");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn temp_file(name: &str, content: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sniff-api-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

#[test]
fn capture_positional_url_and_legacy_alias_agree() {
    // Operando posicional.
    let (code, stdout, _) = run(&["capture", "http://x", "-s", ".btn", "--dry-run"], None);
    assert_eq!(code, 0, "positional URL must parse");
    assert!(stdout.contains(r#""ok":true"#), "{stdout}");

    // Alias legado `-u` continua aceito.
    let (code, stdout, _) = run(
        &["capture", "-u", "http://x", "-s", ".btn", "--dry-run"],
        None,
    );
    assert_eq!(code, 0, "legacy -u must still work");
    assert!(stdout.contains(r#""ok":true"#), "{stdout}");
}

#[test]
fn capture_params_inline_and_stdin() {
    let (code, stdout, _) = run(
        &[
            "capture",
            "--params",
            r#"{"url":"http://x","selector":".btn"}"#,
            "--dry-run",
        ],
        None,
    );
    assert_eq!(code, 0, "inline JSON params");
    assert!(stdout.contains(r#""ok":true"#), "{stdout}");

    let (code, stdout, _) = run(
        &["capture", "--params", "-", "--dry-run"],
        Some(r#"{"url":"http://x","selector":".btn"}"#),
    );
    assert_eq!(code, 0, "stdin params");
    assert!(stdout.contains(r#""ok":true"#), "{stdout}");

    // Sem operando e com pipe: o stdin é tratado como objeto de params.
    let (code, stdout, _) = run(
        &["capture", "--dry-run"],
        Some(r#"{"url":"http://x","selector":".btn"}"#),
    );
    assert_eq!(code, 0, "implicit stdin-as-params");
    assert!(stdout.contains(r#""ok":true"#), "{stdout}");
}

#[test]
fn capture_batch_best_effort_and_exit_code() {
    let batch = concat!(
        r#"{"url":"http://x","selector":".a"}"#,
        "\n",
        "not-json",
        "\n",
        r#"{"url":"http://x"}"#,
        "\n",
        r#"{"url":"http://y","selector":".b"}"#,
        "\n",
    );
    let (code, stdout, stderr) = run(&["capture", "--batch", "-", "--dry-run"], Some(batch));
    // JSON quebrado ou config faltando → exit 2, mas o lote é best-effort e
    // emite um status por item.
    assert_eq!(code, 2, "invalid items must fail the batch");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 4, "one status line per item: {stdout}");
    assert!(lines[0].contains(r#""ok":true"#));
    assert!(lines[1].contains(r#""ok":false"#));
    assert!(lines[2].contains(r#""ok":false"#));
    assert!(lines[3].contains(r#""ok":true"#));
    assert!(
        stderr.contains("item 2"),
        "warning names the bad item: {stderr}"
    );
}

#[test]
fn capture_batch_exceeds_max() {
    let batch: String = (0..101)
        .map(|_| r#"{"url":"http://x","selector":".a"}"#)
        .collect::<Vec<_>>()
        .join("\n");
    let (code, _, stderr) = run(&["capture", "--batch", "-", "--dry-run"], Some(&batch));
    assert_eq!(code, 2, "batch over sniff.batch_max must fail");
    assert!(stderr.contains("sniff.batch_max=100"), "{stderr}");
}

#[test]
fn capture_rejects_duplicate_sources() {
    // Posicional + `--params` → conflito (exit 2).
    let (code, _, _) = run(
        &["capture", "http://x", "--params", r#"{"selector":".a"}"#],
        None,
    );
    assert_eq!(code, 2, "positional + --params must conflict");

    // `--params` + `--batch` → conflito (exit 2).
    let (code, _, _) = run(
        &[
            "capture",
            "--params",
            r#"{"url":"http://x"}"#,
            "--batch",
            "-",
        ],
        Some(r#"{"url":"http://x"}"#),
    );
    assert_eq!(code, 2, "--params and --batch must conflict");
}

#[test]
fn check_operand_stdin_and_params_share_result() {
    let path = temp_file("a.jsonl", SNAP_A);

    // Operando posicional.
    let (code, stdout, _) = run(&["check", path.to_str().unwrap()], None);
    assert_eq!(code, 0);
    assert!(stdout.contains("__check_summary"), "{stdout}");

    // `-` explícito + pipe.
    let (code, stdout, _) = run(&["check", "-"], Some(SNAP_A));
    assert_eq!(code, 0);
    assert!(stdout.contains("__check_summary"), "{stdout}");

    // Sem operando + pipe (stdin implícito).
    let (code, stdout, _) = run(&["check"], Some(SNAP_A));
    assert_eq!(code, 0);
    assert!(stdout.contains("__check_summary"), "{stdout}");

    // `--params` produz o mesmo resultado.
    let params = format!(r#"{{"input":"{}","rules":true}}"#, path.display());
    let (code, stdout, _) = run(&["check", "--params", &params], None);
    assert_eq!(code, 0);
    assert!(stdout.contains("__check_summary"), "{stdout}");
}

#[test]
fn check_batch_embeds_report_per_item() {
    let a = temp_file("a.jsonl", SNAP_A);
    let b = temp_file("b.jsonl", SNAP_B);
    let batch = format!(
        "{}\n{}\n",
        format_args!(r#"{{"input":"{}"}}"#, a.display()),
        format_args!(r#"{{"input":"{}","rules":true}}"#, b.display()),
    );
    let (code, stdout, _) = run(&["check", "--batch", "-"], Some(&batch));
    assert_eq!(code, 0, "{stdout}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "one line per item");
    assert!(lines.iter().all(|l| l.contains(r#""ok":true"#)));
    assert!(lines[0].contains("__check_summary"), "report embedded");
}

#[test]
fn diff_operands_and_stdin_head() {
    let a = temp_file("a.jsonl", SNAP_A);
    let b = temp_file("b.jsonl", SNAP_B);

    let (code, stdout, _) = run(&["diff", a.to_str().unwrap(), b.to_str().unwrap()], None);
    assert_eq!(code, 0);
    assert!(stdout.contains("CHANGED"), "{stdout}");

    // HEAD via stdin (`-`), apenas um lado pode ser stdin.
    let (code, stdout, _) = run(&["diff", a.to_str().unwrap(), "-"], Some(SNAP_B));
    assert_eq!(code, 0);
    assert!(stdout.contains("CHANGED"), "{stdout}");

    // Ambos stdin → erro.
    let (code, _, stderr) = run(&["diff", "-", "-"], Some(SNAP_B));
    assert_eq!(code, 2, "both sides stdin must fail");
    assert!(stderr.contains("apenas um"), "{stderr}");
}

#[test]
fn init_positional_dir_and_legacy_dir() {
    let base = std::env::temp_dir().join(format!("sniff-api-init-{}", std::process::id()));
    let positional = base.join("via-positional");
    let legacy = base.join("via-legacy");

    let (code, stdout, _) = run(&["init", positional.to_str().unwrap()], None);
    assert_eq!(code, 0, "{stdout}");
    assert!(positional.join("AGENTS.md").is_file());

    let (code, _, _) = run(&["init", "--dir", legacy.to_str().unwrap()], None);
    assert_eq!(code, 0, "legacy --dir must still work");
    assert!(legacy.join("AGENTS.md").is_file());

    drop(std::fs::remove_dir_all(&base));
}

#[test]
fn params_and_batch_share_schema() {
    // O mesmo objeto funciona como `--params` e como linha de `--batch`.
    let params = r#"{"url":"http://x","selector":".btn","output":"jsonl"}"#;
    let (code_a, stdout_a, _) = run(&["capture", "--params", params, "--dry-run"], None);
    let (code_b, stdout_b, _) = run(
        &["capture", "--batch", "-", "--dry-run"],
        Some(&format!("{params}\n")),
    );
    assert_eq!((code_a, code_b), (0, 0));
    assert_eq!(stdout_a, stdout_b, "params == batch of one");
}
