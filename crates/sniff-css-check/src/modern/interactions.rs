//! Family D — interactions, transitions and ARIA state checks.
//!
//! All detections use fields already present in the rendered snapshot:
//! computed styles (`animation`, `interaction`, `layout`, `box_model`,
//! `visual`, `css_variables`), verbatim attributes, resolved ARIA, pseudo
//! element styles, rects and the parent tree. No authoring intent, clock,
//! environment or randomness is consulted. Each registered check documents
//! its `keep_when` in [`CheckSpec::keep_when`] and is exercised by a
//! positive/negative/keepWhen triplet in the tests below.

use crate::modern::{CheckSpec, Facet, ModernCtx};
use crate::rules::CheckLine;
use serde_json::Value;
use sniff_core::modern::Evidence;
use sniff_css_diff::DiffNode;
use std::collections::{BTreeMap, BTreeSet};

// ── Check ids ──────────────────────────────────────────────────────────────

const ID_DUPLICATE_VT: &str = "duplicate-view-transition-name";
const ID_DISPLAY_DISCRETE: &str = "display-transition-without-allow-discrete";
const ID_TOP_LAYER_OVERLAY: &str = "top-layer-transition-missing-overlay";
const ID_ACTIVE_TAB_ARIA: &str = "active-tab-indicator-missing-aria-state";
const ID_CUSTOM_COMMAND: &str = "custom-command-invalid-name-or-state";
const ID_INVOKER_CONFLICT: &str = "invoker-conflicting-attributes-or-missing-type";
const ID_DRAG_HANDLE: &str = "drag-handle-affordance-misapplied";
const ID_REORDER_ORDER: &str = "reorder-faked-with-css-order";
const ID_STAGGER_DELAY: &str = "stagger-delay-collapsed";
const ID_SORT_DOM: &str = "sort-order-not-reflected-in-dom";
const ID_REVEAL_POINTER: &str = "reveal-overlay-blocks-pointer-events";
const ID_INTERESTFOR_TARGET: &str = "interestfor-dangling-target";
const ID_INTEREST_ARIA: &str = "interest-tooltip-wrong-aria-state";
const ID_DIALOG_NAME: &str = "dialog-light-dismiss-without-name";
const ID_MOTION_CLIP: &str = "motion-clipped-focus-indicator";
const ID_LINEAR_DURATION: &str = "linear-easing-missing-duration";
const ID_MODAL_SCROLL: &str = "modal-dialog-scroll-chaining";
const ID_SNAP_UNREACHABLE: &str = "mandatory-snap-unreachable-content";
const ID_HIDDEN_UNTIL_FOUND: &str = "hidden-until-found-display-none";
const ID_SORTABLE_TABLE: &str = "sortable-table-aria-sort-misuse";
const ID_ROVING_ROLE: &str = "roving-tabindex-without-composite-role";
const ID_SWIPE_BUTTON: &str = "swipe-gesture-no-button-alternative";
const ID_SWIPE_OVERSCROLL: &str = "swipe-reveal-overscroll-trapped";
const ID_HOVERCARD_FOCUS: &str = "hovercard-autofocus-on-hint";
const ID_SCROLLBAR_AFFORDANCE: &str = "hidden-scrollbar-without-affordance";
const ID_HASPOPUP_ROLE: &str = "haspopup-without-matching-role";
const ID_SNAP_STACK: &str = "snap-stack-multiple-active-views";
const ID_PROGRESS_AT: &str = "decorative-progress-exposed-to-at";
const ID_SHRINK_PADDING: &str = "scroll-shrink-header-missing-scroll-padding";
const ID_STICKY_ANCHOR: &str = "stateful-sticky-header-without-overflow-anchor-none";
const ID_COPY_HIDDEN: &str = "copy-control-hidden-from-at";
const ID_TOUR_NAME: &str = "tour-popover-unnamed";
const ID_SCROLLSPY_GROUP: &str = "scrollspy-without-scroll-target-group";

// ── Registry ───────────────────────────────────────────────────────────────

