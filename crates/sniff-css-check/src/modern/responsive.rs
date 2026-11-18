//! Comparative checks over `__meta.matrix` states (roadmap F3).
//!
//! Each matrix state is a consecutive run of forest roots. Nodes from
//! different states are paired by `path` (falling back to `selector`) plus a
//! per-state occurrence counter, so repeated selectors stay distinguishable.
//! Incomplete matrices are skipped entirely: the CLI emits the envelope
//! warning, while checks never report partial evidence.

use crate::modern::{CheckSpec, Facet, ModernCtx};
use crate::rules::CheckLine;
use serde_json::Value;
use sniff_core::Viewport;
use sniff_core::modern::{Evidence, MatrixTag};
use sniff_css_diff::{DiffDocument, DiffNode};
use std::collections::{BTreeMap, BTreeSet};

/// Largest rect shift treated as ordinary reflow between two states.
const JUMP_PX: f64 = 24.0;
/// Sub-pixel tolerance for geometry comparisons.
const TOLERANCE_PX: f64 = 1.0;
/// WCAG 2.2 SC 2.5.8 minimum target size (used as the regression floor).
const MIN_TARGET_PX: f64 = 24.0;
/// Bound on ancestor walks, so malformed parent links cannot loop forever.
const MAX_ANCESTOR_DEPTH: u8 = 32;

/// Stable pairing key: node path (or selector) plus its ordinal in the state.
type NodeKey = (String, usize);

const BREAKPOINT_JUMP: CheckSpec = CheckSpec::new(
    "breakpoint-jump",
    &["responsive-design"],
    &[Facet::Matrix, Facet::Rect],
    "keepWhen a paired rect moves by at most JUMP_PX in every axis between adjacent states, or either state has no rect.",
);

const OVERFLOW_ONLY_AT_WIDTH: CheckSpec = CheckSpec::new(
    "overflow-only-at-width",
    &["responsive-design"],
    &[Facet::Matrix, Facet::Rect],
    "keepWhen the node has no viewport-driven horizontal overflow in any state, its overflow-x allows scrolling, or every observed state overflows.",
);

const DARK_MODE_IGNORED: CheckSpec = CheckSpec::new(
    "dark-mode-ignored",
    &["dark-mode"],
    &[Facet::Matrix, Facet::Env, Facet::Styles],
    "keepWhen the theme background changes with prefers-color-scheme, dark tokens are declared, meta color-scheme includes dark, or light-only styling is explicit in __declared.",
);

const REDUCED_MOTION_IGNORED: CheckSpec = CheckSpec::new(
    "reduced-motion-ignored",
    &["natural-interactions"],
    &[Facet::Matrix, Facet::Env, Facet::Styles],
    "keepWhen animation styles differ between reduce and no-preference, a prefers-reduced-motion guard is declared, or the node has no motion signal.",
);

const COARSE_TARGETS: CheckSpec = CheckSpec::new(
    "coarse-targets",
    &["form-factor-adaptation"],
    &[Facet::Matrix, Facet::Env, Facet::Rect],
    "keepWhen the target is at least 24px in the coarse state, it was already undersized in the fine state (see coarse-pointer-target-size), or no coarse/fine pair exists.",
);

const FIXED_VIEWPORT_HEIGHT: CheckSpec = CheckSpec::new(
    "fixed-viewport-height",
    &["responsive-design"],
    &[Facet::Matrix, Facet::Styles],
    "keepWhen the height does not track the viewport height in both states, dynamic viewport units are declared, or fewer than two viewport heights are observed.",
);

/// All check ids owned by the responsive family.
pub fn specs() -> &'static [CheckSpec] {
    &[
        BREAKPOINT_JUMP,
        OVERFLOW_ONLY_AT_WIDTH,
        DARK_MODE_IGNORED,
        REDUCED_MOTION_IGNORED,
        COARSE_TARGETS,
        FIXED_VIEWPORT_HEIGHT,
    ]
}

/// Runs the enabled comparative checks against the decomposed matrix.
pub fn run(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    let states = build_states(ctx.doc);
    if states.is_empty() {
        return;
    }
    tracing::debug!(states = states.len(), "responsive matrix decomposed");
    check_breakpoint_jump(ctx, &states, out);
    check_overflow_only_at_width(ctx, &states, out);
    check_dark_mode_ignored(ctx, &states, out);
    check_reduced_motion_ignored(ctx, &states, out);
    check_coarse_targets(ctx, &states, out);
    check_fixed_viewport_height(ctx, &states, out);
}

/// One observed matrix state: its tag and the nodes keyed for cross-state pairing.
struct State<'a> {
    tag: &'a MatrixTag,
    nodes: BTreeMap<NodeKey, &'a DiffNode>,
    parents: BTreeMap<u64, &'a DiffNode>,
}

/// Worst horizontal overflow observed for a paired node across all states.
struct OverflowRecord<'a> {
    node: &'a DiffNode,
    viewport_width: u32,
    overflow_px: f64,
    seen_clean: bool,
}

