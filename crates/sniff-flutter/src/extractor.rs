//! Flutter widget-tree extractor → `sniff-core` snapshot model.
//!
//! Walks the widget summary tree returned by
//! `ext.flutter.inspector.getRootWidgetSummaryTree`, annotating each node with
//! its render-object geometry (`getLayoutExplorerNode`) and diagnostics
//! properties (`getProperties`), and maps the result onto the same
//! `ElementSnapshot` JSONL model the web backend emits — so `sniffCSS-diff`
//! and `sniffCSS-check` work on Flutter snapshots unchanged.
//!
//! Mapping conventions:
//! - `tag` — widget class name (e.g. `Text`, `ElevatedButton`), derived from
//!   the diagnostics `description`;
//! - `selector` / `path` — the widget breadcrumb (`MaterialApp > Scaffold >
//!   Center > Text[0]`);
//! - `styles` — Flutter diagnostics properties bucketed into the CSS-style
//!   categories (layout/typography/visual/box-model);
//! - `rect` — from the render-object `size` plus the accumulated
//!   `parentData` offset (best-effort: widgets without a render box report
//!   `None`).

use crate::inspector::FlutterInspector;
use crate::vm::Result;
use serde_json::Value;
use sniff_core::properties::StyleCategory;
use sniff_core::types::{ComputedProperty, ComputedStyles, ElementSnapshot, Rect};

/// Extract the whole widget tree as a list of root snapshots.
///
/// `depth` limits how many widget levels below the matched root are captured
/// (`0` = root only). The root of the *app* widget tree is always returned as
/// a single root snapshot; selector-scoped matching is applied by the caller
/// (CLI/MCP) against `selector`/`path`.
pub async fn extract(inspector: &FlutterInspector, depth: usize) -> Result<Vec<ElementSnapshot>> {
    let Some(root) = inspector.root_widget_summary_tree().await? else {
        return Ok(Vec::new());
    };
    let mut nodes = Vec::new();
    let mut next_id = 1u64;
    walk(inspector, &root, depth, None, &mut next_id, &mut nodes).await?;
    let roots = nest(nodes);
    // Reuse the web contrast derivation: it reads `color`/`background-color`/
    // `font-size`/`font-weight` from the node styles, which `map_styles`
    // already emits with web-compatible keys and normalized colors, and it
    // walks the nested `children` to inherit backgrounds.
    let mut roots = roots;
    sniff_core::contrast::apply_contrast_all(&mut roots);
    Ok(roots)
}

/// Reconstruct the nested snapshot tree from the flat pre-order walk.
///
/// Pre-order guarantees a node's subtree occupies a contiguous range where
/// children have `depth == parent.depth + 1`, so the nesting is unambiguous.
fn nest(flat: Vec<ElementSnapshot>) -> Vec<ElementSnapshot> {
    fn build(flat: &[ElementSnapshot], start: usize, depth: usize) -> (ElementSnapshot, usize) {
        let src = &flat[start];
        let mut node = src.clone();
        node.children.clear();
        let mut idx = start + 1;
        while idx < flat.len() && flat[idx].depth == depth + 1 {
            let (child, next) = build(flat, idx, depth + 1);
            node.children.push(child);
            idx = next;
        }
        (node, idx)
    }
    let mut roots = Vec::new();
    let mut idx = 0;
    while idx < flat.len() {
        let depth = flat[idx].depth;
        let (root, next) = build(&flat, idx, depth);
        roots.push(root);
        idx = next;
    }
    roots
}
/// Recursively walk one node: fetch geometry + properties, emit the snapshot,
/// descend into children up to `depth`.
///
/// Implemented as an explicit pre-order stack so the async traversal does not
/// recurse (the widget tree can be arbitrarily deep).
async fn walk(
    inspector: &FlutterInspector,
    root: &Value,
    depth: usize,
    root_parent_id: Option<u64>,
    next_id: &mut u64,
    out: &mut Vec<ElementSnapshot>,
) -> Result<()> {
    struct Frame<'a> {
        node: &'a Value,
        depth_left: usize,
        parent_pos: (f64, f64),
        parent_id: Option<u64>,
    }

    let mut stack = vec![Frame {
        node: root,
        depth_left: depth,
        parent_pos: (0.0, 0.0),
        parent_id: root_parent_id,
    }];
    // Stable sibling ordinal per (parent, class): `Text[0]`, `Text[1]`, ...
    // Unlike the global id, this is stable across captures whose unrelated
    // parts of the tree change, which is what `sniffCSS-diff` needs.
    let mut sibling_seen: std::collections::HashMap<(Option<u64>, String), u64> =
        std::collections::HashMap::new();

    while let Some(frame) = stack.pop() {
        let value_id = frame.node.get("valueId").and_then(Value::as_str);
        let geometry = match value_id {
            Some(id) => Some(inspector.layout_explorer_node(id).await?),
            None => None,
        };
        let properties = match value_id {
            Some(id) => inspector.properties(id).await?,
            None => Vec::new(),
        };

        let id = *next_id;
        *next_id += 1;

        let class = widget_class_name(frame.node);
        let description = frame
            .node
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or(&class);
        let ordinal = {
            let key = (frame.parent_id, class.clone());
            let n = sibling_seen.entry(key).or_insert(0);
            let ordinal = *n;
            *n += 1;
            ordinal
        };
        let identity = widget_identity(&class, description, ordinal);

        let (off_x, off_y) = geometry.as_ref().map(geometry_offset).unwrap_or((0.0, 0.0));
        let pos = (frame.parent_pos.0 + off_x, frame.parent_pos.1 + off_y);
        let rect = geometry.as_ref().and_then(|g| map_rect(g, pos));
        let depth_here = match frame.parent_id {
            Some(pid) => parent_depth(out, pid) + 1,
            None => 0,
        };

        out.push(ElementSnapshot {
            id,
            parent_id: frame.parent_id,
            tag: class,
            selector: identity.clone(),
            path: identity,
            depth: depth_here,
            rect,
            metrics: None,
            noticeable: None,
            aria: None,
            effective_background: None,
            contrast: None,
            ax: None,
            attributes: None,
            styles: map_styles(&properties),
            pseudo: Vec::new(),
            children: Vec::new(),
        });

        if frame.depth_left > 0
            && let Some(children) = frame.node.get("children").and_then(Value::as_array)
        {
            for child in children.iter().rev() {
                stack.push(Frame {
                    node: child,
                    depth_left: frame.depth_left - 1,
                    parent_pos: pos,
                    parent_id: Some(id),
                });
            }
        }
    }
    Ok(())
}

