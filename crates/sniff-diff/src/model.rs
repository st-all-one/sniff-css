//! Snapshot node model and JSONL loading.
//!
//! Parses the JSONL produced by `sniff-computed-style` into a lightweight
//! [`DiffNode`] tree, accepting both `jsonl` (tree: one root per line with
//! nested `children`) and `jsonl-flat` (one node per line with
//! `id`/`parent_id`) inputs. `__meta` lines (global css_variables) are
//! skipped.

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
    pub is_visible: Option<bool>,
    pub hash: Option<String>,
    pub styles: Option<Map<String, Value>>,
    pub pseudo: Option<Map<String, Value>>,
    pub aria: Option<Value>,
    pub contrast: Option<Value>,
    pub ax: Option<Value>,
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
            is_visible: v.get("is_visible").and_then(Value::as_bool),
            hash: v
                .get("computed_style_hash")
                .and_then(Value::as_str)
                .map(String::from),
            styles: v.get("styles").and_then(Value::as_object).cloned(),
            pseudo: v.get("pseudo").and_then(Value::as_object).cloned(),
            aria: v.get("aria").cloned(),
            contrast: v.get("contrast").cloned(),
            ax: v.get("ax").cloned(),
            children,
        }
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

/// Load a snapshot JSONL file, returning one root node per matched element.
///
/// Both `jsonl` (tree) and `jsonl-flat` inputs are supported; `__meta`
/// lines are ignored. The file must be internally consistent (all tree
/// lines or all flat lines).
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

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line)?;
        if v.get("__meta").is_some() || v.get("__ax_tree").is_some() {
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
        Ok(build_forest(&flat))
    } else {
        Ok(roots)
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
}
