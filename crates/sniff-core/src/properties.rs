//! CSS property catalog organized by semantic category.
//!
//! Each category maps to a set of CSS property names that the browser
//! returns from `getComputedStyle`. The catalog is used to request only
//! the properties that matter, avoiding the cost of serializing every
//! computed property for every element.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Semantic category of CSS properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StyleCategory {
    /// width/height, margin, padding, border, box-sizing, overflow.
    BoxModel,
    /// display, position, flex, grid, z-index, contain, multi-column.
    Layout,
    /// font-*, text-*, line-height, letter-spacing, white-space.
    Typography,
    /// color, background, box-shadow, filter, clip-path, outline.
    Visual,
    /// transform, perspective, offset-*, translate/rotate/scale.
    Transform,
    /// animation-*, transition-*, timeline properties.
    Animation,
    /// cursor, scroll-*, overscroll, user-select, touch-action.
    Interaction,
    /// forced-color-adjust, color-scheme, print-color-adjust, content.
    Accessibility,
    /// User-supplied custom properties (`--props`).
    Custom,
    /// All CSS custom properties (`--*`) reported by the browser.
    Variables,
    /// Sentinel: request every property the browser reports.
    All,
}

impl StyleCategory {
    /// Short JSON key used in the output stream.
    pub const fn key(self) -> &'static str {
        match self {
            Self::BoxModel => "box_model",
            Self::Layout => "layout",
            Self::Typography => "typography",
            Self::Visual => "visual",
            Self::Transform => "transform",
            Self::Animation => "animation",
            Self::Interaction => "interaction",
            Self::Accessibility => "accessibility",
            Self::Custom => "custom",
            Self::Variables => "css_variables",
            Self::All => "all",
        }
    }

    /// Human-friendly name used by the CLI (`--categories box-model`).
    pub const fn cli_name(self) -> &'static str {
        match self {
            Self::BoxModel => "box-model",
            Self::Layout => "layout",
            Self::Typography => "typography",
            Self::Visual => "visual",
            Self::Transform => "transform",
            Self::Animation => "animation",
            Self::Interaction => "interaction",
            Self::Accessibility => "accessibility",
            Self::Custom => "custom",
            Self::Variables => "variables",
            Self::All => "all",
        }
    }

    /// All categories in declaration order (without the `All` sentinel).
    pub const fn all() -> [Self; 8] {
        [
            Self::BoxModel,
            Self::Layout,
            Self::Typography,
            Self::Visual,
            Self::Transform,
            Self::Animation,
            Self::Interaction,
            Self::Accessibility,
        ]
    }

    /// The CSS properties belonging to this category.
    ///
    /// `All` returns `&[]`; callers resolve it to the complete set at
    /// runtime (via [`properties_for`]) since the browser's computed
    /// style enumeration is only known at runtime.
    pub fn properties(self) -> &'static [&'static str] {
        match self {
            Self::BoxModel => BOX_MODEL,
            Self::Layout => LAYOUT,
            Self::Typography => TYPOGRAPHY,
            Self::Visual => VISUAL,
            Self::Transform => TRANSFORM,
            Self::Animation => ANIMATION,
            Self::Interaction => INTERACTION,
            Self::Accessibility => ACCESSIBILITY,
            Self::Custom | Self::Variables | Self::All => &[],
        }
    }
}

impl fmt::Display for StyleCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.cli_name())
    }
}

/// Parse a category from its CLI name (`box-model`) or JSON key
/// (`box_model`). Returns `None` for unknown names.
pub fn parse_category(input: &str) -> Option<StyleCategory> {
    let input = input.trim();
    for cat in StyleCategory::all() {
        if cat.cli_name() == input || cat.key() == input {
            return Some(cat);
        }
    }
    if matches!(input, "all" | "every") {
        return Some(StyleCategory::All);
    }
    None
}

/// Resolve a list of categories into the concrete set of property
/// names. The `All` sentinel is replaced by every property known to
/// the catalog (union of all categories).
pub fn properties_for(categories: &[StyleCategory], custom: &[String]) -> Vec<String> {
    let mut seen: Vec<&str> = Vec::with_capacity(256);
    let has_all = categories.contains(&StyleCategory::All);

    for cat in categories {
        if !has_all {
            seen.extend(cat.properties().iter().copied());
        }
    }
    if has_all {
        for cat in StyleCategory::all() {
            seen.extend(cat.properties().iter().copied());
        }
    }

    // De-duplicate preserving order.
    let mut out: Vec<String> = Vec::with_capacity(seen.len() + custom.len());
    let mut used = std::collections::HashSet::with_capacity(seen.len());
    for name in seen {
        if used.insert(name) {
            out.push(name.to_string());
        }
    }
    for name in custom {
        let name = name.trim();
        if !name.is_empty() && used.insert(name) {
            out.push(name.to_string());
        }
    }
    out
}

