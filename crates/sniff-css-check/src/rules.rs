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
//! - `occluded`: an element's rect is substantially covered by another
//!   (non-ancestor, non-descendant) element painted above it — the element
//!   is visually *behind* an overlapping element. Paint order is a
//!   deterministic heuristic over the captured tree: higher `z-index`
//!   (from `metrics`) wins, then later DOM order. Approximates the CSS
//!   stacking order within the captured subtree; an element outside the
//!   capture depth can still occlude unseen.

use serde_json::Value;
use sniff_core::TriState;
use sniff_core::contrast::derive_contrast_values;
use sniff_css_diff::DiffNode;
use std::collections::HashMap;

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
///
/// Per-node checks run over every node; the `occluded` check runs once over
/// the whole forest so cross-subtree coverage (e.g. a page-level overlay over
/// a deeply nested button) is reported exactly once.
pub fn run_rules(nodes: &[DiffNode]) -> Vec<CheckLine> {
    let mut out = Vec::new();
    for node in nodes {
        check_node(node, &mut out);
        out.extend(run_rules(&node.children));
    }
    out.extend(check_occlusion(nodes));
    out
}

/// A flattened node with pre-order index and subtree range, used by the
/// occlusion sweep for O(1) ancestry tests and DOM paint order.
struct Flat<'a> {
    node: &'a DiffNode,
    /// Pre-order index (document order for equal z-index tie-breaks).
    idx: usize,
    /// Last pre-order index in this node's subtree.
    end: usize,
    rect: Option<(f64, f64, f64, f64)>,
}

/// Detect occlusion: a node whose rect is substantially covered by a
/// different (non-ancestor, non-descendant) node painted above it.
///
/// Optimized via an x-interval sweep (only x-overlapping candidate pairs are
/// y-tested), so the cost is near O(n log n) + O(k) for k overlapping pairs
/// instead of a full O(n²) pairwise comparison.
///
/// Coverage semantics: `Fail` when a single other node covers ≥ 75% of the
/// element's area, `Warn` when ≥ 50%. Ancestor/descendant pairs are ignored
/// (a child inside its parent is "contained by design"). Paint order is a
/// heuristic: numeric `z-index` (from `metrics`, falling back to
/// `styles.layout.z-index`) wins, then later document order.
pub fn check_occlusion(nodes: &[DiffNode]) -> Vec<CheckLine> {
    let mut flat = Vec::new();
    flatten(nodes, &mut flat, &mut 0);

    let mut cands: Vec<usize> = (0..flat.len())
        .filter(|&i| flat[i].rect.is_some() && flat[i].node.display_visible() != Some(false))
        .collect();
    cands.sort_by_key(|&i| flat[i].rect.unwrap().0 as i64);

    // covered node index -> (covering node index, coverage fraction), keeping
    // the worst (highest-coverage) coverer per covered node.
    let mut covered: HashMap<usize, (usize, f64)> = HashMap::new();

    for (ci, &i) in cands.iter().enumerate() {
        let (xi, yi, wi, hi) = flat[i].rect.unwrap();
        let area_i = wi * hi;
        for &j in &cands[ci + 1..] {
            let (xj, yj, wj, hj) = flat[j].rect.unwrap();
            if xj >= xi + wi {
                // Sorted by x; no later node can overlap i in x.
                break;
            }
            let ix0 = xi.max(xj);
            let ix1 = (xi + wi).min(xj + wj);
            if ix1 <= ix0 {
                continue;
            }
            let iy0 = yi.max(yj);
            let iy1 = (yi + hi).min(yj + hj);
            if iy1 <= iy0 {
                continue;
            }
            if related(&flat, i, j) {
                continue;
            }
            let inter = (ix1 - ix0) * (iy1 - iy0);
            let cov_i_by_j = inter / area_i;
            let cov_j_by_i = inter / (wj * hj);
            // Only the one painted *below* can be reported as covered.
            if cov_i_by_j >= 0.5 && paints_above(&flat, j, i) {
                record_cover(&mut covered, i, j, cov_i_by_j);
            }
            if cov_j_by_i >= 0.5 && paints_above(&flat, i, j) {
                record_cover(&mut covered, j, i, cov_j_by_i);
            }
        }
    }

    let mut out = Vec::new();
    let mut keys: Vec<usize> = covered.keys().copied().collect();
    keys.sort_unstable();
    for k in keys {
        let (coverer, cover) = covered[&k];
        let covered_sel = &flat[k].node.selector;
        let coverer_sel = &flat[coverer].node.selector;
        let pct = (cover * 100.0).round() as u32;
        out.push(CheckLine {
            check: "occluded".into(),
            selector: covered_sel.clone(),
            tag: flat[k].node.tag.clone(),
            status: if cover >= 0.75 {
                RuleStatus::Fail
            } else {
                RuleStatus::Warn
            },
            evidence: format!(
                "{pct}% of {covered_sel} is covered by {coverer_sel} — \
                 the element is visually behind an overlapping element"
            ),
        });
    }
    out
}

