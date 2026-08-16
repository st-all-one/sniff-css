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
    aaa: TriState,
    large: bool,
    foreground: String,
    background: String,
    unknown_reason: Option<String>,
}

/// Map a JSON `pass`/`fail` (anything else → `unknown`) to [`TriState`].
fn tri_from_json(v: Option<&Value>) -> TriState {
    match v.and_then(Value::as_str) {
        Some("pass") => TriState::Pass,
        Some("fail") => TriState::Fail,
        _ => TriState::Unknown,
    }
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
pub fn run_rules(nodes: &[DiffNode], viewport: Option<(f64, f64)>) -> Vec<CheckLine> {
    let mut out = Vec::new();
    for node in nodes {
        let ctx = AncestorCtx::default();
        check_node(node, &ctx, viewport, &mut out);
        run_children(node, ctx, viewport, &mut out);
    }
    out.extend(check_occlusion(nodes));
    out.extend(check_backdrop_over_modal(nodes, viewport));
    out
}

/// Recursively run per-node checks carrying the ancestor context down the
/// tree (a single stack walk resolves the ancestor predicates once).
fn run_children(
    node: &DiffNode,
    parent_ctx: AncestorCtx,
    viewport: Option<(f64, f64)>,
    out: &mut Vec<CheckLine>,
) {
    let ctx = AncestorCtx::from_node(node, &parent_ctx);
    for child in &node.children {
        check_node(child, &ctx, viewport, out);
        run_children(child, ctx, viewport, out);
    }
}

/// Ancestor-derived predicates resolved once per node from the styles of its
/// ancestors (used by positioning/overflow rules that browsers compute
/// across the tree, e.g. `position:sticky` breaks under an
/// `overflow:hidden` ancestor).
#[derive(Debug, Clone, Copy, Default)]
struct AncestorCtx {
    /// Any ancestor has `overflow-x`/`overflow-y`/`overflow` not `visible`
    /// (breaks `position:sticky`; also clips descendants).
    has_overflow_hidden_ancestor: bool,
    /// Any ancestor has `transform`/`filter`/`will-change`/`perspective`/
    /// `contain:paint` — makes `position:fixed` relative to that ancestor.
    has_transform_ancestor: bool,
    /// The direct parent is a flex/grid container (so a child `z-index`
    /// applies even with `position:static`).
    parent_is_flex_or_grid: bool,
}

impl AncestorCtx {
    fn from_node(node: &DiffNode, parent: &AncestorCtx) -> AncestorCtx {
        let overflow = [
            style_val(node, "box_model", "overflow-x"),
            style_val(node, "box_model", "overflow-y"),
        ]
        .into_iter()
        .flatten()
        .any(|v| !matches!(v, "visible" | "clip"));
        let transform = [
            style_val(node, "transform", "transform").map(|v| v != "none"),
            style_val(node, "transform", "perspective").map(|v| v != "none"),
            style_val(node, "visual", "filter").map(|v| v != "none"),
            style_val(node, "visual", "will-change").map(|v| v != "auto"),
            style_val(node, "layout", "contain").map(|v| v.contains("paint")),
        ]
        .into_iter()
        .flatten()
        .any(|b| b);
        let display = style_val(node, "layout", "display").unwrap_or("");
        let parent_is_flex_or_grid =
            matches!(display, "flex" | "inline-flex" | "grid" | "inline-grid");
        AncestorCtx {
            has_overflow_hidden_ancestor: parent.has_overflow_hidden_ancestor || overflow,
            has_transform_ancestor: parent.has_transform_ancestor || transform,
            parent_is_flex_or_grid,
        }
    }
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
        .filter(|&i| {
            flat[i].rect.is_some()
                && flat[i].node.display_visible() != Some(false)
                // Skip elements with opacity:0 (carousel slides, hidden panels)
                && !is_opaque_zero(flat[i].node)
        })
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
            // Skip SVG elements within the same SVG — they render in document
            // order by design (path over circle is intentional, not occlusion).
            if same_svg_container(&flat, i, j) {
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

/// Detect a dark translucent scrim ("backdrop") painted **over** a modal's
/// content — the classic bug where a modal opens with a dark overlay that
/// ends up covering the dialog itself, making it unreadable/unclickable.
///
/// The generic `occluded` rule deliberately skips ancestor/descendant pairs
/// ("contained by design"), which is exactly the common DOM shape for a modal
/// (the scrim wraps the dialog). This rule therefore targets that pair
/// specifically:
///
/// - **backdrop**: `position:fixed` (or near-viewport-sized) node whose
///   translucent dark background covers ≥ 80% of the viewport.
/// - **modal content**: a node **inside the backdrop's subtree** that is
///   interactive/text-bearing.
/// - **paint order**: children paint above their parent's background, so the
///   only way a descendant dialog ends up *behind* the scrim is a **negative
///   z-index** (or the scrim itself sitting above via stacking). `Fail` when
///   the backdrop covers ≥ 50% of such a descendant — a child that paints
///   below the scrim it is wrapped in.
pub fn check_backdrop_over_modal(
    nodes: &[DiffNode],
    viewport: Option<(f64, f64)>,
) -> Vec<CheckLine> {
    let Some((vp_w, vp_h)) = viewport else {
        return Vec::new();
    };
    let mut flat = Vec::new();
    flatten(nodes, &mut flat, &mut 0);

    let is_backdrop = |f: &Flat| -> bool {
        let node = f.node;
        let pos = style_val(node, "layout", "position").unwrap_or("");
        let Some((_x, _y, w, h)) = f.rect else {
            return false;
        };
        let covers_viewport = w >= vp_w * 0.8 && h >= vp_h * 0.8;
        let fixed = pos == "fixed";
        if !(fixed || covers_viewport) {
            return false;
        }
        let bg = style_val(node, "visual", "background-color").unwrap_or("");
        let translucent_dark = is_translucent_dark_bg(bg);
        translucent_dark && node.display_visible() != Some(false)
    };

    // The dialog content it could cover: interactive or text-bearing nodes.
    let is_modal_content = |f: &Flat| -> bool {
        let tag = f.node.tag.as_deref().unwrap_or("");
        let interactive = INTERACTIVE_TAGS.contains(&tag);
        let text = f
            .node
            .aria
            .as_ref()
            .and_then(|a| a.get("has_text"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        interactive || text || TEXT_TAGS.contains(&tag)
    };

    let mut out = Vec::new();
    for bi in 0..flat.len() {
        if !is_backdrop(&flat[bi]) {
            continue;
        }
        let (bx, by, bw, bh) = flat[bi].rect.unwrap();
        for mi in 0..flat.len() {
            if bi == mi || !is_modal_content(&flat[mi]) {
                continue;
            }
            // Only the ancestor/descendant pair (which `occluded` skips).
            if !related(&flat, bi, mi) {
                continue;
            }
            let Some((mx, my, mw, mh)) = flat[mi].rect else {
                continue;
            };
            // Coverage of the modal rect by the backdrop.
            let ix0 = bx.max(mx);
            let ix1 = (bx + bw).min(mx + mw);
            let iy0 = by.max(my);
            let iy1 = (by + bh).min(my + mh);
            if ix1 <= ix0 || iy1 <= iy0 {
                continue;
            }
            let inter = (ix1 - ix0) * (iy1 - iy0);
            let cov = inter / (mw * mh);
            if cov < 0.5 {
                continue;
            }
            // A descendant normally paints ABOVE its parent's background;
            // only a negative z-index drops it behind the scrim.
            if node_z_index(flat[mi].node).unwrap_or(0) >= 0 {
                continue;
            }
            let pct = (cov * 100.0).round() as u32;
            out.push(CheckLine {
                check: "backdrop-over-modal".into(),
                selector: flat[mi].node.selector.clone(),
                tag: flat[mi].node.tag.clone(),
                status: RuleStatus::Fail,
                evidence: format!(
                    "{pct}% of {} is covered by the modal backdrop {} — \
                     the dialog paints below its own dark scrim (negative z-index); \
                     check the stacking order",
                    flat[mi].node.selector, flat[bi].node.selector
                ),
            });
        }
    }
    out
}

/// A translucent dark background — the typical modal scrim. Accepts either
/// the raw `rgba(r, g, b, a)` the engine reports, or the normalized 8-digit
/// hex `#rrggbbaa` it converts to by default.
fn is_translucent_dark_bg(bg: &str) -> bool {
    let bg = bg.trim();
    if let Some(inner) = bg.strip_prefix("rgba(") {
        let parts: Vec<&str> = inner
            .trim_end_matches(')')
            .splitn(4, ',')
            .map(str::trim)
            .collect();
        if parts.len() == 4 {
            let dark = parts[..3]
                .iter()
                .filter_map(|c| c.parse::<f64>().ok())
                .all(|c| c < 90.0);
            let alpha = parts[3].parse::<f64>().ok();
            return dark && alpha.is_some_and(|a| (0.05..1.0).contains(&a));
        }
        return false;
    }
    // Normalized 8-digit hex `#rrggbbaa` (alpha byte → 0..1).
    if bg.len() == 9 && bg.starts_with('#') {
        let (Ok(r), Ok(g), Ok(b), Ok(a)) = (
            u8::from_str_radix(&bg[1..3], 16),
            u8::from_str_radix(&bg[3..5], 16),
            u8::from_str_radix(&bg[5..7], 16),
            u8::from_str_radix(&bg[7..9], 16),
        ) else {
            return false;
        };
        let (r, g, b) = (r as f64, g as f64, b as f64);
        let alpha = a as f64 / 255.0;
        let dark = r < 90.0 && g < 90.0 && b < 90.0;
        return dark && (0.05..1.0).contains(&alpha);
    }
    false
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

/// Both nodes are SVG elements inside the same `<svg>` container.
/// SVG renders in document order within a single SVG, so overlapping
/// siblings are intentional (path over circle, etc.), not occlusion.
fn same_svg_container(flat: &[Flat], a: usize, b: usize) -> bool {
    let is_svg_el = |tag: &str| {
        matches!(
            tag,
            "circle"
                | "ellipse"
                | "line"
                | "path"
                | "polygon"
                | "polyline"
                | "rect"
                | "text"
                | "use"
                | "g"
        )
    };
    let tag_a = flat[a].node.tag.as_deref().unwrap_or("");
    let tag_b = flat[b].node.tag.as_deref().unwrap_or("");
    if !is_svg_el(tag_a) || !is_svg_el(tag_b) {
        return false;
    }
    // Walk ancestors of a looking for an <svg> that also contains b.
    // Walk the flat array backwards from position a-1.
    let mut pos = a;
    while pos > 0 {
        pos -= 1;
        let anc = &flat[pos];
        let anc_tag = anc.node.tag.as_deref().unwrap_or("");
        if anc_tag == "svg" {
            // Check if b is inside this svg's subtree
            return anc.idx <= flat[b].idx && flat[b].idx <= anc.end;
        }
    }
    false
}

/// Whether a node creates a stacking context. A stacking context is created
/// by positioned elements with an explicit z-index, elements with opacity < 1,
/// transformed elements, filtered elements, etc.
fn creates_stacking_context(node: &DiffNode) -> bool {
    // If metrics reports stacking_context, trust it (covers test fixtures).
    if node
        .metrics
        .as_ref()
        .and_then(|m| m.get("stacking_context"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        return true;
    }
    let pos = style_val(node, "layout", "position").unwrap_or("static");
    if pos != "static" && node_z_index(node).is_some() {
        return true;
    }
    if style_val(node, "visual", "opacity")
        .and_then(|v| v.parse::<f64>().ok())
        .is_some_and(|v| v < 1.0)
    {
        return true;
    }
    if style_val(node, "transform", "transform").is_some_and(|t| t != "none") {
        return true;
    }
    if style_val(node, "visual", "filter").is_some_and(|f| f != "none") {
        return true;
    }
    if style_val(node, "visual", "isolation").is_some_and(|i| i == "isolate") {
        return true;
    }
    false
}

/// Find the effective z-index of a node by climbing ancestors until reaching a
/// stacking context boundary. Returns `None` for `auto`/unset.
fn effective_z_index(flat: &[Flat], i: usize) -> Option<i64> {
    // Check the node itself first.
    if creates_stacking_context(flat[i].node) {
        return node_z_index(flat[i].node);
    }
    // Climb ancestors: find the nearest ancestor whose subtree contains this node.
    // We scan backwards (ancestors come before descendants in pre-order).
    let idx = flat[i].idx;
    for j in (0..i).rev() {
        if flat[j].idx <= idx && idx <= flat[j].end {
            // j is an ancestor of i.
            if creates_stacking_context(flat[j].node) {
                return node_z_index(flat[j].node);
            }
        } else if flat[j].idx > idx {
            // Past the ancestor range — stop.
            break;
        }
    }
    None
}

/// Whether `a` paints above `b` (deterministic stacking heuristic).
///
/// Uses effective z-index (climbing ancestors to the nearest stacking context)
/// instead of comparing the node's own z-index directly.
fn paints_above(flat: &[Flat], a: usize, b: usize) -> bool {
    let za = effective_z_index(flat, a);
    let zb = effective_z_index(flat, b);
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

fn check_node(
    node: &DiffNode,
    ctx: &AncestorCtx,
    viewport: Option<(f64, f64)>,
    out: &mut Vec<CheckLine>,
) {
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
                let aa = tri_from_json(c.get("aa"));
                let aaa = tri_from_json(c.get("aaa"));
                Some(ContrastFacet {
                    ratio,
                    aa,
                    aaa,
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
                    aaa: i.aaa,
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

            // AAA (WCAG 1.4.6): threshold 7.0:1 normal / 4.5:1 large text.
            let (aaa_th, label) = if info.large {
                (4.5, "large")
            } else {
                (7.0, "normal")
            };
            match info.aaa {
                TriState::Pass => out.push(CheckLine {
                    check: "contrast-aaa".into(),
                    selector: node.selector.clone(),
                    tag: node.tag.clone(),
                    status: RuleStatus::Pass,
                    evidence: format!("ratio {}:1 (need {aaa_th}:1 {label} text AAA)", info.ratio),
                }),
                TriState::Fail => out.push(CheckLine {
                    check: "contrast-aaa".into(),
                    selector: node.selector.clone(),
                    tag: node.tag.clone(),
                    status: RuleStatus::Fail,
                    evidence: format!(
                        "ratio {}:1 on {} against {} (need {aaa_th}:1 {label} text AAA)",
                        info.ratio, info.foreground, info.background
                    ),
                }),
                TriState::Unknown => out.push(CheckLine {
                    check: "contrast-aaa".into(),
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

    // --- position:sticky broken by an overflow:hidden ancestor ---
    if visible
        && style_val(node, "layout", "position") == Some("sticky")
        && ctx.has_overflow_hidden_ancestor
    {
        out.push(CheckLine {
            check: "sticky-in-overflow-hidden".into(),
            selector: node.selector.clone(),
            tag: node.tag.clone(),
            status: RuleStatus::Warn,
            evidence:
                "position:sticky inside an overflow:hidden/auto/scroll ancestor — the stickiness "
                    .to_string()
                    + "never engages",
        });
    }

    // --- position:fixed broken by a transformed ancestor ---
    if visible
        && style_val(node, "layout", "position") == Some("fixed")
        && ctx.has_transform_ancestor
    {
        out.push(CheckLine {
            check: "fixed-broken-by-transform".into(),
            selector: node.selector.clone(),
            tag: node.tag.clone(),
            status: RuleStatus::Warn,
            evidence: "position:fixed inside a transformed/filtered ancestor — the element "
                .to_string()
                + "is positioned relative to that ancestor, not the viewport",
        });
    }

    // --- position:absolute with no insets (sits at its static position) ---
    if visible && style_val(node, "layout", "position") == Some("absolute") {
        let insets = [
            "top",
            "right",
            "bottom",
            "left",
            "inset-block-start",
            "inset-block-end",
            "inset-inline-start",
            "inset-inline-end",
        ]
        .into_iter()
        .filter_map(|p| style_val(node, "layout", p))
        .all(|v| v == "auto");
        if insets {
            // If the element has large explicit dimensions (likely a
            // decorative overlay / backdrop), skip — it's sized intentionally.
            let is_likely_overlay = rect_size(node).is_some_and(|(w, h)| w >= 300.0 && h >= 300.0);
            if !is_likely_overlay {
                out.push(CheckLine {
                    check: "absolute-without-insets".into(),
                    selector: node.selector.clone(),
                    tag: node.tag.clone(),
                    status: RuleStatus::Warn,
                    evidence: "position:absolute with no top/right/bottom/left — the element stays "
                        .to_string()
                        + "at its static position (a common dropdown/overlay bug)",
                });
            }
        }
    }

    // --- Interactive element that can't receive pointer input ---
    if visible
        && INTERACTIVE_TAGS.contains(&tag)
        && style_val(node, "interaction", "pointer-events") == Some("none")
    {
        out.push(CheckLine {
            check: "interactive-pointer-events-none".into(),
            selector: node.selector.clone(),
            tag: node.tag.clone(),
            status: RuleStatus::Fail,
            evidence: format!(
                "interactive {tag} has pointer-events:none — it cannot be clicked or tapped"
            ),
        });
    }

    // --- aria-hidden on a focusable element (focus trap / invisible focus) ---
    if focusable {
        let aria_hidden = node
            .aria
            .as_ref()
            .and_then(|a| a.get("ariaHidden"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if aria_hidden {
            out.push(CheckLine {
                check: "aria-hidden-focusable".into(),
                selector: node.selector.clone(),
                tag: node.tag.clone(),
                status: RuleStatus::Fail,
                evidence: "aria-hidden=\"true\" on a focusable element — keyboard focus lands on "
                    .to_string()
                    + "invisible content (WCAG 4.1.2)",
            });
        }
    }

    // --- text-overflow:ellipsis without the required overflow clip ---
    if visible && is_text && style_val(node, "typography", "text-overflow") == Some("ellipsis") {
        let clipped = ["overflow-x", "overflow-y"]
            .into_iter()
            .filter_map(|p| style_val(node, "box_model", p))
            .any(|v| matches!(v, "hidden" | "clip" | "auto" | "scroll"));
        let nowrap = style_val(node, "typography", "white-space")
            .is_some_and(|v| matches!(v, "nowrap" | "pre"));
        if !clipped && !nowrap {
            out.push(CheckLine {
                check: "ellipsis-without-clip".into(),
                selector: node.selector.clone(),
                tag: node.tag.clone(),
                status: RuleStatus::Warn,
                evidence: "text-overflow:ellipsis needs overflow:hidden (and usually ".to_string()
                    + "white-space:nowrap) to take effect",
            });
        }
    }

    // --- width:100% + padding with content-box → guaranteed horizontal overflow ---
    if visible && style_val(node, "box_model", "box-sizing") == Some("content-box") {
        let full = style_val(node, "box_model", "width").is_some_and(|v| v == "100%")
            || style_val(node, "box_model", "min-width").is_some_and(|v| v == "100%");
        let has_padding = [
            "padding-top",
            "padding-right",
            "padding-bottom",
            "padding-left",
        ]
        .into_iter()
        .filter_map(|p| style_val(node, "box_model", p))
        .any(|v| v != "0px" && v != "0");
        if full && has_padding {
            out.push(CheckLine {
                check: "width-100-with-padding".into(),
                selector: node.selector.clone(),
                tag: node.tag.clone(),
                status: RuleStatus::Warn,
                evidence: "width:100% with box-sizing:content-box and padding overflows its "
                    .to_string()
                    + "container — use box-sizing:border-box",
            });
        }
    }

    // --- Very small text ---
    if visible && is_text {
        let fs = style_val(node, "typography", "font-size");
        let size_px = fs.and_then(parse_px);
        if let Some(px) = size_px
            && px > 0.0
            && px < 12.0
        {
            out.push(CheckLine {
                check: "small-text".into(),
                selector: node.selector.clone(),
                tag: node.tag.clone(),
                status: RuleStatus::Warn,
                evidence: format!(
                    "font-size {} is below the ~12px comfortable minimum",
                    fs.unwrap_or("?")
                ),
            });
        }
    }

    // --- Small image thumbnail (gallery/images that are too small to view) ---
    if visible {
        let has_bg_image = style_val(node, "visual", "background-image")
            .is_some_and(|v| v != "none" && v.starts_with("url("));
        let is_clickable = INTERACTIVE_TAGS.contains(&tag)
            || style_val(node, "interaction", "cursor") == Some("pointer");
        let is_img_or_container = matches!(
            tag,
            "img" | "IMG" | "div" | "DIV" | "li" | "LI" | "a" | "A" | "figure" | "FIGURE"
        );
        if has_bg_image
            && is_clickable
            && is_img_or_container
            && let Some((_, _, w, h)) = rect_coords(node)
            && (w < 150.0 || h < 150.0)
        {
            out.push(CheckLine {
                check: "small-thumbnail".into(),
                selector: node.selector.clone(),
                tag: node.tag.clone(),
                status: RuleStatus::Warn,
                evidence: format!(
                    "image thumbnail is {}×{}px (recommended minimum 150×150 for comfortable viewing)",
                    w.round() as u32,
                    h.round() as u32
                ),
            });
        }
    }

    // --- line-height smaller than font-size (clipped/overlapping glyphs) ---
    if visible && is_text {
        let fs = style_val(node, "typography", "font-size").and_then(parse_px);
        let lh =
            style_val(node, "typography", "line-height").and_then(|v| parse_line_height(v, fs));
        if let (Some(fs_px), Some(lh_px)) = (fs, lh)
            && lh_px > 0.0
            && lh_px < fs_px
        {
            out.push(CheckLine {
                check: "line-height-below-font-size".into(),
                selector: node.selector.clone(),
                tag: node.tag.clone(),
                status: RuleStatus::Warn,
                evidence: format!(
                    "line-height {lh_px:0.1}px is smaller than font-size {fs_px:0.1}px — glyphs can clip or overlap"
                ),
            });
        }
    }

    // --- z-index set on a static (non-flex/grid) element → ignored ---
    if visible {
        let zi = style_val(node, "layout", "z-index").or_else(|| {
            node.metrics
                .as_ref()
                .and_then(|m| m.get("z_index"))
                .and_then(Value::as_str)
        });
        let numeric = zi.is_some_and(|v| {
            v != "auto"
                && v.chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit() || c == '-')
        });
        if numeric
            && style_val(node, "layout", "position") == Some("static")
            && !ctx.parent_is_flex_or_grid
        {
            out.push(CheckLine {
                check: "z-index-on-static".into(),
                selector: node.selector.clone(),
                tag: node.tag.clone(),
                status: RuleStatus::Warn,
                evidence: format!(
                    "z-index {} on a position:static element is ignored — position the element or make the parent a flex/grid container",
                    zi.unwrap_or("?")
                ),
            });
        }
    }

    // --- Interactive control without an accessible name ---
    if visible && INTERACTIVE_TAGS.contains(&tag) {
        let needs_name = tag == "INPUT" || tag == "BUTTON" || tag == "SELECT" || tag == "TEXTAREA";
        if needs_name && !has_name && !has_text {
            out.push(CheckLine {
                check: "control-without-name".into(),
                selector: node.selector.clone(),
                tag: node.tag.clone(),
                status: RuleStatus::Warn,
                evidence: format!(
                    "interactive {tag} has no accessible name (no text, aria-label or label) — icon-only controls need one"
                ),
            });
        }
    }

    // --- user-select:none on body text (users can't copy) ---
    // Skip interactive elements (buttons, links) — user-select:none is normal there.
    if visible
        && is_text
        && TEXT_TAGS.contains(&tag)
        && !INTERACTIVE_TAGS.contains(&tag)
        && tag != "A"
    {
        let us = style_val(node, "interaction", "user-select");
        if us.is_some_and(|v| matches!(v, "none" | "-webkit-none")) {
            out.push(CheckLine {
                check: "text-not-selectable".into(),
                selector: node.selector.clone(),
                tag: node.tag.clone(),
                status: RuleStatus::Warn,
                evidence: "user-select:none on body text prevents users from selecting/copying "
                    .to_string()
                    + "content",
            });
        }
    }

    // --- Infinite, very fast animation (flashing risk) ---
    if visible {
        let iter = style_val(node, "animation", "animation-iteration-count");
        let infinite = iter.is_some_and(|v| v == "infinite");
        if infinite {
            let dur = style_val(node, "animation", "animation-duration").and_then(parse_duration);
            if let Some(d) = dur
                && d < 0.5
            {
                out.push(CheckLine {
                    check: "infinite-fast-animation".into(),
                    selector: node.selector.clone(),
                    tag: node.tag.clone(),
                    status: RuleStatus::Warn,
                    evidence: format!(
                        "infinite animation with a {d:0.2}s cycle may flicker fast enough to trigger photosensitive conditions (WCAG 2.3.1)"
                    ),
                });
            }
        }
    }

    // --- transition-property: all (repaint cost / perf) ---
    if visible {
        let tp = style_val(node, "animation", "transition-property");
        if tp.is_some_and(|v| v == "all") {
            out.push(CheckLine {
                check: "transition-all".into(),
                selector: node.selector.clone(),
                tag: node.tag.clone(),
                status: RuleStatus::Warn,
                evidence: "transition-property:all animates every property (layout+repaint) — "
                    .to_string()
                    + "list the specific properties instead",
            });
        }
    }

    // --- overflow-x:hidden on html/body masks horizontal scroll ---
    if visible && matches!(tag, "HTML" | "BODY") {
        let ox = style_val(node, "box_model", "overflow-x")
            .or_else(|| style_val(node, "box_model", "overflow"));
        if ox.is_some_and(|v| v == "hidden") {
            out.push(CheckLine {
                check: "overflow-x-hidden-on-body".into(),
                selector: node.selector.clone(),
                tag: node.tag.clone(),
                status: RuleStatus::Warn,
                evidence: "overflow-x:hidden on the document masks horizontal overflow — find and "
                    .to_string()
                    + "fix the overflowing element instead",
            });
        }
    }

    // --- Content wider than the viewport (horizontal scroll / CLS) ---
    if visible
        && let Some((vp_w, _)) = viewport
        && let Some((x, w)) = node
            .rect
            .as_ref()
            .and_then(|r| Some((r.get("x")?.as_f64()?, r.get("width")?.as_f64()?)))
        && !ctx.has_overflow_hidden_ancestor
        && x + w > vp_w + 1.0
    {
        out.push(CheckLine {
            check: "horizontal-overflow".into(),
            selector: node.selector.clone(),
            tag: node.tag.clone(),
            status: RuleStatus::Warn,
            evidence: format!(
                "element extends to x={x:0.0}+{w:0.0}px, past the {vp_w:0.0}px viewport — forces horizontal scrolling"
            ),
        });
    }
}

/// Read a style value from a category group (e.g. `visual.color`).
fn style_val<'a>(node: &'a DiffNode, category: &str, prop: &str) -> Option<&'a str> {
    node.styles.as_ref()?.get(category)?.get(prop)?.as_str()
}

/// Element has `opacity:0` — visually hidden (carousel slides, off-screen panels).
fn is_opaque_zero(node: &DiffNode) -> bool {
    style_val(node, "visual", "opacity").is_some_and(|v| v == "0" || v == "0.0")
}

/// Bounding rect size of a node.
fn rect_size(node: &DiffNode) -> Option<(f64, f64)> {
    let rect = node.rect.as_ref()?;
    let width = rect.get("width")?.as_f64()?;
    let height = rect.get("height")?.as_f64()?;
    Some((width, height))
}

/// Parse a length with a `px` unit (or a bare number) into pixels.
fn parse_px(v: &str) -> Option<f64> {
    let t = v.trim();
    if let Some(n) = t.strip_suffix("px") {
        n.trim().parse().ok()
    } else if let Some(n) = t.strip_suffix("rem") {
        n.trim().parse::<f64>().ok().map(|x| x * 16.0)
    } else if let Some(n) = t.strip_suffix("em") {
        n.trim().parse::<f64>().ok().map(|x| x * 16.0)
    } else {
        t.parse().ok()
    }
}

/// Resolve `line-height` to pixels: unitless is a font-size multiplier,
/// `px`/`rem`/`em` are lengths (em relative to the element's font-size).
/// `normal` (~1.2) also resolves against the font-size.
fn parse_line_height(v: &str, font_size_px: Option<f64>) -> Option<f64> {
    let t = v.trim();
    if t == "normal" {
        return font_size_px.map(|fs| fs * 1.2);
    }
    if let Some(n) = t.strip_suffix("px") {
        return n.trim().parse().ok();
    }
    if let Some(n) = t.strip_suffix("rem") {
        return n.trim().parse::<f64>().ok().map(|x| x * 16.0);
    }
    if let Some(n) = t.strip_suffix("em") {
        return n.trim().parse::<f64>().ok().map(|x| x * 16.0);
    }
    // Unitless number → multiplier of the font-size.
    let mult: f64 = t.parse().ok()?;
    font_size_px.map(|fs| fs * mult)
}

/// Parse an animation duration (`0.5s`, `300ms`, `1s`) into seconds.
fn parse_duration(v: &str) -> Option<f64> {
    let t = v.trim();
    if let Some(n) = t.strip_suffix("ms") {
        n.trim().parse::<f64>().ok().map(|x| x / 1000.0)
    } else if let Some(n) = t.strip_suffix('s') {
        n.trim().parse().ok()
    } else {
        t.parse().ok()
    }
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
        let lines = run_rules(&[pass], None);
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
        let lines = run_rules(&[fail], None);
        let aa = lines
            .iter()
            .find(|l| l.check == "contrast-aa")
            .expect("check");
        assert_eq!(aa.status, RuleStatus::Fail);
        assert!(aa.evidence.contains("ratio"), "got: {}", aa.evidence);
    }

    #[test]
    fn contrast_aaa_emitted_with_proper_threshold() {
        // Regression guard: the `contrast-aaa` check was documented but never
        // emitted. #2563eb on white (~5.17:1) passes AA but fails AAA for
        // normal text; large text (24px) passes both.
        let normal = style_node(
            "p.aaa-fail",
            "P",
            serde_json::json!({
                "color": "#2563eb",
                "background-color": "#ffffff",
                "background-image": "none"
            }),
            serde_json::json!({"font-size": "16px", "font-weight": "400"}),
        );
        let lines = run_rules(&[normal], None);
        let aa = lines
            .iter()
            .find(|l| l.check == "contrast-aa")
            .expect("contrast-aa");
        let aaa = lines
            .iter()
            .find(|l| l.check == "contrast-aaa")
            .expect("contrast-aaa");
        assert_eq!(aa.status, RuleStatus::Pass, "AA passes");
        assert_eq!(aaa.status, RuleStatus::Fail, "AAA fails for normal text");
        assert!(aaa.evidence.contains("7:1"), "got: {}", aaa.evidence);

        let large = style_node(
            "p.aaa-pass",
            "P",
            serde_json::json!({
                "color": "#2563eb",
                "background-color": "#ffffff",
                "background-image": "none"
            }),
            serde_json::json!({"font-size": "24px", "font-weight": "400"}),
        );
        let lines = run_rules(&[large], None);
        let aaa = lines
            .iter()
            .find(|l| l.check == "contrast-aaa")
            .expect("contrast-aaa");
        assert_eq!(aaa.status, RuleStatus::Pass, "large text passes AAA");
        assert!(aaa.evidence.contains("4.5:1"), "got: {}", aaa.evidence);
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
        let lines = run_rules(&[node], None);
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
        let lines = run_rules(&[node], None);
        let ts = lines
            .iter()
            .find(|l| l.check == "target-size")
            .expect("check");
        assert_eq!(ts.status, RuleStatus::Fail);
    }

    #[test]
    fn suppressed_focus_indicator_warns() {
        let node = text_node();
        let lines = run_rules(&[node], None);
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
        let lines = run_rules(&[node], None);
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
        let lines = run_rules(&[node], None);
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
        let lines = run_rules(&[node], None);
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
        let lines = run_rules(&[container, overlay], None);
        let occ: Vec<_> = lines.iter().filter(|l| l.check == "occluded").collect();
        assert_eq!(occ.len(), 1, "occlusion must be reported exactly once");
        assert_eq!(occ[0].selector, "button.x");
    }

    #[test]
    fn child_of_high_z_parent_not_occluded_by_lower_zibling() {
        // Reproduces the footer bug: container (z:2) is a sibling of scrim
        // (z:1). The container's children should NOT be flagged as occluded by
        // the scrim because they paint inside the container's stacking context.
        let mut container = occ_node(
            "div.container",
            "DIV",
            (0.0, 0.0, 800.0, 600.0),
            Some(2),
            true,
        );
        let child = occ_node(
            "div.contact-info",
            "DIV",
            (0.0, 0.0, 800.0, 200.0),
            None,
            true,
        );
        container.children = vec![child];
        let scrim = occ_node("div.scrim", "DIV", (0.0, 0.0, 1000.0, 800.0), Some(1), true);
        let lines = check_occlusion(&[container, scrim]);
        let occ: Vec<_> = lines.iter().filter(|l| l.check == "occluded").collect();
        // The scrim may be flagged as occluded by the container (z:2 > z:1),
        // but the CHILD must NOT be flagged.
        let child_flagged = occ.iter().any(|l| l.selector == "div.contact-info");
        assert!(
            !child_flagged,
            "child of high-z container must not be occluded by lower-z sibling"
        );
    }

    #[test]
    fn footer_bottom_still_occluded_by_scrim() {
        // The footer-bottom (z:auto) IS below the scrim (z:1). This must
        // still be detected as a true positive.
        let mut footer = occ_node(
            "footer.footer-section",
            "DIV",
            (0.0, 0.0, 1000.0, 800.0),
            None,
            true,
        );
        let scrim = occ_node("div.scrim", "DIV", (0.0, 0.0, 1000.0, 800.0), Some(1), true);
        let footer_bottom = occ_node(
            "div.footer-bottom",
            "DIV",
            (0.0, 0.0, 1000.0, 68.0),
            None,
            true,
        );
        footer.children = vec![scrim, footer_bottom];
        let lines = check_occlusion(&[footer]);
        let occ: Vec<_> = lines.iter().filter(|l| l.check == "occluded").collect();
        // footer-bottom should be flagged because z-auto (0) < scrim z:1
        assert!(
            !occ.is_empty(),
            "footer-bottom with z-auto must be occluded by scrim with z:1"
        );
        assert_eq!(occ[0].selector, "div.footer-bottom");
    }

    #[test]
    fn absolute_without_insets_skipped_for_large_overlay() {
        // A large element (>=300x300) with position:absolute and no insets
        // is likely a decorative overlay — should not warn.
        let mut node = raw_node(
            "div.scrim",
            "DIV",
            serde_json::json!({
                "layout": {"position": "absolute", "top": "auto", "left": "auto"}
            }),
            serde_json::json!({"focusable": false, "has_text": false}),
        );
        // Override rect to be large (>=300x300)
        node.rect = Some(serde_json::json!({"x": 0, "y": 0, "width": 1362, "height": 1028}));
        let lines = run_rules(&[node], None);
        assert!(
            !lines.iter().any(|l| l.check == "absolute-without-insets"),
            "large overlay should not trigger absolute-without-insets"
        );
    }

    #[test]
    fn svg_siblings_not_occluded() {
        // SVG elements within the same <svg> render in document order by design.
        // A <path> painted over a <circle> is intentional, not occlusion.
        let _circle = occ_node("svg > circle", "circle", (0.0, 0.0, 10.0, 10.0), None, true);
        let _path = occ_node("svg > path", "path", (0.0, 0.0, 10.0, 10.0), None, true);
        let svg = occ_node("svg.test", "svg", (0.0, 0.0, 10.0, 10.0), None, true);
        // Both children are fully overlapping — must NOT be flagged.
        let lines = check_occlusion(&[svg]);
        let occ: Vec<_> = lines.iter().filter(|l| l.check == "occluded").collect();
        assert!(
            occ.is_empty(),
            "SVG siblings must not be flagged as occluded, got {:?}",
            occ.iter().map(|l| &l.selector).collect::<Vec<_>>()
        );
    }

    /// Build a node with full control over styles/rect/aria for the new rules.
    fn raw_node(
        selector: &str,
        tag: &str,
        styles: serde_json::Value,
        aria: serde_json::Value,
    ) -> DiffNode {
        DiffNode {
            id: 0,
            parent_id: None,
            selector: selector.into(),
            tag: Some(tag.into()),
            path: Some(selector.into()),
            depth: Some(0),
            rect: Some(serde_json::json!({"x": 0, "y": 0, "width": 200, "height": 100})),
            metrics: None,
            noticeable: Some(serde_json::json!({
                "display_visible": true, "accessibility_grade": "AAA"
            })),
            hash: None,
            styles: Some(styles.as_object().unwrap().clone()),
            pseudo: None,
            aria: Some(aria),
            contrast: None,
            ax: None,
            attributes: None,
            children: vec![],
        }
    }

    fn node_with(selector: &str, styles: serde_json::Value) -> DiffNode {
        raw_node(
            selector,
            "DIV",
            styles,
            serde_json::json!({"focusable": false, "has_text": true}),
        )
    }

    /// Assert a specific check fires exactly once with the given status.
    fn assert_check<'a>(lines: &'a [CheckLine], check: &str, status: RuleStatus) -> &'a CheckLine {
        let hits: Vec<&CheckLine> = lines.iter().filter(|l| l.check == check).collect();
        assert_eq!(hits.len(), 1, "expected one `{check}` line: {lines:#?}");
        assert_eq!(hits[0].status, status, "`{check}` status");
        hits[0]
    }

    #[test]
    fn sticky_in_overflow_hidden_fires() {
        let child = raw_node(
            "header.sticky",
            "HEADER",
            serde_json::json!({
                "layout": {"position": "sticky"},
                "box_model": {"overflow-x": "hidden"}
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let parent = node_with(
            "div.wrapper",
            serde_json::json!({"box_model": {"overflow-x": "auto"}}),
        );
        let mut parent = parent;
        parent.children = vec![child];
        let lines = run_rules(&[parent], None);
        let l = assert_check(&lines, "sticky-in-overflow-hidden", RuleStatus::Warn);
        assert!(l.evidence.contains("overflow"), "{}", l.evidence);
    }

    #[test]
    fn sticky_without_overflow_ancestor_is_clean() {
        let child = raw_node(
            "header.sticky",
            "HEADER",
            serde_json::json!({"layout": {"position": "sticky"}}),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let parent = node_with("div.wrapper", serde_json::json!({"box_model": {}}));
        let mut parent = parent;
        parent.children = vec![child];
        let lines = run_rules(&[parent], None);
        assert!(
            !lines.iter().any(|l| l.check == "sticky-in-overflow-hidden"),
            "no overflow ancestor → clean: {lines:#?}"
        );
    }

    #[test]
    fn fixed_broken_by_transform_ancestor_fires() {
        let child = raw_node(
            "div.toast",
            "DIV",
            serde_json::json!({"layout": {"position": "fixed"}}),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let parent = node_with(
            "section.animated",
            serde_json::json!({"transform": {"transform": "translateY(10px)"}}),
        );
        let mut parent = parent;
        parent.children = vec![child];
        let lines = run_rules(&[parent], None);
        assert_check(&lines, "fixed-broken-by-transform", RuleStatus::Warn);
    }

    #[test]
    fn fixed_without_transform_ancestor_is_clean() {
        let child = raw_node(
            "div.toast",
            "DIV",
            serde_json::json!({"layout": {"position": "fixed"}}),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let parent = node_with("div.plain", serde_json::json!({"transform": {}}));
        let mut parent = parent;
        parent.children = vec![child];
        let lines = run_rules(&[parent], None);
        assert!(!lines.iter().any(|l| l.check == "fixed-broken-by-transform"));
    }

    #[test]
    fn absolute_without_insets_fires() {
        let node = raw_node(
            "div.dropdown",
            "DIV",
            serde_json::json!({
                "layout": {"position": "absolute", "top": "auto", "left": "auto"}
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert_check(&lines, "absolute-without-insets", RuleStatus::Warn);
    }

    #[test]
    fn absolute_with_insets_is_clean() {
        let node = raw_node(
            "div.dropdown",
            "DIV",
            serde_json::json!({
                "layout": {"position": "absolute", "top": "10px", "left": "0px"}
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert!(!lines.iter().any(|l| l.check == "absolute-without-insets"));
    }

    #[test]
    fn interactive_pointer_events_none_fails() {
        let node = raw_node(
            "button.dead",
            "BUTTON",
            serde_json::json!({"interaction": {"pointer-events": "none"}}),
            serde_json::json!({"focusable": true, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert_check(&lines, "interactive-pointer-events-none", RuleStatus::Fail);
    }

    #[test]
    fn aria_hidden_focusable_fails() {
        let node = raw_node(
            "a.hidden-focus",
            "A",
            serde_json::json!({"layout": {}}),
            serde_json::json!({"focusable": true, "ariaHidden": true, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert_check(&lines, "aria-hidden-focusable", RuleStatus::Fail);
    }

    #[test]
    fn ellipsis_without_clip_fires() {
        let node = raw_node(
            "span.title",
            "SPAN",
            serde_json::json!({
                "typography": {"text-overflow": "ellipsis", "white-space": "normal"},
                "box_model": {"overflow-x": "visible"}
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert_check(&lines, "ellipsis-without-clip", RuleStatus::Warn);
    }

    #[test]
    fn ellipsis_with_clip_is_clean() {
        let node = raw_node(
            "span.title",
            "SPAN",
            serde_json::json!({
                "typography": {"text-overflow": "ellipsis", "white-space": "nowrap"},
                "box_model": {"overflow-x": "hidden"}
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert!(!lines.iter().any(|l| l.check == "ellipsis-without-clip"));
    }

    #[test]
    fn width_100_with_padding_fires() {
        let node = raw_node(
            "div.wide",
            "DIV",
            serde_json::json!({
                "box_model": {
                    "box-sizing": "content-box",
                    "width": "100%",
                    "padding-left": "16px",
                    "padding-right": "16px"
                }
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert_check(&lines, "width-100-with-padding", RuleStatus::Warn);
    }

    #[test]
    fn border_box_full_width_is_clean() {
        let node = raw_node(
            "div.wide",
            "DIV",
            serde_json::json!({
                "box_model": {
                    "box-sizing": "border-box",
                    "width": "100%",
                    "padding-left": "16px"
                }
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert!(!lines.iter().any(|l| l.check == "width-100-with-padding"));
    }

    #[test]
    fn small_text_fires_below_12px() {
        let node = raw_node(
            "p.tiny",
            "P",
            serde_json::json!({"typography": {"font-size": "10px"}}),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert_check(&lines, "small-text", RuleStatus::Warn);
    }

    #[test]
    fn small_thumbnail_fires_for_clickable_bg_image() {
        // A clickable div with background-image smaller than 150×150 should warn.
        let node = DiffNode {
            id: 0,
            parent_id: None,
            selector: "div.img".into(),
            tag: Some("DIV".into()),
            path: Some("div.img".into()),
            depth: Some(0),
            rect: Some(serde_json::json!({"x": 0, "y": 0, "width": 137, "height": 90})),
            metrics: None,
            noticeable: Some(serde_json::json!({
                "display_visible": true, "accessibility_grade": "AAA"
            })),
            hash: None,
            styles: Some(
                serde_json::json!({
                    "visual": {"background-image": "url(photo.jpg)"},
                    "interaction": {"cursor": "pointer"}
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            pseudo: None,
            aria: Some(serde_json::json!({"focusable": false, "has_text": false})),
            contrast: None,
            ax: None,
            attributes: None,
            children: vec![],
        };
        let lines = run_rules(&[node], None);
        assert_check(&lines, "small-thumbnail", RuleStatus::Warn);
    }

    #[test]
    fn small_thumbnail_skips_large_image() {
        // A clickable div with background-image >=150×150 should NOT warn.
        let node = DiffNode {
            id: 0,
            parent_id: None,
            selector: "div.img".into(),
            tag: Some("DIV".into()),
            path: Some("div.img".into()),
            depth: Some(0),
            rect: Some(serde_json::json!({"x": 0, "y": 0, "width": 200, "height": 200})),
            metrics: None,
            noticeable: Some(serde_json::json!({
                "display_visible": true, "accessibility_grade": "AAA"
            })),
            hash: None,
            styles: Some(
                serde_json::json!({
                    "visual": {"background-image": "url(photo.jpg)"},
                    "interaction": {"cursor": "pointer"}
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            pseudo: None,
            aria: Some(serde_json::json!({"focusable": false, "has_text": false})),
            contrast: None,
            ax: None,
            attributes: None,
            children: vec![],
        };
        let lines = run_rules(&[node], None);
        let hits: Vec<_> = lines
            .iter()
            .filter(|l| l.check == "small-thumbnail")
            .collect();
        assert!(
            hits.is_empty(),
            "large thumbnail should not trigger small-thumbnail"
        );
    }

    #[test]
    fn small_thumbnail_skips_non_clickable() {
        // A non-clickable div with background-image should NOT warn.
        let node = DiffNode {
            id: 0,
            parent_id: None,
            selector: "div.hero".into(),
            tag: Some("DIV".into()),
            path: Some("div.hero".into()),
            depth: Some(0),
            rect: Some(serde_json::json!({"x": 0, "y": 0, "width": 100, "height": 100})),
            metrics: None,
            noticeable: Some(serde_json::json!({
                "display_visible": true, "accessibility_grade": "AAA"
            })),
            hash: None,
            styles: Some(
                serde_json::json!({
                    "visual": {"background-image": "url(hero.jpg)"},
                    "interaction": {"cursor": "auto"}
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            pseudo: None,
            aria: Some(serde_json::json!({"focusable": false, "has_text": false})),
            contrast: None,
            ax: None,
            attributes: None,
            children: vec![],
        };
        let lines = run_rules(&[node], None);
        let hits: Vec<_> = lines
            .iter()
            .filter(|l| l.check == "small-thumbnail")
            .collect();
        assert!(
            hits.is_empty(),
            "non-clickable element should not trigger small-thumbnail"
        );
    }

    #[test]
    fn line_height_below_font_size_fires() {
        let node = raw_node(
            "p.cramped",
            "P",
            serde_json::json!({
                "typography": {"font-size": "16px", "line-height": "12px"}
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert_check(&lines, "line-height-below-font-size", RuleStatus::Warn);
    }

    #[test]
    fn unitless_line_height_resolves_against_font_size() {
        // line-height:1 with 16px font → 16px, NOT below → clean.
        let node = raw_node(
            "p.ok",
            "P",
            serde_json::json!({
                "typography": {"font-size": "16px", "line-height": "1"}
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert!(
            !lines
                .iter()
                .any(|l| l.check == "line-height-below-font-size")
        );
        // line-height:0.8 with 16px font → 12.8px < 16px → fires.
        let node = raw_node(
            "p.bad",
            "P",
            serde_json::json!({
                "typography": {"font-size": "16px", "line-height": "0.8"}
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert_check(&lines, "line-height-below-font-size", RuleStatus::Warn);
    }

    #[test]
    fn z_index_on_static_non_flex_fires() {
        let child = raw_node(
            "div.x",
            "DIV",
            serde_json::json!({
                "layout": {"position": "static", "z-index": "10"}
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let parent = node_with(
            "div.plain",
            serde_json::json!({"layout": {"display": "block"}}),
        );
        let mut parent = parent;
        parent.children = vec![child];
        let lines = run_rules(&[parent], None);
        assert_check(&lines, "z-index-on-static", RuleStatus::Warn);
    }

    #[test]
    fn z_index_on_static_flex_child_is_clean() {
        let child = raw_node(
            "div.x",
            "DIV",
            serde_json::json!({
                "layout": {"position": "static", "z-index": "10"}
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let parent = node_with(
            "div.flex",
            serde_json::json!({"layout": {"display": "flex"}}),
        );
        let mut parent = parent;
        parent.children = vec![child];
        let lines = run_rules(&[parent], None);
        assert!(!lines.iter().any(|l| l.check == "z-index-on-static"));
    }

    #[test]
    fn control_without_name_fires_for_icon_button() {
        let node = raw_node(
            "button.icon-only",
            "BUTTON",
            serde_json::json!({"layout": {}}),
            serde_json::json!({"focusable": true, "has_text": false}),
        );
        let lines = run_rules(&[node], None);
        assert_check(&lines, "control-without-name", RuleStatus::Warn);
    }

    #[test]
    fn control_with_name_is_clean() {
        let node = raw_node(
            "button.save",
            "BUTTON",
            serde_json::json!({"layout": {}}),
            serde_json::json!({"focusable": true, "has_text": true, "name": "Save"}),
        );
        let lines = run_rules(&[node], None);
        assert!(!lines.iter().any(|l| l.check == "control-without-name"));
    }

    #[test]
    fn text_not_selectable_fires() {
        let node = raw_node(
            "p.copy-blocked",
            "P",
            serde_json::json!({"interaction": {"user-select": "none"}}),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert_check(&lines, "text-not-selectable", RuleStatus::Warn);
    }

    #[test]
    fn infinite_fast_animation_fires() {
        let node = raw_node(
            "div.blink",
            "DIV",
            serde_json::json!({
                "animation": {
                    "animation-iteration-count": "infinite",
                    "animation-duration": "0.3s"
                }
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert_check(&lines, "infinite-fast-animation", RuleStatus::Warn);
    }

    #[test]
    fn slow_infinite_animation_is_clean() {
        let node = raw_node(
            "div.pulse",
            "DIV",
            serde_json::json!({
                "animation": {
                    "animation-iteration-count": "infinite",
                    "animation-duration": "2s"
                }
            }),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert!(!lines.iter().any(|l| l.check == "infinite-fast-animation"));
    }

    #[test]
    fn transition_all_fires() {
        let node = raw_node(
            "a.smooth",
            "A",
            serde_json::json!({"animation": {"transition-property": "all"}}),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert_check(&lines, "transition-all", RuleStatus::Warn);
    }

    #[test]
    fn overflow_x_hidden_on_body_fires() {
        let node = raw_node(
            "body",
            "BODY",
            serde_json::json!({"box_model": {"overflow-x": "hidden"}}),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let lines = run_rules(&[node], None);
        assert_check(&lines, "overflow-x-hidden-on-body", RuleStatus::Warn);
    }

    #[test]
    fn horizontal_overflow_fires_with_viewport() {
        let mut node = node_with("div.too-wide", serde_json::json!({"box_model": {}}));
        node.rect =
            Some(serde_json::json!({"x": 1200.0, "y": 0.0, "width": 400.0, "height": 100.0}));
        let lines = run_rules(&[node], Some((1366.0, 768.0)));
        assert_check(&lines, "horizontal-overflow", RuleStatus::Warn);
    }

    #[test]
    fn horizontal_overflow_without_viewport_is_skipped() {
        let node = node_with("div.wide", serde_json::json!({"box_model": {}}));
        let lines = run_rules(&[node], None);
        assert!(!lines.iter().any(|l| l.check == "horizontal-overflow"));
    }

    #[test]
    fn horizontal_overflow_clipped_by_ancestor_is_clean() {
        let child = raw_node(
            "div.wide",
            "DIV",
            serde_json::json!({"box_model": {}}),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let parent = node_with(
            "div.clip",
            serde_json::json!({"box_model": {"overflow-x": "hidden"}}),
        );
        let mut parent = parent;
        parent.children = vec![child];
        let lines = run_rules(&[parent], Some((1366.0, 768.0)));
        assert!(!lines.iter().any(|l| l.check == "horizontal-overflow"));
    }

    #[test]
    fn backdrop_over_modal_fires_when_scrim_covers_content() {
        // Modal content inside a fixed translucent-dark backdrop, painted
        // below the scrim (negative z-index) → the generic `occluded` rule
        // skips ancestor/descendant, but backdrop-over-modal must flag it.
        let content = raw_node(
            "div.modal-content",
            "DIV",
            serde_json::json!({"layout": {"position": "relative", "z-index": "-1"}}),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        let backdrop = raw_node(
            "div.modal-backdrop",
            "DIV",
            serde_json::json!({
                "layout": {"position": "fixed"},
                "visual": {"background-color": "rgba(0, 0, 0, 0.5)"}
            }),
            serde_json::json!({"focusable": false, "has_text": false}),
        );
        let viewport = (1366.0, 768.0);
        // Backdrop covers the whole viewport; content sits on top of it.
        let mut backdrop = backdrop;
        backdrop.rect =
            Some(serde_json::json!({"x": 0.0, "y": 0.0, "width": 1366.0, "height": 768.0}));
        let mut content = content;
        content.rect =
            Some(serde_json::json!({"x": 300.0, "y": 200.0, "width": 500.0, "height": 300.0}));
        backdrop.children = vec![content];
        let lines = run_rules(&[backdrop], Some(viewport));
        assert_check(&lines, "backdrop-over-modal", RuleStatus::Fail);
    }

    #[test]
    fn backdrop_over_modal_skips_non_dark_or_opaque_scrim() {
        // Opaque black (alpha=1) is a solid overlay, not a scrim: the modal
        // content would be invisible behind it → still a fail (the scrim is
        // over the dialog). White translucent is not a "dark scrim" → clean.
        let white = raw_node(
            "div.backdrop",
            "DIV",
            serde_json::json!({
                "layout": {"position": "fixed"},
                "visual": {"background-color": "rgba(255, 255, 255, 0.5)"}
            }),
            serde_json::json!({"focusable": false, "has_text": false}),
        );
        let mut white = white;
        white.rect =
            Some(serde_json::json!({"x": 0.0, "y": 0.0, "width": 1366.0, "height": 768.0}));
        let mut content = raw_node(
            "div.modal-content",
            "DIV",
            serde_json::json!({"layout": {"position": "relative", "z-index": "-1"}}),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        content.rect =
            Some(serde_json::json!({"x": 300.0, "y": 200.0, "width": 500.0, "height": 300.0}));
        white.children = vec![content];
        let lines = run_rules(&[white], Some((1366.0, 768.0)));
        assert!(
            !lines.iter().any(|l| l.check == "backdrop-over-modal"),
            "white translucent bg is not a dark scrim: {lines:#?}"
        );
    }

    #[test]
    fn backdrop_does_not_fire_when_descendant_paints_above() {
        // A dialog child with a non-negative z-index paints ABOVE the scrim
        // (correct stacking) — must NOT be flagged even though it is covered
        // by the backdrop's rect.
        let mut backdrop = raw_node(
            "div.backdrop",
            "DIV",
            serde_json::json!({
                "layout": {"position": "fixed"},
                "visual": {"background-color": "#00000099"}
            }),
            serde_json::json!({"focusable": false, "has_text": false}),
        );
        backdrop.rect =
            Some(serde_json::json!({"x": 0.0, "y": 0.0, "width": 1366.0, "height": 768.0}));
        let mut content = raw_node(
            "div.modal-content",
            "DIV",
            serde_json::json!({"layout": {"position": "fixed", "z-index": "50"}}),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        content.rect =
            Some(serde_json::json!({"x": 300.0, "y": 200.0, "width": 500.0, "height": 300.0}));
        backdrop.children = vec![content];
        let lines = run_rules(&[backdrop], Some((1366.0, 768.0)));
        assert!(
            !lines.iter().any(|l| l.check == "backdrop-over-modal"),
            "descendant above the scrim is correct stacking: {lines:#?}"
        );
    }

    #[test]
    fn helper_parsers_handle_units() {
        assert_eq!(parse_px("16px"), Some(16.0));
        assert_eq!(parse_px("1rem"), Some(16.0));
        assert_eq!(parse_px("2em"), Some(32.0));
        assert_eq!(parse_px("12"), Some(12.0));
        assert_eq!(parse_px("auto"), None);
        assert_eq!(parse_duration("0.3s"), Some(0.3));
        assert_eq!(parse_duration("300ms"), Some(0.3));
        assert_eq!(parse_duration("2s"), Some(2.0));
        assert_eq!(parse_line_height("12px", None), Some(12.0));
        assert_eq!(parse_line_height("normal", Some(16.0)), Some(19.2));
        assert_eq!(parse_line_height("1.5", Some(16.0)), Some(24.0));
        assert!(is_translucent_dark_bg("rgba(0, 0, 0, 0.5)"));
        assert!(is_translucent_dark_bg("#00000099"));
        assert!(!is_translucent_dark_bg("#000000")); // opaque → no scrim
        assert!(!is_translucent_dark_bg("#ffffff99")); // light → not a scrim
    }

    #[test]
    fn backdrop_over_modal_fires_with_normalized_hex_scrim() {
        // The engine normalizes rgba(0,0,0,0.6) → #00000099 by default; the
        // rule must still recognize the scrim and flag the covered modal.
        let mut backdrop = raw_node(
            "div.backdrop",
            "DIV",
            serde_json::json!({
                "layout": {"position": "fixed"},
                "visual": {"background-color": "#00000099"}
            }),
            serde_json::json!({"focusable": false, "has_text": false}),
        );
        backdrop.rect =
            Some(serde_json::json!({"x": 0.0, "y": 0.0, "width": 1366.0, "height": 768.0}));
        let mut content = raw_node(
            "div.modal-content",
            "DIV",
            serde_json::json!({"layout": {"position": "fixed", "z-index": "-1"}}),
            serde_json::json!({"focusable": false, "has_text": true}),
        );
        content.rect =
            Some(serde_json::json!({"x": 409.0, "y": 230.0, "width": 300.0, "height": 200.0}));
        backdrop.children = vec![content];
        let lines = run_rules(&[backdrop], Some((1366.0, 768.0)));
        assert_check(&lines, "backdrop-over-modal", RuleStatus::Fail);
    }
}
