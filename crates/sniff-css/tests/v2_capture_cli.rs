//! End-to-end proof that the v2 capture flags produce real JSONL facets.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "arquivo de teste: `.unwrap()`/`.expect()` é o mecanismo de falha, o panic aponta a linha"
)]

use sniff_cdp::BrowserProcess;
use std::process::Command;

#[test]
fn v2_capture_emits_facets_attrs_and_matrix_states() {
    if BrowserProcess::available().is_none() {
        eprintln!("skipping: no Chrome binary found");
        return;
    }
    let fixture = format!(
        "{}/../sniff-engine/tests/fixtures/forms.html",
        env!("CARGO_MANIFEST_DIR")
    );
    let url = format!("file://{fixture}");

    let out = Command::new(env!("CARGO_BIN_EXE_sniff"))
        .args([
            "capture",
            &url,
            "-s",
            "form#sucesu-form",
            "--depth",
            "2",
            "--output",
            "jsonl",
            "--attrs-preset",
            "forms",
            "--declared",
            "--viewport-matrix",
            "360x800,768x1024",
            "--emulate",
            "prefers-color-scheme=light",
            "--quiet",
        ])
        .output()
        .expect("run sniff capture");
    assert!(
        out.status.success(),
        "capture failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("utf8 stdout");
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSONL line"))
        .collect();

    let meta = lines
        .iter()
        .find_map(|line| line.get("__meta"))
        .expect("__meta line");
    assert_eq!(meta["schema"].as_u64(), Some(2));
    let matrix = meta["matrix"].as_array().expect("matrix tags");
    assert_eq!(matrix.len(), 3);
    assert_eq!(
        matrix
            .iter()
            .map(|tag| tag["index"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(matrix.iter().all(|tag| tag["total"].as_u64() == Some(3)));

    for key in ["__document", "__env", "__declared"] {
        assert!(
            lines.iter().any(|line| line.get(key).is_some()),
            "{key} line missing: {text}"
        );
    }

    let nodes: Vec<_> = lines
        .iter()
        .filter(|line| line.get("id").is_some())
        .collect();
    assert_eq!(nodes.len() % matrix.len(), 0, "equal forests per state");
    assert!(
        nodes.iter().any(|node| node
            .get("attrs")
            .and_then(|attrs| attrs.get("id"))
            .and_then(|id| id.as_str())
            == Some("sucesu-form")),
        "curated attrs preset must appear on captured nodes: {text}"
    );
}
