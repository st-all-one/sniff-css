//! Family I — CSS and visual-design opportunities.
//!
//! Two groups live here: zero-cost signals derived from already captured
//! `styles`/`rect`/`aria` facets (table D), and guards that need `declared`
//! (CSSOM, including at-rule preludes) or `env` preferences (table F).
//! Every check is deterministic: no clock, no environment reads, no random
//! input, and all grouped evidence uses `BTreeMap`/`BTreeSet`.

use crate::modern::{CheckSpec, Facet, ModernCtx};
use crate::rules::CheckLine;
use serde_json::Value;
use sniff_core::contrast::{contrast_ratio, parse_color};
use sniff_core::modern::{DeclaredFacet, DeclaredRule, Evidence};
use sniff_css_diff::{DiffDocument, DiffNode};
use std::collections::{BTreeMap, BTreeSet};

const F_STYLES: &[Facet] = &[Facet::Styles];
const F_STYLES_ENV: &[Facet] = &[Facet::Styles, Facet::Env];
const F_STYLES_RECT: &[Facet] = &[Facet::Styles, Facet::Rect];
const F_STYLES_ATTRS: &[Facet] = &[Facet::Styles, Facet::Attrs];
const F_STYLES_AX_ATTRS: &[Facet] = &[Facet::Styles, Facet::Rect, Facet::Attrs, Facet::Ax];
const F_DECLARED: &[Facet] = &[Facet::Declared];
const F_STYLES_DECLARED: &[Facet] = &[Facet::Styles, Facet::Declared];
const F_STYLES_DECLARED_ENV: &[Facet] = &[Facet::Styles, Facet::Declared, Facet::Env];
const F_DOC_DECLARED_ENV: &[Facet] = &[Facet::Document, Facet::Declared, Facet::Env];

/// All family I ids, in catalog order (15 zero-cost, 15 declared/env).
static SPECS: &[CheckSpec] = &[
    // --- D: zero-cost -------------------------------------------------
    CheckSpec::new(
        "visual-order-breaks-tab-order",
        &["css-layout"],
        F_STYLES_RECT,
        "silent when no reordered descendant is focusable or rect/order is absent",
    ),
    CheckSpec::new(
        "transform-stacking-context-shift-on-hover",
        &["individual-transform-properties"],
        &[Facet::Styles, Facet::Pseudo],
        "silent when the base already declares identity transforms or no :hover pseudo is captured",
    ),
    CheckSpec::new(
        "overflow-clip-margin-ignored",
        &["overflow-clipping-control"],
        F_STYLES,
        "silent for replaced elements, active overflow: clip or unsupported (empty) computed value",
    ),
    CheckSpec::new(
        "media-aspect-ratio-missing",
        &["responsive-design"],
        F_STYLES_ATTRS,
        "silent when width/height attributes or a CSS aspect-ratio reserve space",
    ),
    CheckSpec::new(
        "size-containment-without-definite-block-size",
        &["size-aware-styling"],
        F_STYLES_RECT,
        "silent for inline-size containment or when a definite block size is set",
    ),
    CheckSpec::new(
        "scrollbar-width-thin-or-none",
        &["browser-ui-theming"],
        F_STYLES,
        "silent on non-scrollable regions, auto width or an explicit scroll alternative",
    ),
    CheckSpec::new(
        "decoration-faked-with-border-or-gradient",
        &["custom-text-decorations"],
        F_STYLES_ATTRS,
        "silent when underline is kept or the bottom border is a legitimate box edge",
    ),
    CheckSpec::new(
        "scrollbar-thumb-track-contrast",
        &["customize-scrollbar-color-and-thickness"],
        F_STYLES_ENV,
        "silent for auto/transparent colors, unresolved overlay tracks or ratio >= 3:1",
    )
    .failing(),
    CheckSpec::new(
        "text-fit-on-intrinsic-width-container",
        &["equal-width-text-lines"],
        F_STYLES,
        "silent without computed text-fit, on definite widths or when the @supports fallback exists",
    ),
    CheckSpec::new(
        "drawable-without-geometry",
        &["expose-canvas-content-to-browser-features"],
        F_STYLES_AX_ATTRS,
        "silent without the ax tree, for decorative drawables or when the overlay has non-zero rect",
    ),
    CheckSpec::new(
        "text-wrap-balance-on-long-block",
        &["improve-text-layout-and-legibility"],
        F_STYLES_RECT,
        "silent for pretty/wrap, <= 6 estimated lines or unsupported (empty) computed value",
    ),
    CheckSpec::new(
        "text-box-on-long-form-text",
        &["precise-text-alignment"],
        F_STYLES_RECT,
        "silent on headings/buttons/badges and when the block has at most three lines",
    ),
    CheckSpec::new(
        "mask-on-text-containing-element",
        &["shaped-cutouts"],
        F_STYLES,
        "silent for gradient notches, unmasked text siblings or masks without an SVG url(#...)",
    ),
    CheckSpec::new(
        "justify-without-hyphens",
        &["typography"],
        F_STYLES_RECT,
        "silent on single-line blocks, with hyphens: auto or without a measured line height",
    ),
    CheckSpec::new(
        "blend-mode-without-isolation",
        &["visual-effects"],
        F_STYLES,
        "silent for mix-blend-mode: normal, isolation: isolate or a single isolated root",
    ),
    // --- F: declared / env / at-rules ---------------------------------
    CheckSpec::new(
        "intrinsic-size-transition-no-reduced-motion",
        &["animate-to-intrinsic-sizes"],
        F_STYLES_DECLARED_ENV,
        "silent unless reduce is observed and the size transition duration stays > 0s; @media guard wins",
    ),
    CheckSpec::new(
        "calc-size-missing-fallback",
        &["calculate-with-intrinsic-sizes"],
        F_STYLES_DECLARED,
        "silent when the browser ignores calc-size(), a keyword fallback precedes it or @supports not exists",
    ),
    CheckSpec::new(
        "has-critical-state-without-fallback",
        &["child-state-based-styling", "content-based-styling"],
        F_DECLARED,
        "silent for decorative color changes or when @supports not selector(:has(...)) exists",
    ),
    CheckSpec::new(
        "inline-nontrivial-style",
        &["css"],
        F_DECLARED,
        "silent for trivial values, state-driven inline styles and author/ua origins",
    ),
    CheckSpec::new(
        "style-query-without-selector-fallback",
        &[
            "design-token-reactivity",
            "usage-aware-component-variations",
        ],
        F_DECLARED,
        "silent for cosmetic-only variations and when a :where([data-*]) selector fallback exists",
    ),
    CheckSpec::new(
        "sibling-functions-without-feature-detect",
        &["dynamic-sibling-styling"],
        F_DECLARED,
        "silent with @supports plus --sibling-index/--sibling-count fallback or no differing computed values",
    ),
    CheckSpec::new(
        "fluid-type-clamp-exceeds-2_5x",
        &["fluid-scaling"],
        F_DECLARED,
        "silent without an explicit clamp(), for vw fallbacks or when max <= 2.5 * min",
    ),
    CheckSpec::new(
        "color-scheme-mismatch",
        &["selector-atrule-combinations"],
        F_DOC_DECLARED_ENV,
        "silent when the meta/root scheme is dark or a @media (prefers-color-scheme: dark) block exists",
    ),
    CheckSpec::new(
        "has-user-invalid-without-aria-invalid",
        &["style-parent-with-has"],
        F_DECLARED,
        "silent for non-form fields, decorative rules or when aria-invalid=true was already synced",
    ),
    CheckSpec::new(
        "scrollbar-contrast-ignores-prefers-contrast",
        &["adapt-scrollbar-to-contrast-preferences"],
        F_STYLES_DECLARED_ENV,
        "silent for auto/transparent colors, extreme contrast or a @media (prefers-contrast: more) override",
    ),
    CheckSpec::new(
        "forced-colors-unhandled",
        &["color"],
        F_STYLES_DECLARED_ENV,
        "silent without active forced colors, with forced-color-adjust: none, system-color outlines or a media override",
    ),
    CheckSpec::new(
        "mask-no-graceful-fallback",
        &[
            "complex-shapes",
            "soft-edge-content-fade",
            "visually-texture-content",
        ],
        F_STYLES_DECLARED,
        "silent for decorative masks and when @supports not (mask-image...) carries a fallback",
    ),
    CheckSpec::new(
        "drawable-3d-geometry-unverifiable",
        &["interactive-content-in-3d-scenes"],
        F_STYLES_AX_ATTRS,
        "silent without a 3D-transformed drawable inside a canvas(content=drawable) or without ax exposure",
    ),
    CheckSpec::new(
        "font-size-adjust-missing-fallback",
        &[
            "visually-stable-font-fallbacks",
            "visually-stable-mixed-fonts",
        ],
        F_STYLES_DECLARED,
        "silent for numeric/from-font values, font-display: optional, @font-face metric overrides or @supports not fallback",
    ),
];

/// Family I registry (the single source of ids for this family).
pub fn specs() -> &'static [CheckSpec] {
    SPECS
}

