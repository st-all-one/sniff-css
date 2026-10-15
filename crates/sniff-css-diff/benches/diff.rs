//! Criterion benchmarks for the structural diff (`diff_trees`).

use criterion::{Criterion, criterion_group, criterion_main};
use serde_json::json;
use sniff_css_diff::{DiffNode, DiffOptions, diff_trees};
use std::hint::black_box;

/// A synthetic card node with a small computed-style map.
fn node(id: u64, color: &str) -> DiffNode {
    let index = id.saturating_add(1);
    let styles = json!({
        "box_model": {"width": "100px", "height": "40px"},
        "typography": {"font-size": "16px", "font-weight": "400"},
        "visual": {"color": color, "background-color": "#ffffff"},
    });
    DiffNode {
        id,
        parent_id: None,
        selector: format!("div.card:nth-child({index})"),
        tag: Some("DIV".to_string()),
        path: Some(format!("body > main > div.card:nth-child({index})")),
        depth: Some(1),
        rect: Some(json!({"x": 0, "y": id, "width": 100, "height": 40})),
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

/// `count` sibling cards; when `mutate`, every tenth card changes color.
fn fixture(count: u64, mutate: bool) -> Vec<DiffNode> {
    (0..count)
        .map(|i| {
            let color = if mutate && i.is_multiple_of(10) {
                "#ff0000"
            } else {
                "#212529"
            };
            node(i, color)
        })
        .collect()
}

fn bench_diff_trees(c: &mut Criterion) {
    let opts = DiffOptions::default();
    for count in [100u64, 500, 1000] {
        let base = fixture(count, false);
        let head = fixture(count, true);
        c.bench_function(&format!("diff_trees/{count}"), |b| {
            b.iter(|| {
                let (deltas, stats) =
                    diff_trees(black_box(&base), black_box(&head), black_box(&opts));
                black_box((deltas, stats));
            });
        });
    }
}

criterion_group!(benches, bench_diff_trees);
criterion_main!(benches);