fn flatten<'a>(nodes: &'a [DiffNode], out: &mut Vec<Flat<'a>>, next: &mut usize) {
    for node in nodes {
        let idx = *next;
        *next += 1;
        out.push(Flat {
            node,
            idx,
            end: idx,
            rect: rect_coords(node),
        });
        flatten(&node.children, out, next);
        // Subtree range is closed by the last descendant pushed.
        out[idx].end = *next - 1;
    }
}

/// `a` is an ancestor of `b` or vice versa (subtree interval containment).
fn related(flat: &[Flat], a: usize, b: usize) -> bool {
    let (ai, ae) = (flat[a].idx, flat[a].end);
    let (bi, be) = (flat[b].idx, flat[b].end);
    (ai <= bi && bi <= ae) || (bi <= ai && ai <= be)
}

/// Whether `a` paints above `b` (deterministic stacking heuristic).
fn paints_above(flat: &[Flat], a: usize, b: usize) -> bool {
    let za = node_z_index(flat[a].node);
    let zb = node_z_index(flat[b].node);
    match (za, zb) {
        (Some(va), Some(vb)) if va != vb => va > vb,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        _ => flat[a].idx > flat[b].idx,
    }
}

/// Read a node's rect as `(x, y, width, height)`, if present and non-empty.
fn rect_coords(node: &DiffNode) -> Option<(f64, f64, f64, f64)> {
    let rect = node.rect.as_ref()?;
    let x = rect.get("x")?.as_f64()?;
    let y = rect.get("y")?.as_f64()?;
    let w = rect.get("width")?.as_f64()?;
    let h = rect.get("height")?.as_f64()?;
    (w > 0.0 && h > 0.0).then_some((x, y, w, h))
}

/// Numeric z-index from `metrics.z_index` (string or int) or
/// `styles.layout.z-index`. `None` for `auto`/unset.
fn node_z_index(node: &DiffNode) -> Option<i64> {
    if let Some(metrics) = node.metrics.as_ref()
        && let Some(z) = metrics.get("z_index")
    {
        if let Some(i) = z.as_i64() {
            return Some(i);
        }
        if let Some(s) = z.as_str()
            && let Ok(i) = s.parse::<i64>()
        {
            return Some(i);
        }
    }
    style_val(node, "layout", "z-index")?.parse().ok()
}