/// Runs every enabled family I check against the rendered snapshot.
pub fn run(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    tracing::debug!(
        family = "css-visual",
        registered = SPECS.len(),
        "running modern family I"
    );
    let by_id: BTreeMap<u64, &DiffNode> = ctx.flat.iter().map(|n| (n.id, *n)).collect();
    for spec in SPECS {
        if !ctx.enabled(spec) {
            tracing::trace!(id = spec.id, "modern check suppressed by facet or options");
            continue;
        }
        match spec.id {
            "visual-order-breaks-tab-order" => check_visual_order(ctx, out, spec),
            "transform-stacking-context-shift-on-hover" => check_hover_transform(ctx, out, spec),
            "overflow-clip-margin-ignored" => check_overflow_clip_margin(ctx, out, spec),
            "media-aspect-ratio-missing" => check_media_aspect_ratio(ctx, out, spec),
            "size-containment-without-definite-block-size" => {
                check_size_containment(ctx, out, spec)
            }
            "scrollbar-width-thin-or-none" => check_scrollbar_width(ctx, out, spec),
            "decoration-faked-with-border-or-gradient" => check_faked_decoration(ctx, out, spec),
            "scrollbar-thumb-track-contrast" => check_scrollbar_contrast(ctx, out, spec),
            "text-fit-on-intrinsic-width-container" => check_text_fit(ctx, out, spec),
            "drawable-without-geometry" => check_drawable_geometry(ctx, out, spec, &by_id),
            "text-wrap-balance-on-long-block" => check_text_wrap(ctx, out, spec),
            "text-box-on-long-form-text" => check_text_box(ctx, out, spec),
            "mask-on-text-containing-element" => check_svg_mask_on_text(ctx, out, spec, &by_id),
            "justify-without-hyphens" => check_justify(ctx, out, spec),
            "blend-mode-without-isolation" => check_blend_mode(ctx, out, spec),
            "intrinsic-size-transition-no-reduced-motion" => {
                check_intrinsic_transition(ctx, out, spec)
            }
            "calc-size-missing-fallback" => check_calc_size(ctx, out, spec),
            "has-critical-state-without-fallback" => check_has_critical(ctx, out, spec),
            "inline-nontrivial-style" => check_inline_style(ctx, out, spec),
            "style-query-without-selector-fallback" => check_style_query(ctx, out, spec),
            "sibling-functions-without-feature-detect" => check_sibling_functions(ctx, out, spec),
            "fluid-type-clamp-exceeds-2_5x" => check_fluid_type(ctx, out, spec),
            "color-scheme-mismatch" => check_color_scheme(ctx, out, spec),
            "has-user-invalid-without-aria-invalid" => check_user_invalid(ctx, out, spec),
            "scrollbar-contrast-ignores-prefers-contrast" => check_scrollbar_pref(ctx, out, spec),
            "forced-colors-unhandled" => check_forced_colors(ctx, out, spec),
            "mask-no-graceful-fallback" => check_mask_fallback(ctx, out, spec),
            "drawable-3d-geometry-unverifiable" => check_drawable_3d(ctx, out, spec, &by_id),
            "font-size-adjust-missing-fallback" => check_font_size_adjust(ctx, out, spec),
            other => tracing::warn!(id = other, "unhandled family I check id"),
        }
    }
    tracing::debug!(
        family = "css-visual",
        emitted = out.len(),
        "family I finished"
    );
}

// ---------------------------------------------------------------------------
// D — zero-cost checks
// ---------------------------------------------------------------------------

fn check_visual_order(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        let display = ctx
            .style(node, "layout", "display")
            .unwrap_or("")
            .to_ascii_lowercase();
        if !display.contains("flex") && !display.contains("grid") {
            continue;
        }
        let reverse = ctx
            .style(node, "layout", "flex-direction")
            .is_some_and(|d| d.trim().to_ascii_lowercase().ends_with("-reverse"));
        let focusables: Vec<&DiffNode> = descendants(node)
            .into_iter()
            .filter(|n| is_focusable(ctx, n))
            .collect();
        if focusables.len() < 2 {
            continue;
        }
        let reordered = reverse || focusables.iter().any(|n| order_of(ctx, n) != 0);
        if !reordered {
            continue;
        }
        let Some(first_y) = rect_num(focusables[0], "y") else {
            continue;
        };
        let row: Vec<(usize, &DiffNode)> = focusables
            .iter()
            .enumerate()
            .filter(|(_, n)| rect_num(n, "y").is_some_and(|y| (y - first_y).abs() <= 4.0))
            .map(|(index, n)| (index, *n))
            .collect();
        if row.len() < 2 {
            continue;
        }
        let mut visual: Vec<usize> = row.iter().map(|(index, _)| *index).collect();
        visual.sort_by(|a, b| {
            let xa = rect_num(row[*a].1, "x").unwrap_or(0.0);
            let xb = rect_num(row[*b].1, "x").unwrap_or(0.0);
            xa.total_cmp(&xb)
        });
        let dom: Vec<usize> = row.iter().map(|(index, _)| *index).collect();
        if visual == dom {
            continue;
        }
        let evidence = Evidence::numeric(
            format!(
                "display={display}; focusable={}; visual order differs from path order (WCAG 2.4.3)",
                focusables.len()
            ),
            "styles.layout+rect",
            "visually_reordered_focusables",
            1,
            0,
            ">",
        );
        ctx.emit(out, spec, node, evidence);
        tracing::debug!(check = spec.id, selector = %node.selector, "visual focus order diverges from DOM");
    }
}

fn check_hover_transform(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        let Some(pseudo) = node.pseudo.as_ref() else {
            continue;
        };
        let Some(hover) = pseudo.get(":hover").or_else(|| pseudo.get("hover")) else {
            continue;
        };
        let transform_group = hover.get("transform");
        let hover_shifts = ["transform", "translate", "rotate", "scale"]
            .iter()
            .any(|prop| {
                transform_group
                    .and_then(|group| group.get(*prop))
                    .and_then(Value::as_str)
                    .is_some_and(|value| !identity_transform(value))
            });
        if !hover_shifts {
            continue;
        }
        let base_identity = ["transform", "translate", "rotate", "scale"]
            .iter()
            .all(|prop| {
                ctx.style(node, "transform", prop)
                    .is_none_or(identity_transform)
            });
        if !base_identity {
            continue;
        }
        if node
            .metrics
            .as_ref()
            .and_then(|m| m.get("stacking_context"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            continue;
        }
        let evidence = Evidence::new(
            "hover applies a non-identity transform on a base without identity transforms; stacking context shifts",
            "pseudo+:hover+styles.transform",
        );
        ctx.emit(out, spec, node, evidence);
        tracing::debug!(check = spec.id, selector = %node.selector, "hover creates a stacking context");
    }
}