/// Splits the forest into complete, ordered matrix states.
fn build_states(doc: &DiffDocument) -> Vec<State<'_>> {
    let tags = &doc.matrix;
    let roots = &doc.nodes;
    let observed = tags.len();
    if observed < 2 || roots.is_empty() {
        tracing::trace!(states = observed, "matrix needs at least two states");
        return Vec::new();
    }
    if tags.iter().any(|tag| tag.total != observed) {
        tracing::warn!(
            observed = observed,
            "incomplete __meta.matrix; comparative checks skipped"
        );
        return Vec::new();
    }
    let mut ordered: Vec<&MatrixTag> = tags.iter().collect();
    ordered.sort_by_key(|tag| tag.index);
    if ordered
        .iter()
        .enumerate()
        .any(|(position, tag)| tag.index != position)
    {
        tracing::warn!("non-contiguous matrix indices; comparative checks skipped");
        return Vec::new();
    }
    let has_ranges = ordered.iter().all(|tag| {
        matches!((tag.node_id_start, tag.node_id_end), (Some(start), Some(end)) if start <= end)
    });
    if !has_ranges {
        let Some(per_state) = roots.len().checked_div(observed) else {
            return Vec::new();
        };
        if per_state == 0 || roots.len().rem_euclid(observed) != 0 {
            tracing::warn!(
                roots = roots.len(),
                states = observed,
                "unequal matrix state forests"
            );
            return Vec::new();
        }
    }
    let mut states = Vec::with_capacity(observed);
    for (index, tag) in ordered.into_iter().enumerate() {
        let chunk: &[DiffNode] =
            if let (Some(start_id), Some(end_id)) = (tag.node_id_start, tag.node_id_end) {
                let Some(start) = roots.iter().position(|node| node.id >= start_id) else {
                    return Vec::new();
                };
                let end = roots
                    .iter()
                    .position(|node| node.id > end_id)
                    .unwrap_or(roots.len());
                let Some(chunk) = roots.get(start..end) else {
                    return Vec::new();
                };
                chunk
            } else {
                let Some(per_state) = roots.len().checked_div(observed) else {
                    return Vec::new();
                };
                let Some(start) = index.checked_mul(per_state) else {
                    return Vec::new();
                };
                let Some(end) = start.checked_add(per_state) else {
                    return Vec::new();
                };
                let Some(chunk) = roots.get(start..end) else {
                    return Vec::new();
                };
                chunk
            };
        if chunk.is_empty() {
            tracing::warn!(
                state = tag.index,
                "empty matrix state; comparative checks skipped"
            );
            return Vec::new();
        }
        states.push(State {
            tag,
            nodes: index_nodes(chunk),
            parents: index_parents(chunk),
        });
    }
    states
}

/// Flattens a state's roots in pre-order.
fn flatten_roots(roots: &[DiffNode]) -> Vec<&DiffNode> {
    fn visit<'a>(node: &'a DiffNode, out: &mut Vec<&'a DiffNode>) {
        out.push(node);
        for child in &node.children {
            visit(child, out);
        }
    }
    let mut out = Vec::new();
    for root in roots {
        visit(root, &mut out);
    }
    out
}

/// Chooses the pairing key of a node, preferring the DOM path.
fn node_key(node: &DiffNode) -> &str {
    match node.path.as_deref() {
        Some(path) if !path.trim().is_empty() => path,
        _ => node.selector.as_str(),
    }
}

/// Indexes a state's nodes by `(path-or-selector, occurrence)`.
fn index_nodes(roots: &[DiffNode]) -> BTreeMap<NodeKey, &DiffNode> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for node in flatten_roots(roots) {
        let key = node_key(node);
        if key.is_empty() {
            continue;
        }
        let owned = key.to_string();
        let ordinal = counts
            .entry(owned.clone())
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1)
            .saturating_sub(1);
        out.insert((owned, ordinal), node);
    }
    out
}

/// Indexes a state's nodes by stable id for ancestor walks.
fn index_parents(roots: &[DiffNode]) -> BTreeMap<u64, &DiffNode> {
    let mut out = BTreeMap::new();
    for node in flatten_roots(roots) {
        out.insert(node.id, node);
    }
    out
}

