//! Accessibility-tree capture via the CDP `Accessibility` domain.
//!
//! Chrome computes the real accessibility tree; we are one CDP call away
//! from the ground truth (roles, names, values, focusability, ignored
//! state). Two outputs are supported:
//!
//! - **Per-node facets**: a compact [`AxInfo`] per extracted node,
//!   resolved by matching each node's unique selector back to a DOM node.
//! - **Tree dump**: the full AX subtree rooted at each matched element,
//!   emitted as a JSON document (`__ax_tree`).
//!
//! Both are deterministic and diffable; no LLM is involved.

use serde_json::{Map, Value, json};
use sniff_cdp::session::CdpSession;
use sniff_core::types::{AxInfo, ElementSnapshot};
use sniff_core::{SniffError, SniffResult};
use std::collections::HashMap;

/// Result of an AX capture pass.
#[derive(Debug, Default)]
pub struct AxCapture {
    /// Snapshot node id -> compact AX facet (when facets were requested).
    pub facets: Option<HashMap<u64, AxInfo>>,
    /// Full AX subtree per matched root (when the tree was requested).
    pub tree: Option<Value>,
}

/// Capture accessibility information for a snapshot forest.
///
/// Each extracted node carries a unique selector; we resolve it to a DOM
/// node (`DOM.querySelectorAll`) and fetch its AX node
/// (`Accessibility.getPartialAXTree`). When the tree is requested, the
/// full AX tree is fetched once and subtrees are walked via `childIds`.
///
/// Failures are non-fatal: a missing/ignored AX node yields an empty
/// facet and the run continues.
pub async fn capture(
    session: &CdpSession,
    roots: &[ElementSnapshot],
    include_facets: bool,
    include_tree: bool,
) -> SniffResult<AxCapture> {
    if !include_facets && !include_tree {
        return Ok(AxCapture::default());
    }
    if session
        .call_no_params("Accessibility.enable")
        .await
        .is_err()
    {
        // The Accessibility domain is unavailable (unusual): degrade to empty.
        return Ok(AxCapture::default());
    }

    let mut full_tree: HashMap<String, Value> = HashMap::new();
    if include_tree {
        match session.call_no_params("Accessibility.getFullAXTree").await {
            Ok(value) => {
                if let Some(nodes) = value.get("nodes").and_then(Value::as_array) {
                    for n in nodes {
                        if let Some(id) = n.get("nodeId").and_then(Value::as_str) {
                            full_tree.insert(id.to_string(), n.clone());
                        }
                    }
                }
            }
            Err(_) => return Ok(AxCapture::default()),
        }
    }

    let doc = session
        .call("DOM.getDocument", json!({"depth": 0, "pierce": true}))
        .await
        .map_err(|e| SniffError::Cdp(e.to_string()))?;
    let root_node_id = doc
        .get("root")
        .and_then(|r| r.get("nodeId"))
        .and_then(Value::as_i64)
        .ok_or_else(|| SniffError::Other("DOM.getDocument missing root nodeId".into()))?;

    let mut facets = if include_facets {
        Some(HashMap::new())
    } else {
        None
    };
    let mut root_ax_ids: Vec<String> = Vec::new();

    // Root pass: resolve each matched root and record its AX node id.
    for root in roots {
        let Some(node_id) = query_node_id(session, root_node_id, &root.selector).await else {
            continue;
        };
        let Some(ax_node) = fetch_ax_node(session, node_id).await else {
            continue;
        };
        if include_facets && let Some(map) = facets.as_mut() {
            map.insert(root.id, parse_ax_facet(&ax_node));
        }
        if include_tree && let Some(id) = ax_node.get("nodeId").and_then(Value::as_str) {
            root_ax_ids.push(id.to_string());
        }
    }

    // Descendant pass: facets for every non-root node.
    if include_facets {
        let mut stack: Vec<&ElementSnapshot> =
            roots.iter().flat_map(|r| r.children.iter()).collect();
        while let Some(node) = stack.pop() {
            if let Some(node_id) = query_node_id(session, root_node_id, &node.selector).await
                && let Some(ax_node) = fetch_ax_node(session, node_id).await
                && let Some(map) = facets.as_mut()
            {
                map.insert(node.id, parse_ax_facet(&ax_node));
            }
            stack.extend(node.children.iter());
        }
    }

    let tree = if include_tree {
        let subtrees: Vec<Value> = root_ax_ids
            .iter()
            .filter_map(|id| build_subtree(id, &full_tree))
            .collect();
        if subtrees.is_empty() {
            None
        } else {
            Some(Value::Array(subtrees))
        }
    } else {
        None
    };

    Ok(AxCapture { facets, tree })
}