fn check_overflow_clip_margin(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let replaced = ["IMG", "VIDEO", "CANVAS", "SVG", "IFRAME"];
    for node in &ctx.flat {
        let Some(margin) = ctx.style(node, "box_model", "overflow-clip-margin") else {
            continue;
        };
        if matches!(
            margin.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "0px" | "normal" | "content-box"
        ) {
            continue;
        }
        if replaced.contains(&ctx.tag(node)) {
            continue;
        }
        let overflow_x = ctx.style(node, "box_model", "overflow-x").unwrap_or("");
        let overflow_y = ctx.style(node, "box_model", "overflow-y").unwrap_or("");
        if matches!(overflow_x, "clip") || matches!(overflow_y, "clip") {
            continue;
        }
        let clips = |v: &str| matches!(v, "hidden" | "auto" | "scroll");
        if !clips(overflow_x) && !clips(overflow_y) {
            continue;
        }
        let evidence = Evidence::new(
            format!(
                "overflow-x={overflow_x}; overflow-y={overflow_y}; overflow-clip-margin={margin} is ignored without overflow: clip"
            ),
            "styles.box_model",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

fn check_media_aspect_ratio(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        if !matches!(ctx.tag(node), "IMG" | "VIDEO") {
            continue;
        }
        let ratio = ctx
            .style_any(
                node,
                &[("box_model", "aspect-ratio"), ("custom", "aspect-ratio")],
            )
            .unwrap_or("");
        if !matches!(
            ratio.trim().to_ascii_lowercase().as_str(),
            "" | "auto" | "none"
        ) {
            continue;
        }
        if ctx.attr(node, "width").is_some() || ctx.attr(node, "height").is_some() {
            continue;
        }
        let evidence = Evidence::numeric(
            format!(
                "tag={}; aspect-ratio={ratio}; no width/height attributes reserve space",
                ctx.tag(node)
            ),
            "styles.box_model+attrs",
            "declared_media_dimensions",
            0,
            1,
            "<",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

fn check_size_containment(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        let containment = ctx
            .style_any(
                node,
                &[("layout", "container-type"), ("custom", "container-type")],
            )
            .unwrap_or("");
        if containment.trim().to_ascii_lowercase() != "size" {
            continue;
        }
        let block = ctx.style(node, "box_model", "block-size").unwrap_or("");
        if matches!(
            block.trim().to_ascii_lowercase().as_str(),
            "" | "auto" | "none"
        ) {
            let collapsed = descendants(node)
                .iter()
                .any(|child| rect_num(child, "height").is_some_and(|h| h <= 1.0));
            if collapsed {
                let evidence = Evidence::new(
                    format!(
                        "container-type=size; block-size={block}; a descendant collapses to zero height"
                    ),
                    "styles.layout+styles.box_model+rect",
                );
                ctx.emit(out, spec, node, evidence);
            }
        }
    }
}

fn check_scrollbar_width(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let alternative = ctx.flat.iter().any(|node| {
        node.tag.as_deref() == Some("BUTTON")
            && ctx
                .attr(node, "aria-label")
                .is_some_and(|label| label.to_ascii_lowercase().contains("scroll"))
    });
    if alternative {
        tracing::debug!(
            check = spec.id,
            "explicit scroll alternative found; check silenced"
        );
        return;
    }
    for node in &ctx.flat {
        let width = ctx
            .style(node, "interaction", "scrollbar-width")
            .unwrap_or("");
        if !matches!(width.trim(), "thin" | "none") {
            continue;
        }
        let scrollable = ["auto", "scroll"].iter().any(|value| {
            ["overflow-x", "overflow-y"]
                .iter()
                .any(|prop| ctx.style(node, "box_model", prop) == Some(*value))
        });
        if !scrollable {
            continue;
        }
        let evidence = Evidence::new(
            format!(
                "scrollbar-width={width} on a scrollable region; no accessible scroll alternative captured"
            ),
            "styles.interaction+styles.box_model",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

fn check_faked_decoration(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        if ctx.tag(node) != "A" || ctx.attr(node, "href").is_none() {
            continue;
        }
        let line = ctx
            .style(node, "typography", "text-decoration-line")
            .unwrap_or("");
        if line.contains("underline") || line.trim() != "none" {
            continue;
        }
        let image = ctx.style(node, "visual", "background-image").unwrap_or("");
        let gradient = image.contains("linear-gradient");
        let side =
            |name: &str| px_value(ctx.style(node, "box_model", name).unwrap_or("")).unwrap_or(0.0);
        let bottom = side("border-bottom-width");
        let other_edges =
            side("border-top-width") + side("border-right-width") + side("border-left-width");
        if !gradient && !(bottom > 0.0 && other_edges <= 0.0) {
            continue;
        }
        let evidence = Evidence::new(
            format!(
                "a[href]; text-decoration-line=none; background-image={image}; border-bottom-width={bottom}px"
            ),
            "styles.typography+styles.visual+attrs",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

fn check_scrollbar_contrast(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        let value = ctx
            .style_any(
                node,
                &[
                    ("interaction", "scrollbar-color"),
                    ("visual", "scrollbar-color"),
                ],
            )
            .unwrap_or("");
        if matches!(value.trim().to_ascii_lowercase().as_str(), "" | "auto") {
            continue;
        }
        let Some((thumb_text, track_text, thumb, track)) = parse_color_pair(value) else {
            tracing::debug!(
                check = spec.id,
                value_len = value.len(),
                "scrollbar colors unresolved; limitation recorded by silence"
            );
            continue;
        };
        let ratio = contrast_ratio(thumb, track);
        if ratio >= 3.0 {
            continue;
        }
        let rounded = (ratio * 100.0).round() / 100.0;
        let evidence = Evidence::numeric(
            format!(
                "scrollbar-color={thumb_text} {track_text}; contrast={rounded:.2}:1 (<3:1); WCAG 1.4.11"
            ),
            "styles.interaction",
            "scrollbar_thumb_track_ratio",
            rounded,
            3.0,
            "<",
        );
        ctx.emit(out, spec, node, evidence);
        tracing::debug!(check = spec.id, selector = %node.selector, "scrollbar thumb/track below 3:1");
    }
}

fn check_text_fit(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        let fit = ctx
            .style_any(node, &[("custom", "text-fit"), ("typography", "text-fit")])
            .unwrap_or("");
        if matches!(fit.trim().to_ascii_lowercase().as_str(), "" | "none") {
            continue;
        }
        let inline = ctx.style(node, "box_model", "inline-size").unwrap_or("");
        let intrinsic = inline.contains("fit-content")
            || inline.contains("max-content")
            || inline.trim() == "auto";
        if !intrinsic {
            continue;
        }
        if ctx.doc.declared.as_ref().is_some_and(|declared| {
            at_rule_has(declared, &["not", "text-fit"])
                || all_rules(declared)
                    .any(|rule| rule.declarations.contains_key("--text-fit-scale"))
        }) {
            continue;
        }
        let evidence = Evidence::new(
            format!("text-fit={fit}; inline-size={inline} is intrinsic, so text-fit has no effect"),
            "styles.custom+styles.box_model",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

fn check_drawable_geometry(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
    by_id: &BTreeMap<u64, &DiffNode>,
) {
    for node in &ctx.flat {
        if !has_attr(node, "drawable") || !is_focusable(ctx, node) {
            continue;
        }
        if canvas_ancestor(ctx, node, by_id).is_none() {
            continue;
        }
        if node.ax.as_ref().and_then(|ax| ax.get("ignored")) == Some(&Value::Bool(true)) {
            continue;
        }
        let width = rect_num(node, "width").unwrap_or(0.0);
        let height = rect_num(node, "height").unwrap_or(0.0);
        if width > 0.0 && height > 0.0 {
            continue;
        }
        let evidence = Evidence::numeric(
            format!(
                "canvas content=drawable; focusable drawable has rect={width:.0}x{height:.0} and remains exposed in the ax tree"
            ),
            "attrs+ax+rect",
            "drawable_zero_geometry",
            0,
            1,
            "<=",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

fn check_text_wrap(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        let mode = ctx
            .style_any(
                node,
                &[
                    ("typography", "text-wrap-mode"),
                    ("typography", "text-wrap"),
                ],
            )
            .unwrap_or("")
            .to_ascii_lowercase();
        if !mode.contains("balance") {
            continue;
        }
        let Some(lines) = line_count(ctx, node) else {
            continue;
        };
        if lines <= 6.0 {
            continue;
        }
        let rounded = (lines * 10.0).round() / 10.0;
        let evidence = Evidence::numeric(
            format!(
                "text-wrap-mode={mode}; approximately {rounded:.1} lines (>6); Chromium stops balancing at 6"
            ),
            "styles.typography+rect",
            "estimated_lines",
            rounded,
            6.0,
            ">",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

fn check_text_box(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let excluded = [
        "H1", "H2", "H3", "H4", "H5", "H6", "BUTTON", "BADGE", "SPAN",
    ];
    for node in &ctx.flat {
        let box_value = ctx
            .style_any(node, &[("custom", "text-box"), ("typography", "text-box")])
            .unwrap_or("");
        if matches!(box_value.trim().to_ascii_lowercase().as_str(), "" | "none") {
            continue;
        }
        if excluded.contains(&ctx.tag(node)) {
            continue;
        }
        let long_form = matches!(ctx.tag(node), "P" | "BLOCKQUOTE" | "LI")
            || line_count(ctx, node).is_some_and(|lines| lines > 3.0);
        if !long_form {
            continue;
        }
        let evidence = Evidence::new(
            format!(
                "text-box={box_value}; tag={} on long-form text",
                ctx.tag(node)
            ),
            "styles.typography+rect",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

fn check_svg_mask_on_text(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
    by_id: &BTreeMap<u64, &DiffNode>,
) {
    for node in &ctx.flat {
        let mask = ctx.style(node, "visual", "mask-image").unwrap_or("");
        if !mask.contains("url(#") || !has_text(ctx, node) {
            continue;
        }
        let safe_sibling = node
            .parent_id
            .and_then(|id| by_id.get(&id))
            .is_some_and(|parent| {
                parent.children.iter().any(|sibling| {
                    sibling.id != node.id
                        && has_text(ctx, sibling)
                        && !ctx
                            .style(sibling, "visual", "mask-image")
                            .is_some_and(|m| m.contains("url(#"))
                })
            });
        if safe_sibling {
            continue;
        }
        let evidence = Evidence::new(
            format!(
                "mask-image={mask} applied directly to a text-containing element; glyphs can be cropped"
            ),
            "styles.visual+aria+path",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

fn check_justify(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        let align = ctx.style(node, "typography", "text-align").unwrap_or("");
        if align.trim().to_ascii_lowercase() != "justify" {
            continue;
        }
        let hyphens = ctx
            .style(node, "typography", "hyphens")
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if !matches!(hyphens.as_str(), "none" | "manual") {
            continue;
        }
        let Some(lines) = line_count(ctx, node) else {
            continue;
        };
        if lines <= 1.0 {
            continue;
        }
        let lang = ctx
            .doc
            .document
            .as_ref()
            .and_then(|doc| doc.lang.as_deref())
            .or_else(|| ctx.aria(node, "lang"))
            .or_else(|| ctx.attr(node, "lang"));
        let lang_text = lang.unwrap_or("absent");
        let evidence = Evidence::numeric(
            format!(
                "text-align=justify; hyphens={hyphens}; lang={lang_text}; approximately {lines:.1} lines"
            ),
            "styles.typography+rect+aria",
            "estimated_lines",
            (lines * 10.0).round() / 10.0,
            1.0,
            ">",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

fn check_blend_mode(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        let blend = ctx
            .style(node, "visual", "mix-blend-mode")
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if matches!(blend.as_str(), "" | "normal") {
            continue;
        }
        let isolation = ctx
            .style(node, "visual", "isolation")
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if isolation == "isolate" {
            continue;
        }
        if node.parent_id.is_none() && ctx.flat.len() < 2 {
            continue;
        }
        let path = node.path.as_deref().unwrap_or("");
        let evidence = Evidence::new(
            format!(
                "mix-blend-mode={blend}; isolation={isolation}; path={path} can affect ancestors/siblings"
            ),
            "styles.visual+path",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

// ---------------------------------------------------------------------------
// F — declared / env / at-rules
// ---------------------------------------------------------------------------

fn check_intrinsic_transition(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = declared_of(ctx.doc) else {
        return;
    };
    if !env_value(ctx.doc, "prefers-reduced-motion").is_some_and(|v| v == "reduce") {
        tracing::debug!(check = spec.id, "reduce preference not observed; silenced");
        return;
    }
    if at_rule_has(declared, &["prefers-reduced-motion", "reduce"]) {
        tracing::debug!(
            check = spec.id,
            "reduced-motion guard present in declared at-rules"
        );
        return;
    }
    let sizing = [
        ("block-size", "block-size"),
        ("inline-size", "inline-size"),
        ("height", "height"),
        ("width", "width"),
    ];
    for node in &ctx.flat {
        let properties = ctx
            .style(node, "animation", "transition-property")
            .unwrap_or("");
        let Some((_, property)) = sizing
            .iter()
            .find(|(animation_name, _)| properties.contains(animation_name))
        else {
            continue;
        };
        let dimension = ctx.style(node, "box_model", property).unwrap_or("");
        let intrinsic = matches!(
            dimension.trim().to_ascii_lowercase().as_str(),
            "auto" | "min-content" | "max-content" | "fit-content"
        ) || dimension.contains("calc-size(");
        if !intrinsic {
            continue;
        }
        let duration = ctx
            .style(node, "animation", "transition-duration")
            .unwrap_or("");
        let Some(seconds) = first_number(duration) else {
            continue;
        };
        if seconds <= 0.0 {
            continue;
        }
        let evidence = Evidence::new(
            format!(
                "transition-property={properties}; {property}={dimension}; prefers-reduced-motion=reduce; transition-duration={duration} (expected 0s)"
            ),
            "styles.animation+styles.box_model+__env+declared",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

fn check_calc_size(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = declared_of(ctx.doc) else {
        return;
    };
    let sizing = ["inline-size", "block-size", "width", "height"];
    let mut seen = BTreeSet::new();
    for rule in all_rules(declared) {
        for (property, value) in &rule.declarations {
            if !sizing.contains(&property.as_str()) || !value.contains("calc-size(") {
                continue;
            }
            let supported = ctx.flat.iter().any(|node| {
                ctx.style(node, "box_model", property)
                    .is_some_and(|v| v.contains("calc-size("))
            });
            if !supported {
                continue;
            }
            let fallback = all_rules(declared).any(|candidate| {
                candidate.declarations.iter().any(|(prop, val)| {
                    prop == property && !val.contains("calc-size(") && is_plain_size(val)
                })
            }) || at_rule_has(declared, &["not", "calc-size"]);
            if fallback {
                continue;
            }
            if seen.insert(rule.selector.clone()) {
                let evidence = Evidence::new(
                    format!(
                        "{property}={value}; no keyword/length declaration precedes calc-size() in declared CSS"
                    ),
                    "declared+styles.box_model",
                );
                ctx.emit_global(out, spec, &rule.selector, evidence);
                tracing::debug!(check = spec.id, selector = %rule.selector, "calc-size() without fallback");
            }
        }
    }
}

fn check_has_critical(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = declared_of(ctx.doc) else {
        return;
    };
    let critical = [
        "display",
        "visibility",
        "flex-direction",
        "grid-template-columns",
        "grid-template-rows",
        "position",
        "opacity",
    ];
    let selectors: BTreeSet<&str> = declared
        .rules
        .iter()
        .filter(|rule| rule.selector.contains(":has("))
        .filter(|rule| {
            rule.declarations
                .keys()
                .any(|p| critical.contains(&p.as_str()))
        })
        .map(|rule| rule.selector.as_str())
        .collect();
    if selectors.is_empty() || at_rule_has(declared, &["not", "selector(:has"]) {
        return;
    }
    let joined = selectors
        .iter()
        .copied()
        .take(3)
        .collect::<Vec<_>>()
        .join(", ");
    let evidence = Evidence::new(
        format!(
            ":has() selectors change layout/visibility ({joined}); @supports not selector(:has(*)) fallback absent"
        ),
        "declared.at_rules",
    );
    ctx.emit_global(out, spec, ":root", evidence);
}

fn check_inline_style(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = declared_of(ctx.doc) else {
        return;
    };
    let state_markers = [
        "active", "selected", "open", "error", "expanded", "dragging",
    ];
    for rule in &declared.rules {
        if rule.origin != "inline" {
            continue;
        }
        let lower_selector = rule.selector.to_ascii_lowercase();
        if state_markers
            .iter()
            .any(|marker| lower_selector.contains(marker))
        {
            continue;
        }
        let Some((property, value)) = rule
            .declarations
            .iter()
            .find(|(_, value)| is_nontrivial_literal(value))
        else {
            continue;
        };
        let evidence = Evidence::new(
            format!(
                "origin=inline; {property}={value} is a nontrivial literal; tag/selector={}",
                rule.selector
            ),
            "declared+attrs",
        );
        ctx.emit_global(out, spec, &rule.selector, evidence);
    }
}

fn check_style_query(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = declared_of(ctx.doc) else {
        return;
    };
    let structural = [
        "padding",
        "padding-inline",
        "padding-block",
        "margin",
        "display",
        "visibility",
        "inline-size",
        "block-size",
        "width",
        "height",
        "flex-direction",
        "grid-template-columns",
    ];
    let conditions: BTreeSet<&str> = declared
        .at_rules
        .iter()
        .filter(|at| {
            let condition = at.condition_text.to_ascii_lowercase();
            condition.contains("container") && condition.contains("style(")
        })
        .filter(|at| {
            at.rules.iter().any(|rule| {
                rule.declarations
                    .keys()
                    .any(|property| structural.contains(&property.as_str()))
            })
        })
        .map(|at| at.condition_text.as_str())
        .collect();
    if conditions.is_empty() {
        return;
    }
    let selector_fallback = declared
        .rules
        .iter()
        .any(|rule| rule.selector.contains(":where(") && rule.selector.contains("[data-"));
    if selector_fallback {
        return;
    }
    let joined = conditions
        .iter()
        .copied()
        .take(3)
        .collect::<Vec<_>>()
        .join("; ");
    let evidence = Evidence::new(
        format!(
            "@container style() changes structure ({joined}); :where([data-*]) selector fallback absent"
        ),
        "declared.at_rules",
    );
    ctx.emit_global(out, spec, ":root", evidence);
}

fn check_sibling_functions(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = declared_of(ctx.doc) else {
        return;
    };
    let uses_index = all_rules(declared).any(|rule| {
        rule.declarations
            .values()
            .any(|value| value.contains("sibling-index(") || value.contains("sibling-count("))
    });
    if !uses_index {
        return;
    }
    let feature_detect = at_rule_has(declared, &["supports", "sibling-index"])
        || at_rule_has(declared, &["supports", "sibling-count"]);
    let custom_fallback = declared.rules.iter().any(|rule| {
        rule.declarations.contains_key("--sibling-index")
            || rule.declarations.contains_key("--sibling-count")
    });
    if feature_detect && custom_fallback {
        return;
    }
    if ctx.has_facet(Facet::Styles) {
        let colors: BTreeSet<&str> = ctx
            .flat
            .iter()
            .filter_map(|node| ctx.style(node, "visual", "background-color"))
            .collect();
        if colors.len() == 1 {
            tracing::debug!(
                check = spec.id,
                "computed colors identical; no real index usage"
            );
            return;
        }
    }
    let evidence = Evidence::new(
        "sibling-index()/sibling-count() used without @supports and --sibling-index/--sibling-count fallback",
        "declared+styles.visual",
    );
    ctx.emit_global(out, spec, ":root", evidence);
}

fn check_fluid_type(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = declared_of(ctx.doc) else {
        return;
    };
    let vw_fallback = declared.at_rules.iter().any(|at| {
        let condition = at.condition_text.to_ascii_lowercase();
        condition.contains("not")
            && at.rules.iter().any(|rule| {
                rule.declarations
                    .get("font-size")
                    .is_some_and(|value| value.contains("vw"))
            })
    });
    if vw_fallback {
        return;
    }
    for rule in all_rules(declared) {
        let Some(font_size) = rule.declarations.get("font-size") else {
            continue;
        };
        let Some((minimum, maximum)) = clamp_bounds(font_size) else {
            continue;
        };
        if minimum <= 0.0 {
            continue;
        }
        let ratio = maximum / minimum;
        if ratio <= 2.5 {
            continue;
        }
        let rounded = (ratio * 100.0).round() / 100.0;
        let evidence = Evidence::numeric(
            format!(
                "font-size={font_size}; min={minimum:.0}px; max={maximum:.0}px; ratio={rounded:.2} (>2.5)"
            ),
            "declared+styles.typography",
            "clamp_max_over_min",
            rounded,
            2.5,
            ">",
        );
        ctx.emit_global(out, spec, &rule.selector, evidence);
    }
}

fn check_color_scheme(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = declared_of(ctx.doc) else {
        return;
    };
    if env_value(ctx.doc, "prefers-color-scheme") != Some("dark") {
        return;
    }
    let meta = ctx
        .doc
        .document
        .as_ref()
        .and_then(|doc| doc.meta.get("color-scheme"))
        .map(String::as_str);
    if meta.is_some_and(|value| value.to_ascii_lowercase().contains("dark")) {
        return;
    }
    let root_scheme = root_node(ctx)
        .and_then(|node| ctx.style(node, "accessibility", "color-scheme"))
        .unwrap_or("");
    if root_scheme.to_ascii_lowercase().contains("dark") {
        return;
    }
    if at_rule_has(declared, &["prefers-color-scheme", "dark"]) {
        return;
    }
    let blocked = if declared.blocked_sheets.is_empty() {
        "none".to_string()
    } else {
        declared.blocked_sheets.join(", ")
    };
    let evidence = Evidence::new(
        format!(
            "prefers-color-scheme=dark; meta.color-scheme={}; root.color-scheme={root_scheme}; blocked_sheets={blocked}",
            meta.unwrap_or("absent")
        ),
        "__env+__document+declared+styles.accessibility",
    );
    ctx.emit_global(out, spec, ":root", evidence);
}

fn check_user_invalid(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = declared_of(ctx.doc) else {
        return;
    };
    let rule = declared
        .rules
        .iter()
        .find(|rule| rule.selector.contains(":has(") && rule.selector.contains(":user-invalid"));
    let Some(rule) = rule else {
        return;
    };
    let error_style = rule.declarations.keys().any(|property| {
        property.contains("border") || property == "outline-color" || property == "background-color"
    });
    if !error_style {
        return;
    }
    for node in &ctx.flat {
        if !matches!(ctx.tag(node), "INPUT" | "SELECT" | "TEXTAREA") {
            continue;
        }
        if ctx.aria(node, "invalid") == Some("true")
            || ctx.aria(node, "aria_invalid") == Some("true")
            || ctx.attr(node, "aria-invalid") == Some("true")
        {
            continue;
        }
        let evidence = Evidence::new(
            format!(
                "selector={}; error styling present; field {} has no aria-invalid=true",
                rule.selector, node.selector
            ),
            "declared+attrs+aria",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

fn check_scrollbar_pref(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = declared_of(ctx.doc) else {
        return;
    };
    if env_value(ctx.doc, "prefers-contrast") != Some("more") {
        return;
    }
    let override_exists = declared.at_rules.iter().any(|at| {
        at.condition_text
            .to_ascii_lowercase()
            .contains("prefers-contrast")
            && at
                .rules
                .iter()
                .any(|rule| rule.declarations.contains_key("scrollbar-color"))
    });
    if override_exists {
        return;
    }
    let mut emitted = BTreeSet::new();
    for node in &ctx.flat {
        let value = ctx
            .style_any(
                node,
                &[
                    ("interaction", "scrollbar-color"),
                    ("visual", "scrollbar-color"),
                ],
            )
            .unwrap_or("");
        let Some((thumb_text, track_text, thumb, track)) = parse_color_pair(value) else {
            continue;
        };
        let ratio = contrast_ratio(thumb, track);
        if ratio >= 7.0 {
            continue;
        }
        if emitted.insert(node.selector.clone()) {
            let evidence = Evidence::new(
                format!(
                    "prefers-contrast=more; scrollbar-color={thumb_text} {track_text}; no @media override (ratio {ratio:.2})"
                ),
                "__env+styles.interaction+declared.at_rules",
            );
            ctx.emit_global(out, spec, &node.selector, evidence);
        }
    }
}

fn check_forced_colors(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = declared_of(ctx.doc) else {
        return;
    };
    if env_value(ctx.doc, "forced-colors") != Some("active") {
        return;
    }
    let system_color = [
        "canvastext",
        "linktext",
        "highlight",
        "buttontext",
        "canvas",
    ];
    for node in &ctx.flat {
        if ctx.style(node, "accessibility", "forced-color-adjust") == Some("none") {
            continue;
        }
        let shadow = ctx.style(node, "visual", "box-shadow").unwrap_or("");
        let image = ctx.style(node, "visual", "background-image").unwrap_or("");
        let border_image = ctx
            .style(node, "visual", "border-image-source")
            .unwrap_or("");
        if matches!(shadow, "" | "none")
            && !image.contains("gradient")
            && matches!(border_image, "" | "none")
        {
            continue;
        }
        let outline = ctx.style(node, "visual", "outline-style").unwrap_or("");
        let outline_color = ctx
            .style(node, "visual", "outline-color")
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(outline, "" | "none")
            && system_color
                .iter()
                .any(|color| outline_color.contains(color))
        {
            continue;
        }
        if at_rule_has(declared, &["forced-colors", "active"]) {
            continue;
        }
        let emulated = ctx
            .doc
            .env
            .as_ref()
            .and_then(|env| env.emulated.get("forced-colors"))
            .map(String::as_str)
            .unwrap_or("not-requested");
        let evidence = Evidence::new(
            format!(
                "forced-colors=active (emulated={emulated}); box-shadow={shadow}; indicator depends on author color without system-color outline"
            ),
            "__env+styles.visual+declared.at_rules",
        );
        ctx.emit(out, spec, node, evidence);
    }
}

fn check_mask_fallback(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = declared_of(ctx.doc) else {
        return;
    };
    let guard = at_rule_has(declared, &["not", "mask-image"])
        || at_rule_has(declared, &["not", "-webkit-mask-image"]);
    if guard {
        return;
    }
    let mut emitted = BTreeSet::new();
    for node in &ctx.flat {
        let mask = ctx.style(node, "visual", "mask-image").unwrap_or("");
        if matches!(mask, "" | "none") {
            continue;
        }
        let affected = has_text(ctx, node)
            || descendants(node)
                .iter()
                .any(|child| is_focusable(ctx, child));
        if !affected {
            continue;
        }
        if emitted.insert(node.selector.clone()) {
            let evidence = Evidence::new(
                format!(
                    "mask-image={mask} hides text or interactive content; @supports not (mask-image...) fallback absent"
                ),
                "styles.visual+declared.at_rules",
            );
            ctx.emit_global(out, spec, &node.selector, evidence);
        }
    }
}

fn check_drawable_3d(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
    by_id: &BTreeMap<u64, &DiffNode>,
) {
    for node in &ctx.flat {
        if !has_attr(node, "drawable") || !is_focusable(ctx, node) {
            continue;
        }
        let transform = ctx.style(node, "transform", "transform").unwrap_or("");
        let style_3d = ctx
            .style(node, "transform", "transform-style")
            .unwrap_or("");
        let is_3d = transform.contains("matrix3d")
            || style_3d.to_ascii_lowercase().contains("preserve-3d")
            || ctx
                .style(node, "transform", "perspective")
                .is_some_and(|v| !matches!(v, "" | "none"));
        if !is_3d {
            continue;
        }
        let Some(canvas) = canvas_ancestor(ctx, node, by_id) else {
            continue;
        };
        if node.ax.as_ref().and_then(|ax| ax.get("ignored")) == Some(&Value::Bool(true)) {
            continue;
        }
        let evidence = Evidence::new(
            format!(
                "canvas {} contains a focusable 3D-transformed drawable; no canvas.geometry record → bounds/hit-test unverifiable",
                canvas.selector
            ),
            "attrs+ax+rect+styles.transform",
        );
        ctx.emit_global(out, spec, &node.selector, evidence);
    }
}

fn check_font_size_adjust(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = declared_of(ctx.doc) else {
        return;
    };
    let has_supports_fallback = at_rule_has(declared, &["not", "font-size-adjust"]);
    let mut face_families = BTreeSet::new();
    let mut optional_families = BTreeSet::new();
    let mut metric_overrides = BTreeSet::new();
    let mut collect_face = |rule: &DeclaredRule| {
        let Some(family) = rule.declarations.get("font-family") else {
            return;
        };
        let name = family.trim().trim_matches(['"', '\'']).to_string();
        if !rule.declarations.contains_key("src") {
            return;
        }
        face_families.insert(name.clone());
        if rule
            .declarations
            .get("font-display")
            .is_some_and(|v| v.trim() == "optional")
        {
            optional_families.insert(name.clone());
        }
        if ["size-adjust", "ascent-override", "descent-override"]
            .iter()
            .any(|prop| rule.declarations.contains_key(*prop))
        {
            metric_overrides.insert(name);
        }
    };
    for rule in declared.rules.iter().filter(|r| {
        r.selector.to_ascii_lowercase().contains("font-face") || r.declarations.contains_key("src")
    }) {
        collect_face(rule);
    }
    for at in &declared.at_rules {
        if at.condition_text.to_ascii_lowercase().contains("font-face") {
            for rule in &at.rules {
                collect_face(rule);
            }
        }
    }
    if has_supports_fallback {
        return;
    }
    for family in face_families {
        if optional_families.contains(&family) || metric_overrides.contains(&family) {
            continue;
        }
        let used = ctx.flat.iter().any(|node| {
            let font = ctx.style(node, "typography", "font-family").unwrap_or("");
            if !font.contains(&family) {
                return false;
            }
            let adjust = ctx
                .style(node, "typography", "font-size-adjust")
                .unwrap_or("");
            matches!(adjust.trim().to_ascii_lowercase().as_str(), "" | "none")
        });
        if used {
            let evidence = Evidence::new(
                format!(
                    "@font-face {family} used with font-size-adjust=none; no @supports not (font-size-adjust: from-font) or @font-face metric override"
                ),
                "declared+styles.typography",
            );
            ctx.emit_global(out, spec, &format!("@font-face {family}"), evidence);
        }
    }
}

// ---------------------------------------------------------------------------
// Small deterministic helpers
// ---------------------------------------------------------------------------

fn declared_of(doc: &DiffDocument) -> Option<&DeclaredFacet> {
    doc.declared.as_ref()
}

fn all_rules(declared: &DeclaredFacet) -> impl Iterator<Item = &DeclaredRule> {
    declared
        .rules
        .iter()
        .chain(declared.at_rules.iter().flat_map(|at| at.rules.iter()))
}

fn at_rule_has(declared: &DeclaredFacet, needles: &[&str]) -> bool {
    declared.at_rules.iter().any(|at| {
        let condition = at.condition_text.to_ascii_lowercase();
        needles.iter().all(|needle| condition.contains(*needle))
    })
}

fn env_value<'a>(doc: &'a DiffDocument, key: &str) -> Option<&'a str> {
    let env = doc.env.as_ref()?;
    env.media
        .get(key)
        .or_else(|| env.emulated.get(key))
        .map(String::as_str)
}

fn root_node<'a>(ctx: &ModernCtx<'a>) -> Option<&'a DiffNode> {
    ctx.flat
        .iter()
        .copied()
        .find(|node| node.tag.as_deref() == Some("HTML"))
        .or_else(|| ctx.flat.first().copied())
}

fn descendants(node: &DiffNode) -> Vec<&DiffNode> {
    let mut out = Vec::new();
    // Push reversed so the LIFO stack yields children in document order.
    let mut stack: Vec<&DiffNode> = node.children.iter().rev().collect();
    while let Some(current) = stack.pop() {
        out.push(current);
        stack.extend(current.children.iter().rev());
    }
    out
}

fn canvas_ancestor<'a>(
    ctx: &ModernCtx<'_>,
    node: &'a DiffNode,
    by_id: &BTreeMap<u64, &'a DiffNode>,
) -> Option<&'a DiffNode> {
    let _ = ctx;
    let mut current = node;
    let mut visited = BTreeSet::new();
    while visited.insert(current.id) {
        if current
            .attributes
            .as_ref()
            .and_then(|attrs| attrs.get("content"))
            .and_then(Value::as_str)
            == Some("drawable")
        {
            return Some(current);
        }
        let parent_id = current.parent_id?;
        current = *by_id.get(&parent_id)?;
    }
    None
}

fn has_attr(node: &DiffNode, name: &str) -> bool {
    node.attributes
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|attrs| attrs.contains_key(name))
}

fn is_focusable(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    ctx.aria_bool(node, "focusable")
        || node
            .ax
            .as_ref()
            .and_then(|ax| ax.get("focusable"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn has_text(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    ctx.aria_bool(node, "has_text")
        || (node.aria.is_none()
            && matches!(
                ctx.tag(node),
                "P" | "SPAN" | "LI" | "BLOCKQUOTE" | "H1" | "H2" | "H3" | "H4" | "H5" | "H6"
            ))
}

fn order_of(ctx: &ModernCtx<'_>, node: &DiffNode) -> i32 {
    ctx.style(node, "layout", "order")
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

fn rect_num(node: &DiffNode, key: &str) -> Option<f64> {
    node.rect.as_ref()?.get(key)?.as_f64()
}

fn line_count(ctx: &ModernCtx<'_>, node: &DiffNode) -> Option<f64> {
    let height = rect_num(node, "height")?;
    let line_height = ctx.style(node, "typography", "line-height")?;
    let px = px_value(line_height)?;
    (px > 0.0).then_some(height / px)
}

fn px_value(value: &str) -> Option<f64> {
    let text = value.trim().to_ascii_lowercase();
    let split = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(text.len());
    let number = text[..split].parse::<f64>().ok()?;
    match text[split..].trim() {
        "" | "px" => Some(number),
        "em" | "rem" => Some(number * 16.0),
        _ => None,
    }
}

fn first_number(value: &str) -> Option<f64> {
    px_value(value.split(',').next().unwrap_or(value))
}

fn identity_transform(value: &str) -> bool {
    let text = value.trim().to_ascii_lowercase();
    if text.is_empty() || text == "none" {
        return true;
    }
    if let Some(inner) = text
        .strip_prefix("matrix(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let values: Vec<f64> = inner
            .split(',')
            .filter_map(|v| v.trim().parse().ok())
            .collect();
        return values == [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    }
    let numbers: Vec<f64> = text
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .filter_map(|part| part.parse().ok())
        .collect();
    if numbers.is_empty() {
        return false;
    }
    if text.starts_with("scale") || text.starts_with("matrix3d") {
        return numbers.iter().all(|n| (n - 1.0).abs() < 1e-6);
    }
    numbers.iter().all(|n| n.abs() < 1e-6)
}

fn color_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for character in value.trim().chars() {
        match character {
            '(' => {
                depth += 1;
                current.push(character);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(character);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn parse_color_pair(value: &str) -> Option<(String, String, [f64; 3], [f64; 3])> {
    let tokens = color_tokens(value);
    let thumb_text = tokens.first()?.clone();
    let track_text = tokens.get(1)?.clone();
    let (thumb, thumb_alpha) = parse_color(&thumb_text)?;
    let (track, track_alpha) = parse_color(&track_text)?;
    if thumb_alpha < 0.999 || track_alpha < 0.999 {
        return None;
    }
    Some((thumb_text, track_text, thumb, track))
}

fn clamp_bounds(value: &str) -> Option<(f64, f64)> {
    let lower = value.to_ascii_lowercase();
    let rest = lower.strip_prefix("clamp(")?;
    let arguments = rest.strip_suffix(')')?;
    let parts: Vec<&str> = arguments.split(',').collect();
    if parts.len() < 3 {
        return None;
    }
    Some((px_value(parts[0].trim())?, px_value(parts[2].trim())?))
}

fn is_plain_size(value: &str) -> bool {
    let text = value.trim().to_ascii_lowercase();
    matches!(
        text.as_str(),
        "auto" | "min-content" | "max-content" | "fit-content" | "none"
    ) || px_value(&text).is_some()
}

fn is_nontrivial_literal(value: &str) -> bool {
    let text = value.trim().to_ascii_lowercase();
    if matches!(
        text.as_str(),
        "transparent"
            | "none"
            | "auto"
            | "0"
            | "0px"
            | "0rem"
            | "normal"
            | "currentcolor"
            | "inherit"
            | "initial"
            | "unset"
            | "revert"
    ) {
        return false;
    }
    if text.starts_with('#') || text.starts_with("rgb(") || text.starts_with("hsl(") {
        return true;
    }
    px_value(&text).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Map, json};
    use sniff_core::modern::{AtRuleInfo, DeclaredRule, DocumentFacet, EnvFacet, Severity};

    /// Helper de teste: encadeia `node(...).with_child(...)`.
    trait WithChild {
        /// Anexa um filho e devolve o nó.
        fn with_child(self, child: DiffNode) -> Self;
    }

    impl WithChild for DiffNode {
        fn with_child(mut self, mut child: DiffNode) -> Self {
            child.parent_id = Some(self.id);
            self.children.push(child);
            self
        }
    }

    fn node(id: u64, tag: &str, selector: &str) -> DiffNode {
        DiffNode {
            id,
            parent_id: None,
            selector: selector.to_string(),
            tag: Some(tag.to_string()),
            path: Some(selector.to_string()),
            depth: Some(0),
            rect: Some(json!({"x": 0, "y": 0, "width": 0, "height": 0})),
            metrics: None,
            noticeable: None,
            hash: None,
            styles: Some(Map::new()),
            pseudo: Some(Map::new()),
            aria: Some(Value::Object(Map::new())),
            contrast: None,
            ax: Some(Value::Object(Map::new())),
            attributes: Some(Value::Object(Map::new())),
            effective_background: None,
            children: Vec::new(),
        }
    }

    fn with_style(mut target: DiffNode, category: &str, properties: Value) -> DiffNode {
        target
            .styles
            .get_or_insert_with(Map::new)
            .insert(category.to_string(), properties);
        target
    }

    fn with_rect(mut target: DiffNode, x: f64, y: f64, width: f64, height: f64) -> DiffNode {
        target.rect = Some(json!({"x": x, "y": y, "width": width, "height": height}));
        target
    }

    fn with_aria(mut target: DiffNode, value: Value) -> DiffNode {
        target.aria = Some(value);
        target
    }

    fn with_attrs(mut target: DiffNode, value: Value) -> DiffNode {
        target.attributes = Some(value);
        target
    }

    fn with_pseudo(mut target: DiffNode, value: Value) -> DiffNode {
        target.pseudo = Some(value.as_object().cloned().unwrap_or_default());
        target
    }

    fn rule(selector: &str, origin: &str, declarations: Value) -> DeclaredRule {
        DeclaredRule {
            selector: selector.to_string(),
            href: None,
            index: 0,
            origin: origin.to_string(),
            declarations: declarations
                .as_object()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|(key, value)| (key, value.as_str().unwrap_or("").to_string()))
                .collect(),
        }
    }

    fn declared(rules: Vec<DeclaredRule>, at_rules: Vec<AtRuleInfo>) -> DeclaredFacet {
        DeclaredFacet {
            rules,
            blocked_sheets: Vec::new(),
            at_rules,
            truncated: false,
        }
    }

    fn env(pairs: &[(&str, &str)]) -> EnvFacet {
        let mut facet = EnvFacet::default();
        for (key, value) in pairs {
            facet.media.insert((*key).to_string(), (*value).to_string());
        }
        facet
    }

    fn document(meta: Value, lang: Option<&str>) -> DocumentFacet {
        let mut facet = DocumentFacet::default();
        facet.lang = lang.map(String::from);
        facet.meta = meta
            .as_object()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|(key, value)| (key, value.as_str().unwrap_or("").to_string()))
            .collect();
        facet
    }

    fn doc(nodes: Vec<DiffNode>) -> DiffDocument {
        let mut result = DiffDocument::default();
        result.nodes = nodes;
        result
    }

    fn lines(document: &DiffDocument) -> Vec<CheckLine> {
        let options = crate::modern::ModernOptions::default();
        let ctx = ModernCtx::new(document, &options);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        out
    }

    fn has(lines: &[CheckLine], id: &str) -> bool {
        lines.iter().any(|line| line.check == id)
    }

    #[test]
    fn registry_covers_all_family_i_ids_once() {
        let unique: BTreeSet<&str> = specs().iter().map(|spec| spec.id).collect();
        // `dark-mode-ignored` lives in the responsive family (matrix-aware).
        assert_eq!(specs().len(), 29);
        assert_eq!(unique.len(), specs().len());
        assert!(
            specs()
                .iter()
                .all(|spec| !spec.guides.is_empty() && !spec.keep_when.is_empty())
        );
        let scrollbar = specs()
            .iter()
            .find(|spec| spec.id == "scrollbar-thumb-track-contrast")
            .unwrap();
        assert_eq!(scrollbar.severity, Severity::Fail);
    }

    #[test]
    fn visual_order_positive_negative_keepwhen() {
        let positive = doc(vec![
            with_style(
                node(1, "DIV", "div.container"),
                "layout",
                json!({"display": "flex"}),
            )
            .with_child(with_rect(
                with_aria(
                    node(2, "BUTTON", "button:nth-child(1)"),
                    json!({"focusable": true}),
                ),
                120.0,
                10.0,
                60.0,
                24.0,
            ))
            .with_child(with_rect(
                with_style(
                    with_aria(
                        node(3, "BUTTON", "button:nth-child(2)"),
                        json!({"focusable": true}),
                    ),
                    "layout",
                    json!({"order": "-1"}),
                ),
                10.0,
                10.0,
                60.0,
                24.0,
            )),
        ]);
        assert!(has(&lines(&positive), "visual-order-breaks-tab-order"));

        let mut negative = positive.clone();
        negative.nodes[0].children[1].styles = None;
        assert!(!has(&lines(&negative), "visual-order-breaks-tab-order"));

        let mut keep = doc(vec![
            with_style(
                node(1, "DIV", "div.container"),
                "layout",
                json!({"display": "flex"}),
            )
            .with_child(with_rect(
                node(2, "SPAN", "span:nth-child(1)"),
                120.0,
                10.0,
                60.0,
                24.0,
            ))
            .with_child(with_rect(
                with_style(
                    node(3, "SPAN", "span:nth-child(2)"),
                    "layout",
                    json!({"order": "-1"}),
                ),
                10.0,
                10.0,
                60.0,
                24.0,
            )),
        ]);
        keep.nodes[0].aria = Some(json!({"focusable": false}));
        assert!(!has(&lines(&keep), "visual-order-breaks-tab-order"));
    }

    #[test]
    fn media_aspect_ratio_positive_negative_keepwhen() {
        let img = || with_attrs(node(1, "IMG", "img.hero"), json!({"alt": "hero"}));
        let positive = doc(vec![img()]);
        assert!(has(&lines(&positive), "media-aspect-ratio-missing"));

        let negative = doc(vec![with_attrs(
            img(),
            json!({"width": "800", "height": "450"}),
        )]);
        assert!(!has(&lines(&negative), "media-aspect-ratio-missing"));

        let keep = doc(vec![with_style(
            img(),
            "box_model",
            json!({"aspect-ratio": "16/9"}),
        )]);
        assert!(!has(&lines(&keep), "media-aspect-ratio-missing"));
    }

    #[test]
    fn blend_mode_positive_negative_keepwhen() {
        let overlay = || {
            with_style(
                node(1, "DIV", "div.overlay"),
                "visual",
                json!({"mix-blend-mode": "soft-light", "isolation": "auto"}),
            )
        };
        let positive = doc(vec![node(0, "BODY", "body").with_child(overlay())]);
        assert!(has(&lines(&positive), "blend-mode-without-isolation"));

        let negative = doc(vec![with_style(
            overlay(),
            "visual",
            json!({"mix-blend-mode": "soft-light", "isolation": "isolate"}),
        )]);
        assert!(!has(&lines(&negative), "blend-mode-without-isolation"));

        let keep = doc(vec![with_style(
            overlay(),
            "visual",
            json!({"mix-blend-mode": "normal", "isolation": "auto"}),
        )]);
        assert!(!has(&lines(&keep), "blend-mode-without-isolation"));
    }

    #[test]
    fn overflow_clip_margin_positive_negative_keepwhen() {
        let styled = |overflow: &str| {
            with_style(
                node(1, "DIV", "div.clip"),
                "box_model",
                json!({"overflow-x": overflow, "overflow-clip-margin": "15px"}),
            )
        };
        assert!(has(
            &lines(&doc(vec![styled("hidden")])),
            "overflow-clip-margin-ignored"
        ));
        assert!(!has(
            &lines(&doc(vec![styled("clip")])),
            "overflow-clip-margin-ignored"
        ));

        let replaced = with_style(
            node(1, "IMG", "img.thumb"),
            "box_model",
            json!({"overflow-x": "hidden", "overflow-clip-margin": "15px"}),
        );
        assert!(!has(
            &lines(&doc(vec![replaced])),
            "overflow-clip-margin-ignored"
        ));
    }

    #[test]
    fn text_wrap_balance_positive_negative_keepwhen() {
        let paragraph = |mode: &str, height: f64| {
            with_rect(
                with_style(
                    node(1, "P", "p.long"),
                    "typography",
                    json!({"text-wrap-mode": mode, "line-height": "24px"}),
                ),
                0.0,
                0.0,
                400.0,
                height,
            )
        };
        assert!(has(
            &lines(&doc(vec![paragraph("balance", 264.0)])),
            "text-wrap-balance-on-long-block"
        ));
        assert!(!has(
            &lines(&doc(vec![paragraph("pretty", 264.0)])),
            "text-wrap-balance-on-long-block"
        ));
        assert!(!has(
            &lines(&doc(vec![paragraph("balance", 72.0)])),
            "text-wrap-balance-on-long-block"
        ));
    }

    #[test]
    fn scrollbar_contrast_fails_below_three_and_keeps_fallbacks() {
        let scroller = |color: &str| {
            let mut document = doc(vec![with_style(
                node(1, "DIV", "div.scroller"),
                "interaction",
                json!({"scrollbar-color": color}),
            )]);
            document.env = Some(env(&[]));
            document
        };
        let positive = scroller("#a0a0a0 #b8b8b8");
        let found = lines(&positive)
            .into_iter()
            .find(|line| line.check == "scrollbar-thumb-track-contrast")
            .unwrap();
        assert_eq!(
            found.evidence.metric.as_deref(),
            Some("scrollbar_thumb_track_ratio")
        );
        assert_eq!(found.evidence.threshold, Some(json!(3.0)));
        assert_eq!(found.evidence.comparator.as_deref(), Some("<"));
        assert!(!has(
            &lines(&scroller("#000000 #ffffff")),
            "scrollbar-thumb-track-contrast"
        ));
        assert!(!has(
            &lines(&scroller("auto")),
            "scrollbar-thumb-track-contrast"
        ));
    }

    #[test]
    fn hover_transform_positive_negative_keepwhen() {
        let hover_pseudo = json!({
            ":hover": {"transform": {"scale": "1.05", "transform": "none"}}
        });
        let base = || node(1, "DIV", "div.card");
        let positive = doc(vec![with_pseudo(base(), hover_pseudo.clone())]);
        assert!(has(
            &lines(&positive),
            "transform-stacking-context-shift-on-hover"
        ));

        let negative = doc(vec![with_style(
            with_pseudo(base(), hover_pseudo.clone()),
            "transform",
            json!({"scale": "1"}),
        )]);
        assert!(!has(
            &lines(&negative),
            "transform-stacking-context-shift-on-hover"
        ));

        let keep = doc(vec![base()]);
        assert!(!has(
            &lines(&keep),
            "transform-stacking-context-shift-on-hover"
        ));
    }

    #[test]
    fn mask_on_text_positive_negative_keepwhen() {
        let masked = || {
            with_style(
                with_aria(node(1, "DIV", "div.card"), json!({"has_text": true})),
                "visual",
                json!({"mask-image": "url(#splat)"}),
            )
        };
        assert!(has(
            &lines(&doc(vec![masked()])),
            "mask-on-text-containing-element"
        ));

        let gradient = with_style(
            with_aria(node(1, "DIV", "div.card"), json!({"has_text": true})),
            "visual",
            json!({"mask-image": "linear-gradient(black, transparent)"}),
        );
        assert!(!has(
            &lines(&doc(vec![gradient])),
            "mask-on-text-containing-element"
        ));

        let sibling = with_aria(node(2, "DIV", "div.card-body"), json!({"has_text": true}));
        let parent = node(0, "DIV", "div.card-wrap")
            .with_child(masked())
            .with_child(sibling);
        assert!(!has(
            &lines(&doc(vec![parent])),
            "mask-on-text-containing-element"
        ));
    }

    #[test]
    fn justify_without_hyphens_positive_negative_keepwhen() {
        let paragraph = |hyphens: &str, height: f64| {
            let mut snap = doc(vec![with_rect(
                with_style(
                    node(1, "P", "p.body"),
                    "typography",
                    json!({"text-align": "justify", "hyphens": hyphens, "line-height": "24px"}),
                ),
                0.0,
                0.0,
                400.0,
                height,
            )]);
            snap.document = Some(document(serde_json::json!({}), Some("en")));
            snap
        };
        assert!(has(
            &lines(&paragraph("manual", 120.0)),
            "justify-without-hyphens"
        ));
        assert!(!has(
            &lines(&paragraph("auto", 120.0)),
            "justify-without-hyphens"
        ));
        assert!(!has(
            &lines(&paragraph("manual", 24.0)),
            "justify-without-hyphens"
        ));
    }

    #[test]
    fn calc_size_positive_negative_keepwhen() {
        let rule_with = |value: &str| {
            rule(
                ".panel",
                "author",
                json!({"inline-size": value, "color": "red"}),
            )
        };
        let make_doc = |rules: Vec<DeclaredRule>, computed: &str| {
            let mut document = doc(vec![with_style(
                node(1, "DIV", ".panel"),
                "box_model",
                json!({"inline-size": computed}),
            )]);
            document.declared = Some(declared(rules, Vec::new()));
            document
        };
        let value = "calc-size(min-content, size + 2rem)";
        assert!(has(
            &lines(&make_doc(vec![rule_with(value)], value)),
            "calc-size-missing-fallback"
        ));
        assert!(!has(
            &lines(&make_doc(
                vec![rule_with("min-content"), rule_with(value)],
                value
            )),
            "calc-size-missing-fallback"
        ));
        assert!(!has(
            &lines(&make_doc(vec![rule_with(value)], "")),
            "calc-size-missing-fallback"
        ));
    }

    #[test]
    fn inline_style_positive_negative_keepwhen() {
        let make_doc = |origin: &str, values: Value| {
            let mut document = doc(vec![node(1, "DIV", "div.card")]);
            document.declared = Some(declared(vec![rule("div.card", origin, values)], Vec::new()));
            document
        };
        assert!(has(
            &lines(&make_doc(
                "inline",
                json!({"background-color": "#f06", "padding": "0.3em"})
            )),
            "inline-nontrivial-style"
        ));
        assert!(!has(
            &lines(&make_doc(
                "inline",
                json!({"background-color": "transparent", "padding": "0"})
            )),
            "inline-nontrivial-style"
        ));
        assert!(!has(
            &lines(&make_doc("author", json!({"background-color": "#f06"}))),
            "inline-nontrivial-style"
        ));
    }

    #[test]
    fn has_critical_state_positive_negative_keepwhen() {
        let rule_has = || {
            rule(
                ".theme-card:has(.toggle:checked)",
                "author",
                json!({"flex-direction": "row"}),
            )
        };
        let make_doc = |rules: Vec<DeclaredRule>, at_rules: Vec<AtRuleInfo>| {
            let mut document = doc(vec![node(1, "DIV", ".theme-card")]);
            document.declared = Some(declared(rules, at_rules));
            document
        };
        assert!(has(
            &lines(&make_doc(vec![rule_has()], Vec::new())),
            "has-critical-state-without-fallback"
        ));
        let decorative = rule(
            ".theme-card:has(.toggle:checked)",
            "author",
            json!({"color": "red"}),
        );
        assert!(!has(
            &lines(&make_doc(vec![decorative], Vec::new())),
            "has-critical-state-without-fallback"
        ));
        let fallback = AtRuleInfo {
            condition_text: "@supports not selector(:has(*))".to_string(),
            index: 0,
            rules: Vec::new(),
        };
        assert!(!has(
            &lines(&make_doc(vec![rule_has()], vec![fallback])),
            "has-critical-state-without-fallback"
        ));
    }

    #[test]
    fn style_query_positive_negative_keepwhen() {
        let at = AtRuleInfo {
            condition_text: "@container style(--density: compact)".to_string(),
            index: 0,
            rules: vec![rule(".card", "author", json!({"padding": "8px"}))],
        };
        let make_doc = |at_rules: Vec<AtRuleInfo>, rules: Vec<DeclaredRule>| {
            let mut document = doc(vec![node(1, "DIV", ".card")]);
            document.declared = Some(declared(rules, at_rules));
            document
        };
        assert!(has(
            &lines(&make_doc(vec![at.clone()], Vec::new())),
            "style-query-without-selector-fallback"
        ));
        let cosmetic = AtRuleInfo {
            rules: vec![rule(".card", "author", json!({"color": "red"}))],
            ..at.clone()
        };
        assert!(!has(
            &lines(&make_doc(vec![cosmetic], Vec::new())),
            "style-query-without-selector-fallback"
        ));
        let fallback = rule(
            ":where([data-density=compact]) .card",
            "author",
            json!({"padding": "8px"}),
        );
        assert!(!has(
            &lines(&make_doc(vec![at], vec![fallback])),
            "style-query-without-selector-fallback"
        ));
    }

    #[test]
    fn fluid_clamp_ratio_positive_negative_keepwhen() {
        let make_doc = |value: &str| {
            let mut document = doc(vec![node(1, "H1", "h1.title")]);
            document.declared = Some(declared(
                vec![rule("h1.title", "author", json!({"font-size": value}))],
                Vec::new(),
            ));
            document
        };
        let found = lines(&make_doc("clamp(1rem, 5cqi, 3rem)"))
            .into_iter()
            .find(|line| line.check == "fluid-type-clamp-exceeds-2_5x")
            .unwrap();
        assert_eq!(found.evidence.metric.as_deref(), Some("clamp_max_over_min"));
        assert_eq!(found.evidence.threshold, Some(json!(2.5)));
        assert!(!has(
            &lines(&make_doc("clamp(1rem, 5cqi, 2.5rem)")),
            "fluid-type-clamp-exceeds-2_5x"
        ));
        assert!(!has(
            &lines(&make_doc("2rem")),
            "fluid-type-clamp-exceeds-2_5x"
        ));
    }

    #[test]
    fn dark_preference_checks_positive_negative_keepwhen() {
        let root = with_style(
            node(1, "HTML", "html"),
            "accessibility",
            json!({"color-scheme": "light"}),
        );
        let make_doc = |scheme: Option<&str>, at_rules: Vec<AtRuleInfo>| {
            let mut meta = json!({});
            if let Some(scheme) = scheme {
                meta["color-scheme"] = json!(scheme);
            }
            let mut snap = doc(vec![root.clone()]);
            snap.document = Some(document(meta, Some("en")));
            snap.env = Some(env(&[("prefers-color-scheme", "dark")]));
            snap.declared = Some(declared(Vec::new(), at_rules));
            snap
        };
        let positive = make_doc(None, Vec::new());
        let emitted = lines(&positive);
        assert!(has(&emitted, "color-scheme-mismatch"));

        let negative = make_doc(Some("dark"), Vec::new());
        let emitted = lines(&negative);
        assert!(!has(&emitted, "color-scheme-mismatch"));

        let fallback = AtRuleInfo {
            condition_text: "@media (prefers-color-scheme: dark)".to_string(),
            index: 0,
            rules: Vec::new(),
        };
        let keep = make_doc(None, vec![fallback]);
        let emitted = lines(&keep);
        assert!(!has(&emitted, "color-scheme-mismatch"));
    }

    #[test]
    fn forced_colors_positive_negative_keepwhen() {
        let make_doc = |shadow: &str, at_rules: Vec<AtRuleInfo>| {
            let mut document = doc(vec![with_style(
                node(1, "DIV", "div.badge"),
                "visual",
                json!({"box-shadow": shadow, "outline-style": "none"}),
            )]);
            let mut environment = env(&[("forced-colors", "active")]);
            environment
                .emulated
                .insert("forced-colors".to_string(), "active".to_string());
            document.env = Some(environment);
            document.declared = Some(declared(Vec::new(), at_rules));
            document
        };
        assert!(has(
            &lines(&make_doc("0 0 0 2px #2563eb", Vec::new())),
            "forced-colors-unhandled"
        ));
        assert!(!has(
            &lines(&make_doc("none", Vec::new())),
            "forced-colors-unhandled"
        ));
        let fallback = AtRuleInfo {
            condition_text: "@media (forced-colors: active)".to_string(),
            index: 0,
            rules: Vec::new(),
        };
        assert!(!has(
            &lines(&make_doc("0 0 0 2px #2563eb", vec![fallback])),
            "forced-colors-unhandled"
        ));
    }

    #[test]
    fn font_size_adjust_positive_negative_keepwhen() {
        let face = |extras: Value| {
            let mut values = json!({"font-family": "MyWebFont", "src": "url(x.woff2)"});
            if let Some(map) = extras.as_object() {
                for (key, value) in map {
                    values[key.clone()] = value.clone();
                }
            }
            rule("@font-face", "author", values)
        };
        let make_doc = |face_rule: DeclaredRule, adjust: &str, at_rules: Vec<AtRuleInfo>| {
            let mut document = doc(vec![with_style(
                node(1, "P", "p.text"),
                "typography",
                json!({"font-family": "MyWebFont, serif", "font-size-adjust": adjust}),
            )]);
            document.declared = Some(declared(vec![face_rule], at_rules));
            document
        };
        assert!(has(
            &lines(&make_doc(face(json!({})), "none", Vec::new())),
            "font-size-adjust-missing-fallback"
        ));
        assert!(!has(
            &lines(&make_doc(face(json!({})), "from-font", Vec::new())),
            "font-size-adjust-missing-fallback"
        ));
        assert!(!has(
            &lines(&make_doc(
                face(json!({"font-display": "optional"})),
                "none",
                Vec::new()
            )),
            "font-size-adjust-missing-fallback"
        ));
        let fallback = AtRuleInfo {
            condition_text: "@supports not (font-size-adjust: from-font)".to_string(),
            index: 0,
            rules: Vec::new(),
        };
        assert!(!has(
            &lines(&make_doc(face(json!({})), "none", vec![fallback])),
            "font-size-adjust-missing-fallback"
        ));
    }

    #[test]
    fn emitted_lines_are_byte_for_byte_deterministic() {
        let mut document = doc(vec![with_style(
            node(1, "DIV", "div.scroller"),
            "interaction",
            json!({"scrollbar-color": "#707070 #f1f1f1"}),
        )]);
        document.env = Some(env(&[]));
        let first = serde_json::to_string(&lines(&document)).unwrap();
        let second = serde_json::to_string(&lines(&document)).unwrap();
        assert_eq!(first, second);
        assert!(!first.is_empty());
    }

    #[test]
    fn helpers_handle_malformed_values() {
        assert_eq!(px_value("not-a-length"), None);
        assert_eq!(px_value("12cqi"), None);
        assert_eq!(px_value("1.5rem"), Some(24.0));
        assert_eq!(clamp_bounds("clamp(1rem, 5cqi)"), None);
        assert_eq!(clamp_bounds("clamp(1rem, 5cqi, 3rem)"), Some((16.0, 48.0)));
        assert!(identity_transform("none"));
        assert!(identity_transform("matrix(1, 0, 0, 1, 0, 0)"));
        assert!(!identity_transform("matrix(1.05, 0, 0, 1, 0, 0)"));
        assert_eq!(color_tokens("rgb(1, 2, 3) rgb(4, 5, 6)").len(), 2);
        assert_eq!(parse_color_pair("auto"), None);
        assert!(!is_nontrivial_literal("transparent"));
        assert!(is_nontrivial_literal("#f06"));
    }
}