/// Depth of a node whose parent is `pid` (the parent's stored depth + 1).
fn parent_depth(out: &[ElementSnapshot], pid: u64) -> usize {
    out.iter()
        .rev()
        .find(|n| n.id == pid)
        .map(|n| n.depth)
        .unwrap_or(0)
}

/// Widget class name from the diagnostics description: the leading
/// identifier, e.g. `Text("hi")` → `Text`, `ColoredBox` → `ColoredBox`.
pub fn widget_class_name(node: &Value) -> String {
    let description = node
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let ident = description.split(['(', ' ']).next().unwrap_or(description);
    let ident = ident.trim();
    if ident.is_empty() {
        node.get("name")
            .and_then(Value::as_str)
            .unwrap_or("Widget")
            .to_string()
    } else {
        ident.to_string()
    }
}

/// Stable identity: `ClassName[ordinal]`, where `ordinal` is the sibling
/// index among same-class siblings under the same parent (`Text[0]`,
/// `Text[1]`), followed by the diagnostics description for readability.
fn widget_identity(class: &str, description: &str, ordinal: u64) -> String {
    let mut identity = format!("{class}[{ordinal}]");
    if !description.is_empty() && description != class {
        identity.push(' ');
        identity.push_str(description);
    }
    identity
}

/// Read the render-object offset from layout-explorer geometry
/// (`parentData.offsetX/offsetY`, as strings).
pub fn geometry_offset(geometry: &Value) -> (f64, f64) {
    let pd = geometry.get("parentData");
    let get = |key: &str| {
        pd.and_then(|p| p.get(key))
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    (get("offsetX"), get("offsetY"))
}

/// Build a `Rect` from geometry `size` plus the accumulated absolute offset.
pub fn map_rect(geometry: &Value, pos: (f64, f64)) -> Option<Rect> {
    let size = geometry.get("size")?;
    let width = size.get("width").and_then(Value::as_str)?.parse().ok()?;
    let height = size.get("height").and_then(Value::as_str)?.parse().ok()?;
    Some(Rect {
        x: pos.0,
        y: pos.1,
        width,
        height,
    })
}

/// Bucket diagnostics properties into CSS-style style groups.
///
/// Properties arrive as `{"name": ..., "value": ..., "propertyType": ...}`
/// objects (value only present for num/String/bool/null). Unknown properties
/// go to a per-node "custom" group so nothing is silently dropped. Colors are
/// normalized to `#rrggbb`/`#rrggbbaa` and a few keys are renamed to their web
/// equivalents (`backgroundColor` → `background-color`, `fontSize` →
/// `font-size`, ...) so `sniff_core::contrast` and `sniffCSS-check` work on
/// Flutter snapshots unchanged.
pub fn map_styles(properties: &[Value]) -> ComputedStyles {
    let mut groups: Vec<(StyleCategory, Vec<ComputedProperty>)> = Vec::new();
    let mut push = |category: StyleCategory, name: String, value: String| {
        if let Some((_, props)) = groups.iter_mut().find(|(c, _)| *c == category) {
            props.push(ComputedProperty { name, value });
        } else {
            groups.push((category, vec![ComputedProperty { name, value }]));
        }
    };

    for prop in properties {
        let Some(name) = prop.get("name").and_then(Value::as_str) else {
            continue;
        };
        let value = prop
            .get("value")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                prop.get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            });
        if value.is_empty() {
            continue;
        }
        let web_key = to_web_key(name);
        let value = if crate::color::is_color_property(name) {
            crate::color::parse_flutter_color(&value).unwrap_or(value)
        } else {
            value
        };
        let category = category_for(web_key);
        push(category, web_key.to_string(), value);
    }
    ComputedStyles { groups }
}

