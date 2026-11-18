//! Snapshot node model and JSONL loading.
//!
//! Parses the JSONL produced by `sniff` into a lightweight
//! [`DiffNode`] tree, accepting both `jsonl` (tree: one root per line with
//! nested `children`) and `jsonl-flat` (one node per line with
//! `id`/`parent_id`) inputs. `__meta` lines (global `css_variables`, the
//! compact `style_defaults` hoist map and the v2 schema/matrix metadata) are
//! parsed; `style_defaults` are merged back into every node's `styles` so the
//! diff always sees the full effective values (lossless reconstruction of the
//! compact dedup).
//!
//! The v2 snapshot facetas (`__document`, `__env`, `__declared`) and the
//! matrix/shadow-root metadata are parsed in a single pass, tolerantly: a
//! malformed or missing line leaves the corresponding field absent. A v1
//! snapshot (schema absent or `1`) is promoted in memory to schema `2` with
//! empty facets — it is never rejected, so `diff` keeps working across the
//! v1/v2 boundary.

use serde_json::{Map, Value};
use sniff_core::modern::{DeclaredFacet, DocumentFacet, EnvFacet, MatrixTag};
use std::path::Path;

use crate::error::{DiffError, DiffResult};

/// A flat-mode entry: `(id, parent_id, node)` as emitted by `jsonl-flat`.
type FlatEntry = (u64, Option<u64>, DiffNode);

/// `node id -> flat index` lookup used while rebuilding the forest.
type NodeIndex = std::collections::BTreeMap<u64, usize>;

/// A single captured element, in a form convenient for structural diffing.
#[derive(Debug, Clone)]
pub struct DiffNode {
    /// Stable node id assigned during extraction (pre-order).
    pub id: u64,
    /// Id of the parent node, if any.
    pub parent_id: Option<u64>,
    /// Stable selector identifying the node.
    pub selector: String,
    /// Tag name, when known.
    pub tag: Option<String>,
    /// DOM path, when captured.
    pub path: Option<String>,
    /// Depth in the captured tree, when known.
    pub depth: Option<usize>,
    /// Bounding client rect, when captured.
    pub rect: Option<Value>,
    /// Derived layout metrics (`z-index`, stacking context).
    pub metrics: Option<Value>,
    /// `{"display_visible": bool, "accessibility_grade": "NONE"|"AA"|"AAA"}`.
    pub noticeable: Option<Value>,
    /// Computed-style hash used for fast equality checks.
    pub hash: Option<String>,
    /// Computed styles by category.
    pub styles: Option<Map<String, Value>>,
    /// Per-pseudo-element styles.
    pub pseudo: Option<Map<String, Value>>,
    /// Resolved ARIA attributes.
    pub aria: Option<Value>,
    /// Contrast facet.
    pub contrast: Option<Value>,
    /// Accessibility-tree facet.
    pub ax: Option<Value>,
    /// Extra captured DOM attributes.
    pub attributes: Option<Value>,
    /// In-page computed effective background (behind transparent nodes),
    /// parsed from `effectiveBackground` or `effective_background`.
    pub effective_background: Option<Value>,
    /// Child nodes.
    pub children: Vec<Self>,
}

impl DiffNode {
    fn from_value(v: &Value) -> Self {
        let children = v
            .get("children")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(Self::from_value).collect())
            .unwrap_or_default();
        Self {
            id: v.get("id").and_then(Value::as_u64).unwrap_or(0),
            parent_id: v.get("parent_id").and_then(Value::as_u64),
            selector: v
                .get("selector")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tag: v.get("tag").and_then(Value::as_str).map(String::from),
            path: v.get("path").and_then(Value::as_str).map(String::from),
            depth: v
                .get("depth")
                .and_then(Value::as_u64)
                .map(|d| usize::try_from(d).unwrap_or(usize::MAX)),
            rect: v.get("rect").cloned(),
            metrics: v.get("metrics").cloned(),
            noticeable: v.get("is_user_noticeable").cloned(),
            hash: v
                .get("computed_style_hash")
                .and_then(Value::as_str)
                .map(String::from),
            styles: v.get("styles").and_then(Value::as_object).cloned(),
            pseudo: v.get("pseudo").and_then(Value::as_object).cloned(),
            aria: v.get("aria").cloned(),
            contrast: v.get("contrast").cloned(),
            ax: v.get("ax").cloned(),
            attributes: v.get("attrs").cloned(),
            effective_background: v
                .get("effectiveBackground")
                .or_else(|| v.get("effective_background"))
                .filter(|value| !value.is_null())
                .cloned(),
            children,
        }
    }

    /// Whether the element is rendered in layout (`display_visible`), when
    /// noticeability was captured. `None` if unknown.
    pub fn display_visible(&self) -> Option<bool> {
        self.noticeable
            .as_ref()
            .and_then(|v| v.get("display_visible"))
            .and_then(Value::as_bool)
    }

    /// The captured accessibility grade (`NONE`, `AA`, `AAA`), when
    /// noticeability was captured.
    pub fn accessibility_grade(&self) -> Option<&str> {
        self.noticeable
            .as_ref()
            .and_then(|v| v.get("accessibility_grade"))
            .and_then(Value::as_str)
    }
}

