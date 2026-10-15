//! Criterion benchmarks for Flutter color normalization.

use criterion::{Criterion, criterion_group, criterion_main};
use sniff_flutter::color::{is_color_property, parse_color_value, parse_flutter_color};
use std::hint::black_box;

/// Representative Flutter `Color(...)` diagnostics strings.
const INPUTS: [&str; 4] = [
    "Color(0xff2563eb)",
    "Color.fromARGB(255, 37, 99, 235)",
    "Color.fromRGBO(37, 99, 235, 1.0)",
    "Color(alpha: 1.0000, red: 1.0000, green: 1.0000, blue: 1.0000, colorSpace: ColorSpace.sRGB)",
];

fn bench_color(c: &mut Criterion) {
    c.bench_function("parse_flutter_color", |b| {
        b.iter(|| {
            for input in INPUTS {
                black_box(parse_flutter_color(black_box(input)));
            }
        });
    });

    let properties = serde_json::json!({"red": 37, "green": 99, "blue": 235, "alpha": 255});
    let map = properties.as_object().cloned().unwrap_or_default();
    c.bench_function("parse_color_value", |b| {
        b.iter(|| {
            black_box(parse_color_value(
                Some(black_box(&map)),
                "Color(0xff2563eb)",
            ));
        });
    });

    c.bench_function("is_color_property", |b| {
        b.iter(|| black_box(is_color_property(black_box("background-color"))));
    });
}

criterion_group!(benches, bench_color);
criterion_main!(benches);