/// Keep the highest-coverage coverer for a covered node.
fn record_cover(covered: &mut HashMap<usize, (usize, f64)>, who: usize, by: usize, cov: f64) {
    match covered.get(&who) {
        Some((_, best)) if *best >= cov => {}
        _ => {
            covered.insert(who, (by, cov));
        }
    }
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
            attributes: None,
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

    // --- occlusion ---

    fn occ_node(
        selector: &str,
        tag: &str,
        rect: (f64, f64, f64, f64),
        z_index: Option<i64>,
        visible: bool,
    ) -> DiffNode {
        let (x, y, w, h) = rect;
        DiffNode {
            id: 0,
            parent_id: None,
            selector: selector.into(),
            tag: Some(tag.into()),
            path: None,
            depth: Some(0),
            rect: Some(serde_json::json!({"x": x, "y": y, "width": w, "height": h})),
            metrics: z_index
                .map(|z| serde_json::json!({"z_index": z.to_string(), "stacking_context": true})),
            noticeable: Some(serde_json::json!({
                "display_visible": visible, "accessibility_grade": "AAA"
            })),
            hash: None,
            styles: None,
            pseudo: None,
            aria: None,
            contrast: None,
            ax: None,
            attributes: None,
            children: vec![],
        }
    }

    #[test]
    fn occluded_fail_when_covered_by_overlay() {
        let button = occ_node("button.save", "BUTTON", (0.0, 0.0, 200.0, 60.0), None, true);
        let overlay = occ_node(
            "div.modal-backdrop",
            "DIV",
            (0.0, 0.0, 1440.0, 900.0),
            Some(1000),
            true,
        );
        let lines = check_occlusion(&[button, overlay]);
        let occ = lines
            .iter()
            .find(|l| l.check == "occluded")
            .expect("occluded check");
        assert_eq!(occ.status, RuleStatus::Fail);
        assert_eq!(occ.selector, "button.save");
        assert!(
            occ.evidence.contains("covered by div.modal-backdrop"),
            "got: {}",
            occ.evidence
        );
        assert!(occ.evidence.contains("100%"), "got: {}", occ.evidence);
    }

    #[test]
    fn occluded_warn_on_partial_cover() {
        let button = occ_node(
            "button.save",
            "BUTTON",
            (0.0, 0.0, 100.0, 100.0),
            None,
            true,
        );
        let toast = occ_node("div.toast", "DIV", (0.0, 0.0, 100.0, 60.0), Some(10), true);
        let lines = check_occlusion(&[button, toast]);
        let occ = lines
            .iter()
            .find(|l| l.check == "occluded")
            .expect("occluded check");
        assert_eq!(occ.status, RuleStatus::Warn);
        assert!(occ.evidence.contains("60%"), "got: {}", occ.evidence);
    }

    #[test]
    fn lower_z_index_is_occluded_but_not_above() {
        let below = occ_node("div.footer", "DIV", (0.0, 0.0, 100.0, 100.0), Some(1), true);
        let above = occ_node("div.popup", "DIV", (0.0, 0.0, 100.0, 100.0), Some(2), true);
        let lines = check_occlusion(&[below, above]);
        let occ = lines
            .iter()
            .find(|l| l.check == "occluded")
            .expect("occluded check");
        assert_eq!(occ.selector, "div.footer");
        // Flipped stacking: the popup (z=1) now sits *under* the footer
        // (z=2), so the popup is the one reported as covered.
        let below = occ_node("div.footer", "DIV", (0.0, 0.0, 100.0, 100.0), Some(2), true);
        let above = occ_node("div.popup", "DIV", (0.0, 0.0, 100.0, 100.0), Some(1), true);
        let lines = check_occlusion(&[below, above]);
        let occ = lines
            .iter()
            .find(|l| l.check == "occluded")
            .expect("occluded check");
        assert_eq!(occ.selector, "div.popup");
    }

    #[test]
    fn later_tree_order_covers_earlier_when_no_z_index() {
        let earlier = occ_node(
            "div.menu-trigger",
            "DIV",
            (0.0, 0.0, 120.0, 40.0),
            None,
            true,
        );
        let later = occ_node("div.dropdown", "DIV", (0.0, 0.0, 120.0, 40.0), None, true);
        let lines = check_occlusion(&[earlier, later]);
        let occ = lines
            .iter()
            .find(|l| l.check == "occluded")
            .expect("occluded check");
        assert_eq!(occ.selector, "div.menu-trigger");
    }

    #[test]
    fn adjacent_siblings_not_occluded() {
        let a = occ_node(
            "div.card:nth-child(1)",
            "DIV",
            (0.0, 0.0, 300.0, 120.0),
            None,
            true,
        );
        let b = occ_node(
            "div.card:nth-child(2)",
            "DIV",
            (300.0, 0.0, 300.0, 120.0),
            None,
            true,
        );
        assert!(
            check_occlusion(&[a, b]).is_empty(),
            "non-overlapping cards must not be flagged"
        );
    }

    #[test]
    fn ancestor_containment_not_reported() {
        let mut parent = occ_node("div.card", "DIV", (0.0, 0.0, 400.0, 300.0), None, true);
        let child = occ_node("p.text", "P", (20.0, 20.0, 100.0, 40.0), None, true);
        parent.children = vec![child];
        assert!(
            check_occlusion(&[parent]).is_empty(),
            "parent/child containment is by design"
        );
    }

    #[test]
    fn occlusion_reported_once_across_subtree() {
        // A page-level overlay covering a deeply nested button must yield a
        // single occluded line (no per-subtree duplicates).
        let button = occ_node("button.x", "BUTTON", (10.0, 10.0, 100.0, 40.0), None, true);
        let mut container = occ_node(
            "div.modal-content",
            "DIV",
            (0.0, 0.0, 500.0, 400.0),
            None,
            true,
        );
        container.children = vec![button];
        let overlay = occ_node(
            "div.backdrop",
            "DIV",
            (10.0, 10.0, 100.0, 40.0),
            Some(100),
            true,
        );
        let lines = run_rules(&[container, overlay]);
        let occ: Vec<_> = lines.iter().filter(|l| l.check == "occluded").collect();
        assert_eq!(occ.len(), 1, "occlusion must be reported exactly once");
        assert_eq!(occ[0].selector, "button.x");
    }
}