const SPEC_DUPLICATE_VT: CheckSpec = CheckSpec::new(
    ID_DUPLICATE_VT,
    &["group-element-transitions", "same-document-transitions"],
    &[Facet::Styles],
    "Names are unique, or `match-element`/`root`/`none` is used; explicit per-item names (p1, p2, p3) are legitimate.",
);
const SPEC_DISPLAY_DISCRETE: CheckSpec = CheckSpec::new(
    ID_DISPLAY_DISCRETE,
    &["animate-element-entry-exit"],
    &[Facet::Styles],
    "`transition-behavior: allow-discrete` is present, `display` is not an explicit transition token (`all` is not this signal), or the node never toggles visibility.",
);
const SPEC_TOP_LAYER_OVERLAY: CheckSpec = CheckSpec::new(
    ID_TOP_LAYER_OVERLAY,
    &["animate-to-from-top-layer"],
    &[Facet::Styles],
    "`overlay` is in the transition list, the node is not promoted to the top layer (`DIALOG`/`popover`), or exit is intentionally instant.",
);
const SPEC_ACTIVE_TAB_ARIA: CheckSpec = CheckSpec::new(
    ID_ACTIVE_TAB_ARIA,
    &["anchor-positioning-tab-underline"],
    &[Facet::Pseudo, Facet::Attrs],
    "No visual active-tab indicator, or the active item/link already carries `aria-current`/`aria-selected` (the two states are not interchangeable).",
);
const SPEC_CUSTOM_COMMAND: CheckSpec = CheckSpec::new(
    ID_CUSTOM_COMMAND,
    &["custom-button-actions"],
    &[Facet::Attrs],
    "The command is native, starts with `--`, or is imperative with no observable state; stateful custom commands already carry `aria-pressed`/`aria-expanded`.",
);
const SPEC_INVOKER_CONFLICT: CheckSpec = CheckSpec::new(
    ID_INVOKER_CONFLICT,
    &["declarative-dialog-popover-control"],
    &[Facet::Attrs],
    "Attributes do not conflict, `type` is explicit inside a form, or the button is outside a form (`type` is then not required). Invalid command names are covered by `custom-command-invalid-name-or-state`.",
);
const SPEC_DRAG_HANDLE: CheckSpec = CheckSpec::new(
    ID_DRAG_HANDLE,
    &["drag"],
    &[Facet::Styles, Facet::Attrs],
    "Only a dedicated, named button handle carries `touch-action`/`user-select: none` and a live region announces moves. The existing `text-not-selectable` check is deliberately not added on handles — this check owns that false positive.",
);
const SPEC_REORDER_ORDER: CheckSpec = CheckSpec::new(
    ID_REORDER_ORDER,
    &["drag-and-drop"],
    &[Facet::Styles],
    "DOM order already matches visual/reading order (for example after `moveBefore()`), `order` is zero and no reverse flex direction is used, or the collection is not reordered.",
);
const SPEC_STAGGER_DELAY: CheckSpec = CheckSpec::new(
    ID_STAGGER_DELAY,
    &["dynamic-sibling-animations"],
    &[Facet::Styles],
    "Delays are distinct across the group, fewer than three siblings share the animation, `--stagger-time` is unset, or animation is `none` (reduced motion).",
);
const SPEC_SORT_DOM: CheckSpec = CheckSpec::new(
    ID_SORT_DOM,
    &["dynamic-sort"],
    &[Facet::Styles],
    "DOM order follows the visual order of the transition-named items, or no CSS ordering is used to express the sort.",
);
const SPEC_REVEAL_POINTER: CheckSpec = CheckSpec::new(
    ID_REVEAL_POINTER,
    &["interactive-content-reveal"],
    &[Facet::Styles],
    "The reveal layer has `pointer-events: none`, is not a radial mask reveal, or `var(--inner-size,0%)`/`var(--outer-size,0%)` fallbacks are present.",
);
const SPEC_INTERESTFOR_TARGET: CheckSpec = CheckSpec::new(
    ID_INTERESTFOR_TARGET,
    &["interest-triggered-action-previews"],
    &[Facet::Attrs],
    "`interestfor` resolves to exactly one element id. A missing live region is never a signal — the guide explicitly advises against announcing previews.",
);
const SPEC_INTEREST_ARIA: CheckSpec = CheckSpec::new(
    ID_INTEREST_ARIA,
    &["interest-triggered-tooltips"],
    &[Facet::Attrs],
    "The target is not `popover=hint`; action triggers use `aria-describedby` without `aria-expanded`, toggletips pair `aria-expanded` with `popovertarget`/`commandfor`, and hints have `role=tooltip` with no `aria-haspopup`/live region. Pre-existing author ARIA wins.",
);
const SPEC_DIALOG_NAME: CheckSpec = CheckSpec::new(
    ID_DIALOG_NAME,
    &["light-dismiss-a-dialog"],
    &[Facet::Attrs],
    "The dialog has an accessible name (`aria-label`, resolved `aria-labelledby` or named text), or `closedby` is absent/`closerequest`/`none` — those defaults are valid.",
);
const SPEC_MOTION_CLIP: CheckSpec = CheckSpec::new(
    ID_MOTION_CLIP,
    &["motion"],
    &[Facet::Styles],
    "The node is not focusable, neither `clip-path` nor `mask-image` clips it, or an unclipped ancestor already carries a projected outline.",
);
const SPEC_LINEAR_DURATION: CheckSpec = CheckSpec::new(
    ID_LINEAR_DURATION,
    &["physics-based-easing"],
    &[Facet::Styles],
    "No `linear()` easing is computed, or the matching `transition-duration`/`animation-duration` is positive (including cascaded values). An earlier `cubic-bezier` fallback is preserved, not removed.",
);
const SPEC_MODAL_SCROLL: CheckSpec = CheckSpec::new(
    ID_MODAL_SCROLL,
    &["prevent-page-scroll-under-modal-dialog"],
    &[Facet::Attrs, Facet::Pseudo],
    "The dialog is not open, both dialog and `::backdrop` contain overscroll with hidden/clipped overflow, or no modal is present. `overflow:hidden` on `<html>`/`<body>` is not proposed as a fix.",
);
const SPEC_SNAP_UNREACHABLE: CheckSpec = CheckSpec::new(
    ID_SNAP_UNREACHABLE,
    &["scroll-snap-state-sync"],
    &[Facet::Styles, Facet::Rect],
    "Snapping is `proximity`, the snap target fits the scrollport, is nested in a sub-scroller, or scrolls internally. `aria-current` absence alone is not deterministic evidence.",
);
const SPEC_HIDDEN_UNTIL_FOUND: CheckSpec = CheckSpec::new(
    ID_HIDDEN_UNTIL_FOUND,
    &["search-hidden-content"],
    &[Facet::Attrs, Facet::Styles],
    "`hidden` is boolean/absent, the element is a closed `<details>`, or computed display/visibility still allows find-in-page to reveal it.",
);
const SPEC_SORTABLE_TABLE: CheckSpec = CheckSpec::new(
    ID_SORTABLE_TABLE,
    &["sortable-table"],
    &[Facet::Attrs],
    "Only one `<th>` carries `ascending`/`descending`, `aria-sort` is absent or on a `<td>` (ignored), and no `aria-sort=\"none\"` or `<th role=button>` is present.",
);
const SPEC_ROVING_ROLE: CheckSpec = CheckSpec::new(
    ID_ROVING_ROLE,
    &["spatial-navigation"],
    &[Facet::Attrs],
    "The container has a composite role (`grid`, `listbox`, `tablist`, `tree`, `menu`, …), or the `-1` entries are hidden panels (`accessibility_grade=NONE`). Arrow-key interception is not observable in a static snapshot.",
);
const SPEC_SWIPE_BUTTON: CheckSpec = CheckSpec::new(
    ID_SWIPE_BUTTON,
    &["swipe-to-remove"],
    &[Facet::Styles],
    "The row/subtree contains a named `BUTTON`/`A` (or `ax` name) offering the action, or the track is decorative. Existing `target-size` covers action button geometry.",
);
const SPEC_SWIPE_OVERSCROLL: CheckSpec = CheckSpec::new(
    ID_SWIPE_OVERSCROLL,
    &["swipe-to-reveal"],
    &[Facet::Styles],
    "The outer wrapper uses `auto`/`chain`, has no scroll container ancestor, or `contain`/`none` is confined to the inner `.is-scrollable` panel. `chain` is progressive enhancement over `auto`.",
);
const SPEC_HOVERCARD_FOCUS: CheckSpec = CheckSpec::failing(CheckSpec::new(
    ID_HOVERCARD_FOCUS,
    &["hovercard"],
    &[Facet::Attrs],
    "No `autofocus` participates in a hint relationship, or focus is moved only on explicit user activation (the WCAG 3.2.1 exception for user-initiated context changes).",
));
const SPEC_SCROLLBAR_AFFORDANCE: CheckSpec = CheckSpec::failing(CheckSpec::new(
    ID_SCROLLBAR_AFFORDANCE,
    &["scrollability-affordance-hints"],
    &[Facet::Styles, Facet::Pseudo],
    "The scroller retains a styled thumb/track, `scrollbar-color`, snap points or another visible scroll affordance; the node is `overflow:hidden` and therefore not scrollable.",
));
const SPEC_HASPOPUP_ROLE: CheckSpec = CheckSpec::new(
    ID_HASPOPUP_ROLE,
    &["viewport-aware-dropdowns"],
    &[Facet::Attrs],
    "`aria-haspopup` matches the target role (`menu`, `listbox`, `dialog`, `tree`, `grid`), or the popup target is absent because ARIA is authored elsewhere. Pre-existing author ARIA wins.",
);
const SPEC_SNAP_STACK: CheckSpec = CheckSpec::new(
    ID_SNAP_STACK,
    &["stack-drill-down"],
    &[Facet::Styles, Facet::Attrs],
    "All inactive views are `inert`/hidden so a single view is active, or the snap container has fewer than two snap-aligned children.",
);
const SPEC_PROGRESS_AT: CheckSpec = CheckSpec::new(
    ID_PROGRESS_AT,
    &["scroll-progress-indicator"],
    &[Facet::Styles],
    "The decorative bar has `role=presentation`/`none` or `aria-hidden=true`, or no scroll-driven/progress transform is present.",
);
const SPEC_SHRINK_PADDING: CheckSpec = CheckSpec::new(
    ID_SHRINK_PADDING,
    &["shrinking-header-on-scroll"],
    &[Facet::Styles],
    "The scroll root reserves space with a positive `scroll-padding-top`/`scroll-padding-block-start`, or the header is not sticky/shrinking on scroll.",
);
const SPEC_STICKY_ANCHOR: CheckSpec = CheckSpec::new(
    ID_STICKY_ANCHOR,
    &["state-aware-sticky-headers"],
    &[Facet::Styles],
    "An ancestor scroll container sets `overflow-anchor: none`, or the sticky header has no scroll-driven state change.",
);
const SPEC_COPY_HIDDEN: CheckSpec = CheckSpec::new(
    ID_COPY_HIDDEN,
    &["copy-button"],
    &[Facet::Attrs],
    "The copy control is exposed to assistive technology (named, not `aria-hidden`, not `role=presentation`/`none`) or is not identifiable as a copy control from the snapshot.",
);
const SPEC_TOUR_NAME: CheckSpec = CheckSpec::new(
    ID_TOUR_NAME,
    &["persistent-app-tours"],
    &[Facet::Attrs],
    "The tour popover has an accessible name (`aria-label`, `aria-labelledby`, named text or `ax` name) or is not a dialog-like popover.",
);
const SPEC_SCROLLSPY_GROUP: CheckSpec = CheckSpec::new(
    ID_SCROLLSPY_GROUP,
    &["scrollspy"],
    &[Facet::Attrs],
    "The spy container declares a non-empty `scroll-target-group`, or fewer than two descendants expose `aria-current` (no observable scrollspy state).",
);

