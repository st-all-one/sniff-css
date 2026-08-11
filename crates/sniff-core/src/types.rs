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
    /// Whether and how noticeably the element is exposed to the user
    /// (visual and assistive-tech perceptibility), derived in-page from
    /// `display`, `visibility`, `opacity`, the bounding rect and the
    /// resolved `aria` facet. `None` when noticeability capture is
    /// disabled.
    pub noticeable: Option<Noticeability>,
    /// Resolved accessibility attributes (role, accessible name, focus
    /// state), computed in-page. `None` when aria capture is disabled.
    pub aria: Option<AriaInfo>,
    /// Effective background painted behind the element, composited in-page
    /// over its ancestors up to the page canvas (used by the contrast
    /// derivation). `Some("#rrggbb")` for a solid color, `Some("image")`
    /// when a background image is involved, `None` when not captured or
    /// unresolvable.
    pub effective_background: Option<String>,
    /// Measured WCAG contrast of text against a solid background,
    /// derived in Rust from the captured colors. `None` when contrast
    /// capture is disabled.
    pub contrast: Option<ContrastInfo>,
    /// The browser-computed accessibility-tree node for this element,
    /// captured via the CDP `Accessibility` domain. `None` when AX
    /// capture is disabled.
    pub ax: Option<AxInfo>,
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

/// Resolved accessibility attributes of an element, computed deterministically
/// in-page (no AX tree required).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AriaInfo {
    /// Effective role: the explicit `role` attribute when present,
    /// otherwise inferred from the element's tag.
    pub role: Option<String>,
    /// Accessible name: `aria-labelledby` (resolved) -> `aria-label` ->
    /// `alt` -> `title` -> `placeholder` -> text content.
    pub name: Option<String>,
    /// Whether the element can receive focus (tabindex or interactive tag).
    pub focusable: bool,
    /// Present only when the attribute is set and non-empty.
    pub aria_hidden: Option<String>,
    pub aria_expanded: Option<String>,
    pub aria_checked: Option<String>,
    pub aria_controls: Option<String>,
    pub aria_labelledby: Option<String>,
    pub aria_describedby: Option<String>,
    pub lang: Option<String>,
    pub alt: Option<String>,
    pub title: Option<String>,
    /// `hidden` HTML attribute present.
    pub html_hidden: bool,
    /// `disabled` DOM property or `aria-disabled="true"`.
    pub disabled: bool,
    /// Whether the element renders visible text (a non-whitespace text
    /// node among its direct children) — the deterministic signal used by
    /// the derived contrast checks.
    pub has_text: bool,
}

/// The browser-computed accessibility-tree node for one element, captured via
/// the CDP `Accessibility` domain. `None` fields mean the value was absent or
/// `ignored`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AxInfo {
    pub role: Option<String>,
    pub name: Option<String>,
    pub value: Option<String>,
    pub focusable: Option<bool>,
    pub ignored: bool,
    pub level: Option<i64>,
    pub expanded: Option<bool>,
    pub checked: Option<String>,
    pub disabled: Option<bool>,
}

/// Result of a tri-state evaluation (e.g. contrast compliance).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TriState {
    Pass,
    Fail,
    Unknown,
}

/// Accessibility grade of an element's exposure to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AccessibilityGrade {
    /// Not perceivable at all: hidden from assistive tech and/or not
    /// rendered (`display:none`, `visibility:hidden`, `aria-hidden`,
    /// `hidden`/`inert`, zero-size).
    None,
    /// Perceivable but not fully: present in the accessibility tree yet
    /// off-screen (below the fold), fully transparent, or missing a
    /// required accessible name.
    Aa,
    /// Fully perceivable: rendered on screen, exposed to assistive tech
    /// and named where the role requires a name.
    Aaa,
}

/// How noticeably an element reaches the user, split into the visual
/// ("is it actually displayed?") and the accessibility ("how accessible
/// is it?") axes. Replaces the old single `is_visible` boolean, which
/// conflated off-screen-but-navigable content with truly hidden content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Noticeability {
    /// The element is rendered in layout: `display` is not `none`,
    /// `visibility` is not `hidden`/`collapse`, `opacity` is non-zero and
    /// it has a non-zero bounding rect. Independent of the viewport —
    /// scrolled-out content is still *displayed*.
    pub display_visible: bool,
    /// Perceptibility grade (see [`AccessibilityGrade`]).
    pub accessibility_grade: AccessibilityGrade,
}

/// Measured WCAG contrast of a node, derived in Rust from the captured
/// `color`, `background-color`, `font-size` and `font-weight`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContrastInfo {
    /// Contrast ratio (1.0..=21.0). `0.0` when the value is `unknown`.
    pub ratio: f64,
    /// Foreground color as captured.
    pub foreground: String,
    /// Background color as captured.
    pub background: String,
    /// Whether WCAG "large text" thresholds apply (>=24px, or >=18.66px bold).
    pub large: bool,
    /// AA compliance for the node's actual text size.
    pub aa: TriState,
    /// AAA compliance for the node's actual text size.
    pub aaa: TriState,
    /// Why the value could not be measured, when `unknown`.
    pub unknown_reason: Option<String>,
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
            noticeable: None,
            aria: None,
            effective_background: None,
            contrast: None,
            ax: None,
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
            noticeable: None,
            aria: None,
            effective_background: None,
            contrast: None,
            ax: None,
            styles: ComputedStyles::default(),
            pseudo: vec![],
            children: vec![child],
        };
        assert_eq!(root.node_count(), 2);
        assert_eq!(root.children[0].parent_id, Some(1));
    }
}
