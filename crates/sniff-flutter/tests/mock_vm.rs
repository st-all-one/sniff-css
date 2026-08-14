//! Integration test: full JSON-RPC round-trip against a mock Dart VM Service.
//!
//! Spins up a real WebSocket server that answers `getVM` and the
//! `ext.flutter.inspector.*` service extensions with canned responses shaped
//! like the real Flutter serialization, then drives `FlutterInspector` +
//! `extract` over the wire. This validates the protocol layer end-to-end
//! without needing an emulator or a Flutter app.

mod common;

use common::{capture, default_properties, spawn_mock_vm_service};

#[tokio::test]
async fn freeze_animations_sets_time_dilation() {
    let addr = spawn_mock_vm_service(default_properties()).await;
    let uri = format!("ws://{addr}/token/ws");
    let inspector = sniff_flutter::FlutterInspector::connect(&uri)
        .await
        .expect("connect");
    inspector.freeze_animations().await.expect("freeze ok");
    inspector.set_time_dilation(1.0).await.expect("restore ok");
    inspector.close().await;
}

#[tokio::test]
async fn extractor_round_trips_over_vm_service() {
    let addr = spawn_mock_vm_service(default_properties()).await;
    let uri = format!("ws://{addr}/token/ws");

    let snapshots = capture(&uri, 5).await;

    assert_eq!(snapshots.len(), 1, "single root: MaterialApp");
    let app = &snapshots[0];
    assert_eq!(app.tag, "MaterialApp");
    assert_eq!(app.parent_id, None);
    assert_eq!(app.depth, 0);
    assert_eq!(
        app.node_count(),
        5,
        "MaterialApp + Scaffold + ColoredBox + Center + Text"
    );

    // Find Text via the nested tree.
    fn find<'a>(
        node: &'a sniff_flutter::ElementSnapshot,
        tag: &str,
    ) -> Option<&'a sniff_flutter::ElementSnapshot> {
        if node.tag == tag {
            return Some(node);
        }
        node.children.iter().find_map(|c| find(c, tag))
    }
    let text = find(app, "Text").expect("Text node");
    assert_eq!(text.parent_id, Some(4), "Text parent is Center (id 4)");
    assert_eq!(text.depth, 4);
    assert_eq!(text.styles.get("data"), Some("Olá, sniff"));
    assert_eq!(
        text.styles.get("color"),
        Some("#ffffff"),
        "Flutter Color normalized"
    );

    // The ColoredBox surface color maps to background-color, is inherited by
    // the Text, whose own color is #ffffff → measured WCAG ratio ≈ 5.17.
    let color_box = find(app, "ColoredBox").expect("ColoredBox node");
    assert_eq!(
        color_box.styles.get("background-color"),
        Some("#2563eb"),
        "surface color → background-color"
    );
    let contrast = text.contrast.as_ref().expect("contrast derived");
    assert_eq!(contrast.foreground, "#ffffff");
    assert_eq!(contrast.background, "#2563eb");
    assert!(
        (contrast.ratio - 5.17).abs() < 0.1,
        "expected ~5.17 ratio, got {}",
        contrast.ratio
    );

    assert!(
        text.selector.contains("Text"),
        "identity includes class: {}",
        text.selector
    );

    // Geometry: Text sits at Center's origin + its parentData offset.
    let center = find(app, "Center").expect("Center");
    let center_rect = center.rect.expect("center rect");
    let text_rect = text.rect.expect("text rect");
    assert_eq!(center_rect.width, 300.0);
    assert_eq!(text_rect.x, center_rect.x + 12.0);
    assert_eq!(text_rect.y, center_rect.y + 8.0);
    assert_eq!(text_rect.width, 100.0);
}