/// All check ids owned by family D, in catalog order.
pub fn specs() -> &'static [CheckSpec] {
    &[
        SPEC_DUPLICATE_VT,
        SPEC_DISPLAY_DISCRETE,
        SPEC_TOP_LAYER_OVERLAY,
        SPEC_ACTIVE_TAB_ARIA,
        SPEC_CUSTOM_COMMAND,
        SPEC_INVOKER_CONFLICT,
        SPEC_DRAG_HANDLE,
        SPEC_REORDER_ORDER,
        SPEC_STAGGER_DELAY,
        SPEC_SORT_DOM,
        SPEC_REVEAL_POINTER,
        SPEC_INTERESTFOR_TARGET,
        SPEC_INTEREST_ARIA,
        SPEC_DIALOG_NAME,
        SPEC_MOTION_CLIP,
        SPEC_LINEAR_DURATION,
        SPEC_MODAL_SCROLL,
        SPEC_SNAP_UNREACHABLE,
        SPEC_HIDDEN_UNTIL_FOUND,
        SPEC_SORTABLE_TABLE,
        SPEC_ROVING_ROLE,
        SPEC_SWIPE_BUTTON,
        SPEC_SWIPE_OVERSCROLL,
        SPEC_HOVERCARD_FOCUS,
        SPEC_SCROLLBAR_AFFORDANCE,
        SPEC_HASPOPUP_ROLE,
        SPEC_SNAP_STACK,
        SPEC_PROGRESS_AT,
        SPEC_SHRINK_PADDING,
        SPEC_STICKY_ANCHOR,
        SPEC_COPY_HIDDEN,
        SPEC_TOUR_NAME,
        SPEC_SCROLLSPY_GROUP,
    ]
}

/// Runs every enabled interaction check and appends deterministic lines.
pub fn run(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    tracing::debug!(
        nodes = ctx.flat.len(),
        checks = specs().len(),
        "running modern interactions family"
    );
    for spec in specs() {
        if !ctx.enabled(spec) {
            tracing::trace!(
                check = spec.id,
                "interactions check disabled by facets or filters"
            );
            continue;
        }
        let before = out.len();
        match spec.id {
            ID_DUPLICATE_VT => duplicate_view_transition_name(ctx, out, spec),
            ID_DISPLAY_DISCRETE => display_transition_without_allow_discrete(ctx, out, spec),
            ID_TOP_LAYER_OVERLAY => top_layer_transition_missing_overlay(ctx, out, spec),
            ID_ACTIVE_TAB_ARIA => active_tab_indicator_missing_aria_state(ctx, out, spec),
            ID_CUSTOM_COMMAND => custom_command_invalid_name_or_state(ctx, out, spec),
            ID_INVOKER_CONFLICT => invoker_conflicting_attributes_or_missing_type(ctx, out, spec),
            ID_DRAG_HANDLE => drag_handle_affordance_misapplied(ctx, out, spec),
            ID_REORDER_ORDER => reorder_faked_with_css_order(ctx, out, spec),
            ID_STAGGER_DELAY => stagger_delay_collapsed(ctx, out, spec),
            ID_SORT_DOM => sort_order_not_reflected_in_dom(ctx, out, spec),
            ID_REVEAL_POINTER => reveal_overlay_blocks_pointer_events(ctx, out, spec),
            ID_INTERESTFOR_TARGET => interestfor_dangling_target(ctx, out, spec),
            ID_INTEREST_ARIA => interest_tooltip_wrong_aria_state(ctx, out, spec),
            ID_DIALOG_NAME => dialog_light_dismiss_without_name(ctx, out, spec),
            ID_MOTION_CLIP => motion_clipped_focus_indicator(ctx, out, spec),
            ID_LINEAR_DURATION => linear_easing_missing_duration(ctx, out, spec),
            ID_MODAL_SCROLL => modal_dialog_scroll_chaining(ctx, out, spec),
            ID_SNAP_UNREACHABLE => mandatory_snap_unreachable_content(ctx, out, spec),
            ID_HIDDEN_UNTIL_FOUND => hidden_until_found_display_none(ctx, out, spec),
            ID_SORTABLE_TABLE => sortable_table_aria_sort_misuse(ctx, out, spec),
            ID_ROVING_ROLE => roving_tabindex_without_composite_role(ctx, out, spec),
            ID_SWIPE_BUTTON => swipe_gesture_no_button_alternative(ctx, out, spec),
            ID_SWIPE_OVERSCROLL => swipe_reveal_overscroll_trapped(ctx, out, spec),
            ID_HOVERCARD_FOCUS => hovercard_autofocus_on_hint(ctx, out, spec),
            ID_SCROLLBAR_AFFORDANCE => hidden_scrollbar_without_affordance(ctx, out, spec),
            ID_HASPOPUP_ROLE => haspopup_without_matching_role(ctx, out, spec),
            ID_SNAP_STACK => snap_stack_multiple_active_views(ctx, out, spec),
            ID_PROGRESS_AT => decorative_progress_exposed_to_at(ctx, out, spec),
            ID_SHRINK_PADDING => scroll_shrink_header_missing_scroll_padding(ctx, out, spec),
            ID_STICKY_ANCHOR => stateful_sticky_header_without_overflow_anchor_none(ctx, out, spec),
            ID_COPY_HIDDEN => copy_control_hidden_from_at(ctx, out, spec),
            ID_TOUR_NAME => tour_popover_unnamed(ctx, out, spec),
            ID_SCROLLSPY_GROUP => scrollspy_without_scroll_target_group(ctx, out, spec),
            other => tracing::warn!(check = other, "unhandled interactions check id"),
        }
        let emitted = out.len() - before;
        if emitted > 0 {
            tracing::debug!(
                check = spec.id,
                emitted,
                "modern interactions check emitted"
            );
        }
    }
}

// ── Small, allocation-frugal helpers over the snapshot facets ─────────────

fn trimmed_style<'a>(
    ctx: &'a ModernCtx<'_>,
    node: &'a DiffNode,
    category: &str,
    property: &str,
) -> Option<&'a str> {
    ctx.style(node, category, property)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn raw_attr<'a>(node: &'a DiffNode, name: &str) -> Option<&'a str> {
    node.attributes.as_ref()?.get(name)?.as_str()
}

fn has_attr(node: &DiffNode, name: &str) -> bool {
    raw_attr(node, name).is_some()
}

fn attr_is(node: &DiffNode, name: &str, expected: &str) -> bool {
    raw_attr(node, name) == Some(expected)
}

fn aria_or_attr<'a>(ctx: &'a ModernCtx<'_>, node: &'a DiffNode, key: &str) -> Option<&'a str> {
    ctx.aria(node, key).or_else(|| ctx.attr(node, key))
}

fn pseudo_style<'a>(
    node: &'a DiffNode,
    pseudo: &str,
    category: &str,
    property: &str,
) -> Option<&'a str> {
    node.pseudo
        .as_ref()?
        .get(pseudo)?
        .get(category)?
        .get(property)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn number(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok()
}

fn positive_number(value: Option<&str>) -> bool {
    value.and_then(number).is_some_and(|v| v > 0.0)
}

fn duration_seconds(value: &str) -> Option<f64> {
    let raw = value.trim();
    if let Some(rest) = raw.strip_suffix("ms") {
        return rest.trim().parse::<f64>().ok().map(|v| v / 1000.0);
    }
    if let Some(rest) = raw.strip_suffix('s') {
        return rest.trim().parse::<f64>().ok();
    }
    if raw == "0" {
        return Some(0.0);
    }
    None
}