const BOX_MODEL: &[&str] = &[
    "width",
    "min-width",
    "max-width",
    "height",
    "min-height",
    "max-height",
    "block-size",
    "min-block-size",
    "max-block-size",
    "inline-size",
    "min-inline-size",
    "max-inline-size",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "margin-block-start",
    "margin-block-end",
    "margin-inline-start",
    "margin-inline-end",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "padding-block-start",
    "padding-block-end",
    "padding-inline-start",
    "padding-inline-end",
    "border-top-width",
    "border-right-width",
    "border-bottom-width",
    "border-left-width",
    "border-block-start-width",
    "border-block-end-width",
    "border-inline-start-width",
    "border-inline-end-width",
    "border-top-style",
    "border-right-style",
    "border-bottom-style",
    "border-left-style",
    "border-block-start-style",
    "border-block-end-style",
    "border-inline-start-style",
    "border-inline-end-style",
    "border-top-color",
    "border-right-color",
    "border-bottom-color",
    "border-left-color",
    "border-block-start-color",
    "border-block-end-color",
    "border-inline-start-color",
    "border-inline-end-color",
    "box-sizing",
    "aspect-ratio",
    "overflow-x",
    "overflow-y",
    "overflow-block",
    "overflow-inline",
    "overflow-anchor",
    "overflow-clip-margin",
];

const LAYOUT: &[&str] = &[
    "display",
    "position",
    "inset-block-start",
    "inset-block-end",
    "inset-inline-start",
    "inset-inline-end",
    "top",
    "right",
    "bottom",
    "left",
    "float",
    "clear",
    "z-index",
    "flex-direction",
    "flex-wrap",
    "justify-content",
    "align-items",
    "align-content",
    "gap",
    "row-gap",
    "column-gap",
    "flex-grow",
    "flex-shrink",
    "flex-basis",
    "align-self",
    "justify-self",
    "order",
    "place-content",
    "place-items",
    "place-self",
    "grid-template-columns",
    "grid-template-rows",
    "grid-template-areas",
    "grid-auto-columns",
    "grid-auto-rows",
    "grid-auto-flow",
    "grid-column-start",
    "grid-column-end",
    "grid-row-start",
    "grid-row-end",
    "grid-area",
    "grid-column-gap",
    "grid-row-gap",
    "contain",
    "content-visibility",
    "contain-intrinsic-size",
    "contain-intrinsic-width",
    "contain-intrinsic-height",
    "columns",
    "column-width",
    "column-count",
    "column-rule-width",
    "column-rule-style",
    "column-rule-color",
    "column-span",
    "column-fill",
    "break-before",
    "break-after",
    "break-inside",
    "box-decoration-break",
];

const TYPOGRAPHY: &[&str] = &[
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "font-stretch",
    "font-optical-sizing",
    "font-variation-settings",
    "font-kerning",
    "font-feature-settings",
    "font-variant-ligatures",
    "font-variant-numeric",
    "font-variant-east-asian",
    "font-variant-alternates",
    "font-variant-position",
    "font-variant-caps",
    "font-variant-emoji",
    "font-size-adjust",
    "font-synthesis-weight",
    "font-synthesis-style",
    "font-synthesis-small-caps",
    "font-synthesis-position",
    "line-height",
    "letter-spacing",
    "word-spacing",
    "text-align",
    "text-align-last",
    "text-indent",
    "text-transform",
    "text-decoration-line",
    "text-decoration-style",
    "text-decoration-color",
    "text-decoration-thickness",
    "text-underline-offset",
    "text-underline-position",
    "text-emphasis-style",
    "text-emphasis-color",
    "text-emphasis-position",
    "text-shadow",
    "text-wrap",
    "text-wrap-mode",
    "text-wrap-style",
    "white-space",
    "white-space-collapse",
    "word-break",
    "overflow-wrap",
    "hyphens",
    "hyphenate-character",
    "hyphenate-limit-chars",
    "vertical-align",
    "writing-mode",
    "direction",
    "unicode-bidi",
    "tab-size",
    "line-break",
    "text-justify",
    "text-orientation",
    "text-combine-upright",
    "text-overflow",
    "text-size-adjust",
    "hanging-punctuation",
];

