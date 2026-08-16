//! Diff parity: two Flutter captures of the same tree must diff through the
//! shared `sniffCSS-diff` machinery with only the real change reported.
//!
//! This is the Flutter analogue of the web regression contract: stable
//! selectors (`ClassName[ordinal]`) let `diff_trees` match nodes across
//! captures, so a single color change produces exactly one `CHANGED` and no
//! `ADDED`/`REMOVED`.

mod common;

use common::{capture, default_properties, spawn_mock_vm_service};
use sniff_core::OutputConfig;
use sniff_core::config::OutputFormat;

fn jsonl_of(roots: Vec<sniff_flutter::ElementSnapshot>) -> String {
    let outcome = sniff_engine::extractor::SniffOutcome {
        snapshots: roots,
        global_css_variables: None,
        ax_tree: None,
        actions: None,
        screenshot: None,
    };
    let config = OutputConfig {
        format: OutputFormat::JsonLines,
        include_rect: true,
        include_path: true,
        include_metrics: true,
        normalize_colors: true,
        group_by_category: true,
        pretty: false,
        compact: true,
        include_visibility: true,
        include_style_hash: true,
        include_aria: true,
        include_contrast: true,
        include_ax: false,
        viewport: None,
    };
    let mut buf = Vec::new();
    sniff_engine::write_output(&mut buf, &outcome, &config).expect("serialize");
    String::from_utf8(buf).expect("utf8")
}

#[tokio::test]
async fn flutter_node_shape_matches_web_golden() {
    // The Flutter backend must emit the SAME per-node JSON shape as the web
    // backend, so sniffCSS-diff / sniffCSS-check are backend-agnostic.
    let addr = spawn_mock_vm_service(default_properties()).await;
    let uri = format!("ws://{addr}/token/ws");
    let roots = capture(&uri, 5).await;
    let flutter_jsonl = jsonl_of(roots);
    let flutter_root: serde_json::Value = flutter_jsonl
        .lines()
        .find_map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).ok()?;
            if v.get("id").is_some() { Some(v) } else { None }
        })
        .expect("a node line");

    // The web golden (fixture.card.jsonl) — the locked regression baseline.
    let golden_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sniff-engine/tests/golden/fixture.card.jsonl");
    let golden_text = std::fs::read_to_string(&golden_path).expect("web golden");
    let web_root: serde_json::Value = golden_text
        .lines()
        .find_map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).ok()?;
            if v.get("id").is_some() { Some(v) } else { None }
        })
        .expect("a web node line");

    // Structural keys a Flutter node emits must exist on the web node too
    // (the shared JSONL contract), and the diff/check-critical fields must be
    // present in both.
    let web_keys: std::collections::BTreeSet<String> =
        web_root.as_object().unwrap().keys().cloned().collect();
    for (k, v) in flutter_root.as_object().unwrap() {
        assert!(
            web_keys.contains(k),
            "Flutter node emits `{k}` but web node does not — shape diverged"
        );
        let _ = v;
    }
    for required in [
        "id", "tag", "selector", "path", "depth", "rect", "styles", "children",
    ] {
        assert!(
            flutter_root.get(required).is_some(),
            "Flutter node missing `{required}`"
        );
        assert!(
            web_root.get(required).is_some(),
            "web node missing `{required}`"
        );
    }
    // Both carry the measured contrast facet (the shared contrast derivation).
    assert!(
        flutter_root.get("contrast").is_some(),
        "Flutter node missing contrast facet"
    );
}

#[tokio::test]
async fn single_color_change_diffs_as_one_changed() {
    // Base capture: text color #ffffff on Scaffold bg #2563eb.
    let addr = spawn_mock_vm_service(default_properties()).await;
    let uri = format!("ws://{addr}/token/ws");
    let base = capture(&uri, 5).await;
    let base_jsonl = jsonl_of(base);

    // Head capture: same tree, text color now #000000.
    let mut props = default_properties();
    props.insert(
        "inspector-4".into(),
        r#"[
          {"name":"data","value":"Olá, sniff","propertyType":"String","description":"Olá, sniff"},
          {"name":"color","propertyType":"Color","description":"Color(alpha: 1.0000, red: 0.0, green: 0.0, blue: 0.0, colorSpace: ColorSpace.sRGB)","valueProperties":{"red":0,"green":0,"blue":0,"alpha":255}},
          {"name":"size","value":24.0,"propertyType":"double","description":"24.0"},
          {"name":"weight","propertyType":"FontWeight","description":"700"}
        ]"#,
    );
    let addr2 = spawn_mock_vm_service(props).await;
    let uri2 = format!("ws://{addr2}/token/ws");
    let head = capture(&uri2, 5).await;
    let head_jsonl = jsonl_of(head);

    let base_doc = sniff_css_diff::load_str(&base_jsonl).expect("base doc");
    let head_doc = sniff_css_diff::load_str(&head_jsonl).expect("head doc");

    let opts = sniff_css_diff::DiffOptions {
        tolerance: 0.5,
        ignore_props: vec![],
        ignore_structural: false,
    };
    let (deltas, stats) = sniff_css_diff::diff_trees(&base_doc, &head_doc, &opts);

    assert_eq!(stats.base_nodes, 5);
    assert_eq!(stats.head_nodes, 5);
    assert_eq!(stats.added, 0, "no nodes added: {deltas:?}");
    assert_eq!(stats.removed, 0, "no nodes removed: {deltas:?}");
    assert_eq!(stats.changed, 1, "exactly one changed node: {deltas:?}");

    let changed = deltas
        .iter()
        .find(|d| d.status == "CHANGED")
        .expect("a CHANGED line");
    let changes = changed
        .changes
        .as_ref()
        .expect("CHANGED carries property diffs");
    let text = changes.to_string();
    assert!(
        text.contains("color"),
        "color change surfaced in diff: {changes:?}"
    );
}