fn has_token(value: &str, token: &str) -> bool {
    value
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .any(|part| part == token)
}

fn snap_is_horizontal_mandatory(value: &str) -> bool {
    let mut axis = false;
    let mut mandatory = false;
    for token in value.split_ascii_whitespace() {
        axis |= token == "x";
        mandatory |= token == "mandatory";
    }
    axis && mandatory
}

fn is_scroll_container(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    [
        "overflow-x",
        "overflow-y",
        "overflow-block",
        "overflow-inline",
    ]
    .iter()
    .any(|prop| {
        matches!(
            trimmed_style(ctx, node, "box_model", prop),
            Some("auto" | "scroll")
        )
    })
}

fn scroll_driven(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    ["animation-timeline", "view-timeline-name"]
        .iter()
        .any(|prop| match trimmed_style(ctx, node, "animation", prop) {
            Some(value) => !matches!(value, "auto" | "none" | "normal"),
            None => false,
        })
}

fn parent<'a>(ctx: &'a ModernCtx<'_>, node: &DiffNode) -> Option<&'a DiffNode> {
    node.parent_id.and_then(|id| {
        ctx.flat
            .iter()
            .find(|candidate| candidate.id == id)
            .copied()
    })
}

fn has_ancestor(
    ctx: &ModernCtx<'_>,
    node: &DiffNode,
    predicate: impl Fn(&DiffNode) -> bool,
) -> bool {
    nearest_ancestor(ctx, node, predicate).is_some()
}

fn nearest_ancestor<'a>(
    ctx: &'a ModernCtx<'_>,
    node: &DiffNode,
    predicate: impl Fn(&DiffNode) -> bool,
) -> Option<&'a DiffNode> {
    let mut current = node;
    let mut seen = BTreeSet::new();
    while let Some(ancestor) = parent(ctx, current) {
        if !seen.insert(ancestor.id) {
            return None;
        }
        if predicate(ancestor) {
            return Some(ancestor);
        }
        current = ancestor;
    }
    None
}

fn in_subtree(ctx: &ModernCtx<'_>, root: &DiffNode, node: &DiffNode) -> bool {
    let mut current = node;
    let mut seen = BTreeSet::new();
    loop {
        if current.id == root.id {
            return true;
        }
        if !seen.insert(current.id) {
            return false;
        }
        match parent(ctx, current) {
            Some(ancestor) => current = ancestor,
            None => return false,
        }
    }
}

fn id_index<'a>(ctx: &'a ModernCtx<'_>) -> BTreeMap<&'a str, &'a DiffNode> {
    let mut ids = BTreeMap::new();
    for &node in &ctx.flat {
        if let Some(id) = raw_attr(node, "id").filter(|value| !value.is_empty()) {
            ids.entry(id).or_insert(node);
        }
    }
    ids
}

fn rect_number(node: &DiffNode, key: &str) -> Option<f64> {
    node.rect.as_ref()?.get(key)?.as_f64()
}

fn order_value(ctx: &ModernCtx<'_>, node: &DiffNode) -> i64 {
    trimmed_style(ctx, node, "layout", "order")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
}

fn visual_order_differs(
    ctx: &ModernCtx<'_>,
    container: &DiffNode,
    children: &[DiffNode],
    axis: &str,
) -> bool {
    let mut measured: Vec<(usize, f64, &str)> = Vec::with_capacity(children.len());
    for (position, child) in children.iter().enumerate() {
        let Some(coordinate) = rect_number(child, axis) else {
            // Without geometry, the CSS ordering signal itself is the finding.
            return order_value(ctx, container) != 0
                || children.iter().any(|child| order_value(ctx, child) != 0);
        };
        measured.push((position, coordinate, child.selector.as_str()));
    }
    measured.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.2.cmp(b.2)));
    measured
        .iter()
        .enumerate()
        .any(|(visual, (dom, _, _))| visual != *dom)
}

fn flex_axis<'b>(ctx: &ModernCtx<'_>, container: &'b DiffNode) -> &'b str {
    match trimmed_style(ctx, container, "layout", "flex-direction") {
        Some("column" | "column-reverse") => "y",
        _ => "x",
    }
}

fn is_reverse_flex(ctx: &ModernCtx<'_>, container: &DiffNode) -> bool {
    matches!(
        trimmed_style(ctx, container, "layout", "flex-direction"),
        Some("row-reverse" | "column-reverse")
    )
}

fn named_control(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    matches!(ctx.tag(node), "BUTTON" | "A" | "SUMMARY")
        && aria_or_attr(ctx, node, "name")
            .or_else(|| ctx.aria(node, "label"))
            .is_some_and(|name| !name.trim().is_empty())
        || node
            .ax
            .as_ref()
            .and_then(|ax| ax.get("name"))
            .and_then(Value::as_str)
            .is_some_and(|name| !name.trim().is_empty())
}

fn composite_role(role: &str) -> bool {
    matches!(
        role,
        "grid"
            | "gridcell"
            | "listbox"
            | "option"
            | "tablist"
            | "tab"
            | "tree"
            | "treegrid"
            | "radiogroup"
            | "menu"
            | "menubar"
            | "menuitem"
    )
}

fn popup_role_matches(haspopup: &str, role: &str) -> bool {
    match haspopup {
        "menu" => matches!(role, "menu" | "menubar"),
        "listbox" => role == "listbox",
        "dialog" => role == "dialog",
        "tree" => role == "tree",
        "grid" => role == "grid",
        _ => matches!(
            role,
            "menu" | "menubar" | "listbox" | "dialog" | "tree" | "grid"
        ),
    }
}

// ── Individual checks ──────────────────────────────────────────────────────

fn duplicate_view_transition_name(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let mut names: BTreeMap<&str, Vec<&DiffNode>> = BTreeMap::new();
    for &node in &ctx.flat {
        let Some(name) = trimmed_style(ctx, node, "animation", "view-transition-name") else {
            continue;
        };
        if matches!(name, "none" | "match-element" | "root") {
            continue;
        }
        names.entry(name).or_default().push(node);
    }
    for (name, nodes) in names {
        if nodes.len() < 2 {
            continue;
        }
        let selectors = nodes
            .iter()
            .map(|node| node.selector.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        for node in nodes.iter().skip(1) {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::numeric(
                    format!(
                        "view-transition-name '{name}' is used by {} nodes ({selectors}) — the transition will be skipped",
                        nodes.len()
                    ),
                    "styles.animation",
                    "duplicate_view_transition_names",
                    nodes.len(),
                    2,
                    ">=",
                ),
            );
        }
    }
}

fn display_transition_without_allow_discrete(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        let Some(properties) = trimmed_style(ctx, node, "animation", "transition-property") else {
            continue;
        };
        if !has_token(properties, "display") {
            continue;
        }
        let behavior = trimmed_style(ctx, node, "animation", "transition-behavior").unwrap_or("");
        if has_token(behavior, "allow-discrete") {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!(
                    "transition-property includes display but transition-behavior={behavior} — exit will be instant"
                ),
                "styles.animation",
            ),
        );
    }
}

fn top_layer_transition_missing_overlay(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        let is_top_layer = ctx.tag(node) == "DIALOG" || has_attr(node, "popover");
        if !is_top_layer {
            continue;
        }
        let Some(properties) = trimmed_style(ctx, node, "animation", "transition-property") else {
            continue;
        };
        if !has_token(properties, "display") || has_token(properties, "overlay") {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!("top-layer element transitions display but not overlay — it may be unlayered before the exit finishes; transition-property={properties}"),
                "styles.animation+attrs",
            ),
        );
    }
}

