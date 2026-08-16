//! End-to-end regression: the `sniffCSS` binary must actually write a PNG
//! file when `--screenshot` (and `--fullpage-screenshot`) is used on the web
//! backend.
//!
//! This guards the historical bug where the CLI refactor rebuilt the
//! `SniffOutcome` with `screenshot: None`, so the engine's captured bytes
//! never reached the file. It is skipped when no Chrome/Chromium binary is
//! available (mirroring sniff-engine's `require_chrome` pattern).

use std::path::PathBuf;
use std::process::Command;

fn chrome_path() -> Option<String> {
    sniff_cdp::BrowserProcess::available()
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tall.html")
}

fn snapshot_png(path: &PathBuf) {
    let url = format!("file://{}", fixture_path().display());
    let status = Command::new(env!("CARGO_BIN_EXE_sniffCSS"))
        .args(["--chrome", &chrome_path().unwrap()])
        .args(["-u", &url, "-s", ".filler", "--depth", "1"])
        .args(["--no-summary", "--screenshot"])
        .arg(path)
        .status()
        .expect("run sniffCSS");
    assert!(status.success(), "sniffCSS exited with {status}");
}

/// PNG width/height from the IHDR chunk (big-endian u32 at offsets 16/20).
fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "must be a PNG");
    let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    (w, h)
}

#[test]
fn screenshot_flag_writes_a_png_file() {
    let Some(_chrome) = chrome_path() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let out = std::env::temp_dir().join(format!("sniffcss-shot-{}.png", std::process::id()));
    snapshot_png(&out);
    let bytes = std::fs::read(&out).expect("screenshot written");
    let (w, h) = png_size(&bytes);
    // Default viewport is 1366x768; non-fullpage must match the viewport
    // height exactly (the width may differ by the scrollbar width).
    assert_eq!(h, 768, "viewport screenshot height");
    assert!(w >= 1300, "viewport screenshot width: {w}");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn fullpage_screenshot_is_taller_than_viewport() {
    let Some(_chrome) = chrome_path() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let url = format!("file://{}", fixture_path().display());
    let out = std::env::temp_dir().join(format!("sniffcss-full-{}.png", std::process::id()));
    let status = Command::new(env!("CARGO_BIN_EXE_sniffCSS"))
        .args(["--chrome", &chrome_path().unwrap()])
        .args(["-u", &url, "-s", ".filler", "--depth", "1"])
        .args(["--no-summary", "--fullpage-screenshot", "--screenshot"])
        .arg(&out)
        .status()
        .expect("run sniffCSS");
    assert!(status.success(), "sniffCSS exited with {status}");
    let bytes = std::fs::read(&out).expect("screenshot written");
    let (w, h) = png_size(&bytes);
    // The `.filler` is 3000px tall: fullpage captures beyond the 768px viewport.
    assert!(
        h > 768,
        "fullpage height {h} must exceed the 768px viewport"
    );
    assert!(w >= 1300, "fullpage width: {w}");
    let _ = std::fs::remove_file(&out);
}
