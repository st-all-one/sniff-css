//! Derived rule checks ("lighthouse-lite"): deterministic PASS/WARN/FAIL
//! heuristics computed from the captured data — no LLM, no AX magic.
//!
//! Checks:
//! - `contrast-aa` / `contrast-aaa`: measured WCAG ratio vs. the threshold
//!   for the node's text size. Uses the engine's `contrast` facet (which
//!   resolves the effective background in-page); `UNKNOWN` (background
//!   image) surfaces as a `warn` for manual review.
//! - `target-size`: interactive controls below the WCAG 2.2 24x24 CSS px
//!   minimum.
//! - `focus-indicator`: focusable element with no visible focus signal
//!   (outline suppressed and no box-shadow).
//! - `hidden-focusable`: focusable but not exposed to assistive tech
//!   (`accessibility_grade == NONE`).
//! - `empty-alt-image`: non-decorative image with an empty `alt`.

use serde_json::Value;
use sniff_core::TriState;
use sniff_core::contrast::derive_contrast_values;
use sniff_css_diff::DiffNode;

/// Contrast data used by the rule, from either the engine's `contrast`
/// facet (preferred) or a fallback derivation from raw styles.
struct ContrastFacet {
    ratio: f64,
    aa: TriState,
    large: bool,
    foreground: String,
    background: String,
    unknown_reason: Option<String>,
}

/// Status of a single check line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleStatus {
    Pass,
    Warn,
    Fail,
}

/// One emitted check result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CheckLine {
    /// Check identifier, e.g. `contrast-aa`.
    pub check: String,
    /// Selector of the evaluated node.
    pub selector: String,
    /// Tag name, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    pub status: RuleStatus,
    /// Measured evidence backing the verdict.
    pub evidence: String,
}

/// Tags that typically render visible text.
const TEXT_TAGS: &[&str] = &[
    "P",
    "SPAN",
    "LABEL",
    "LI",
    "TD",
    "TH",
    "DT",
    "DD",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "A",
    "BUTTON",
    "Q",
    "BLOCKQUOTE",
    "PRE",
    "CODE",
    "SMALL",
    "STRONG",
    "EM",
    "B",
    "I",
    "U",
    "SUMMARY",
    "LEGEND",
    "CAPTION",
    "FIGCAPTION",
    "OPTION",
];

/// Interactive tags whose tap/click target must meet the 24x24 minimum.
const INTERACTIVE_TAGS: &[&str] = &["BUTTON", "INPUT", "SELECT", "TEXTAREA", "SUMMARY"];

/// Run all derived rule checks over a snapshot forest (recursively).
pub fn run_rules(nodes: &[DiffNode]) -> Vec<CheckLine> {
    let mut out = Vec::new();
    for node in nodes {
        check_node(node, &mut out);
        out.extend(run_rules(&node.children));
    }
    out
}