/// Reads a finite numeric field of a bounding rect.
fn rect_number(node: &DiffNode, field: &str) -> Option<f64> {
    node.rect
        .as_ref()?
        .get(field)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

/// Right edge of a node's bounding rect.
fn right_edge(node: &DiffNode) -> Option<f64> {
    Some(rect_number(node, "x")? + rect_number(node, "width")?)
}

/// Emulation value of a state, when this matrix state overrides it.
fn emulated<'b>(state: &'b State<'_>, key: &str) -> Option<&'b str> {
    state.tag.emulation.get(key).map(String::as_str)
}

/// Preference fallback for states that rely on the global `__env` facet.
fn media_value<'b>(ctx: &'b ModernCtx<'_>, key: &str) -> Option<&'b str> {
    ctx.doc.env.as_ref()?.media.get(key).map(String::as_str)
}

/// Preference of a state: matrix emulation first, then `__env.media`.
fn preference<'b>(ctx: &'b ModernCtx<'_>, state: &'b State<'_>, key: &str) -> Option<&'b str> {
    emulated(state, key).or_else(|| media_value(ctx, key))
}

/// Whether an at-rule guard for a preference was declared.
fn declared_guard(ctx: &ModernCtx<'_>, needle: &str) -> bool {
    ctx.doc.declared.as_ref().is_some_and(|declared| {
        declared
            .at_rules
            .iter()
            .any(|rule| rule.condition_text.contains(needle))
    })
}

/// Whether a paired node is a root-level theme carrier.
fn theme_score(ctx: &ModernCtx<'_>, node: &DiffNode) -> u8 {
    if matches!(ctx.tag(node), "BODY" | "HTML") {
        2
    } else if node.selector.contains("body") || node.selector.contains("html") {
        1
    } else {
        0
    }
}

/// True for nodes with a real animation or scroll-linked motion signal.
fn has_motion(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    let name = ctx
        .style(node, "animation", "animation-name")
        .unwrap_or("none");
    let duration = ctx
        .style(node, "animation", "animation-duration")
        .unwrap_or("0s");
    let timeline = ctx
        .style(node, "animation", "animation-timeline")
        .unwrap_or("auto");
    let view = ctx
        .style(node, "animation", "view-timeline-name")
        .unwrap_or("none");
    let iterations = ctx
        .style(node, "animation", "animation-iteration-count")
        .unwrap_or("1");
    (name != "none" && duration != "0s")
        || timeline != "auto"
        || view != "none"
        || iterations == "infinite"
}

/// Motion properties that a reduced-motion guard would have to change.
const MOTION_PROPERTIES: &[&str] = &[
    "animation-name",
    "animation-duration",
    "animation-delay",
    "animation-play-state",
    "animation-iteration-count",
    "animation-timeline",
    "view-timeline-name",
    "transition-property",
    "transition-duration",
];

/// Whether any motion property differs between two paired states.
fn motion_changed(ctx: &ModernCtx<'_>, first: &DiffNode, second: &DiffNode) -> bool {
    MOTION_PROPERTIES.iter().any(|property| {
        ctx.style(first, "animation", property) != ctx.style(second, "animation", property)
    })
}

/// Whether an interactive element can be activated by touch.
fn is_interactive(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    ctx.aria_bool(node, "focusable")
        || matches!(
            ctx.tag(node),
            "BUTTON" | "INPUT" | "SELECT" | "TEXTAREA" | "A" | "SUMMARY"
        )
}

/// Whether the node itself is an intentional horizontal scroller.
fn scrollable_x(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    matches!(
        ctx.style(node, "box_model", "overflow-x"),
        Some("auto" | "scroll" | "hidden" | "clip")
    )
}

/// Whether the node sits inside a horizontal scroller in its own state.
fn inside_scroller(node: &DiffNode, state: &State<'_>, ctx: &ModernCtx<'_>) -> bool {
    let mut current = node;
    let mut visited = BTreeSet::new();
    for _ in 0..MAX_ANCESTOR_DEPTH {
        if !visited.insert(current.id) || scrollable_x(ctx, current) {
            return true;
        }
        let Some(parent_id) = current.parent_id else {
            return false;
        };
        let Some(parent) = state.parents.get(&parent_id) else {
            return false;
        };
        current = parent;
    }
    false
}

/// Whether a computed height value represents the full viewport height.
fn is_viewport_height(value: Option<&str>, viewport: Viewport) -> bool {
    let Some(value) = value else {
        return false;
    };
    let text = value.trim();
    if let Some(units) = text.strip_suffix("vh") {
        return units
            .trim()
            .parse::<f64>()
            .is_ok_and(|value| (value - 100.0).abs() <= TOLERANCE_PX);
    }
    let Some(pixels) = text
        .strip_suffix("px")
        .and_then(|value| value.trim().parse::<f64>().ok())
    else {
        return false;
    };
    (pixels - f64::from(viewport.height)).abs() <= TOLERANCE_PX
}

/// Whether an explicit height uses dynamic viewport units (a safe alternative).
fn dynamic_height_declared(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    ctx.doc.declared.as_ref().is_some_and(|declared| {
        declared.rules.iter().any(|rule| {
            rule.selector == node.selector
                && rule.declarations.get("height").is_some_and(|value| {
                    ["dvh", "svh", "lvh"]
                        .iter()
                        .any(|unit| value.contains(unit))
                })
        })
    })
}

/// Logs one emission without ever logging snapshot contents.
fn log_emit(check: &str, selector: &str) {
    tracing::debug!(id = check, selector = selector, "responsive check emitted");
}

/// Viewport size change between two adjacent states (zero when unknown).
fn viewport_delta(first: &State<'_>, second: &State<'_>) -> (f64, f64) {
    match (first.tag.viewport, second.tag.viewport) {
        (Some(before), Some(after)) => (
            f64::from(after.width) - f64::from(before.width),
            f64::from(after.height) - f64::from(before.height),
        ),
        _ => (0.0, 0.0),
    }
}

/// Detects abrupt rect shifts between adjacent matrix states.
fn check_breakpoint_jump(ctx: &ModernCtx<'_>, states: &[State<'_>], out: &mut Vec<CheckLine>) {
    if !ctx.enabled(&BREAKPOINT_JUMP) {
        return;
    }
    for (index, before) in states.iter().enumerate() {
        for after in states.iter().skip(index.saturating_add(1)) {
            // Emulation-only transitions are not breakpoints; coarse mode also
            // changes device metrics without changing the tagged viewport.
            if before.tag.emulation != after.tag.emulation
                || before.tag.viewport == after.tag.viewport
            {
                continue;
            }
            let (viewport_dx, viewport_dy) = viewport_delta(before, after);
            for (key, left) in &before.nodes {
                let Some(right) = after.nodes.get(key) else {
                    continue;
                };
                let (Some(lx), Some(ly), Some(lw), Some(lh)) = (
                    rect_number(left, "x"),
                    rect_number(left, "y"),
                    rect_number(left, "width"),
                    rect_number(left, "height"),
                ) else {
                    continue;
                };
                let (Some(rx), Some(ry), Some(rw), Some(rh)) = (
                    rect_number(right, "x"),
                    rect_number(right, "y"),
                    rect_number(right, "width"),
                    rect_number(right, "height"),
                ) else {
                    continue;
                };
                // A shift proportional to the viewport is ordinary responsive
                // reflow, not a jump; only the unexplained excess is evidence.
                // Height is excluded: narrowing a viewport legitimately makes
                // the document taller, which says nothing about an abrupt jump.
                let dx = ((rx - lx).abs() - viewport_dx.abs()).max(0.0);
                let dy = ((ry - ly).abs() - viewport_dy.abs()).max(0.0);
                let excess_width = ((rw - lw).abs() - viewport_dx.abs()).max(0.0);
                let largest = dx.max(dy).max(excess_width);
                if largest <= JUMP_PX {
                    continue;
                }
                let evidence = Evidence::numeric(
                    format!(
                        "rect moved from ({lx:.0},{ly:.0}) {lw:.0}x{lh:.0} to ({rx:.0},{ry:.0}) {rw:.0}x{rh:.0} between matrix states {} and {}",
                        before.tag.index, after.tag.index
                    ),
                    "__meta.matrix/rect",
                    "max_rect_delta_px",
                    largest,
                    JUMP_PX,
                    ">",
                );
                ctx.emit(out, &BREAKPOINT_JUMP, right, evidence);
                log_emit(BREAKPOINT_JUMP.id, &right.selector);
            }
        }
    }
}