/// Reconstruct a forest of `DiffNode`s from flat JSONL (nodes referencing
/// each other via `parent_id`, emitted in pre-order).
fn build_forest(flat: &[FlatEntry]) -> Vec<DiffNode> {
    fn build(i: usize, flat: &[FlatEntry], children: &[Vec<usize>]) -> Option<DiffNode> {
        let entry = flat.get(i)?;
        let mut node = entry.2.clone();
        node.children = children
            .get(i)
            .map(|ids| {
                ids.iter()
                    .filter_map(|&c| build(c, flat, children))
                    .collect()
            })
            .unwrap_or_default();
        Some(node)
    }

    if flat.is_empty() {
        return Vec::new();
    }
    let by_id: NodeIndex = flat
        .iter()
        .enumerate()
        .map(|(i, (id, _, _))| (*id, i))
        .collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); flat.len()];
    let mut roots: Vec<usize> = Vec::new();

    for (i, (_, parent, _)) in flat.iter().enumerate() {
        match parent {
            Some(p) => match by_id.get(p) {
                Some(&pi) => match children.get_mut(pi) {
                    Some(slot) => slot.push(i),
                    // Should be unreachable (`pi` indexes `flat`), but never
                    // drop a node: treat it as a root instead.
                    None => roots.push(i),
                },
                // Orphan parent: treat as a root so no node is lost.
                None => roots.push(i),
            },
            None => roots.push(i),
        }
    }

    roots.sort_unstable();
    roots
        .iter()
        .filter_map(|&i| build(i, flat, &children))
        .collect()
}

/// A parsed snapshot: the node forest plus the global v2 facetas carried by
/// the capture (`__document`, `__env`, `__declared`, matrix and shadow roots).
#[derive(Debug, Clone, Default)]
pub struct DiffDocument {
    /// Root nodes of the captured forest.
    pub nodes: Vec<DiffNode>,
    /// `__actions` UI-effect entries carried by the capture.
    pub actions: Vec<Value>,
    /// Emulated viewport `(width, height)` from the `__meta` line, when the
    /// capture recorded it (used by viewport-relative offline checks).
    pub viewport: Option<(f64, f64)>,
    /// `__document` facet: title/lang/doctype/meta/links/scripts/stylesheets.
    pub document: Option<DocumentFacet>,
    /// `__env` facet: normalized media preferences, emulation and device.
    pub env: Option<EnvFacet>,
    /// `__declared` facet: CSSOM rule origins (opt-in capture).
    pub declared: Option<DeclaredFacet>,
    /// Matrix states in capture order (`__meta.matrix`, one per state).
    pub matrix: Vec<MatrixTag>,
    /// Global CSS custom properties from `__meta.css_variables`.
    pub css_variables: Option<Map<String, Value>>,
    /// Snapshot schema: `1` is promoted to `2` on load; v2 is used directly.
    pub schema: u8,
    /// Number of open shadow roots traversed during capture.
    pub shadow_roots: usize,
}

/// Narrows a snapshot to the first matrix state for node-level checks.
///
/// Uses captured ID ranges when present and positional slicing as a
/// backward-compatible fallback for older v2 snapshots without ranges.
pub fn baseline_document(doc: &DiffDocument) -> DiffDocument {
    let Some(first) = doc.matrix.iter().min_by_key(|tag| tag.index) else {
        return doc.clone();
    };
    let mut baseline = doc.clone();
    let ranged = match (first.node_id_start, first.node_id_end) {
        (Some(start), Some(end)) if start <= end => {
            baseline
                .nodes
                .retain(|node| node.id >= start && node.id <= end);
            true
        }
        _ => false,
    };
    if !ranged && doc.matrix.len() > 1 {
        let per_state = doc.nodes.len().checked_div(doc.matrix.len()).unwrap_or(0);
        baseline.nodes.truncate(per_state);
    }
    baseline.matrix.clear();
    baseline
}

