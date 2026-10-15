//! CLI-level tests for `sniff init` (AGENTS.md managed block).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "arquivo de teste: `.unwrap()`/`.expect()` é o mecanismo de falha, o panic aponta a linha"
)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

fn temp_dir(label: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("sniff-init-{label}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &PathBuf, args: &[&str]) -> String {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_sniff"))
        .arg("init")
        .arg("--dir")
        .arg(dir)
        .args(args)
        .output()
        .expect("run sniff init");
    assert!(
        out.status.success(),
        "sniff init {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn init_creates_block_and_is_idempotent() {
    let dir = temp_dir("new");
    let first = run(&dir, &[]);
    assert!(first.contains("criado"), "{first}");
    let content = std::fs::read_to_string(dir.join("AGENTS.md")).unwrap();
    assert!(content.contains("<!-- sniff:start -->"));
    assert!(content.contains("--ignore-certificate-errors"));

    let second = run(&dir, &[]);
    assert!(second.contains("inalterado"), "{second}");
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn init_preserves_user_content_and_force_overwrites() {
    let dir = temp_dir("keep");
    let path = dir.join("AGENTS.md");
    std::fs::write(&path, "# Meu projeto\nconteudo do usuario\n").unwrap();

    run(&dir, &[]);
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.starts_with("# Meu projeto\n"));
    assert!(content.contains("conteudo do usuario"));
    assert!(content.contains("<!-- sniff:start -->"));

    run(&dir, &["--force"]);
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.starts_with("<!-- sniff:start -->"));
    assert!(!content.contains("conteudo do usuario"));
    drop(std::fs::remove_dir_all(&dir));
}