/// Resolve a unique selector to a DOM `nodeId` (best effort).
async fn query_node_id(session: &CdpSession, root_node_id: i64, selector: &str) -> Option<i64> {
    let value = session
        .call(
            "DOM.querySelectorAll",
            json!({"nodeId": root_node_id, "selector": selector}),
        )
        .await
        .ok()?;
    let ids = value.get("nodeIds")?.as_array()?;
    ids.first()?.as_i64()
}

/// Fetch the AX node for a DOM node (best effort).
async fn fetch_ax_node(session: &CdpSession, node_id: i64) -> Option<Value> {
    let value = session
        .call(
            "Accessibility.getPartialAXTree",
            json!({"nodeId": node_id, "fetchRelatives": false}),
        )
        .await
        .ok()?;
    let nodes = value.get("nodes")?.as_array()?;
    nodes.first().cloned()
}

/// Compact the full AX node JSON into an [`AxInfo`].
fn parse_ax_facet(node: &Value) -> AxInfo {
    AxInfo {
        role: ax_str(node, "role"),
        name: ax_str(node, "name"),
        value: ax_str(node, "value"),
        focusable: ax_bool(node, "focusable"),
        ignored: node
            .get("ignored")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        level: ax_int(node, "level"),
        expanded: ax_bool(node, "expanded"),
        checked: ax_str(node, "checked"),
        disabled: ax_bool(node, "disabled"),
    }
}

fn ax_str(node: &Value, field: &str) -> Option<String> {
    // Top-level value (role/name/value) or a property entry (checked).
    node.get(field)
        .and_then(|v| v.get("value"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| ax_prop_str(node, field))
}

fn ax_prop_str(node: &Value, name: &str) -> Option<String> {
    ax_prop(node, name)
        .and_then(|p| p.get("value"))
        .and_then(|v| v.get("value"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

fn ax_prop<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
    node.get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(Value::as_str) == Some(name))
}

fn ax_bool(node: &Value, name: &str) -> Option<bool> {
    ax_prop(node, name)
        .and_then(|p| p.get("value"))
        .and_then(|v| v.get("value"))
        .and_then(Value::as_bool)
}

fn ax_int(node: &Value, name: &str) -> Option<i64> {
    ax_prop(node, name)
        .and_then(|p| p.get("value"))
        .and_then(|v| v.get("value"))
        .and_then(Value::as_i64)
}

/// Build a compact AX subtree from a root AX node id, walking `childIds`.
fn build_subtree(ax_id: &str, full_tree: &HashMap<String, Value>) -> Option<Value> {
    let node = full_tree.get(ax_id)?;
    let children: Vec<Value> = node
        .get("childIds")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().and_then(|i| build_subtree(i, full_tree)))
                .collect()
        })
        .unwrap_or_default();

    let mut out = Map::new();
    if let Some(role) = ax_str(node, "role") {
        out.insert("role".into(), Value::String(role));
    }
    if let Some(name) = ax_str(node, "name") {
        out.insert("name".into(), Value::String(name));
    }
    if let Some(value) = ax_str(node, "value") {
        out.insert("value".into(), Value::String(value));
    }
    if let Some(focusable) = ax_bool(node, "focusable") {
        out.insert("focusable".into(), Value::Bool(focusable));
    }
    if let Some(disabled) = ax_bool(node, "disabled") {
        out.insert("disabled".into(), Value::Bool(disabled));
    }
    if let Some(level) = ax_int(node, "level") {
        out.insert("level".into(), Value::from(level));
    }
    if let Some(expanded) = ax_bool(node, "expanded") {
        out.insert("expanded".into(), Value::Bool(expanded));
    }
    if let Some(checked) = ax_str(node, "checked") {
        out.insert("checked".into(), Value::String(checked));
    }
    let ignored = node
        .get("ignored")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    out.insert("ignored".into(), Value::Bool(ignored));
    out.insert("children".into(), Value::Array(children));
    Some(Value::Object(out))
}