/// Load a snapshot JSONL file into a [`DiffDocument`].
pub fn load_file_doc(path: &Path) -> DiffResult<DiffDocument> {
    let content = std::fs::read_to_string(path)?;
    load_str_doc(&content)
}

/// Parse snapshot JSONL from an in-memory string into a [`DiffDocument`].
///
/// Nodes are parsed by [`load_str`]; every global line is collected in a
/// single additional pass by [`extract_globals`].
pub fn load_str_doc(content: &str) -> DiffResult<DiffDocument> {
    let mut doc = extract_globals(content);
    doc.nodes = load_str(content)?;
    Ok(doc)
}

/// Collect all global metadata in one pass over the JSONL lines.
///
/// Parsing is tolerant: a missing, malformed or wrongly typed line leaves its
/// field empty rather than failing the load. Repeated facets (matrix states)
/// keep the last successfully parsed value; matrix tags are appended in wire
/// order. Unknown `__*` lines are ignored.
fn extract_globals(content: &str) -> DiffDocument {
    let mut doc = DiffDocument::default();
    let mut raw_schema: u8 = 1;
    let mut explicit_v1 = false;
    let mut schema_declared = false;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };

        if let Some(facet) = value.get("__document") {
            match serde_json::from_value::<DocumentFacet>(facet.clone()) {
                Ok(parsed) => doc.document = Some(parsed),
                Err(err) => tracing::warn!(error = %err, "ignoring malformed __document facet"),
            }
            continue;
        }
        if let Some(facet) = value.get("__env") {
            match serde_json::from_value::<EnvFacet>(facet.clone()) {
                Ok(parsed) => doc.env = Some(parsed),
                Err(err) => tracing::warn!(error = %err, "ignoring malformed __env facet"),
            }
            continue;
        }
        if let Some(facet) = value.get("__declared") {
            match serde_json::from_value::<DeclaredFacet>(normalize_at_rules(facet)) {
                Ok(parsed) => doc.declared = Some(parsed),
                Err(err) => tracing::warn!(error = %err, "ignoring malformed __declared facet"),
            }
            continue;
        }
        if let Some(actions) = value.get("__actions").and_then(Value::as_array) {
            doc.actions.extend(actions.iter().cloned());
            continue;
        }

        if let Some(meta) = value.get("__meta").and_then(Value::as_object) {
            if doc.viewport.is_none()
                && let Some((width, height)) = viewport_from_meta(meta)
            {
                doc.viewport = Some((width, height));
            }
            if let Some(schema) = meta.get("schema").and_then(Value::as_u64) {
                schema_declared = true;
                if let Ok(schema) = u8::try_from(schema) {
                    raw_schema = schema;
                    explicit_v1 = schema == 1;
                }
            }
            if let Some(roots) = meta.get("shadow_roots").and_then(Value::as_u64) {
                doc.shadow_roots = usize::try_from(roots).unwrap_or(0);
            }
            if let Some(variables) = meta.get("css_variables").and_then(Value::as_object) {
                doc.css_variables = Some(variables.clone());
            }
            if let Some(matrix) = meta.get("matrix") {
                append_matrix_tags(matrix, &mut doc.matrix);
            }
        }
    }

    // Snapshot migration (docs/modernization/08-contrato-v2.md §8.6): a v1
    // snapshot has no `__meta.schema` (or declares `1`) and no new facets.
    // Promote it in memory to schema 2 with empty facets instead of rejecting
    // it, so `sniff diff base-v1.jsonl head-v2.jsonl` keeps working. v2 is
    // used directly. Pre-existing v1 data (nodes, actions, viewport and
    // css_variables) is preserved; only the new facets are empty.
    let has_v2_facets = doc.document.is_some()
        || doc.env.is_some()
        || doc.declared.is_some()
        || !doc.matrix.is_empty()
        || doc.shadow_roots > 0;
    let promote = explicit_v1 || (!schema_declared && !has_v2_facets);
    if promote {
        tracing::debug!(
            from = raw_schema,
            to = 2u8,
            "promoting v1 snapshot in memory"
        );
        doc.document = None;
        doc.env = None;
        doc.declared = None;
        doc.matrix.clear();
        doc.shadow_roots = 0;
    }
    doc.schema = if promote { 2 } else { raw_schema.max(2) };
    doc
}

