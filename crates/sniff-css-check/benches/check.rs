//! Criterion benchmarks for the deterministic checks (`rules`, `uniformity`).

use criterion::{Criterion, criterion_group, criterion_main};
use serde_json::json;
use sniff_css_check::{check_uniformity, run_rules};
use sniff_css_diff::DiffNode;
use std::hint::black_box;

/// A synthetic `span.badge` node with a small computed-style map.
fn badge(id: u64, color: &str) -> DiffNode {
    let index = id.saturating_add(1);
    let styles = json!({
        "box_model": {"width": "24px", "height": "24px"},
        "typography": {"font-size": "14px", "font-weight": "400"},
        "visual": {"color": color, "background-color": "#ffffff"},
    });
    DiffNode {
        id,
        parent_id: None,
        selector: "span.badge".to_string(),
        tag: Some("SPAN".to_string()),
        path: Some(format!("body > span.badge:nth-child({index})")),
        depth: Some(1),
        rect: Some(json!({"x": 0, "y": id.saturating_mul(24), "width": 24, "height": 24})),
        metrics: None,
        noticeable: Some(json!({"display_visible": true, "accessibility_grade": "AAA"})),
        hash: None,
        styles: Some(styles.as_object().cloned().unwrap_or_default()),
        pseudo: None,
        aria: None,
        contrast: None,
        ax: None,
        attributes: None,
        children: Vec::new(),
    }
}

/// `count` siblings of the same selector; the middle one is the outlier.
fn siblings(count: u64) -> Vec<DiffNode> {
    let outlier = count.saturating_div(2);
    (0..count)
        .map(|i| {
            let color = if i == outlier { "#c0ffee" } else { "#212529" };
            badge(i, color)
        })
        .collect()
}

fn bench_checks(c: &mut Criterion) {
    for count in [10u64, 50, 200] {
        let nodes = siblings(count);
        c.bench_function(&format!("uniformity/{count}"), |b| {
            b.iter(|| black_box(check_uniformity(black_box(&nodes), 0.5)));
        });
        c.bench_function(&format!("rules/{count}"), |b| {
            b.iter(|| black_box(run_rules(black_box(&nodes), Some((1366.0, 768.0)))));
        });
    }
}

criterion_group!(benches, bench_checks);
criterion_main!(benches);