fn active_tab_indicator_missing_aria_state(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        let Some(content) = pseudo_style(node, "::before", "accessibility", "content") else {
            continue;
        };
        let empty_content = content
            .trim_matches(|c: char| c == '"' || c == '\'')
            .is_empty();
        let absolute = trimmed_style(ctx, node, "layout", "position") == Some("absolute")
            || pseudo_style(node, "::before", "layout", "position") == Some("absolute");
        let underline = positive_number(pseudo_style(
            node,
            "::before",
            "box_model",
            "border-block-end-width",
        )) || positive_number(pseudo_style(
            node,
            "::before",
            "box_model",
            "border-bottom-width",
        ));
        if !empty_content || !absolute || !underline {
            continue;
        }
        let candidates: Vec<&DiffNode> = std::iter::once(node)
            .chain(
                ctx.flat
                    .iter()
                    .copied()
                    .filter(|candidate| in_subtree(ctx, node, candidate)),
            )
            .filter(|candidate| {
                matches!(ctx.tag(candidate), "A" | "BUTTON" | "LI")
                    || trimmed_style(ctx, candidate, "layout", "anchor-name")
                        .is_some_and(|name| name.starts_with("--"))
            })
            .collect();
        let has_state = candidates.iter().any(|candidate| {
            raw_attr(candidate, "aria-current").is_some()
                || raw_attr(candidate, "aria-selected").is_some()
                || ctx.aria(candidate, "current").is_some()
                || ctx.aria(candidate, "selected").is_some()
        });
        if has_state {
            continue;
        }
        let Some(target) = candidates.first().copied().or(Some(node)) else {
            continue;
        };
        ctx.emit(
            out,
            spec,
            target,
            Evidence::new(
                format!("visual active-tab underline on {} has no aria-current/aria-selected on the active item", node.selector),
                "pseudo+attrs+aria",
            ),
        );
    }
}

fn custom_command_invalid_name_or_state(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    const NATIVE_COMMANDS: &[&str] = &[
        "show-modal",
        "close",
        "show-popover",
        "hide-popover",
        "toggle-popover",
        "request-close",
    ];
    let ids = id_index(ctx);
    for &node in &ctx.flat {
        let Some(command) = ctx.attr(node, "command") else {
            continue;
        };
        if !command.starts_with("--") && !NATIVE_COMMANDS.contains(&command) {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!("custom command button uses command=\"{command}\" without the mandatory \"--\" prefix"),
                    "attrs",
                ),
            );
            continue;
        }
        let Some(target_id) = ctx.attr(node, "commandfor") else {
            continue;
        };
        let Some(target) = ids.get(target_id).copied() else {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!("custom command invoker references commandfor=\"{target_id}\" with no matching element id"),
                    "attrs",
                ),
            );
            continue;
        };
        if !command.starts_with("--") {
            continue;
        }
        let target_is_stateful = has_attr(target, "popover")
            || has_attr(target, "open")
            || raw_attr(target, "aria-expanded").is_some()
            || raw_attr(target, "aria-pressed").is_some();
        let has_state = raw_attr(node, "aria-pressed").is_some()
            || raw_attr(node, "aria-expanded").is_some()
            || ctx.aria(node, "pressed").is_some()
            || ctx.aria(node, "expanded").is_some();
        if target_is_stateful && !has_state {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!("custom command invoker for '{command}' has no aria-pressed/aria-expanded state"),
                    "attrs+aria",
                ),
            );
        }
    }
}

fn invoker_conflicting_attributes_or_missing_type(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        if ctx.tag(node) != "BUTTON" {
            continue;
        }
        let command_for = ctx.attr(node, "commandfor");
        let popover_target = ctx.attr(node, "popovertarget");
        if let (Some(command), Some(popover)) = (command_for, popover_target) {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!("button has both popovertarget=\"{popover}\" and commandfor=\"{command}\" — the popup will double-toggle"),
                    "attrs",
                ),
            );
            continue;
        }
        if command_for.is_some()
            && !has_attr(node, "type")
            && has_ancestor(ctx, node, |ancestor| ctx.tag(ancestor) == "FORM")
        {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    "command button inside a form has no explicit type — it will submit the form",
                    "attrs+path",
                ),
            );
        }
    }
}

fn drag_handle_affordance_misapplied(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    let live_region = ctx.flat.iter().any(|node| {
        aria_or_attr(ctx, node, "live").is_some()
            || matches!(aria_or_attr(ctx, node, "role"), Some("status"))
    });
    for &node in &ctx.flat {
        let cursor = trimmed_style(ctx, node, "interaction", "cursor");
        if !matches!(cursor, Some("grab" | "grabbing")) {
            continue;
        }
        let touch_none = trimmed_style(ctx, node, "interaction", "touch-action") == Some("none");
        let select_none = trimmed_style(ctx, node, "interaction", "user-select") == Some("none");
        if !touch_none && !select_none {
            continue;
        }
        let dedicated_named_handle = ctx.tag(node) == "BUTTON"
            && !aria_or_attr(ctx, node, "name")
                .unwrap_or("")
                .trim()
                .is_empty();
        if dedicated_named_handle && live_region {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!("drag affordances (touch-action/user-select: none) applied to {} instead of a dedicated named button handle", node.selector),
                "styles.interaction+aria+attrs",
            ),
        );
    }
}

fn reorder_faked_with_css_order(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for &container in &ctx.flat {
        let children = &container.children;
        if children.len() < 2 {
            continue;
        }
        let reverse = is_reverse_flex(ctx, container);
        let any_order = children.iter().any(|child| order_value(ctx, child) != 0);
        if !reverse && !any_order {
            continue;
        }
        let differs = visual_order_differs(ctx, container, children, flex_axis(ctx, container));
        if !differs {
            continue;
        }
        let ordered = children
            .iter()
            .find(|child| order_value(ctx, child) != 0)
            .unwrap_or(container);
        let detail = if reverse {
            "flex-direction reverse"
        } else {
            "CSS order"
        };
        ctx.emit(
            out,
            spec,
            ordered,
            Evidence::new(
                format!(
                    "collection {} uses {detail} to fake reorder — DOM and reading order diverge",
                    container.selector
                ),
                "styles.layout+rect+path",
            ),
        );
    }
}

fn stagger_delay_collapsed(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let mut groups: BTreeMap<(u64, &str), Vec<&DiffNode>> = BTreeMap::new();
    for &node in &ctx.flat {
        let Some(animation) = trimmed_style(ctx, node, "animation", "animation-name") else {
            continue;
        };
        if animation == "none" {
            continue;
        }
        let key = node.parent_id.unwrap_or(0);
        groups.entry((key, animation)).or_default().push(node);
    }
    for ((_, _animation), siblings) in groups {
        if siblings.len() < 3 {
            continue;
        }
        let stagger = siblings.iter().find_map(|node| {
            trimmed_style(ctx, node, "css_variables", "--stagger-time")
                .or_else(|| trimmed_style(ctx, node, "custom", "--stagger-time"))
        });
        if stagger.is_none() {
            continue;
        }
        let delays: BTreeSet<&str> = siblings
            .iter()
            .map(|node| trimmed_style(ctx, node, "animation", "animation-delay").unwrap_or("0s"))
            .collect();
        if delays.len() > 1 {
            continue;
        }
        let Some(parent) = parent(ctx, siblings[0]) else {
            continue;
        };
        ctx.emit(
            out,
            spec,
            parent,
            Evidence::numeric(
                format!("stagger list {}: --stagger-time is set but all {} siblings share animation-delay — stagger collapsed", parent.selector, siblings.len()),
                "styles.animation+css_variables",
                "collapsed_stagger_siblings",
                siblings.len(),
                3,
                ">=",
            ),
        );
    }
}