/// Read the `(width, height)` viewport from a `__meta` object, when valid.
fn viewport_from_meta(meta: &Map<String, Value>) -> Option<(f64, f64)> {
    let viewport = meta.get("viewport")?;
    let width = viewport.get("width")?.as_f64()?;
    let height = viewport.get("height")?.as_f64()?;
    Some((width, height))
}

/// Append matrix tags to `out` in wire order, accepting both the array form
/// and the single-object form emitted per state by the capture.
fn append_matrix_tags(value: &Value, out: &mut Vec<MatrixTag>) {
    match value {
        Value::Array(items) => {
            for item in items {
                push_matrix_tag(item, out);
            }
        }
        Value::Object(_) => push_matrix_tag(value, out),
        _ => {}
    }
}

/// Parse one matrix tag, skipping malformed entries without dropping the rest.
fn push_matrix_tag(value: &Value, out: &mut Vec<MatrixTag>) {
    match serde_json::from_value::<MatrixTag>(value.clone()) {
        Ok(tag) => {
            tracing::trace!(index = tag.index, total = tag.total, "parsed matrix tag");
            out.push(tag);
        }
        Err(err) => tracing::warn!(error = %err, "ignoring malformed matrix tag"),
    }
}

/// Normalize the capture's `conditionText` spelling to the typed field name
/// `condition_text` used by [`DeclaredFacet`], keeping all other keys intact.
fn normalize_at_rules(value: &Value) -> Value {
    let Some(mut declared) = value.as_object().cloned() else {
        return value.clone();
    };
    let Some(rules) = declared.get_mut("at_rules").and_then(Value::as_array_mut) else {
        return Value::Object(declared);
    };
    for rule in rules {
        if let Some(obj) = rule.as_object_mut()
            && !obj.contains_key("condition_text")
            && let Some(text) = obj.remove("conditionText")
        {
            obj.insert("condition_text".into(), text);
        }
    }
    Value::Object(declared)
}

/// Load a snapshot JSONL file, returning one root node per matched element.
///
/// Both `jsonl` (tree) and `jsonl-flat` inputs are supported; `__meta`,
/// `__ax_tree` and `__actions` lines are ignored. The file must be
/// internally consistent (all tree lines or all flat lines).
pub fn load_file(path: &Path) -> DiffResult<Vec<DiffNode>> {
    let content = std::fs::read_to_string(path)?;
    load_str(&content)
}

/// Parse snapshot JSONL from an in-memory string (same rules as
/// [`load_file`], without touching disk).
pub fn load_str(content: &str) -> DiffResult<Vec<DiffNode>> {
    let mut roots: Vec<DiffNode> = Vec::new();
    let mut flat: Vec<FlatEntry> = Vec::new();
    let mut mode: Option<bool> = None;
    let mut style_defaults: Map<String, Value> = Map::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line)?;
        if v.get("__meta").is_some() {
            if let Some(meta) = v.get("__meta")
                && let Some(d) = meta.get("style_defaults").and_then(Value::as_object)
            {
                style_defaults.clone_from(d);
            }
            continue;
        }
        if v.get("__ax_tree").is_some()
            || v.get("__actions").is_some()
            || v.get("__document").is_some()
            || v.get("__env").is_some()
            || v.get("__declared").is_some()
        {
            continue;
        }
        let tree = v.get("children").is_some();
        match mode {
            None => mode = Some(tree),
            Some(m) if m != tree => {
                return Err(DiffError::MixedLayout);
            }
            _ => {}
        }
        let node = DiffNode::from_value(&v);
        if tree {
            roots.push(node);
        } else {
            let id = node.id;
            let parent = node.parent_id;
            flat.push((id, parent, node));
        }
    }

    if mode == Some(false) {
        let mut forest = build_forest(&flat);
        for root in &mut forest {
            apply_style_defaults(root, &style_defaults);
        }
        Ok(forest)
    } else {
        for root in &mut roots {
            apply_style_defaults(root, &style_defaults);
        }
        Ok(roots)
    }
}

