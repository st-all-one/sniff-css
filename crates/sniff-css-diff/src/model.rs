//! Snapshot node model and JSONL loading.
//!
//! Parses the JSONL produced by `sniffCSS` into a lightweight
//! [`DiffNode`] tree, accepting both `jsonl` (tree: one root per line with
//! nested `children`) and `jsonl-flat` (one node per line with
//! `id`/`parent_id`) inputs. `__meta` lines (global css_variables and the
//! compact `style_defaults` hoist map) are parsed; `style_defaults` are
//! merged back into every node's `styles` so the diff always sees the full
//! effective values (lossless reconstruction of the compact dedup).

use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;

use crate::error::{DiffError, DiffResult};

/// A single captured element, in a form convenient for structural diffing.
#[derive(Debug, Clone)]
pub struct DiffNode {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub selector: String,
    pub tag: Option<String>,
    pub path: Option<String>,
    pub depth: Option<usize>,
    pub rect: Option<Value>,
    pub metrics: Option<Value>,
    /// `{"display_visible": bool, "accessibility_grade": "NONE"|"AA"|"AAA"}`.
    pub noticeable: Option<Value>,
    pub hash: Option<String>,
    pub styles: Option<Map<String, Value>>,
    pub pseudo: Option<Map<String, Value>>,
    pub aria: Option<Value>,
    pub contrast: Option<Value>,
    pub ax: Option<Value>,
    pub attributes: Option<Value>,
    pub children: Vec<DiffNode>,
}

impl DiffNode {
    fn from_value(v: &Value) -> Self {
        let children = v
            .get("children")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(DiffNode::from_value).collect())
            .unwrap_or_default();
        DiffNode {
            id: v.get("id").and_then(Value::as_u64).unwrap_or(0),
            parent_id: v.get("parent_id").and_then(Value::as_u64),
            selector: v
                .get("selector")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tag: v.get("tag").and_then(Value::as_str).map(String::from),
            path: v.get("path").and_then(Value::as_str).map(String::from),
            depth: v.get("depth").and_then(Value::as_u64).map(|d| d as usize),
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
fn build_forest(flat: &[(u64, Option<u64>, DiffNode)]) -> Vec<DiffNode> {
    if flat.is_empty() {
        return Vec::new();
    }
    let by_id: HashMap<u64, usize> = flat
        .iter()
        .enumerate()
        .map(|(i, (id, _, _))| (*id, i))
        .collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); flat.len()];
    let mut roots: Vec<usize> = Vec::new();
    for (i, (_, parent, _)) in flat.iter().enumerate() {
        match parent {
            Some(p) => match by_id.get(p) {
                Some(&pi) => children[pi].push(i),
                // Orphan parent: treat as a root so no node is lost.
                None => roots.push(i),
            },
            None => roots.push(i),
        }
    }

    fn build(i: usize, flat: &[(u64, Option<u64>, DiffNode)], children: &[Vec<usize>]) -> DiffNode {
        let mut node = flat[i].2.clone();
        node.children = children[i]
            .iter()
            .map(|&c| build(c, flat, children))
            .collect();
        node
    }

    roots.sort_unstable();
    roots.iter().map(|&i| build(i, flat, &children)).collect()
}

/// A parsed snapshot: the node forest plus any `__actions` UI-effect map
/// (per-interaction before/after reports) that the capture carried.
#[derive(Debug, Clone, Default)]
pub struct DiffDocument {
    pub nodes: Vec<DiffNode>,
    pub actions: Vec<Value>,
}

/// Load a snapshot JSONL file into a [`DiffDocument`].
pub fn load_file_doc(path: &Path) -> DiffResult<DiffDocument> {
    let content = std::fs::read_to_string(path)?;
    load_str_doc(&content)
}

/// Parse snapshot JSONL from an in-memory string into a [`DiffDocument`].
pub fn load_str_doc(content: &str) -> DiffResult<DiffDocument> {
    let nodes = load_str(content)?;
    let actions = extract_actions(content);
    Ok(DiffDocument { nodes, actions })
}

/// Collect the `__actions` UI-effect reports embedded in the JSONL.
fn extract_actions(content: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(arr) = v.get("__actions").and_then(Value::as_array) {
            out.extend(arr.iter().cloned());
        }
    }
    out
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
    let mut flat: Vec<(u64, Option<u64>, DiffNode)> = Vec::new();
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
                style_defaults = d.clone();
            }
            continue;
        }
        if v.get("__ax_tree").is_some() || v.get("__actions").is_some() {
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
        let _ = std::fs::remove_file(&path);
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
        let _ = std::fs::remove_file(&path);
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
        let _ = std::fs::remove_file(&path);
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
}