/// Attach AX facets to a snapshot forest by node id.
pub fn attach_ax(snaps: &mut [ElementSnapshot], facets: &HashMap<u64, AxInfo>) {
    for snap in snaps {
        snap.ax = facets.get(&snap.id).cloned();
        attach_ax(&mut snap.children, facets);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ax_node(role: &str, name: &str, props: &[(&str, Value)]) -> Value {
        json!({
            "nodeId": "n1",
            "ignored": false,
            "role": {"value": role},
            "name": {"value": name},
            "value": {"value": ""},
            "properties": props.iter().map(|(k, v)| json!({"name": k, "value": {"value": v}})).collect::<Vec<_>>(),
            "childIds": [],
            "backendDOMNodeId": 42
        })
    }

    #[test]
    fn facet_extracts_role_name_and_properties() {
        let node = ax_node(
            "button",
            "Confirmar",
            &[
                ("focusable", Value::Bool(true)),
                ("disabled", Value::Bool(false)),
                ("level", Value::from(2)),
                ("expanded", Value::Bool(true)),
                ("checked", Value::String("false".into())),
            ],
        );
        let facet = parse_ax_facet(&node);
        assert_eq!(facet.role.as_deref(), Some("button"));
        assert_eq!(facet.name.as_deref(), Some("Confirmar"));
        assert_eq!(facet.focusable, Some(true));
        assert_eq!(facet.level, Some(2));
        assert_eq!(facet.expanded, Some(true));
        assert_eq!(facet.checked.as_deref(), Some("false"));
        assert!(!facet.ignored);
    }

    #[test]
    fn facet_missing_properties_are_none() {
        let node = ax_node("generic", "", &[]);
        let facet = parse_ax_facet(&node);
        assert_eq!(facet.focusable, None);
        assert_eq!(facet.level, None);
        assert_eq!(facet.name, None);
    }

    #[test]
    fn subtree_walks_child_ids() {
        let mut tree: HashMap<String, Value> = HashMap::new();
        tree.insert(
            "a".into(),
            json!({
                "nodeId": "a", "ignored": false,
                "role": {"value": "group"}, "name": {"value": "Root"},
                "value": {"value": ""},
                "properties": [], "childIds": ["b", "c"], "backendDOMNodeId": 1
            }),
        );
        tree.insert(
            "b".into(),
            json!({
                "nodeId": "b", "ignored": false,
                "role": {"value": "heading"}, "name": {"value": "Título"},
                "value": {"value": ""}, "properties": [{"name":"level","value":{"value":2}}],
                "childIds": [], "backendDOMNodeId": 2
            }),
        );
        tree.insert(
            "c".into(),
            json!({
                "nodeId": "c", "ignored": true,
                "role": {"value": "generic"}, "name": {"value": ""},
                "value": {"value": ""}, "properties": [], "childIds": [], "backendDOMNodeId": 3
            }),
        );
        let sub = build_subtree("a", &tree).expect("subtree");
        assert_eq!(sub["role"], "group");
        assert_eq!(sub["children"].as_array().unwrap().len(), 2);
        assert_eq!(sub["children"][0]["role"], "heading");
        assert_eq!(sub["children"][0]["level"], 2);
        assert_eq!(sub["children"][1]["ignored"], true);
    }

    #[test]
    fn attach_ax_maps_by_id_recursively() {
        let mut facets = HashMap::new();
        facets.insert(
            1u64,
            AxInfo {
                role: Some("group".into()),
                ..Default::default()
            },
        );
        facets.insert(
            2u64,
            AxInfo {
                role: Some("button".into()),
                ..Default::default()
            },
        );
        let mut roots = vec![ElementSnapshot {
            id: 1,
            parent_id: None,
            tag: "DIV".into(),
            selector: ".a".into(),
            path: ".a".into(),
            depth: 0,
            rect: None,
            metrics: None,
            noticeable: None,
            aria: None,
            effective_background: None,
            contrast: None,
            ax: None,
            styles: Default::default(),
            pseudo: vec![],
            children: vec![ElementSnapshot {
                id: 2,
                parent_id: Some(1),
                tag: "BUTTON".into(),
                selector: ".a > button".into(),
                path: ".a > button".into(),
                depth: 1,
                rect: None,
                metrics: None,
                noticeable: None,
                aria: None,
                effective_background: None,
                contrast: None,
                ax: None,
                styles: Default::default(),
                pseudo: vec![],
                children: vec![],
            }],
        }];
        attach_ax(&mut roots, &facets);
        assert_eq!(
            roots[0].ax.as_ref().and_then(|a| a.role.as_deref()),
            Some("group")
        );
        assert_eq!(
            roots[0].children[0]
                .ax
                .as_ref()
                .and_then(|a| a.role.as_deref()),
            Some("button")
        );
    }
}