/// Collects the worst overflow per paired node across all viewport states.
fn overflow_records<'a>(
    ctx: &ModernCtx<'_>,
    states: &[State<'a>],
) -> BTreeMap<NodeKey, OverflowRecord<'a>> {
    let mut records: BTreeMap<NodeKey, OverflowRecord<'_>> = BTreeMap::new();
    for state in states {
        let Some(viewport) = state.tag.viewport else {
            continue;
        };
        for (key, node) in &state.nodes {
            let Some(right) = right_edge(node) else {
                continue;
            };
            let overflow_px = right - f64::from(viewport.width);
            let record = records
                .entry(key.clone())
                .or_insert_with(|| OverflowRecord {
                    node,
                    viewport_width: viewport.width,
                    overflow_px: 0.0,
                    seen_clean: false,
                });
            if overflow_px <= TOLERANCE_PX {
                record.seen_clean = true;
            } else if !inside_scroller(node, state, ctx)
                && (record.overflow_px <= TOLERANCE_PX || viewport.width < record.viewport_width)
            {
                record.node = node;
                record.viewport_width = viewport.width;
                record.overflow_px = overflow_px;
            }
        }
    }
    records
}

/// Detects horizontal overflow that only happens at some observed widths.
fn check_overflow_only_at_width(
    ctx: &ModernCtx<'_>,
    states: &[State<'_>],
    out: &mut Vec<CheckLine>,
) {
    if !ctx.enabled(&OVERFLOW_ONLY_AT_WIDTH) {
        return;
    }
    for (_, record) in overflow_records(ctx, states) {
        if !record.seen_clean || record.overflow_px <= TOLERANCE_PX {
            continue;
        }
        let evidence = Evidence::numeric(
            format!(
                "rect exceeds viewport width {} by {:.1}px only in this matrix state",
                record.viewport_width, record.overflow_px
            ),
            "__meta.matrix/rect",
            "horizontal_overflow_px",
            record.overflow_px,
            f64::from(record.viewport_width),
            ">",
        );
        ctx.emit(out, &OVERFLOW_ONLY_AT_WIDTH, record.node, evidence);
        log_emit(OVERFLOW_ONLY_AT_WIDTH.id, &record.node.selector);
    }
}

/// Finds the first light and dark states of a color-scheme matrix.
fn theme_states<'a>(
    ctx: &ModernCtx<'_>,
    states: &'a [State<'a>],
) -> Option<(&'a State<'a>, &'a State<'a>)> {
    let dark = states
        .iter()
        .find(|state| matches!(preference(ctx, state, "prefers-color-scheme"), Some("dark")))?;
    let light = states.iter().find(|state| {
        matches!(
            preference(ctx, state, "prefers-color-scheme"),
            Some("light" | "no-preference")
        )
    })?;
    Some((light, dark))
}

/// Detects a theme background that ignores an emulated dark preference.
fn check_dark_mode_ignored(ctx: &ModernCtx<'_>, states: &[State<'_>], out: &mut Vec<CheckLine>) {
    if !ctx.enabled(&DARK_MODE_IGNORED) {
        return;
    }
    let Some((light, dark)) = theme_states(ctx, states) else {
        return;
    };
    if declared_guard(ctx, "prefers-color-scheme") {
        return;
    }
    let meta_scheme = ctx
        .doc
        .document
        .as_ref()
        .and_then(|document| document.meta.get("color-scheme"));
    if meta_scheme.is_some_and(|value| value.contains("dark")) {
        return;
    }
    let explicit_light = ctx.doc.declared.as_ref().is_some_and(|declared| {
        declared.rules.iter().any(|rule| {
            rule.declarations
                .get("color-scheme")
                .is_some_and(|value| value == "light")
        })
    });
    if explicit_light {
        return;
    }
    let candidate = light
        .nodes
        .iter()
        .filter_map(|(key, node)| {
            let themed = dark.nodes.get(key)?;
            let color = ctx.style(node, "visual", "background-color")?;
            if ctx.style(themed, "visual", "background-color") != Some(color) {
                return None;
            }
            if ctx
                .style(themed, "accessibility", "color-scheme")
                .is_some_and(|value| value.contains("dark"))
            {
                return None;
            }
            Some((theme_score(ctx, node), themed, color))
        })
        .max_by_key(|(score, _, _)| *score);
    if let Some((_, node, color)) = candidate {
        let evidence = Evidence::new(
            format!(
                "prefers-color-scheme=dark but {} keeps background-color {color} from the light state",
                node.selector
            ),
            "__meta.matrix/__env",
        );
        ctx.emit(out, &DARK_MODE_IGNORED, node, evidence);
        log_emit(DARK_MODE_IGNORED.id, &node.selector);
    }
}

/// Finds the first reduce and no-preference states of a motion matrix.
fn motion_states<'a>(
    ctx: &ModernCtx<'_>,
    states: &'a [State<'a>],
) -> Option<(&'a State<'a>, &'a State<'a>)> {
    let normal = states.iter().find(|state| {
        matches!(
            preference(ctx, state, "prefers-reduced-motion"),
            Some("no-preference")
        )
    })?;
    let reduced = states.iter().find(|state| {
        matches!(
            preference(ctx, state, "prefers-reduced-motion"),
            Some("reduce")
        )
    })?;
    Some((normal, reduced))
}

