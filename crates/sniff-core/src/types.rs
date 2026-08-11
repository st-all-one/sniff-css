//! Data structures produced by a sniffing run.

use crate::properties::StyleCategory;
use serde::{Deserialize, Serialize};

/// A computed style value for a single CSS property.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputedProperty {
    pub name: String,
    pub value: String,
}

/// Computed styles grouped by semantic category.
///
/// Groups are kept in declaration order; within each group properties
/// are kept in the order they were requested.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComputedStyles {
    pub groups: Vec<(StyleCategory, Vec<ComputedProperty>)>,
}

impl ComputedStyles {
    /// Total number of properties across all groups.
    pub fn len(&self) -> usize {
        self.groups.iter().map(|(_, p)| p.len()).sum()
    }

    /// Whether no property was captured.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Look up a single property value across all groups.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.groups
            .iter()
            .flat_map(|(_, props)| props.iter())
            .find(|p| p.name == name)
            .map(|p| p.value.as_str())
    }
}

/// Bounding client rect of an element.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Derived layout metrics useful for AI analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementMetrics {
    pub z_index: String,
    pub stacking_context: bool,
}

/// Snapshot of a single element and its subtree (up to the requested depth).
#[derive(Debug, Clone, PartialEq)]
pub struct ElementSnapshot {
    /// Stable node id assigned during extraction (pre-order).
    pub id: u64,
    /// Id of the parent node, if any.
    pub parent_id: Option<u64>,
    /// Uppercase tag name, e.g. `DIV`.
    pub tag: String,
    /// A stable CSS selector that uniquely identifies this element.
    pub selector: String,
    /// DOM path from the root, e.g. `body > main > div.card`.
    pub path: String,
    /// Depth of this node relative to the matched root element.
    pub depth: usize,
    /// Bounding rect, when requested.
    pub rect: Option<Rect>,
    /// Derived metrics, when requested.
    pub metrics: Option<ElementMetrics>,
    /// Whether the element is rendered in layout, derived in-page from
    /// `display`, `visibility`, `opacity` and the bounding rect. `None`
    /// when visibility capture is disabled.
    pub is_visible: Option<bool>,
    /// Captured computed styles.
    pub styles: ComputedStyles,
    /// Styles captured for pseudo-elements (`::before`, ...).
    pub pseudo: Vec<PseudoStyles>,
    /// Child snapshots (empty when depth limit was reached).
    pub children: Vec<ElementSnapshot>,
}

/// Computed styles for a single pseudo-element.
#[derive(Debug, Clone, PartialEq)]
pub struct PseudoStyles {
    /// Pseudo-element name, e.g. `::before`.
    pub name: String,
    /// Captured styles.
    pub styles: ComputedStyles,
}

impl ElementSnapshot {
    /// Recursively count every node in the tree rooted at this snapshot.
    pub fn node_count(&self) -> usize {
        1 + self.children.iter().map(Self::node_count).sum::<usize>()
    }
}

/// Result of a sniffing run: one snapshot per matched root element.
pub type SniffResult = Vec<ElementSnapshot>;

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(name: &str, value: &str) -> ComputedProperty {
        ComputedProperty {
            name: name.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn computed_styles_len_and_get() {
        let styles = ComputedStyles {
            groups: vec![(
                StyleCategory::BoxModel,
                vec![prop("width", "100px"), prop("height", "50px")],
            )],
        };
        assert_eq!(styles.len(), 2);
        assert!(!styles.is_empty());
        assert_eq!(styles.get("width"), Some("100px"));
        assert_eq!(styles.get("color"), None);
    }

    #[test]
    fn empty_styles() {
        let styles = ComputedStyles::default();
        assert!(styles.is_empty());
        assert_eq!(styles.len(), 0);
    }

    #[test]
    fn snapshot_node_count_is_recursive() {
        let child = ElementSnapshot {
            id: 2,
            parent_id: Some(1),
            tag: "SPAN".into(),
            selector: ".a > span".into(),
            path: "body > div.a > span".into(),
            depth: 1,
            rect: None,
            metrics: None,
            is_visible: None,
            styles: ComputedStyles::default(),
            pseudo: vec![],
            children: vec![],
        };
        let root = ElementSnapshot {
            id: 1,
            parent_id: None,
            tag: "DIV".into(),
            selector: ".a".into(),
            path: "body > div.a".into(),
            depth: 0,
            rect: None,
            metrics: None,
            is_visible: None,
            styles: ComputedStyles::default(),
            pseudo: vec![],
            children: vec![child],
        };
        assert_eq!(root.node_count(), 2);
        assert_eq!(root.children[0].parent_id, Some(1));
    }
}
