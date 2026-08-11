//! Deterministic structural tree diff.
//!
//! Nodes are matched first by `selector` (stable anchors such as
//! `data-testid`), then by sibling position. Changes are computed at the
//! property level with an optional numeric tolerance to absorb subpixel
//! jitter. No LLM is involved: this layer only answers *what changed*.

use serde_json::{Map, Value};

use crate::model::DiffNode;

/// Diffing behaviour.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffOptions {
    /// Values closer than this (in their unit) are considered unchanged,
    /// e.g. `0.5` absorbs `16px` -> `16.2px`. `0` disables tolerance.
    pub tolerance: f64,
    /// Property names (e.g. `transform`, `opacity`) whose changes never
    /// mark a node as changed — for volatile props animated on purpose.
    pub ignore_props: Vec<String>,
    /// Suppress ADDED/REMOVED delta lines (report only CHANGED). Useful
    /// for lists whose item count varies by design.
    pub ignore_structural: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            tolerance: 0.5,
            ignore_props: Vec::new(),
            ignore_structural: false,
        }
    }
}

/// One delta line: a node that changed, was added or removed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeltaLine {
    pub status: &'static str,
    pub selector: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
    /// For `CHANGED` nodes: the concrete property-level differences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<Value>,
    /// For `ADDED`/`REMOVED` nodes: the full style snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Value>,
}

/// Aggregate counters for a diff run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct DiffStats {
    pub changed: usize,
    pub added: usize,
    pub removed: usize,
    pub base_nodes: usize,
    pub head_nodes: usize,
}

/// Diff two snapshot forests (pre-order deterministic output).
pub fn diff_trees(
    base: &[DiffNode],
    head: &[DiffNode],
    opts: &DiffOptions,
) -> (Vec<DeltaLine>, DiffStats) {
    let mut out = Vec::new();
    let mut stats = DiffStats {
        base_nodes: count_nodes(base),
        head_nodes: count_nodes(head),
        ..DiffStats::default()
    };
    diff_children(base, head, opts, &mut out, &mut stats);
    (out, stats)
}

fn count_nodes(nodes: &[DiffNode]) -> usize {
    nodes.iter().map(|n| 1 + count_nodes(&n.children)).sum()
}

/// Match and diff two sibling lists.
fn diff_children(
    base: &[DiffNode],
    head: &[DiffNode],
    opts: &DiffOptions,
    out: &mut Vec<DeltaLine>,
    stats: &mut DiffStats,
) {
    let mut used_h = vec![false; head.len()];
    let mut match_h: Vec<Option<usize>> = vec![None; base.len()];

    // Pass 1: match by stable selector.
    for (bi, b) in base.iter().enumerate() {
        for (hi, h) in head.iter().enumerate() {
            if !used_h[hi] && !b.selector.is_empty() && b.selector == h.selector {
                used_h[hi] = true;
                match_h[bi] = Some(hi);
                break;
            }
        }
    }
    // Pass 2: positional fallback for the remainder.
    for slot in match_h.iter_mut() {
        if slot.is_some() {
            continue;
        }
        if let Some(hi) = (0..head.len()).find(|&hi| !used_h[hi]) {
            used_h[hi] = true;
            *slot = Some(hi);
        }
    }

    // Recurse over matches, report removals inline (pre-order).
    for (bi, b) in base.iter().enumerate() {
        match match_h[bi] {
            Some(hi) => diff_node(b, &head[hi], opts, out, stats),
            None => {
                if !opts.ignore_structural {
                    out.push(removed_line(b));
                    stats.removed += 1;
                }
            }
        }
    }
    for (hi, h) in head.iter().enumerate() {
        if !used_h[hi] && !opts.ignore_structural {
            out.push(added_line(h));
            stats.added += 1;
        }
    }
}

