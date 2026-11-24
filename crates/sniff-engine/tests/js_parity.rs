//! Parity guard for the JS sources split out of the Rust modules.
//!
//! The browser evaluates the concatenated modules as one arrow expression;
//! this test compares that composition with the historical inline snapshots
//! kept under `tests/fixtures/js/` so a refactor cannot silently change the
//! JavaScript sent to CDP.

use sniff_engine::js::{CAPTURE_JS, EXTRACT_JS};

#[test]
fn composed_extract_js_matches_fixture() {
    let expected = include_str!("fixtures/js/extract.inline.js");
    assert_eq!(EXTRACT_JS.trim_end(), expected.trim_end());
}

#[test]
fn effects_js_matches_fixture() {
    let expected = include_str!("fixtures/js/effects.inline.js");
    assert_eq!(CAPTURE_JS.trim_end(), expected.trim_end());
}