const VISUAL: &[&str] = &[
    "color",
    "opacity",
    "visibility",
    "background-color",
    "background-image",
    "background-size",
    "background-position-x",
    "background-position-y",
    "background-repeat",
    "background-attachment",
    "background-origin",
    "background-clip",
    "background-blend-mode",
    "box-shadow",
    "border-top-left-radius",
    "border-top-right-radius",
    "border-bottom-right-radius",
    "border-bottom-left-radius",
    "border-start-start-radius",
    "border-start-end-radius",
    "border-end-start-radius",
    "border-end-end-radius",
    "border-image-source",
    "border-image-slice",
    "border-image-width",
    "border-image-outset",
    "border-image-repeat",
    "outline-width",
    "outline-style",
    "outline-color",
    "outline-offset",
    "filter",
    "backdrop-filter",
    "mix-blend-mode",
    "isolation",
    "mask-image",
    "mask-mode",
    "mask-size",
    "mask-position",
    "mask-repeat",
    "mask-origin",
    "mask-clip",
    "mask-composite",
    "clip-path",
    "clip-rule",
    "image-rendering",
    "object-fit",
    "object-position",
    "paint-order",
    "shape-outside",
    "shape-margin",
    "shape-image-threshold",
    "image-orientation",
];

const TRANSFORM: &[&str] = &[
    "transform",
    "transform-origin",
    "transform-style",
    "transform-box",
    "perspective",
    "perspective-origin",
    "backface-visibility",
    "translate",
    "rotate",
    "scale",
    "offset-path",
    "offset-distance",
    "offset-rotate",
    "offset-anchor",
    "offset-position",
];

const ANIMATION: &[&str] = &[
    "animation-name",
    "animation-duration",
    "animation-timing-function",
    "animation-delay",
    "animation-iteration-count",
    "animation-direction",
    "animation-fill-mode",
    "animation-play-state",
    "animation-composition",
    "animation-timeline",
    "animation-range-start",
    "animation-range-end",
    "transition-property",
    "transition-duration",
    "transition-timing-function",
    "transition-delay",
    "transition-behavior",
    "scroll-timeline-name",
    "scroll-timeline-axis",
    "view-timeline-name",
    "view-timeline-axis",
    "view-timeline-inset",
    "view-transition-name",
];

const INTERACTION: &[&str] = &[
    "cursor",
    "pointer-events",
    "user-select",
    "caret-color",
    "scroll-behavior",
    "scroll-snap-type",
    "scroll-snap-align",
    "scroll-snap-stop",
    "scroll-margin-top",
    "scroll-margin-right",
    "scroll-margin-bottom",
    "scroll-margin-left",
    "scroll-margin-block-start",
    "scroll-margin-block-end",
    "scroll-margin-inline-start",
    "scroll-margin-inline-end",
    "scroll-padding-top",
    "scroll-padding-right",
    "scroll-padding-bottom",
    "scroll-padding-left",
    "scroll-padding-block-start",
    "scroll-padding-block-end",
    "scroll-padding-inline-start",
    "scroll-padding-inline-end",
    "scrollbar-width",
    "scrollbar-color",
    "scrollbar-gutter",
    "overscroll-behavior-x",
    "overscroll-behavior-y",
    "overscroll-behavior-block",
    "overscroll-behavior-inline",
    "touch-action",
    "resize",
    "accent-color",
    "appearance",
];

const ACCESSIBILITY: &[&str] = &[
    "forced-color-adjust",
    "color-scheme",
    "print-color-adjust",
    "content",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_have_unique_properties() {
        let mut seen = std::collections::HashSet::new();
        for cat in StyleCategory::all() {
            for prop in cat.properties() {
                assert!(seen.insert(*prop), "duplicate property: {prop}");
            }
        }
    }

    #[test]
    fn every_category_has_a_key_and_cli_name() {
        for cat in StyleCategory::all() {
            assert!(!cat.key().is_empty());
            assert!(!cat.cli_name().is_empty());
        }
        assert_eq!(StyleCategory::BoxModel.key(), "box_model");
        assert_eq!(StyleCategory::BoxModel.cli_name(), "box-model");
    }

    #[test]
    fn parse_category_accepts_both_spellings() {
        assert_eq!(parse_category("box-model"), Some(StyleCategory::BoxModel));
        assert_eq!(parse_category("box_model"), Some(StyleCategory::BoxModel));
        assert_eq!(parse_category("all"), Some(StyleCategory::All));
        assert_eq!(parse_category("nope"), None);
    }

    #[test]
    fn properties_for_union_dedupes_and_appends_custom() {
        let props = properties_for(
            &[StyleCategory::BoxModel, StyleCategory::BoxModel],
            &["custom-x".to_string()],
        );
        assert!(props.contains(&"width".to_string()));
        assert_eq!(props.last().map(String::as_str), Some("custom-x"));
    }

    #[test]
    fn properties_for_all_expands_everything() {
        let props = properties_for(&[StyleCategory::All], &[]);
        let total: usize = StyleCategory::all()
            .iter()
            .map(|c| c.properties().len())
            .sum();
        assert_eq!(props.len(), total);
        assert!(props.contains(&"animation-name".to_string()));
    }
}