/// Detects animations that are unchanged under `prefers-reduced-motion: reduce`.
fn check_reduced_motion_ignored(
    ctx: &ModernCtx<'_>,
    states: &[State<'_>],
    out: &mut Vec<CheckLine>,
) {
    if !ctx.enabled(&REDUCED_MOTION_IGNORED) {
        return;
    }
    let Some((normal, reduced)) = motion_states(ctx, states) else {
        return;
    };
    if declared_guard(ctx, "prefers-reduced-motion") {
        return;
    }
    for (key, node) in &normal.nodes {
        let Some(quiet) = reduced.nodes.get(key) else {
            continue;
        };
        if !ctx.visible(node) || !has_motion(ctx, node) || motion_changed(ctx, node, quiet) {
            continue;
        }
        let evidence = Evidence::new(
            format!(
                "animation styles on {} are identical under prefers-reduced-motion reduce and no-preference",
                node.selector
            ),
            "__meta.matrix/__env",
        );
        ctx.emit(out, &REDUCED_MOTION_IGNORED, node, evidence);
        log_emit(REDUCED_MOTION_IGNORED.id, &node.selector);
    }
}

/// Finds one coarse-pointer state and one non-coarse comparison state.
fn pointer_states<'a>(
    ctx: &ModernCtx<'_>,
    states: &'a [State<'a>],
) -> Option<(&'a State<'a>, &'a State<'a>)> {
    let is_coarse = |state: &State<'_>| {
        matches!(preference(ctx, state, "pointer"), Some("coarse"))
            || matches!(preference(ctx, state, "any-pointer"), Some("coarse"))
    };
    let coarse = states.iter().find(|state| is_coarse(state))?;
    let fine = states.iter().find(|state| !is_coarse(state))?;
    Some((coarse, fine))
}

/// Detects interactive targets that shrink below the size floor under a coarse pointer.
fn check_coarse_targets(ctx: &ModernCtx<'_>, states: &[State<'_>], out: &mut Vec<CheckLine>) {
    if !ctx.enabled(&COARSE_TARGETS) {
        return;
    }
    let Some((coarse, fine)) = pointer_states(ctx, states) else {
        return;
    };
    for (key, node) in &coarse.nodes {
        let Some(fine_node) = fine.nodes.get(key) else {
            continue;
        };
        if !ctx.visible(node) || !is_interactive(ctx, node) {
            continue;
        }
        let (Some(width), Some(height)) = (rect_number(node, "width"), rect_number(node, "height"))
        else {
            continue;
        };
        let (Some(fine_width), Some(fine_height)) = (
            rect_number(fine_node, "width"),
            rect_number(fine_node, "height"),
        ) else {
            continue;
        };
        let smallest = width.min(height);
        if smallest >= MIN_TARGET_PX || fine_width < MIN_TARGET_PX || fine_height < MIN_TARGET_PX {
            continue;
        }
        let evidence = Evidence::numeric(
            format!(
                "interactive target {} shrank to {width:.0}x{height:.0} under pointer coarse (was {fine_width:.0}x{fine_height:.0})",
                node.selector
            ),
            "__meta.matrix/__env/rect",
            "target_size_px",
            smallest,
            MIN_TARGET_PX,
            "<",
        );
        ctx.emit(out, &COARSE_TARGETS, node, evidence);
        log_emit(COARSE_TARGETS.id, &node.selector);
    }
}