fn sort_order_not_reflected_in_dom(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &container in &ctx.flat {
        let children = &container.children;
        if children.len() < 2 {
            continue;
        }
        let transition_named = children.iter().filter(|child| {
            trimmed_style(ctx, child, "animation", "view-transition-name")
                .is_some_and(|name| !matches!(name, "none" | "match-element" | "root"))
        });
        if transition_named.clone().count() < 2 {
            continue;
        }
        let reverse = is_reverse_flex(ctx, container);
        let any_order = children.iter().any(|child| order_value(ctx, child) != 0);
        if !reverse && !any_order {
            continue;
        }
        if !visual_order_differs(ctx, container, children, flex_axis(ctx, container)) {
            continue;
        }
        let status = ctx.flat.iter().any(|node| {
            aria_or_attr(ctx, node, "role") == Some("status") && node.selector != container.selector
        });
        ctx.emit(
            out,
            spec,
            container,
            Evidence::new(
                format!("sortable collection {} relies on CSS order/reverse — keyboard and screen-reader order diverge from visual order (role=status present: {status})", container.selector),
                "styles.layout+styles.animation+aria",
            ),
        );
    }
}

fn reveal_overlay_blocks_pointer_events(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        let Some(mask) = trimmed_style(ctx, node, "visual", "mask-image") else {
            continue;
        };
        if !mask.contains("radial-gradient(") {
            continue;
        }
        let pointer = trimmed_style(ctx, node, "interaction", "pointer-events").unwrap_or("auto");
        let uses_size_vars = mask.contains("var(--mouse")
            || trimmed_style(ctx, node, "css_variables", "--inner-size").is_some()
            || trimmed_style(ctx, node, "css_variables", "--outer-size").is_some();
        let missing_fallback =
            mask.contains("var(--inner-size)") || mask.contains("var(--outer-size)");
        if pointer == "none" && !missing_fallback {
            continue;
        }
        if pointer == "none" && missing_fallback {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!(
                        "mask reveal layer {} uses a size custom property without a 0% fallback",
                        node.selector
                    ),
                    "styles.visual+css_variables",
                ),
            );
            continue;
        }
        if !uses_size_vars && !missing_fallback {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!("mask reveal layer {} has pointer-events={pointer} — input to underlying controls is blocked", node.selector),
                "styles.visual+styles.interaction+css_variables",
            ),
        );
    }
}

fn interestfor_dangling_target(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let mut ids: BTreeMap<&str, usize> = BTreeMap::new();
    for &node in &ctx.flat {
        if let Some(id) = ctx.attr(node, "id") {
            *ids.entry(id).or_default() += 1;
        }
    }
    for &node in &ctx.flat {
        let Some(target) = ctx.attr(node, "interestfor") else {
            continue;
        };
        let count = ids.get(target).copied().unwrap_or(0);
        if count == 1 {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::numeric(
                format!("interest source {} references interestfor=\"{target}\" but {} elements with that id exist", node.selector, count),
                "attrs",
                "interestfor_target_count",
                count,
                1,
                "==",
            ),
        );
    }
}

fn interest_tooltip_wrong_aria_state(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    let ids = id_index(ctx);
    for &source in &ctx.flat {
        let Some(target_id) = ctx.attr(source, "interestfor") else {
            continue;
        };
        let Some(hint) = ids.get(target_id).copied() else {
            continue;
        };
        if !attr_is(hint, "popover", "hint") {
            continue;
        }
        let expanded =
            raw_attr(source, "aria-expanded").is_some() || ctx.aria(source, "expanded").is_some();
        let opens_on_activation =
            ctx.attr(source, "popovertarget").is_some() || ctx.attr(source, "commandfor").is_some();
        if expanded && !opens_on_activation {
            ctx.emit(
                out,
                spec,
                source,
                Evidence::new(
                    format!("hint trigger {} uses aria-expanded but does not open the hint on activation — use aria-describedby and role=tooltip instead", source.selector),
                    "attrs+aria",
                ),
            );
        }
        if ctx.attr(hint, "aria-haspopup").is_some() || ctx.attr(hint, "aria-live").is_some() {
            ctx.emit(
                out,
                spec,
                hint,
                Evidence::new(
                    format!("hint {} carries aria-haspopup or a live region — the guide forbids both on hints", hint.selector),
                    "attrs+aria",
                ),
            );
        }
        if ctx.attr(source, "aria-describedby") == Some(target_id)
            && !matches!(aria_or_attr(ctx, hint, "role"), Some("tooltip"))
        {
            ctx.emit(
                out,
                spec,
                hint,
                Evidence::new(
                    format!(
                        "hint {} referenced by aria-describedby has no role=tooltip",
                        hint.selector
                    ),
                    "attrs+aria",
                ),
            );
        }
    }
}

fn dialog_light_dismiss_without_name(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        if ctx.tag(node) != "DIALOG" || !attr_is(node, "closedby", "any") {
            continue;
        }
        let named = ctx
            .aria(node, "name")
            .is_some_and(|name| !name.trim().is_empty())
            || ctx.attr(node, "aria-label").is_some()
            || ctx.attr(node, "aria-labelledby").is_some();
        if named {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!(
                    "dialog {} enables light-dismiss (closedby=any) but has no accessible name",
                    node.selector
                ),
                "attrs+aria+pseudo",
            ),
        );
    }
}

fn motion_clipped_focus_indicator(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for &node in &ctx.flat {
        let focusable = ctx.aria_bool(node, "focusable") || has_attr(node, "tabindex");
        if !focusable {
            continue;
        }
        let clip = trimmed_style(ctx, node, "visual", "clip-path")
            .filter(|value| *value != "none" && *value != "inset(0px 0px 0px 0px)");
        let mask =
            trimmed_style(ctx, node, "visual", "mask-image").filter(|value| *value != "none");
        let Some(clipping) = clip.or(mask) else {
            continue;
        };
        let projected = nearest_ancestor(ctx, node, |ancestor| {
            positive_number(trimmed_style(ctx, ancestor, "visual", "outline-width"))
        })
        .is_some();
        if projected {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!("focusable {} clips its own focus outline via {clipping}; project the indicator onto an unclipped parent", node.selector),
                "aria+styles.visual",
            ),
        );
    }
}

fn linear_easing_missing_duration(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    const PAIRS: &[(&str, &str, &str)] = &[
        (
            "transition-timing-function",
            "transition-duration",
            "transition-timing-function",
        ),
        (
            "animation-timing-function",
            "animation-duration",
            "animation-timing-function",
        ),
    ];
    for &node in &ctx.flat {
        for &(timing, duration, source) in PAIRS {
            let Some(easing) = trimmed_style(ctx, node, "animation", timing) else {
                continue;
            };
            if !easing.contains("linear(") {
                continue;
            }
            let elapsed = trimmed_style(ctx, node, "animation", duration)
                .and_then(duration_seconds)
                .unwrap_or(0.0);
            if elapsed > 0.0 {
                continue;
            }
            ctx.emit(
                out,
                spec,
                node,
                Evidence::numeric(
                    format!("linear() easing on {} has no positive duration — linear() never derives one", node.selector),
                    source,
                    "easing_duration_seconds",
                    elapsed,
                    0.0,
                    ">",
                ),
            );
        }
    }
}

fn modal_dialog_scroll_chaining(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for &node in &ctx.flat {
        if ctx.tag(node) != "DIALOG" || !has_attr(node, "open") {
            continue;
        }
        let dialog_ob = trimmed_style(ctx, node, "interaction", "overscroll-behavior-x")
            .or_else(|| trimmed_style(ctx, node, "interaction", "overscroll-behavior"));
        let backdrop_overflow = pseudo_style(node, "::backdrop", "box_model", "overflow-x");
        let backdrop_ob = pseudo_style(node, "::backdrop", "interaction", "overscroll-behavior-x")
            .or_else(|| pseudo_style(node, "::backdrop", "interaction", "overscroll-behavior"));
        let dialog_chains = !dialog_ob.is_some_and(|value| has_token(value, "contain"));
        let backdrop_chains = backdrop_overflow
            .is_some_and(|value| matches!(value, "visible" | "clip"))
            || !backdrop_ob.is_some_and(|value| has_token(value, "contain"));
        if !dialog_chains && !backdrop_chains {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!("open dialog {} does not contain overscroll: dialog={}, ::backdrop overflow={}, ::backdrop overscroll={}", node.selector, dialog_ob.unwrap_or("<unset>"), backdrop_overflow.unwrap_or("<unset>"), backdrop_ob.unwrap_or("<unset>")),
                "attrs+pseudo+styles.interaction",
            ),
        );
    }
}