/// Rename Flutter property names to the web-CSS keys the shared contrast and
/// check machinery reads.
fn to_web_key(name: &str) -> &str {
    match name {
        "backgroundColor" => "background-color",
        "foregroundColor" => "color",
        "fontSize" => "font-size",
        "fontWeight" => "font-weight",
        "fontFamily" => "font-family",
        "letterSpacing" => "letter-spacing",
        "wordSpacing" => "word-spacing",
        "textAlign" => "text-align",
        "lineHeight" => "line-height",
        other => other,
    }
}

/// Map a Flutter property name to a CSS-style category.
fn category_for(name: &str) -> StyleCategory {
    match name {
        "width" | "height" | "padding" | "margin" | "size" | "constraints" | "minWidth"
        | "minHeight" | "maxWidth" | "maxHeight" => StyleCategory::BoxModel,
        "font-family" | "font-size" | "font-weight" | "font-style" | "letter-spacing"
        | "word-spacing" | "text-align" | "textDirection" | "textBaseline" | "maxLines"
        | "textScaleFactor" | "line-height" => StyleCategory::Typography,
        "color" | "background-color" | "background" | "opacity" | "shadowColor" | "elevation"
        | "border" | "shape" | "decoration" => StyleCategory::Visual,
        "alignment" | "align" | "mainAxisAlignment" | "crossAxisAlignment" | "mainAxisSize"
        | "direction" | "flex" | "flexFit" | "order" | "fit" | "position" | "aspectRatio"
        | "spacing" => StyleCategory::Layout,
        "semanticLabel"
        | "tooltip"
        | "excludeFromSemantics"
        | "isSemanticBoundary"
        | "enabled"
        | "onPressed"
        | "onTap"
        | "focusNode" => StyleCategory::Accessibility,
        _ => StyleCategory::Custom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn class_name_from_description() {
        assert_eq!(
            widget_class_name(&json!({"description": "Text(\"hi\")"})),
            "Text"
        );
        assert_eq!(
            widget_class_name(&json!({"description": "ColoredBox"})),
            "ColoredBox"
        );
        assert_eq!(
            widget_class_name(&json!({"description": "Padding(padding: EdgeInsets.all(8.0))"})),
            "Padding"
        );
        assert_eq!(widget_class_name(&json!({"description": "Row"})), "Row");
    }

    #[test]
    fn geometry_offset_parses_strings() {
        let geo = json!({
            "parentData": {"offsetX": "10.0", "offsetY": "20.5"},
            "size": {"width": "100.0", "height": "50.0"}
        });
        assert_eq!(geometry_offset(&geo), (10.0, 20.5));
        let geo_empty = json!({});
        assert_eq!(geometry_offset(&geo_empty), (0.0, 0.0));
    }

    #[test]
    fn rect_from_size_and_position() {
        let geo = json!({"size": {"width": "100.0", "height": "50.0"}});
        let r = map_rect(&geo, (10.0, 20.0)).expect("rect");
        assert_eq!(r.x, 10.0);
        assert_eq!(r.y, 20.0);
        assert_eq!(r.width, 100.0);
        assert_eq!(r.height, 50.0);
        assert!(map_rect(&json!({}), (0.0, 0.0)).is_none());
    }

    #[test]
    fn styles_are_bucketed_by_category() {
        let props = vec![
            json!({"name": "fontSize", "value": "16.0", "propertyType": "double"}),
            json!({"name": "color", "description": "Color(0xff2563eb)", "propertyType": "Color"}),
            json!({"name": "mainAxisAlignment", "value": "center", "propertyType": "String"}),
            json!({"name": "customThing", "description": "x"}),
        ];
        let styles = map_styles(&props);
        assert_eq!(styles.get("font-size"), Some("16.0"));
        assert_eq!(
            styles.get("color"),
            Some("#2563eb"),
            "Flutter Color normalized to hex"
        );
        assert_eq!(styles.get("mainAxisAlignment"), Some("center"));
        assert_eq!(styles.get("customThing"), Some("x"));
        assert!(
            styles
                .groups
                .iter()
                .any(|(c, _)| *c == StyleCategory::Typography)
        );
        assert!(
            styles
                .groups
                .iter()
                .any(|(c, _)| *c == StyleCategory::Visual)
        );
        assert!(
            styles
                .groups
                .iter()
                .any(|(c, _)| *c == StyleCategory::Layout)
        );
    }

    #[test]
    fn empty_properties_give_empty_styles() {
        assert!(map_styles(&[]).is_empty());
    }
}