/// Diff a matched node pair: the node's own changes, then its children.
fn diff_node(
    base: &DiffNode,
    head: &DiffNode,
    opts: &DiffOptions,
    out: &mut Vec<DeltaLine>,
    stats: &mut DiffStats,
) {
    if let Some(changes) = compute_changes(base, head, opts) {
        out.push(DeltaLine {
            status: "CHANGED",
            selector: head.selector.clone(),
            tag: head.tag.clone(),
            path: head.path.clone(),
            depth: head.depth,
            changes: Some(changes),
            snapshot: None,
        });
        stats.changed += 1;
    }
    diff_children(&base.children, &head.children, opts, out, stats);
}

/// Compare every observable facet of a node; `None` when unchanged.
fn compute_changes(base: &DiffNode, head: &DiffNode, opts: &DiffOptions) -> Option<Value> {
    let mut changes = Map::new();

    if let Some(styles) = style_diff(base.styles.as_ref(), head.styles.as_ref(), opts) {
        changes.insert("styles".into(), styles);
    }
    if let Some(pseudo) = pseudo_diff(base.pseudo.as_ref(), head.pseudo.as_ref(), opts) {
        changes.insert("pseudo".into(), pseudo);
    }
    if let Some(rect) = rect_diff(base.rect.as_ref(), head.rect.as_ref(), opts) {
        changes.insert("rect".into(), rect);
    }
    if base.metrics != head.metrics {
        changes.insert(
            "metrics".into(),
            json_before_after(base.metrics.as_ref(), head.metrics.as_ref()),
        );
    }
    if base.is_visible != head.is_visible {
        changes.insert(
            "is_visible".into(),
            json_before_after(
                base.is_visible.map(Value::Bool).as_ref(),
                head.is_visible.map(Value::Bool).as_ref(),
            ),
        );
    }
    if let Some(aria) = object_diff(base.aria.as_ref(), head.aria.as_ref()) {
        changes.insert("aria".into(), aria);
    }
    if let Some(contrast) = object_diff(base.contrast.as_ref(), head.contrast.as_ref()) {
        changes.insert("contrast".into(), contrast);
    }
    if let Some(ax) = object_diff(base.ax.as_ref(), head.ax.as_ref()) {
        changes.insert("ax".into(), ax);
    }

    if changes.is_empty() {
        None
    } else {
        Some(Value::Object(changes))
    }
}

/// Diff two arbitrary JSON values. Objects are diffed per key (before/after
/// per differing leaf); scalars/arrays fall back to a whole-value
/// before/after pair.
fn object_diff(base: Option<&Value>, head: Option<&Value>) -> Option<Value> {
    match (base, head) {
        (None, None) => None,
        (Some(a), Some(b)) if a == b => None,
        (Some(a), Some(b)) => {
            if let (Some(am), Some(bm)) = (a.as_object(), b.as_object()) {
                let mut result = Map::new();
                let mut keys: Vec<&String> = am.keys().chain(bm.keys()).collect();
                keys.sort_unstable();
                keys.dedup();
                for k in keys {
                    if let Some(d) = object_diff(am.get(k), bm.get(k)) {
                        result.insert(k.clone(), d);
                    }
                }
                if result.is_empty() {
                    None
                } else {
                    Some(Value::Object(result))
                }
            } else {
                Some(json_before_after(Some(a), Some(b)))
            }
        }
        (Some(a), None) => Some(json_before_after(Some(a), None)),
        (None, Some(b)) => Some(json_before_after(None, Some(b))),
    }
}