/// Merge the compact `__meta.style_defaults` map back into a node's styles:
/// any `category.prop` that the emitter hoisted (omitted from the node) is
/// restored with its global default value, so the diff/check sees the full
/// effective styles. Node-specific values always win.
fn apply_style_defaults(node: &mut DiffNode, defaults: &Map<String, Value>) {
    if defaults.is_empty() {
        return;
    }
    if let Some(styles) = node.styles.as_mut() {
        for (category, cat_defaults) in defaults {
            if let Some(entry) = cat_defaults.as_object() {
                let cat = styles
                    .entry(category.clone())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Some(cat_map) = cat.as_object_mut() {
                    for (prop, val) in entry {
                        cat_map.entry(prop.clone()).or_insert_with(|| val.clone());
                    }
                }
            }
        }
    }
    for child in &mut node.children {
        apply_style_defaults(child, defaults);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(v: &Value) -> String {
        serde_json::to_string(v).unwrap()
    }

    #[test]
    fn load_tree_mode() {
        let dir = std::env::temp_dir();
        let path = dir.join("sniff_diff_tree_test.jsonl");
        let root: Value = serde_json::json!({
            "__meta": {"css_variables": {"--x": "1"}},
        });
        let node: Value = serde_json::json!({
            "id": 1, "tag": "DIV", "selector": "div.card",
            "path": "body > div.card", "depth": 0,
            "styles": {"box_model": {"width": "100px"}},
            "children": []
        });
        std::fs::write(&path, format!("{}\n{}\n", line(&root), line(&node))).unwrap();
        let forest = load_file(&path).unwrap();
        assert_eq!(forest.len(), 1);
        assert_eq!(forest[0].selector, "div.card");
        assert_eq!(forest[0].children.len(), 0);
        drop(std::fs::remove_file(&path));
    }

    #[test]
    fn load_flat_mode_builds_forest() {
        let dir = std::env::temp_dir();
        let path = dir.join("sniff_diff_flat_test.jsonl");
        let parent: Value = serde_json::json!({
            "id": 1, "parent_id": null, "tag": "DIV", "selector": "div.card",
            "depth": 0, "styles": {"layout": {"display": "block"}}
        });
        let child: Value = serde_json::json!({
            "id": 2, "parent_id": 1, "tag": "SPAN", "selector": "div.card > span",
            "depth": 1, "styles": {"typography": {"font-size": "16px"}}
        });
        std::fs::write(&path, format!("{}\n{}\n", line(&parent), line(&child))).unwrap();
        let forest = load_file(&path).unwrap();
        assert_eq!(forest.len(), 1);
        assert_eq!(forest[0].id, 1);
        assert_eq!(forest[0].children.len(), 1);
        assert_eq!(forest[0].children[0].id, 2);
        drop(std::fs::remove_file(&path));
    }

    #[test]
    fn load_orphan_parent_treated_as_root() {
        let dir = std::env::temp_dir();
        let path = dir.join("sniff_diff_orphan_test.jsonl");
        let orphan: Value = serde_json::json!({
            "id": 7, "parent_id": 999, "tag": "B", "selector": "b",
            "depth": 3, "styles": {}
        });
        std::fs::write(&path, line(&orphan)).unwrap();
        let forest = load_file(&path).unwrap();
        assert_eq!(forest.len(), 1);
        assert_eq!(forest[0].id, 7);
        drop(std::fs::remove_file(&path));
    }

    #[test]
    fn load_str_parses_inline_without_disk() {
        let nested: Value = serde_json::json!({
            "id": 1, "tag": "DIV", "selector": "div.a",
            "styles": {"layout": {"display": "block"}},
            "children": [
                {"id": 2, "tag": "SPAN", "selector": "div.a > span",
                 "styles": {}, "children": []}
            ]
        });
        let flat_root: Value = serde_json::json!({
            "id": 10, "tag": "DIV", "selector": "div.b",
            "styles": {}, "children": []
        });
        let content = format!("{}\n{}\n", line(&nested), line(&flat_root));
        let forest = load_str(&content).unwrap();
        assert_eq!(forest.len(), 2);
        assert_eq!(forest[0].children.len(), 1);
        assert_eq!(forest[0].children[0].selector, "div.a > span");
    }

    #[test]
    fn load_str_flat_builds_forest() {
        let root: Value = serde_json::json!({
            "id": 1, "parent_id": null, "tag": "DIV", "selector": "div.a",
            "styles": {"layout": {"display": "block"}}
        });
        let child: Value = serde_json::json!({
            "id": 2, "parent_id": 1, "tag": "SPAN", "selector": "div.a > span",
            "styles": {}
        });
        let content = format!("{}\n{}\n", line(&root), line(&child));
        let forest = load_str(&content).unwrap();
        assert_eq!(forest.len(), 1);
        assert_eq!(forest[0].children.len(), 1);
        assert_eq!(forest[0].children[0].id, 2);
    }

    #[test]
    fn load_str_merges_style_defaults_back_into_nodes() {
        let meta: Value = serde_json::json!({
            "__meta": {
                "css_variables": {"--x": "1"},
                "style_defaults": {"typography": {"font-size": "16px", "font-weight": "400"}}
            }
        });
        let node: Value = serde_json::json!({
            "id": 1, "tag": "DIV", "selector": "div.card", "children": [
                {"id": 2, "tag": "SPAN", "selector": "div.card > span",
                 "styles": {"typography": {"font-weight": "700"}, "visual": {"opacity": "1"}},
                 "children": []}
            ],
            "styles": {"typography": {"color": "#212529"}}
        });
        let content = format!("{}\n{}\n", line(&meta), line(&node));
        let forest = load_str(&content).unwrap();
        assert_eq!(forest.len(), 1);
        // Hoisted defaults are merged into the parent's styles.
        let parent = &forest[0].styles.as_ref().unwrap();
        assert_eq!(parent["typography"]["font-size"], "16px");
        assert_eq!(parent["typography"]["font-weight"], "400");
        assert_eq!(parent["typography"]["color"], "#212529");
        // A node-specific value wins over the default.
        let child = &forest[0].children[0].styles.as_ref().unwrap();
        assert_eq!(child["typography"]["font-weight"], "700");
        assert_eq!(child["typography"]["font-size"], "16px");
        assert_eq!(child["visual"]["opacity"], "1");
    }

    #[test]
    fn load_str_without_defaults_leaves_styles_untouched() {
        let node: Value = serde_json::json!({
            "id": 1, "tag": "DIV", "selector": "div.card",
            "styles": {"typography": {"color": "#212529"}}
        });
        let content = line(&node);
        let forest = load_str(&content).unwrap();
        let styles = forest[0].styles.as_ref().unwrap();
        assert_eq!(styles["typography"]["color"], "#212529");
        assert!(styles["typography"].get("font-size").is_none());
    }

    #[test]
    fn load_str_rejects_mixed_layout() {
        let tree: Value = serde_json::json!({
            "id": 1, "tag": "DIV", "selector": "div.a", "children": []
        });
        let flat: Value = serde_json::json!({
            "id": 2, "parent_id": 1, "tag": "SPAN", "selector": "span"
        });
        let content = format!("{}\n{}\n", line(&tree), line(&flat));
        assert!(matches!(load_str(&content), Err(DiffError::MixedLayout)));
    }

    #[test]
    fn doc_loading_collects_actions_and_skips_them_for_nodes() {
        let action_line = serde_json::json!({
            "__actions": [{"index": 0, "action": "click", "effect": "revealed"}]
        });
        let node: Value = serde_json::json!({
            "id": 1, "tag": "DIV", "selector": "div.card", "children": []
        });
        let content = format!("{}\n{}\n", line(&action_line), line(&node));
        let doc = load_str_doc(&content).unwrap();
        assert_eq!(doc.nodes.len(), 1, "nodes must ignore the __actions line");
        assert_eq!(doc.actions.len(), 1);
        assert_eq!(doc.actions[0]["effect"], "revealed");
        // Plain load_str also skips the __actions line.
        assert_eq!(load_str(&content).unwrap().len(), 1);
    }

    #[test]
    fn doc_loading_reads_viewport_from_meta() {
        let node: Value = serde_json::json!({
            "id": 1, "tag": "DIV", "selector": "div.card", "children": []
        });
        let meta = serde_json::json!({
            "__meta": {"viewport": {"width": 1280, "height": 720}}
        });
        let content = format!("{}\n{}\n", line(&meta), line(&node));
        let doc = load_str_doc(&content).unwrap();
        assert_eq!(doc.viewport, Some((1280.0, 720.0)));
        assert_eq!(doc.nodes.len(), 1);
    }

    #[test]
    fn doc_loading_viewport_absent_when_no_meta() {
        let node: Value = serde_json::json!({
            "id": 1, "tag": "DIV", "selector": "div.card", "children": []
        });
        let content = line(&node);
        let doc = load_str_doc(&content).unwrap();
        assert_eq!(doc.viewport, None);
    }
}