fn check_node(node: &DiffNode, out: &mut Vec<CheckLine>) {
    let tag = node.tag.as_deref().unwrap_or("");
    let visible = node.display_visible().unwrap_or(true);
    let focusable = node
        .aria
        .as_ref()
        .and_then(|a| a.get("focusable"))
        .and_then(Value::as_bool)
        .or_else(|| {
            node.ax
                .as_ref()
                .and_then(|a| a.get("focusable"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(false);

    // Text-bearing heuristic: renders direct text, has an accessible name,
    // or is a typical text tag.
    let has_text = node
        .aria
        .as_ref()
        .and_then(|a| a.get("has_text"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let has_name = node
        .aria
        .as_ref()
        .and_then(|a| a.get("name"))
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty());
    let is_text = has_name || has_text || TEXT_TAGS.contains(&tag);

    // --- Contrast (measured) ---
    if visible && is_text {
        // Prefer the engine's `contrast` facet (ratio composited over the
        // real effective background, resolved in-page). Fall back to
        // re-deriving from this node's own styles for snapshots without it.
        let info = node
            .contrast
            .as_ref()
            .and_then(|c| {
                let ratio = c.get("ratio").and_then(Value::as_f64)?;
                let aa = match c.get("aa").and_then(Value::as_str)? {
                    "pass" => TriState::Pass,
                    "fail" => TriState::Fail,
                    _ => TriState::Unknown,
                };
                Some(ContrastFacet {
                    ratio,
                    aa,
                    large: c.get("large").and_then(Value::as_bool).unwrap_or(false),
                    foreground: c
                        .get("foreground")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    background: c
                        .get("background")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    unknown_reason: c
                        .get("unknown_reason")
                        .and_then(Value::as_str)
                        .map(String::from),
                })
            })
            .or_else(|| {
                derive_contrast_values(
                    style_val(node, "visual", "color"),
                    style_val(node, "visual", "background-color"),
                    style_val(node, "visual", "background-image"),
                    style_val(node, "typography", "font-size"),
                    style_val(node, "typography", "font-weight"),
                )
                .map(|i| ContrastFacet {
                    ratio: i.ratio,
                    aa: i.aa,
                    large: i.large,
                    foreground: i.foreground,
                    background: i.background,
                    unknown_reason: i.unknown_reason,
                })
            });
        if let Some(info) = info {
            let large = if info.large { "large " } else { "" };
            match info.aa {
                TriState::Pass => out.push(CheckLine {
                    check: "contrast-aa".into(),
                    selector: node.selector.clone(),
                    tag: node.tag.clone(),
                    status: RuleStatus::Pass,
                    evidence: format!("ratio {}:1 (need 4.5:1 {large}text AA)", info.ratio),
                }),
                TriState::Fail => out.push(CheckLine {
                    check: "contrast-aa".into(),
                    selector: node.selector.clone(),
                    tag: node.tag.clone(),
                    status: RuleStatus::Fail,
                    evidence: format!(
                        "ratio {}:1 on {} against {} (need 4.5:1 {large}text AA)",
                        info.ratio, info.foreground, info.background
                    ),
                }),
                TriState::Unknown => out.push(CheckLine {
                    check: "contrast-aa".into(),
                    selector: node.selector.clone(),
                    tag: node.tag.clone(),
                    status: RuleStatus::Warn,
                    evidence: format!(
                        "contrast unmeasurable: {} (manual review)",
                        info.unknown_reason.as_deref().unwrap_or("unknown")
                    ),
                }),
            }
        }
    }

    // --- Target size (WCAG 2.2 min 24x24 CSS px) ---
    if visible
        && INTERACTIVE_TAGS.contains(&tag)
        && let Some((w, h)) = rect_size(node)
        && (w < 24.0 || h < 24.0)
    {
        out.push(CheckLine {
            check: "target-size".into(),
            selector: node.selector.clone(),
            tag: node.tag.clone(),
            status: RuleStatus::Fail,
            evidence: format!("interactive {tag} is {w:0.0}x{h:0.0}px (minimum 24x24)"),
        });
    }

    // --- Visible focus indicator ---
    if focusable && visible {
        let outline_width = style_val(node, "visual", "outline-width").unwrap_or("");
        let outline_style = style_val(node, "visual", "outline-style").unwrap_or("");
        let box_shadow = style_val(node, "visual", "box-shadow").unwrap_or("");
        let width_zero = outline_width == "0px" || outline_width == "0";
        let style_none = outline_style == "none";
        let no_shadow = box_shadow == "none" || box_shadow.is_empty();
        if width_zero && style_none && no_shadow {
            out.push(CheckLine {
                check: "focus-indicator".into(),
                selector: node.selector.clone(),
                tag: node.tag.clone(),
                status: RuleStatus::Warn,
                evidence: "focusable element suppresses the outline and has no box-shadow"
                    .to_string(),
            });
        }
    }

    // --- Hidden but focusable ---
    if focusable && node.accessibility_grade() == Some("NONE") {
        out.push(CheckLine {
            check: "hidden-focusable".into(),
            selector: node.selector.clone(),
            tag: node.tag.clone(),
            status: RuleStatus::Warn,
            evidence: "focusable element is not exposed to assistive tech (tab trap)".to_string(),
        });
    }

    // --- Empty alt on a non-decorative image ---
    if tag == "IMG" && visible {
        let alt = node
            .aria
            .as_ref()
            .and_then(|a| a.get("alt"))
            .and_then(Value::as_str);
        if let Some("") = alt
            && let Some((w, h)) = rect_size(node)
            && w > 32.0
            && h > 32.0
        {
            out.push(CheckLine {
                check: "empty-alt-image".into(),
                selector: node.selector.clone(),
                tag: node.tag.clone(),
                status: RuleStatus::Warn,
                evidence: format!(
                    "image {w:0.0}x{h:0.0}px has an empty alt (decorative? otherwise add a label)"
                ),
            });
        }
    }
}

/// Read a style value from a category group (e.g. `visual.color`).
fn style_val<'a>(node: &'a DiffNode, category: &str, prop: &str) -> Option<&'a str> {
    node.styles.as_ref()?.get(category)?.get(prop)?.as_str()
}

/// Bounding rect size of a node.
fn rect_size(node: &DiffNode) -> Option<(f64, f64)> {
    let rect = node.rect.as_ref()?;
    let width = rect.get("width")?.as_f64()?;
    let height = rect.get("height")?.as_f64()?;
    Some((width, height))
}

/// Serialize check lines as JSONL.
pub fn write_checks<W: std::io::Write>(writer: &mut W, lines: &[CheckLine]) -> std::io::Result<()> {
    for line in lines {
        serde_json::to_writer(&mut *writer, line)?;
        writeln!(writer)?;
    }
    writer.flush()?;
    Ok(())
}

/// Aggregate counters over check lines.
pub fn summarize(lines: &[CheckLine]) -> (usize, usize, usize) {
    let mut pass = 0;
    let mut warn = 0;
    let mut fail = 0;
    for line in lines {
        match line.status {
            RuleStatus::Pass => pass += 1,
            RuleStatus::Warn => warn += 1,
            RuleStatus::Fail => fail += 1,
        }
    }
    (pass, warn, fail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style_node(
        selector: &str,
        tag: &str,
        visual: serde_json::Value,
        typography: serde_json::Value,
    ) -> DiffNode {
        DiffNode {
            id: 0,
            parent_id: None,
            selector: selector.into(),
            tag: Some(tag.into()),
            path: Some(selector.into()),
            depth: Some(0),
            rect: Some(serde_json::json!({"x": 0, "y": 0, "width": 100, "height": 40})),
            metrics: None,
            noticeable: Some(serde_json::json!({
                "display_visible": true, "accessibility_grade": "AAA"
            })),
            hash: None,
            styles: Some(
                serde_json::json!({
                    "visual": visual,
                    "typography": typography,
                    "box_model": {"width": "100px", "height": "40px"}
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            pseudo: None,
            aria: Some(serde_json::json!({"focusable": true})),
            contrast: None,
            ax: None,
            children: vec![],
        }
    }

    fn text_node() -> DiffNode {
        style_node(
            "button.btn",
            "BUTTON",
            serde_json::json!({
                "color": "#2563eb",
                "background-color": "#ffffff",
                "outline-width": "0px",
                "outline-style": "none",
                "box-shadow": "none"
            }),
            serde_json::json!({"font-size": "16px", "font-weight": "400"}),
        )
    }

    #[test]
    fn contrast_pass_and_fail() {
        let pass = style_node(
            "p.a",
            "P",
            serde_json::json!({
                "color": "#2563eb",
                "background-color": "#ffffff",
                "background-image": "none"
            }),
            serde_json::json!({"font-size": "16px", "font-weight": "400"}),
        );
        let lines = run_rules(&[pass]);
        let aa = lines
            .iter()
            .find(|l| l.check == "contrast-aa")
            .expect("check");
        assert_eq!(aa.status, RuleStatus::Pass);

        let fail = style_node(
            "p.b",
            "P",
            serde_json::json!({
                "color": "#212529",
                "background-color": "#020842",
                "background-image": "none"
            }),
            serde_json::json!({"font-size": "16px", "font-weight": "400"}),
        );
        let lines = run_rules(&[fail]);
        let aa = lines
            .iter()
            .find(|l| l.check == "contrast-aa")
            .expect("check");
        assert_eq!(aa.status, RuleStatus::Fail);
        assert!(aa.evidence.contains("ratio"), "got: {}", aa.evidence);
    }

    #[test]
    fn transparent_background_warns() {
        let node = style_node(
            "p.t",
            "P",
            serde_json::json!({
                "color": "#000000",
                "background-color": "rgba(255, 255, 255, 0.5)",
                "background-image": "none"
            }),
            serde_json::json!({"font-size": "16px", "font-weight": "400"}),
        );
        let lines = run_rules(&[node]);
        let aa = lines
            .iter()
            .find(|l| l.check == "contrast-aa")
            .expect("check");
        assert_eq!(aa.status, RuleStatus::Warn);
        assert!(aa.evidence.contains("unmeasurable"));
    }

    #[test]
    fn small_target_fails() {
        let mut node = text_node();
        node.rect = Some(serde_json::json!({"x": 0, "y": 0, "width": 18, "height": 18}));
        let lines = run_rules(&[node]);
        let ts = lines
            .iter()
            .find(|l| l.check == "target-size")
            .expect("check");
        assert_eq!(ts.status, RuleStatus::Fail);
    }

    #[test]
    fn suppressed_focus_indicator_warns() {
        let node = text_node();
        let lines = run_rules(&[node]);
        let fi = lines
            .iter()
            .find(|l| l.check == "focus-indicator")
            .expect("check");
        assert_eq!(fi.status, RuleStatus::Warn);
    }

    #[test]
    fn focus_indicator_present_does_not_warn() {
        let node = style_node(
            "button.btn",
            "BUTTON",
            serde_json::json!({
                "color": "#2563eb",
                "background-color": "#ffffff",
                "outline-width": "2px",
                "outline-style": "solid",
                "box-shadow": "none"
            }),
            serde_json::json!({"font-size": "16px", "font-weight": "400"}),
        );
        let lines = run_rules(&[node]);
        assert!(
            !lines.iter().any(|l| l.check == "focus-indicator"),
            "outline present must not warn"
        );
    }

    #[test]
    fn hidden_focusable_warns() {
        let mut node = text_node();
        node.noticeable = Some(serde_json::json!({
            "display_visible": false, "accessibility_grade": "NONE"
        }));
        let lines = run_rules(&[node]);
        let hf = lines
            .iter()
            .find(|l| l.check == "hidden-focusable")
            .expect("check");
        assert_eq!(hf.status, RuleStatus::Warn);
    }

    #[test]
    fn large_empty_alt_image_warns() {
        let node = style_node(
            "img.photo",
            "IMG",
            serde_json::json!({"color": "#000000", "background-color": "#ffffff"}),
            serde_json::json!({"font-size": "16px", "font-weight": "400"}),
        );
        // img with empty alt and 200x100.
        let mut node = node;
        node.rect = Some(serde_json::json!({"x": 0, "y": 0, "width": 200, "height": 100}));
        node.aria = Some(serde_json::json!({"focusable": false, "alt": ""}));
        let lines = run_rules(&[node]);
        let img = lines
            .iter()
            .find(|l| l.check == "empty-alt-image")
            .expect("check");
        assert_eq!(img.status, RuleStatus::Warn);
    }

    #[test]
    fn summary_counts_statuses() {
        let (pass, warn, fail) = summarize(&[
            CheckLine {
                check: "a".into(),
                selector: "s".into(),
                tag: None,
                status: RuleStatus::Pass,
                evidence: String::new(),
            },
            CheckLine {
                check: "b".into(),
                selector: "s".into(),
                tag: None,
                status: RuleStatus::Warn,
                evidence: String::new(),
            },
            CheckLine {
                check: "c".into(),
                selector: "s".into(),
                tag: None,
                status: RuleStatus::Fail,
                evidence: String::new(),
            },
        ]);
        assert_eq!((pass, warn, fail), (1, 1, 1));
    }
}