/// Detects heights that track viewport height instead of dynamic viewport units.
fn check_fixed_viewport_height(
    ctx: &ModernCtx<'_>,
    states: &[State<'_>],
    out: &mut Vec<CheckLine>,
) {
    if !ctx.enabled(&FIXED_VIEWPORT_HEIGHT) {
        return;
    }
    for adjacent in states.windows(2) {
        let Some(before) = adjacent.first() else {
            continue;
        };
        let Some(after) = adjacent.get(1) else {
            continue;
        };
        let (Some(first_viewport), Some(second_viewport)) =
            (before.tag.viewport, after.tag.viewport)
        else {
            continue;
        };
        if first_viewport.height == second_viewport.height {
            continue;
        }
        for (key, node) in &before.nodes {
            let Some(other) = after.nodes.get(key) else {
                continue;
            };
            let first_height = ctx.style(node, "box_model", "height");
            let second_height = ctx.style(other, "box_model", "height");
            if !is_viewport_height(first_height, first_viewport)
                || !is_viewport_height(second_height, second_viewport)
                || dynamic_height_declared(ctx, node)
            {
                continue;
            }
            let evidence = Evidence::numeric(
                format!(
                    "{} height follows viewport height ({}px then {}px) instead of dynamic viewport units",
                    node.selector, first_viewport.height, second_viewport.height
                ),
                "__meta.matrix/box_model.height",
                "height_px",
                f64::from(first_viewport.height),
                f64::from(second_viewport.height),
                "==",
            );
            ctx.emit(out, &FIXED_VIEWPORT_HEIGHT, node, evidence);
            log_emit(FIXED_VIEWPORT_HEIGHT.id, &node.selector);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modern::ModernOptions;
    use sniff_core::modern::{DocumentFacet, EnvFacet};
    use std::collections::BTreeMap;

    fn node(
        id: u64,
        key: &str,
        tag: &str,
        rect: Option<(f64, f64, f64, f64)>,
        styles: Value,
        aria: Option<Value>,
    ) -> DiffNode {
        DiffNode {
            id,
            parent_id: None,
            selector: key.to_string(),
            tag: Some(tag.to_string()),
            path: Some(key.to_string()),
            depth: Some(0),
            rect: rect.map(|(x, y, width, height)| {
                serde_json::json!({"x": x, "y": y, "width": width, "height": height})
            }),
            metrics: None,
            noticeable: Some(serde_json::json!({"display_visible": true})),
            hash: None,
            styles: styles.as_object().cloned(),
            pseudo: Some(serde_json::Map::new()),
            aria,
            contrast: None,
            ax: Some(Value::Object(serde_json::Map::new())),
            attributes: Some(Value::Object(serde_json::Map::new())),
            effective_background: None,
            children: Vec::new(),
        }
    }

    fn tag(
        index: usize,
        total: usize,
        viewport: Option<(u32, u32)>,
        emulation: &[(&str, &str)],
    ) -> MatrixTag {
        MatrixTag {
            index,
            total,
            viewport: viewport.map(|(width, height)| Viewport { width, height }),
            emulation: emulation
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
            node_id_start: None,
            node_id_end: None,
        }
    }

    fn doc(tags: Vec<MatrixTag>, states: Vec<Vec<DiffNode>>) -> DiffDocument {
        let mut document = DiffDocument::default();
        document.matrix = tags;
        for mut state in states {
            document.nodes.append(&mut state);
        }
        document
    }

    fn check_ids(out: &[CheckLine]) -> Vec<&str> {
        out.iter().map(|line| line.check.as_str()).collect()
    }

    fn serialized(out: &[CheckLine]) -> Option<String> {
        serde_json::to_string(out).ok()
    }

    #[test]
    fn breakpoint_jump_flags_large_shift_but_not_small_reflow() {
        let shifting = node(
            1,
            ".card",
            "DIV",
            Some((10.0, 20.0, 200.0, 100.0)),
            serde_json::json!({}),
            None,
        );
        let stable = node(
            2,
            ".stable",
            "DIV",
            Some((10.0, 20.0, 100.0, 40.0)),
            serde_json::json!({}),
            None,
        );
        let shifted = node(
            3,
            ".card",
            "DIV",
            Some((10.0, 320.0, 200.0, 100.0)),
            serde_json::json!({}),
            None,
        );
        let still = node(
            4,
            ".stable",
            "DIV",
            Some((10.0, 26.0, 100.0, 40.0)),
            serde_json::json!({}),
            None,
        );
        let document = doc(
            vec![
                tag(0, 2, Some((360, 800)), &[]),
                tag(1, 2, Some((768, 800)), &[]),
            ],
            vec![vec![shifting, stable], vec![shifted, still]],
        );
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert_eq!(check_ids(&out), vec![BREAKPOINT_JUMP.id]);
        assert_eq!(
            out.first().map(|line| line.selector.as_str()),
            Some(".card")
        );
    }

    #[test]
    fn breakpoint_jump_keep_when_shift_is_under_threshold() {
        let before = node(
            1,
            ".card",
            "DIV",
            Some((10.0, 20.0, 200.0, 100.0)),
            serde_json::json!({}),
            None,
        );
        let after = node(
            2,
            ".card",
            "DIV",
            Some((12.0, 28.0, 200.0, 100.0)),
            serde_json::json!({}),
            None,
        );
        let document = doc(
            vec![
                tag(0, 2, Some((360, 800)), &[]),
                tag(1, 2, Some((768, 800)), &[]),
            ],
            vec![vec![before], vec![after]],
        );
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn breakpoint_jump_keeps_ordinary_viewport_reflow() {
        let narrow = node(
            1,
            ".fluid",
            "DIV",
            Some((0.0, 0.0, 300.0, 900.0)),
            serde_json::json!({}),
            None,
        );
        let wide = node(
            2,
            ".fluid",
            "DIV",
            Some((0.0, 0.0, 708.0, 420.0)),
            serde_json::json!({}),
            None,
        );
        let document = doc(
            vec![
                tag(0, 2, Some((360, 800)), &[]),
                tag(1, 2, Some((768, 800)), &[]),
            ],
            vec![vec![narrow], vec![wide]],
        );
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(
            !check_ids(&out).contains(&"breakpoint-jump"),
            "fluid width/height changes explained by the viewport are not jumps: {out:?}"
        );
    }

    #[test]
    fn overflow_only_at_width_requires_a_clean_state() {
        let wide_narrow = node(
            1,
            ".wide",
            "DIV",
            Some((0.0, 0.0, 420.0, 50.0)),
            serde_json::json!({}),
            None,
        );
        let fine_narrow = node(
            2,
            ".fine",
            "DIV",
            Some((0.0, 0.0, 300.0, 50.0)),
            serde_json::json!({}),
            None,
        );
        let wide_wide = node(
            3,
            ".wide",
            "DIV",
            Some((0.0, 0.0, 420.0, 50.0)),
            serde_json::json!({}),
            None,
        );
        let fine_wide = node(
            4,
            ".fine",
            "DIV",
            Some((0.0, 0.0, 300.0, 50.0)),
            serde_json::json!({}),
            None,
        );
        let document = doc(
            vec![
                tag(0, 2, Some((360, 800)), &[]),
                tag(1, 2, Some((768, 800)), &[]),
            ],
            vec![vec![wide_narrow, fine_narrow], vec![wide_wide, fine_wide]],
        );
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert_eq!(check_ids(&out), vec![OVERFLOW_ONLY_AT_WIDTH.id]);
        assert_eq!(
            out.first().map(|line| line.selector.as_str()),
            Some(".wide")
        );
    }

    #[test]
    fn overflow_only_at_width_keep_when_always_overflowing_or_scrollable() {
        let always = node(
            1,
            ".wide",
            "DIV",
            Some((0.0, 0.0, 420.0, 50.0)),
            serde_json::json!({}),
            None,
        );
        let again = node(
            2,
            ".wide",
            "DIV",
            Some((0.0, 0.0, 420.0, 50.0)),
            serde_json::json!({}),
            None,
        );
        let document = doc(
            vec![
                tag(0, 2, Some((360, 800)), &[]),
                tag(1, 2, Some((320, 800)), &[]),
            ],
            vec![vec![always], vec![again]],
        );
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(check_ids(&out).is_empty());

        let scroll_css = serde_json::json!({"box_model": {"overflow-x": "auto"}});
        let scroller = node(
            1,
            ".wide",
            "DIV",
            Some((0.0, 0.0, 420.0, 50.0)),
            scroll_css,
            None,
        );
        let clean = node(
            2,
            ".wide",
            "DIV",
            Some((0.0, 0.0, 300.0, 50.0)),
            serde_json::json!({}),
            None,
        );
        let document = doc(
            vec![
                tag(0, 2, Some((360, 800)), &[]),
                tag(1, 2, Some((768, 800)), &[]),
            ],
            vec![vec![scroller], vec![clean]],
        );
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(!check_ids(&out).contains(&"overflow-only-at-width"));
    }

    fn themed_doc(dark_style: Value, declared: Option<Value>) -> DiffDocument {
        let mut document = doc(
            vec![
                tag(0, 2, None, &[("prefers-color-scheme", "light")]),
                tag(1, 2, None, &[("prefers-color-scheme", "dark")]),
            ],
            vec![
                vec![node(
                    1,
                    "body",
                    "BODY",
                    None,
                    serde_json::json!({"visual": {"background-color": "#ffffff"}}),
                    None,
                )],
                vec![node(2, "body", "BODY", None, dark_style, None)],
            ],
        );
        let mut env = EnvFacet::default();
        env.media
            .insert("prefers-color-scheme".to_string(), "dark".to_string());
        document.env = Some(env);
        document.document = Some(DocumentFacet::default());
        if let Some(declared) = declared {
            document.declared = declared
                .as_object()
                .cloned()
                .and_then(|map| serde_json::from_value(Value::Object(map)).ok());
        }
        document
    }

    #[test]
    fn dark_mode_ignored_flags_unchanged_background() {
        let document = themed_doc(
            serde_json::json!({"visual": {"background-color": "#ffffff"}}),
            None,
        );
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert_eq!(check_ids(&out), vec![DARK_MODE_IGNORED.id]);
        assert_eq!(
            out.first().map(|line| line.severity),
            Some(Some(sniff_core::modern::Severity::Warn))
        );
    }

    #[test]
    fn dark_mode_ignored_keep_when_theme_changes_or_guard_declared() {
        let changed = themed_doc(
            serde_json::json!({"visual": {"background-color": "#101010"}}),
            None,
        );
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&changed, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(out.is_empty());

        let guarded = themed_doc(
            serde_json::json!({"visual": {"background-color": "#ffffff"}}),
            Some(
                serde_json::json!({"at_rules": [{"condition_text": "@media (prefers-color-scheme: dark)", "index": 0, "rules": []}]}),
            ),
        );
        let ctx = ModernCtx::new(&guarded, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(out.is_empty());
    }

    fn motion_doc(reduced_styles: Value, declared: Option<Value>) -> DiffDocument {
        let animated = |styles: Value| node(1, ".parallax", "DIV", None, styles, None);
        let mut document = doc(
            vec![
                tag(0, 2, None, &[("prefers-reduced-motion", "no-preference")]),
                tag(1, 2, None, &[("prefers-reduced-motion", "reduce")]),
            ],
            vec![
                vec![animated(serde_json::json!({"animation": {
                    "animation-name": "parallax",
                    "animation-duration": "2s",
                    "animation-timeline": "view()"
                }}))],
                vec![node(2, ".parallax", "DIV", None, reduced_styles, None)],
            ],
        );
        let mut env = EnvFacet::default();
        env.media
            .insert("prefers-reduced-motion".to_string(), "reduce".to_string());
        document.env = Some(env);
        if let Some(declared) = declared {
            document.declared = serde_json::from_value(declared).ok();
        }
        document
    }

    #[test]
    fn reduced_motion_ignored_flags_identical_animation_styles() {
        let document = motion_doc(
            serde_json::json!({"animation": {
                "animation-name": "parallax",
                "animation-duration": "2s",
                "animation-timeline": "view()"
            }}),
            None,
        );
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert_eq!(check_ids(&out), vec![REDUCED_MOTION_IGNORED.id]);
    }

    #[test]
    fn reduced_motion_ignored_keep_when_guard_or_style_change() {
        let quiet = motion_doc(
            serde_json::json!({"animation": {"animation-name": "none", "animation-duration": "0s", "animation-timeline": "auto"}}),
            None,
        );
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&quiet, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(out.is_empty());

        let guarded = motion_doc(
            serde_json::json!({"animation": {"animation-name": "parallax", "animation-duration": "2s", "animation-timeline": "view()"}}),
            Some(
                serde_json::json!({"at_rules": [{"condition_text": "@media (prefers-reduced-motion: reduce)", "index": 0, "rules": []}]}),
            ),
        );
        let ctx = ModernCtx::new(&guarded, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(out.is_empty());
    }

    fn pointer_doc(coarse_size: (f64, f64), fine_size: (f64, f64)) -> DiffDocument {
        let interactive = |id: u64, size: (f64, f64)| {
            node(
                id,
                ".tap",
                "BUTTON",
                Some((0.0, 0.0, size.0, size.1)),
                serde_json::json!({}),
                Some(serde_json::json!({"focusable": true})),
            )
        };
        let mut document = doc(
            vec![
                tag(0, 2, None, &[("pointer", "fine")]),
                tag(1, 2, None, &[("pointer", "coarse")]),
            ],
            vec![
                vec![interactive(1, fine_size)],
                vec![interactive(2, coarse_size)],
            ],
        );
        let mut env = EnvFacet::default();
        env.media.insert("pointer".to_string(), "fine".to_string());
        document.env = Some(env);
        document
    }

    #[test]
    fn coarse_targets_flags_target_that_shrinks_under_coarse_pointer() {
        let document = pointer_doc((20.0, 20.0), (44.0, 44.0));
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert_eq!(check_ids(&out), vec![COARSE_TARGETS.id]);
        assert_eq!(
            out.first().and_then(|line| line.evidence.observed.clone()),
            Some(Value::from(20.0))
        );
    }

    #[test]
    fn coarse_targets_keep_when_already_small_or_never_shrinks() {
        let opts = ModernOptions::default();
        let already_small = pointer_doc((20.0, 20.0), (20.0, 20.0));
        let ctx = ModernCtx::new(&already_small, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(out.is_empty());

        let large = pointer_doc((44.0, 44.0), (44.0, 44.0));
        let ctx = ModernCtx::new(&large, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(out.is_empty());
    }

    fn height_doc(
        height_before: &str,
        height_after: &str,
        declared: Option<Value>,
    ) -> DiffDocument {
        let hero = |id, height: &str| {
            node(
                id,
                ".hero",
                "DIV",
                Some((0.0, 0.0, 300.0, 100.0)),
                serde_json::json!({"box_model": {"height": height}}),
                None,
            )
        };
        let mut document = doc(
            vec![
                tag(0, 2, Some((360, 640)), &[]),
                tag(1, 2, Some((360, 800)), &[]),
            ],
            vec![
                vec![
                    hero(1, height_before),
                    node(
                        2,
                        ".auto",
                        "DIV",
                        None,
                        serde_json::json!({"box_model": {"height": "auto"}}),
                        None,
                    ),
                ],
                vec![
                    hero(3, height_after),
                    node(
                        4,
                        ".auto",
                        "DIV",
                        None,
                        serde_json::json!({"box_model": {"height": "auto"}}),
                        None,
                    ),
                ],
            ],
        );
        if let Some(declared) = declared {
            document.declared = serde_json::from_value(declared).ok();
        }
        document
    }

    #[test]
    fn fixed_viewport_height_flags_height_tracking_viewport() {
        let document = height_doc("640px", "800px", None);
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert_eq!(check_ids(&out), vec![FIXED_VIEWPORT_HEIGHT.id]);
        assert_eq!(
            out.first().map(|line| line.selector.as_str()),
            Some(".hero")
        );
    }

    #[test]
    fn fixed_viewport_height_keep_when_dynamic_units_declared() {
        let document = height_doc(
            "640px",
            "800px",
            Some(serde_json::json!({"rules": [{
                "selector": ".hero",
                "href": null,
                "index": 0,
                "origin": "author",
                "declarations": {"height": "100dvh"}
            }]})),
        );
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn incomplete_matrix_skips_all_comparative_checks() {
        let mut document = doc(
            vec![
                tag(0, 3, Some((360, 640)), &[]),
                tag(1, 3, Some((768, 800)), &[]),
            ],
            vec![
                vec![node(
                    1,
                    ".card",
                    "DIV",
                    Some((0.0, 0.0, 420.0, 100.0)),
                    serde_json::json!({}),
                    None,
                )],
                vec![node(
                    2,
                    ".card",
                    "DIV",
                    Some((0.0, 200.0, 420.0, 100.0)),
                    serde_json::json!({}),
                    None,
                )],
            ],
        );
        document.env = Some(EnvFacet::default());
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn repeated_selectors_are_paired_by_occurrence() {
        let first_a = node(
            1,
            ".item",
            "DIV",
            Some((0.0, 0.0, 20.0, 20.0)),
            serde_json::json!({}),
            None,
        );
        let first_b = node(
            2,
            ".item",
            "DIV",
            Some((0.0, 40.0, 100.0, 100.0)),
            serde_json::json!({}),
            None,
        );
        let second_a = node(
            3,
            ".item",
            "DIV",
            Some((0.0, 0.0, 20.0, 20.0)),
            serde_json::json!({}),
            None,
        );
        let second_b = node(
            4,
            ".item",
            "DIV",
            Some((0.0, 260.0, 100.0, 100.0)),
            serde_json::json!({}),
            None,
        );
        let document = doc(
            vec![
                tag(0, 2, Some((360, 800)), &[]),
                tag(1, 2, Some((768, 800)), &[]),
            ],
            vec![vec![first_a, first_b], vec![second_a, second_b]],
        );
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert_eq!(out.len(), 1);
        assert!(
            out.first()
                .is_some_and(|line| line.evidence.text.contains("(0,40)")),
            "unexpected evidence: {:?}",
            out
        );
    }

    #[test]
    fn output_is_byte_for_byte_deterministic() {
        let document = pointer_doc((20.0, 20.0), (44.0, 44.0));
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut first = Vec::new();
        let mut second = Vec::new();
        run(&ctx, &mut first);
        run(&ctx, &mut second);
        assert_eq!(serialized(&first), serialized(&second));
        assert!(!first.is_empty());
    }

    #[test]
    fn registry_has_documents_and_unique_ids() {
        let mut ids = BTreeSet::new();
        for spec in specs() {
            assert!(ids.insert(spec.id), "duplicate check id: {}", spec.id);
            assert!(!spec.guides.is_empty(), "check without guide: {}", spec.id);
            assert!(
                !spec.keep_when.is_empty(),
                "check without keepWhen: {}",
                spec.id
            );
        }
        assert_eq!(ids.len(), 6);
    }

    #[test]
    fn disabled_checks_emit_nothing() {
        let document = pointer_doc((20.0, 20.0), (44.0, 44.0));
        let mut opts = ModernOptions::default();
        opts.ignore.insert(COARSE_TARGETS.id.to_string());
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn parent_scroller_suppresses_overflow() {
        let mut parent = node(
            1,
            ".scroller",
            "DIV",
            Some((0.0, 0.0, 300.0, 100.0)),
            serde_json::json!({"box_model": {"overflow-x": "auto"}}),
            None,
        );
        let mut child = node(
            2,
            ".wide",
            "DIV",
            Some((0.0, 0.0, 420.0, 50.0)),
            serde_json::json!({}),
            None,
        );
        child.parent_id = Some(1);
        parent.children.push(child);
        let clean_parent = node(
            3,
            ".scroller",
            "DIV",
            Some((0.0, 0.0, 300.0, 100.0)),
            serde_json::json!({}),
            None,
        );
        let mut clean_child = node(
            4,
            ".wide",
            "DIV",
            Some((0.0, 0.0, 200.0, 50.0)),
            serde_json::json!({}),
            None,
        );
        clean_child.parent_id = Some(3);
        let mut clean_parent = clean_parent;
        clean_parent.children.push(clean_child);
        let document = doc(
            vec![
                tag(0, 2, Some((360, 800)), &[]),
                tag(1, 2, Some((768, 800)), &[]),
            ],
            vec![vec![parent], vec![clean_parent]],
        );
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(!check_ids(&out).contains(&"overflow-only-at-width"));
    }

    #[test]
    fn matrix_without_matching_states_is_silent() {
        let opts = ModernOptions::default();
        let empty: BTreeMap<String, String> = BTreeMap::new();
        assert!(empty.is_empty());
        let document = DiffDocument::default();
        let ctx = ModernCtx::new(&document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        assert!(out.is_empty());
    }
}