fn mandatory_snap_unreachable_content(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &container in &ctx.flat {
        let Some(snap) = trimmed_style(ctx, container, "interaction", "scroll-snap-type") else {
            continue;
        };
        if !has_token(snap, "mandatory") {
            continue;
        }
        let Some(port_height) = rect_number(container, "height") else {
            continue;
        };
        for child in &container.children {
            let align = trimmed_style(ctx, child, "interaction", "scroll-snap-align");
            if align.is_none_or(|value| value == "none") {
                continue;
            }
            let Some(child_height) = rect_number(child, "height") else {
                continue;
            };
            if child_height <= port_height {
                continue;
            }
            ctx.emit(
                out,
                spec,
                child,
                Evidence::numeric(
                    format!("snap target {} is {child_height}px tall inside a {port_height}px mandatory scrollport — in-between content is unreachable", child.selector),
                    "styles.interaction+rect",
                    "snap_target_height",
                    child_height,
                    port_height,
                    "<=",
                ),
            );
        }
    }
}

fn hidden_until_found_display_none(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        if !attr_is(node, "hidden", "until-found") {
            continue;
        }
        let display_none = trimmed_style(ctx, node, "box_model", "display") == Some("none");
        let visibility = trimmed_style(ctx, node, "visual", "visibility");
        let invisible = matches!(visibility, Some("hidden" | "collapse"));
        if !display_none && !invisible {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!("[hidden=until-found] on {} is also display:none/visibility:hidden — find-in-page can never reveal it", node.selector),
                "attrs+styles.box_model+styles.visual",
            ),
        );
    }
}

fn sortable_table_aria_sort_misuse(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &table in &ctx.flat {
        if ctx.tag(table) != "TABLE" {
            continue;
        }
        let mut sorted_headers: Vec<&DiffNode> = Vec::new();
        for &node in &ctx.flat {
            if !in_subtree(ctx, table, node) {
                continue;
            }
            let Some(sort) = ctx.attr(node, "aria-sort") else {
                continue;
            };
            if sort == "none" {
                ctx.emit(
                    out,
                    spec,
                    node,
                    Evidence::new(
                        format!(
                            "aria-sort=\"none\" on {} — omit the attribute instead",
                            node.selector
                        ),
                        "attrs+tag+path",
                    ),
                );
                continue;
            }
            if ctx.tag(node) != "TH" {
                ctx.emit(
                    out,
                    spec,
                    node,
                    Evidence::new(
                        format!("aria-sort=\"{sort}\" on non-header {}", node.selector),
                        "attrs+tag+path",
                    ),
                );
                continue;
            }
            sorted_headers.push(node);
        }
        for header in sorted_headers.iter().skip(1) {
            ctx.emit(
                out,
                spec,
                header,
                Evidence::numeric(
                    format!("aria-sort is set on {} headers in table {} — only the sorted column may carry it", sorted_headers.len(), table.selector),
                    "attrs+tag+path",
                    "sorted_headers",
                    sorted_headers.len(),
                    1,
                    "<=",
                ),
            );
        }
    }
}

fn roving_tabindex_without_composite_role(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &container in &ctx.flat {
        let active = container
            .children
            .iter()
            .filter(|child| has_attr(child, "tabindex") && attr_is(child, "tabindex", "0"));
        let inactive = container.children.iter().filter(|child| {
            attr_is(child, "tabindex", "-1")
                && child.accessibility_grade() != Some("NONE")
                && ctx.visible(child)
        });
        if active.count() == 0 || inactive.count() == 0 {
            continue;
        }
        let role = aria_or_attr(ctx, container, "role").unwrap_or("");
        if composite_role(role) {
            continue;
        }
        ctx.emit(
            out,
            spec,
            container,
            Evidence::new(
                format!("roving tabindex on {} without a composite role — screen readers cannot expose the widget's structure", container.selector),
                "attrs+aria+path",
            ),
        );
    }
}

fn swipe_gesture_no_button_alternative(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &track in &ctx.flat {
        let Some(snap) = trimmed_style(ctx, track, "interaction", "scroll-snap-type") else {
            continue;
        };
        if !snap_is_horizontal_mandatory(snap) {
            continue;
        }
        let alternative = ctx
            .flat
            .iter()
            .any(|node| in_subtree(ctx, track, node) && named_control(ctx, node));
        if alternative {
            continue;
        }
        ctx.emit(
            out,
            spec,
            track,
            Evidence::new(
                format!("horizontal swipe track {} has no named button alternative — WCAG 2.5.1 Pointer Gestures", track.selector),
                "styles.interaction+tag+aria",
            ),
        );
    }
}

fn swipe_reveal_overscroll_trapped(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &wrapper in &ctx.flat {
        let Some(snap) = trimmed_style(ctx, wrapper, "interaction", "scroll-snap-type") else {
            continue;
        };
        if !snap_is_horizontal_mandatory(snap) {
            continue;
        }
        let overscroll = trimmed_style(ctx, wrapper, "interaction", "overscroll-behavior-x")
            .or_else(|| trimmed_style(ctx, wrapper, "interaction", "overscroll-behavior"));
        let trapped =
            overscroll.is_some_and(|value| has_token(value, "contain") || has_token(value, "none"));
        if !trapped {
            continue;
        }
        let has_outer_scroller = has_ancestor(ctx, wrapper, |ancestor| {
            trimmed_style(ctx, ancestor, "interaction", "scroll-snap-type").is_some()
        });
        if has_outer_scroller {
            continue;
        }
        ctx.emit(
            out,
            spec,
            wrapper,
            Evidence::new(
                format!("outer swipe wrapper {} traps overscroll ({}) — excess scroll cannot chain to the document", wrapper.selector, overscroll.unwrap_or("")),
                "styles.interaction+path",
            ),
        );
    }
}

fn hovercard_autofocus_on_hint(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let ids = id_index(ctx);
    for &node in &ctx.flat {
        if !has_attr(node, "autofocus") {
            continue;
        }
        let is_hint = attr_is(node, "popover", "hint")
            || ctx.attr(node, "interestfor").is_some_and(|id| {
                ids.get(id)
                    .is_some_and(|target| attr_is(target, "popover", "hint"))
            })
            || ctx.attr(node, "popovertarget").is_some_and(|id| {
                ids.get(id)
                    .is_some_and(|target| attr_is(target, "popover", "hint"))
            })
            || has_ancestor(ctx, node, |ancestor| attr_is(ancestor, "popover", "hint"));
        if !is_hint {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!("hovercard hint relationship on {} moves focus with autofocus — WCAG 3.2.1 On Focus", node.selector),
                "attrs",
            ),
        );
    }
}

fn hidden_scrollbar_without_affordance(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        if !is_scroll_container(ctx, node) {
            continue;
        }
        let width_none = trimmed_style(ctx, node, "interaction", "scrollbar-width") == Some("none");
        let webkit_hidden =
            pseudo_style(node, "::-webkit-scrollbar", "visual", "display") == Some("none");
        if !width_none && !webkit_hidden {
            continue;
        }
        let snap = trimmed_style(ctx, node, "interaction", "scroll-snap-type")
            .is_some_and(|value| value != "none");
        let colored = trimmed_style(ctx, node, "interaction", "scrollbar-color").is_some()
            || pseudo_style(
                node,
                "::-webkit-scrollbar-thumb",
                "visual",
                "background-color",
            )
            .is_some();
        if snap || colored {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!("scrollable {} hides its scrollbar with no alternative affordance — WCAG 1.4.11 Non-text Contrast", node.selector),
                "styles.interaction+pseudo",
            ),
        );
    }
}

