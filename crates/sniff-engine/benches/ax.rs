//! Criterion benchmarks for attaching AX facets to extracted snapshots.

use criterion::{Criterion, criterion_group, criterion_main};
use sniff_core::types::{AxInfo, ComputedStyles, ElementSnapshot};
use sniff_engine::ax::{AxFacets, attach_ax};
use std::hint::black_box;

/// A minimal snapshot with the fields `attach_ax` touches.
fn snapshot(id: u64) -> ElementSnapshot {
    let index = id.saturating_add(1);
    ElementSnapshot {
        id,
        parent_id: None,
        tag: "DIV".to_string(),
        selector: format!(".card:nth-child({index})"),
        path: format!("body > .card:nth-child({index})"),
        depth: 1,
        rect: None,
        metrics: None,
        noticeable: None,
        aria: None,
        effective_background: None,
        contrast: None,
        ax: None,
        attributes: None,
        styles: ComputedStyles::default(),
        pseudo: Vec::new(),
        children: Vec::new(),
    }
}

fn bench_attach_ax(c: &mut Criterion) {
    for count in [50u64, 500, 2000] {
        let mut snaps: Vec<ElementSnapshot> = (0..count).map(snapshot).collect();
        let facets: AxFacets = (0..count)
            .map(|id| {
                (
                    id,
                    AxInfo {
                        role: Some("button".to_string()),
                        name: Some(format!("item {id}")),
                        ..AxInfo::default()
                    },
                )
            })
            .collect();
        c.bench_function(&format!("attach_ax/{count}"), |b| {
            b.iter(|| {
                attach_ax(black_box(snaps.as_mut_slice()), black_box(&facets));
            });
        });
    }
}

criterion_group!(benches, bench_attach_ax);
criterion_main!(benches);