/// Diff two `styles` maps (`category -> prop -> value`).
fn style_diff(
    base: Option<&Map<String, Value>>,
    head: Option<&Map<String, Value>>,
    opts: &DiffOptions,
) -> Option<Value> {
    let mut result = Map::new();
    let mut keys: Vec<&String> = base
        .into_iter()
        .flat_map(|m| m.keys())
        .chain(head.into_iter().flat_map(|m| m.keys()))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    for key in keys {
        let bv = base.and_then(|m| m.get(key));
        let hv = head.and_then(|m| m.get(key));
        let props = prop_diff(bv, hv, opts);
        if !props.is_empty() {
            result.insert(key.clone(), Value::Object(props));
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(Value::Object(result))
    }
}

/// Diff the properties within a single category.
fn prop_diff(base: Option<&Value>, head: Option<&Value>, opts: &DiffOptions) -> Map<String, Value> {
    let mut props = Map::new();
    let ignored = |k: &str| opts.ignore_props.iter().any(|p| p == k);
    match (base, head) {
        (Some(b), Some(h)) => match (b.as_object(), h.as_object()) {
            (Some(bm), Some(hm)) => {
                let mut keys: Vec<&String> = bm.keys().chain(hm.keys()).collect();
                keys.sort_unstable();
                keys.dedup();
                for k in keys {
                    if ignored(k) {
                        continue;
                    }
                    let bv = bm.get(k);
                    let hv = hm.get(k);
                    if !value_equal(bv, hv, opts) {
                        props.insert(k.clone(), json_before_after(bv, hv));
                    }
                }
            }
            _ => {
                if !value_equal(Some(b), Some(h), opts) {
                    props.insert("_value".into(), json_before_after(Some(b), Some(h)));
                }
            }
        },
        (Some(b), None) => match b.as_object() {
            Some(bm) => {
                for (k, v) in bm {
                    if ignored(k) {
                        continue;
                    }
                    props.insert(k.clone(), json_before_after(Some(v), None));
                }
            }
            None => {
                props.insert("_value".into(), json_before_after(Some(b), None));
            }
        },
        (None, Some(h)) => match h.as_object() {
            Some(hm) => {
                for (k, v) in hm {
                    if ignored(k) {
                        continue;
                    }
                    props.insert(k.clone(), json_before_after(None, Some(v)));
                }
            }
            None => {
                props.insert("_value".into(), json_before_after(None, Some(h)));
            }
        },
        (None, None) => {}
    }
    props
}

/// Diff pseudo-element maps (`name -> styles`).
fn pseudo_diff(
    base: Option<&Map<String, Value>>,
    head: Option<&Map<String, Value>>,
    opts: &DiffOptions,
) -> Option<Value> {
    let mut result = Map::new();
    let mut keys: Vec<&String> = base
        .into_iter()
        .flat_map(|m| m.keys())
        .chain(head.into_iter().flat_map(|m| m.keys()))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    for key in keys {
        let bv = base.and_then(|m| m.get(key));
        let hv = head.and_then(|m| m.get(key));
        if let Some(diff) = style_diff(
            bv.and_then(Value::as_object),
            hv.and_then(Value::as_object),
            opts,
        ) {
            result.insert(key.clone(), diff);
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(Value::Object(result))
    }
}

/// Diff two rects (numeric coordinates, tolerance-aware).
fn rect_diff(base: Option<&Value>, head: Option<&Value>, opts: &DiffOptions) -> Option<Value> {
    let changed = match (base, head) {
        (None, None) => false,
        (Some(b), Some(h)) => match (rect_coords(b), rect_coords(h)) {
            (Some(a), Some(c)) => {
                let deltas = [
                    (a.0 - c.0).abs(),
                    (a.1 - c.1).abs(),
                    (a.2 - c.2).abs(),
                    (a.3 - c.3).abs(),
                ];
                deltas.iter().any(|d| *d > opts.tolerance)
            }
            _ => b != h,
        },
        _ => true,
    };
    if changed {
        Some(json_before_after(base, head))
    } else {
        None
    }
}

fn rect_coords(v: &Value) -> Option<(f64, f64, f64, f64)> {
    let o = v.as_object()?;
    Some((
        o.get("x")?.as_f64()?,
        o.get("y")?.as_f64()?,
        o.get("width")?.as_f64()?,
        o.get("height")?.as_f64()?,
    ))
}

/// Property-level equality, applying the numeric tolerance to strings with
/// numbers (e.g. `16px` vs `16.2px`).
fn value_equal(base: Option<&Value>, head: Option<&Value>, opts: &DiffOptions) -> bool {
    match (base, head) {
        (None, None) => true,
        (Some(Value::String(a)), Some(Value::String(b))) => {
            a == b || (opts.tolerance > 0.0 && numeric_close(a, b, opts.tolerance))
        }
        (Some(Value::Number(a)), Some(Value::Number(b))) => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => {
                x == y || (opts.tolerance > 0.0 && (x - y).abs() <= opts.tolerance)
            }
            _ => a == b,
        },
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Split a CSS value into a leading number and the unit remainder.
fn strip_number(s: &str) -> Option<(f64, &str)> {
    let s = s.trim();
    let idx = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(s.len());
    if idx == 0 {
        return None;
    }
    let (num, rest) = s.split_at(idx);
    Some((num.parse().ok()?, rest))
}

/// True when both values are numbers with the same unit and within `tol`.
fn numeric_close(a: &str, b: &str, tol: f64) -> bool {
    let (Some((x, ru)), Some((y, rv))) = (strip_number(a), strip_number(b)) else {
        return false;
    };
    ru == rv && (x - y).abs() <= tol
}

fn json_before_after(before: Option<&Value>, after: Option<&Value>) -> Value {
    let mut m = Map::new();
    m.insert("before".into(), before.cloned().unwrap_or(Value::Null));
    m.insert("after".into(), after.cloned().unwrap_or(Value::Null));
    Value::Object(m)
}

fn added_line(node: &DiffNode) -> DeltaLine {
    DeltaLine {
        status: "ADDED",
        selector: node.selector.clone(),
        tag: node.tag.clone(),
        path: node.path.clone(),
        depth: node.depth,
        changes: None,
        snapshot: Some(snapshot_value(node)),
    }
}

fn removed_line(node: &DiffNode) -> DeltaLine {
    DeltaLine {
        status: "REMOVED",
        selector: node.selector.clone(),
        tag: node.tag.clone(),
        path: node.path.clone(),
        depth: node.depth,
        changes: None,
        snapshot: Some(snapshot_value(node)),
    }
}

/// Full style snapshot of a node (no ids, no children).
fn snapshot_value(node: &DiffNode) -> Value {
    let mut m = Map::new();
    if let Some(t) = &node.tag {
        m.insert("tag".into(), Value::String(t.clone()));
    }
    if let Some(p) = &node.path {
        m.insert("path".into(), Value::String(p.clone()));
    }
    if let Some(d) = node.depth {
        m.insert("depth".into(), Value::from(d));
    }
    if let Some(r) = &node.rect {
        m.insert("rect".into(), r.clone());
    }
    if let Some(mt) = &node.metrics {
        m.insert("metrics".into(), mt.clone());
    }
    if let Some(v) = node.is_visible {
        m.insert("is_visible".into(), Value::Bool(v));
    }
    if let Some(s) = &node.styles {
        m.insert("styles".into(), Value::Object(s.clone()));
    }
    if let Some(ps) = &node.pseudo {
        m.insert("pseudo".into(), Value::Object(ps.clone()));
    }
    Value::Object(m)
}

/// Re-export used by the library API.
pub type Node = DiffNode;

#[cfg(test)]
mod tests {
    use super::*;

    fn node(selector: &str, styles: Option<Map<String, Value>>) -> DiffNode {
        DiffNode {
            id: 0,
            parent_id: None,
            selector: selector.to_string(),
            tag: Some("DIV".into()),
            path: Some(selector.to_string()),
            depth: Some(0),
            rect: None,
            metrics: None,
            is_visible: Some(true),
            hash: None,
            styles,
            pseudo: None,
            aria: None,
            contrast: None,
            ax: None,
            children: vec![],
        }
    }

    fn box_width(v: &str) -> Option<Map<String, Value>> {
        let obj: Value = serde_json::json!({
            "box_model": {"width": v}
        });
        Some(obj.as_object().unwrap().clone())
    }

    #[test]
    fn identical_trees_produce_no_delta() {
        let a = node("div.card", box_width("44px"));
        let b = node("div.card", box_width("44px"));
        let (deltas, stats) = diff_trees(&[a], &[b], &DiffOptions::default());
        assert!(deltas.is_empty());
        assert_eq!(stats.changed, 0);
    }

    #[test]
    fn style_change_yields_changed_with_before_after() {
        let a = node("div.card", box_width("44px"));
        let b = node("div.card", box_width("40px"));
        let (deltas, _) = diff_trees(&[a], &[b], &DiffOptions::default());
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].status, "CHANGED");
        let changes = deltas[0].changes.as_ref().unwrap();
        assert_eq!(changes["styles"]["box_model"]["width"]["before"], "44px");
        assert_eq!(changes["styles"]["box_model"]["width"]["after"], "40px");
    }

    #[test]
    fn subpixel_change_within_tolerance_is_ignored() {
        let a = node("div.card", box_width("44px"));
        let b = node("div.card", box_width("44.2px"));
        let (deltas, stats) = diff_trees(
            &[a],
            &[b],
            &DiffOptions {
                tolerance: 0.5,
                ..DiffOptions::default()
            },
        );
        assert!(deltas.is_empty());
        assert_eq!(stats.changed, 0);
    }

    #[test]
    fn unit_mismatch_is_not_tolerated() {
        let a = node("div.card", box_width("44px"));
        let b = node("div.card", box_width("44rem"));
        let (deltas, _) = diff_trees(
            &[a],
            &[b],
            &DiffOptions {
                tolerance: 0.5,
                ..DiffOptions::default()
            },
        );
        assert_eq!(deltas.len(), 1);
    }

    #[test]
    fn beyond_tolerance_is_changed() {
        let a = node("div.card", box_width("44px"));
        let b = node("div.card", box_width("46px"));
        let (deltas, _) = diff_trees(
            &[a],
            &[b],
            &DiffOptions {
                tolerance: 0.5,
                ..DiffOptions::default()
            },
        );
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].status, "CHANGED");
    }

    #[test]
    fn added_and_removed_nodes_are_reported() {
        let base = vec![node("div.a", box_width("10px"))];
        let head = vec![
            node("div.a", box_width("10px")),
            node("div.b", box_width("20px")),
        ];
        let (deltas, stats) = diff_trees(&base, &head, &DiffOptions::default());
        let statuses: Vec<&str> = deltas.iter().map(|d| d.status).collect();
        assert_eq!(statuses, vec!["ADDED"]);
        assert_eq!(deltas[0].selector, "div.b");
        assert_eq!(stats.removed, 0);
        assert_eq!(stats.added, 1);
        assert!(deltas[0].snapshot.is_some());
    }

    #[test]
    fn child_change_detected_through_unchanged_parent() {
        let child_a = DiffNode {
            selector: "button".into(),
            tag: Some("BUTTON".into()),
            path: Some("div.card > button".into()),
            depth: Some(1),
            styles: box_width("40px"),
            children: vec![],
            ..node("button", None)
        };
        let child_b = DiffNode {
            styles: box_width("44px"),
            ..child_a.clone()
        };
        let mut parent_a = node("div.card", box_width("300px"));
        parent_a.children = vec![child_a];
        let mut parent_b = node("div.card", box_width("300px"));
        parent_b.children = vec![child_b];
        let (deltas, _) = diff_trees(&[parent_a], &[parent_b], &DiffOptions::default());
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].selector, "button");
        assert_eq!(deltas[0].status, "CHANGED");
    }

    #[test]
    fn is_visible_change_is_reported() {
        let mut a = node("div.card", box_width("44px"));
        let mut b = node("div.card", box_width("44px"));
        a.is_visible = Some(true);
        b.is_visible = Some(false);
        let (deltas, _) = diff_trees(&[a], &[b], &DiffOptions::default());
        assert_eq!(deltas.len(), 1);
        let changes = deltas[0].changes.as_ref().unwrap();
        assert_eq!(changes["is_visible"]["before"], true);
        assert_eq!(changes["is_visible"]["after"], false);
    }

    #[test]
    fn selector_matching_wins_over_position() {
        // Same positions but swapped selectors: matched by selector, not index.
        let base = vec![
            node("div.first", box_width("1px")),
            node("div.second", box_width("2px")),
        ];
        let head = vec![
            node("div.second", box_width("3px")),
            node("div.first", box_width("1px")),
        ];
        let (deltas, stats) = diff_trees(&base, &head, &DiffOptions::default());
        // first unchanged, second changed 2px->3px
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].selector, "div.second");
        assert_eq!(stats.added, 0);
        assert_eq!(stats.removed, 0);
    }

    #[test]
    fn stats_count_nodes() {
        let mut root = node("div.card", box_width("44px"));
        root.children.push(node("span", box_width("10px")));
        let (_, stats) = diff_trees(
            &[root],
            &[node("div.card", box_width("44px"))],
            &DiffOptions::default(),
        );
        assert_eq!(stats.base_nodes, 2);
        assert_eq!(stats.head_nodes, 1);
    }

    #[test]
    fn aria_change_is_reported_per_key() {
        let mut a = node("button", box_width("44px"));
        let mut b = node("button", box_width("44px"));
        a.aria = Some(serde_json::json!({"role": "button", "name": "Salvar", "focusable": true}));
        b.aria = Some(serde_json::json!({"role": "button", "name": "Cancelar", "focusable": true}));
        let (deltas, _) = diff_trees(&[a], &[b], &DiffOptions::default());
        assert_eq!(deltas.len(), 1);
        let changes = deltas[0].changes.as_ref().unwrap();
        assert_eq!(changes["aria"]["name"]["before"], "Salvar");
        assert_eq!(changes["aria"]["name"]["after"], "Cancelar");
        assert!(changes["aria"].get("role").is_none(), "role unchanged");
    }

    #[test]
    fn contrast_change_is_reported() {
        let mut a = node("div", box_width("44px"));
        let mut b = node("div", box_width("44px"));
        a.contrast = Some(serde_json::json!({"ratio": 4.5, "aa": "pass", "aaa": "fail"}));
        b.contrast = Some(serde_json::json!({"ratio": 2.1, "aa": "fail", "aaa": "fail"}));
        let (deltas, _) = diff_trees(&[a], &[b], &DiffOptions::default());
        assert_eq!(deltas.len(), 1);
        let changes = deltas[0].changes.as_ref().unwrap();
        assert_eq!(changes["contrast"]["ratio"]["after"], 2.1);
        assert_eq!(changes["contrast"]["aa"]["after"], "fail");
    }

    #[test]
    fn ax_removal_is_reported() {
        let mut a = node("div", box_width("44px"));
        let b = node("div", box_width("44px"));
        a.ax = Some(serde_json::json!({"role": "banner", "ignored": false}));
        let (deltas, _) = diff_trees(&[a], &[b], &DiffOptions::default());
        let changes = deltas[0].changes.as_ref().unwrap();
        assert_eq!(changes["ax"]["before"]["role"], "banner");
        assert_eq!(changes["ax"]["after"], Value::Null);
    }

    #[test]
    fn ignore_props_hides_volatile_properties() {
        let a = node(
            "div.card",
            Some(
                serde_json::json!({
                    "box_model": {"width": "44px"},
                    "transform": {"transform": "translateX(0px)"}
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        );
        let b = node(
            "div.card",
            Some(
                serde_json::json!({
                    "box_model": {"width": "44px"},
                    "transform": {"transform": "translateX(-22px)"}
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        );
        let opts = DiffOptions {
            tolerance: 0.5,
            ignore_props: vec!["transform".into()],
            ignore_structural: false,
        };
        let (deltas, stats) = diff_trees(&[a], &[b], &opts);
        assert!(deltas.is_empty(), "transform-only change must be ignored");
        assert_eq!(stats.changed, 0);
    }

    #[test]
    fn ignore_structural_suppresses_added_removed() {
        let base = vec![node("div.a", box_width("10px"))];
        let head = vec![
            node("div.a", box_width("10px")),
            node("div.b", box_width("20px")),
        ];
        let opts = DiffOptions {
            tolerance: 0.5,
            ignore_props: Vec::new(),
            ignore_structural: true,
        };
        let (deltas, stats) = diff_trees(&base, &head, &opts);
        assert!(deltas.is_empty(), "ADDED must be suppressed: {deltas:?}");
        assert_eq!(stats.added, 0);
        assert_eq!(stats.removed, 0);
        assert_eq!(stats.base_nodes, 1);
        assert_eq!(stats.head_nodes, 2);
    }
}