fn haspopup_without_matching_role(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let ids = id_index(ctx);
    for &node in &ctx.flat {
        let Some(haspopup) = ctx
            .attr(node, "aria-haspopup")
            .or_else(|| ctx.aria(node, "haspopup"))
        else {
            continue;
        };
        let targets = [
            ctx.attr(node, "popovertarget"),
            ctx.attr(node, "commandfor"),
            ctx.attr(node, "aria-controls"),
        ];
        let mut checked = false;
        for target_id in targets.into_iter().flatten() {
            let Some(target) = ids.get(target_id).copied() else {
                continue;
            };
            checked = true;
            let role = aria_or_attr(ctx, target, "role").unwrap_or("");
            if popup_role_matches(haspopup, role) {
                continue;
            }
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!(
                        "aria-haspopup=\"{haspopup}\" on {} points to {} with role=\"{role}\"",
                        node.selector, target.selector
                    ),
                    "attrs+aria",
                ),
            );
        }
        if !checked && haspopup == "true" {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!(
                        "aria-haspopup=\"true\" on {} has no popup target to expose",
                        node.selector
                    ),
                    "attrs+aria",
                ),
            );
        }
    }
}

fn snap_stack_multiple_active_views(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &container in &ctx.flat {
        let Some(snap) = trimmed_style(ctx, container, "interaction", "scroll-snap-type") else {
            continue;
        };
        if has_token(snap, "none") {
            continue;
        }
        let active = container
            .children
            .iter()
            .filter(|child| {
                trimmed_style(ctx, child, "interaction", "scroll-snap-align")
                    .is_some_and(|value| value != "none")
                    && !has_attr(child, "inert")
                    && ctx.visible(child)
            })
            .count();
        if active < 2 {
            continue;
        }
        ctx.emit(
            out,
            spec,
            container,
            Evidence::numeric(
                format!("snap stack {} exposes {active} active views without inert — multiple views are announced", container.selector),
                "styles.interaction+attrs",
                "active_snap_views",
                active,
                1,
                "<=",
            ),
        );
    }
}

fn decorative_progress_exposed_to_at(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        let transform = trimmed_style(ctx, node, "transform", "transform")
            .or_else(|| trimmed_style(ctx, node, "transform", "scale"));
        let progress_transform = transform
            .is_some_and(|value| value.contains("scaleX(") || value.contains("scale("))
            && trimmed_style(ctx, node, "css_variables", "--scroll-progress").is_some();
        if !scroll_driven(ctx, node) && !progress_transform {
            continue;
        }
        let role = aria_or_attr(ctx, node, "role").unwrap_or("");
        if matches!(role, "presentation" | "none" | "progressbar")
            || attr_is(node, "aria-hidden", "true")
            || ctx.aria_bool(node, "hidden")
        {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!(
                    "decorative scroll progress indicator {} is exposed to assistive technology",
                    node.selector
                ),
                "styles.animation+aria",
            ),
        );
    }
}

fn scroll_shrink_header_missing_scroll_padding(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &header in &ctx.flat {
        let sticky = matches!(
            trimmed_style(ctx, header, "layout", "position"),
            Some("sticky" | "fixed")
        );
        let shrinking = scroll_driven(ctx, header)
            || trimmed_style(ctx, header, "animation", "transition-property")
                .is_some_and(|value| has_token(value, "height") || has_token(value, "block-size"));
        if !sticky || !shrinking {
            continue;
        }
        let scroller = nearest_ancestor(ctx, header, |ancestor| {
            matches!(ctx.tag(ancestor), "HTML" | "BODY") || is_scroll_container(ctx, ancestor)
        });
        let padding = scroller.and_then(|root| {
            trimmed_style(ctx, root, "interaction", "scroll-padding-top")
                .or_else(|| trimmed_style(ctx, root, "interaction", "scroll-padding-block-start"))
        });
        if positive_number(padding) {
            continue;
        }
        ctx.emit(
            out,
            spec,
            header,
            Evidence::numeric(
                format!("shrinking sticky header {} has no positive scroll-padding on its scroll root — anchored targets can hide under it", header.selector),
                "styles.animation+styles.layout+styles.interaction",
                "scroll_padding_top_px",
                padding.and_then(number).unwrap_or(0.0),
                0.0,
                ">",
            ),
        );
    }
}

fn stateful_sticky_header_without_overflow_anchor_none(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &header in &ctx.flat {
        let sticky = trimmed_style(ctx, header, "layout", "position") == Some("sticky");
        if !sticky || !scroll_driven(ctx, header) {
            continue;
        }
        let anchor = nearest_ancestor(ctx, header, |ancestor| {
            is_scroll_container(ctx, ancestor) || matches!(ctx.tag(ancestor), "HTML" | "BODY")
        });
        let Some(scroller) = anchor else {
            continue;
        };
        let value = trimmed_style(ctx, scroller, "box_model", "overflow-anchor").unwrap_or("auto");
        if value == "none" {
            continue;
        }
        ctx.emit(
            out,
            spec,
            header,
            Evidence::new(
                format!("stateful sticky header {} has overflow-anchor={value} on {} — inserted content shifts the scroll position", header.selector, scroller.selector),
                "styles.layout+styles.box_model+parent_id",
            ),
        );
    }
}

fn copy_control_hidden_from_at(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for &node in &ctx.flat {
        let copyish = ctx.tag(node) == "BUTTON"
            && (ctx.attr(node, "data-copy").is_some()
                || ctx
                    .attr(node, "command")
                    .is_some_and(|command| command.to_ascii_lowercase().contains("copy"))
                || ctx
                    .attr(node, "aria-label")
                    .is_some_and(|label| label.to_ascii_lowercase().contains("copy"))
                || node.selector.to_ascii_lowercase().contains("copy"));
        if !copyish {
            continue;
        }
        let role = aria_or_attr(ctx, node, "role").unwrap_or("");
        let hidden = attr_is(node, "aria-hidden", "true")
            || ctx.aria_bool(node, "hidden")
            || matches!(role, "presentation" | "none");
        if !hidden {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!(
                    "copy control {} is hidden from assistive technology",
                    node.selector
                ),
                "aria+attrs",
            ),
        );
    }
}

fn tour_popover_unnamed(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for &node in &ctx.flat {
        let is_tour = has_attr(node, "popover")
            && (ctx.tag(node) == "DIALOG"
                || aria_or_attr(ctx, node, "role") == Some("dialog")
                || node.selector.to_ascii_lowercase().contains("tour"));
        if !is_tour {
            continue;
        }
        let named = ctx
            .aria(node, "name")
            .is_some_and(|name| !name.trim().is_empty())
            || ctx.attr(node, "aria-label").is_some()
            || ctx.attr(node, "aria-labelledby").is_some()
            || node
                .ax
                .as_ref()
                .and_then(|ax| ax.get("name"))
                .and_then(Value::as_str)
                .is_some_and(|name| !name.trim().is_empty());
        if named {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!("tour popover {} has no accessible name", node.selector),
                "attrs+aria",
            ),
        );
    }
}

fn scrollspy_without_scroll_target_group(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &container in &ctx.flat {
        let group = trimmed_style(ctx, container, "interaction", "scroll-target-group");
        if group.is_some_and(|value| value != "none") {
            continue;
        }
        let current = ctx
            .flat
            .iter()
            .filter(|node| in_subtree(ctx, container, node))
            .filter(|node| ctx.attr(node, "aria-current").is_some())
            .count();
        if current < 2 {
            continue;
        }
        ctx.emit(
            out,
            spec,
            container,
            Evidence::numeric(
                format!(
                    "scrollspy {} tracks {} aria-current items without scroll-target-group",
                    container.selector, current
                ),
                "styles.interaction+attrs",
                "current_items",
                current,
                2,
                ">=",
            ),
        );
    }
}
