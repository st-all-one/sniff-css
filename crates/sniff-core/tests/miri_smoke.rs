//! Smoke de UB sob Miri: só caminhos puros e pequenos do `sniff-core`.
//!
//! O crate é `#![forbid(unsafe_code)]`; sob Miri a suíte vale como sanidade de
//! alocação/bounds/UTF-8. É a **única** suíte rodada com `cargo miri test`; a
//! cobertura funcional fica no `cargo test`.

use sniff_core::config::{parse_action, parse_categories, parse_wait_strategy};
use sniff_core::contrast::parse_color;
use sniff_core::numeric::{f64_to_u32, f64_to_u64};
use sniff_core::snapshot::snapshot_file_name_at;

#[test]
fn config_parsers_never_panic() {
    for spec in [
        "click:.btn",
        "hover:.a:hover:3000",
        "type:#x:olá mundo",
        "upload:#f:a.txt,b.txt",
        "click:.a:1:2:3",
        "",
    ] {
        let _value = parse_action(spec);
    }
    for spec in [
        "delay:1000",
        "network-idle:800:15000",
        "selector:.card:5000",
        "element-ready:.card:visible,has-size:5000",
        "fonts-loaded",
        "bogus",
    ] {
        let _value = parse_wait_strategy(spec);
    }
    let _value = parse_categories("box-model,layout,typography,visual");
}

#[test]
fn contrast_and_numeric_edge_cases() {
    for hex in ["#fff", "#2563eb", "#2563eb80", "#xyz", "rgb(1,2,3)", ""] {
        let _value = parse_color(hex);
    }
    let _value = f64_to_u32(f64::NAN);
    let _value = f64_to_u32(-1.0);
    let _value = f64_to_u64(1e300);
    let _value = f64_to_u64(f64::INFINITY);
}

#[test]
fn snapshot_name_is_deterministic() {
    let a = snapshot_file_name_at(
        "http://localhost:3000/a/b?q=1",
        "div.card:nth-child(2)",
        "jsonl",
        "20260102T030405Z",
    );
    let b = snapshot_file_name_at(
        "http://localhost:3000/a/b?q=1",
        "div.card:nth-child(2)",
        "jsonl",
        "20260102T030405Z",
    );
    assert_eq!(a, b);
}
