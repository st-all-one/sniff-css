#![no_main]
//! Fuzz do parser JSONL de snapshots (`sniff-css-diff`): nenhuma entrada UTF-8
//! pode causar panic (o parser é `#![forbid(unsafe_code)]` e devolve `Result`).

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = sniff_css_diff::load_str(text);
    }
});
