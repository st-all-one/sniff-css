//! Family F — performance proxies (F4).
//!
//! Everything in this module is an **offline lab proxy**: `sniff` does not
//! measure Core Web Vitals, so every `evidence.text` starts with `proxy:` and
//! no line claims a measured LCP/INP/CLS value. All thresholds are constants
//! (never clock, environment or randomness) so the same snapshot always
//! produces the same lines.
//!
//! Aliases consolidated per catalog §4.1:
//! - `always-on-will-change` (roadmap F4) is the same signal as
//!   `excessive-will-change` (catalog §4.8): one id, documented here.
//! - `resolution-optimized-pseudo` (roadmap F4) is the pseudo-element branch
//!   of `image-set-without-legacy-fallback` (catalog §4.8): one id.
//! - `script-blocking` (roadmap F4) is `render-blocking-script-misuse`.
//! - `transition-all` (roadmap F4) is an existing check refined in family D;
//!   `animation-on-layout-props` covers the complementary layout-property
//!   signal with the same `performance` guide and is not a duplicate.

use crate::modern::{CheckSpec, Facet, ModernCtx};
use crate::rules::CheckLine;
use serde_json::Value;
use sniff_core::modern::{Evidence, ScriptInfo};
use sniff_css_diff::DiffNode;
use std::collections::BTreeSet;

/// Minimum visible area for an above/below-the-fold proxy (CSS px²).
const MIN_PROXY_AREA: f64 = 40_000.0;
/// A container with at least this many captured descendants is "heavy".
const CV_CANDIDATE_NODES: usize = 24;
/// Minimum area of a candidate for `content-visibility: auto`.
const CV_CANDIDATE_AREA: f64 = 60_000.0;
/// `will-change` proxies only become interesting beyond this many nodes.
const WILL_CHANGE_LIMIT: usize = 3;
/// Aggregate DOM budget for `heavy-dom-region`.
const HEAVY_DOM_NODES: usize = 300;
/// Aggregate depth budget for `heavy-dom-region`.
const HEAVY_DOM_DEPTH: usize = 12;
/// A live region wrapping more than this many nodes is too broad.
const LIVE_REGION_NODES: usize = 5;
/// Preload budgets from `optimize-preload-priority`.
const PRELOAD_IMAGE_BUDGET: usize = 2;
const PRELOAD_FONT_BUDGET: usize = 3;
/// More than two `fetchpriority=high` scripts is zero-sum.
const HIGH_FETCHPRIORITY_BUDGET: usize = 2;

/// Properties whose animation/transition triggers layout, not just paint.
const LAYOUT_ANIMATION_PROPS: &[&str] = &[
    "width",
    "height",
    "top",
    "right",
    "bottom",
    "left",
    "inset",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "font-size",
    "flex-basis",
    "gap",
    "row-gap",
    "column-gap",
    "grid-template-columns",
    "grid-template-rows",
    "min-width",
    "max-width",
    "min-height",
    "max-height",
];

/// Hints for navigations that change state (must not be speculated).
const STATE_CHANGE_HINTS: &[&str] = &[
    "/logout",
    "/signout",
    "/sign-out",
    "add-to-cart",
    "/checkout",
    "/delete",
    "remove-item",
];

static SPECS: &[CheckSpec] = &[
    CheckSpec::new(
        "content-visibility-without-intrinsic-size",
        &[
            "defer-rendering-heavy-content",
            "efficient-background-processing",
            "interactions-in-complex-layouts",
            "performance",
        ],
        &[Facet::Styles],
        "skip when content-visibility is not auto, an intrinsic-size length is set, the node is display:none, or @supports provides contain:layout style paint",
    ),
    CheckSpec::new(
        "content-visibility-above-the-fold",
        &["defer-rendering-heavy-content", "performance"],
        &[Facet::Styles, Facet::Rect],
        "skip without viewport metadata, when the node is outside the initial viewport, or when content-visibility is hidden/visible",
    ),
    CheckSpec::new(
        "cv-hidden-view-not-inert",
        &["faster-spa-view-transitions"],
        &[Facet::Styles, Facet::Attrs],
        "skip for the active view (visible), display:none, aria-hidden or inert nodes, and when every focusable descendant is disabled",
    ),
    CheckSpec::new(
        "image-without-dimensions",
        &["performance"],
        &[Facet::Attrs],
        "skip when both width and height are declared, when CSS resolves both dimensions or an aspect-ratio, or for inline SVG",
    ),
    CheckSpec::new(
        "srcset-without-sizes",
        &["performance"],
        &[Facet::Attrs],
        "skip when sizes is present, when srcset uses only density descriptors (1x/2x), or on elements outside img/source",
    ),
    CheckSpec::new(
        "lazy-media-above-the-fold",
        &[
            "lazy-load-video-and-audio",
            "optimize-image-priority",
            "performance",
        ],
        &[Facet::Attrs, Facet::Rect],
        "skip without viewport metadata, below the fold, for small decorative media, or when loading is not lazy",
    ),
    CheckSpec::new(
        "lazy-media-without-dimensions-or-controls",
        &["lazy-load-video-and-audio", "performance"],
        &[Facet::Attrs],
        "skip for eager media, when CSS sets both dimensions, when a data-src/data-poster IntersectionObserver fallback exists, or when audio already has controls",
    ),
    CheckSpec::new(
        "fetchpriority-high-with-lazy",
        &["optimize-image-priority", "performance"],
        &[Facet::Attrs],
        "skip when loading is absent/eager (correct LCP setup), when attrs are not captured, or on non-image elements",
    ),
    // Alias: `always-on-will-change` (F4) — same signal, no second id.
    CheckSpec::new(
        "excessive-will-change",
        &["performance"],
        &[Facet::Styles],
        "skip for one or two nodes (typically a transient hover animation), when will-change is auto/none, or when the property was not captured",
    ),
    // Alias: `resolution-optimized-pseudo` (F4) — pseudo branch of this check.
    CheckSpec::new(
        "image-set-without-legacy-fallback",
        &[
            "deliver-optimized-decorative-images",
            "resolution-optimized-pseudo-elements",
        ],
        &[Facet::Declared],
        "skip when image-set() is absent, a url() declaration exists in the same rule, the property is mask-only, or the CSSOM facet is missing",
    ),
    // Alias: `script-blocking` (F4) — same head-script misuse.
    CheckSpec::new(
        "render-blocking-script-misuse",
        &["flicker-free-client-side-ab-testing"],
        &[Facet::Document],
        "skip without __document, for scripts that are async/defer/module, for analytics without blocking=render, and for the anti-flicker fallback in browsers without blocking support",
    ),
    CheckSpec::new(
        "preload-priority-misuse",
        &["optimize-preload-priority", "performance"],
        &[Facet::Document],
        "skip without __document, for as=image with fetchpriority=high (correct LCP), when crossorigin is explicitly present, and within the preload budgets",
    ),
    CheckSpec::new(
        "script-priority-misuse",
        &["optimize-script-priority", "performance"],
        &[Facet::Document],
        "skip without __document, for one or two async fetchpriority=high scripts, when fetchpriority is absent (correct default), and for self-hosted scripts",
    ),
    CheckSpec::new(
        "speculation-rules-misuse",
        &["improve-next-page-load-performance"],
        &[Facet::Document],
        "skip without speculation rules, without state-changing navigation targets, with an explicit no-speculate exclusion marker, or in conservative prefetch rules",
    ),
    CheckSpec::new(
        "streaming-live-region-scope",
        &["out-of-order-html-streaming"],
        &[Facet::Attrs],
        "skip for leaf text live regions, aria-hidden/display:none regions, nested aria-busy regions already covered by the parent, and rewritten polyfill comments",
    ),
    CheckSpec::new(
        "animation-on-layout-props",
        &["performance"],
        &[Facet::Styles],
        "skip when the transition duration is zero, when only paint/compositor properties animate, or when a layout keyframe is never applied to the node",
    ),
    CheckSpec::new(
        "content-visibility-candidate",
        &[
            "defer-rendering-heavy-content",
            "interactions-in-complex-layouts",
            "performance",
        ],
        &[Facet::Styles, Facet::Rect],
        "skip when content-visibility is already set, for hidden/small subtrees, and below the area and subtree thresholds",
    ),
    CheckSpec::new(
        "heavy-dom-region",
        &["interactions-in-complex-layouts", "performance"],
        &[],
        "skip below 300 nodes and 12 levels of depth; derived offline from __dom_stats without a capture facet",
    ),
    CheckSpec::new(
        "image-without-lazy",
        &["performance"],
        &[Facet::Attrs, Facet::Rect],
        "skip without viewport metadata, above the fold, for the single fetchpriority=high hero image, for small images, and when a data-src lazy fallback exists",
    ),
    CheckSpec::new(
        "fetchpriority-missing",
        &["optimize-image-priority", "performance"],
        &[Facet::Attrs, Facet::Rect],
        "skip when another image already has fetchpriority=high, for lazy/small images, outside the initial viewport, or without viewport metadata",
    ),
];

/// All ids registered by this family (single source of truth for family F).
#[must_use]
pub fn specs() -> &'static [CheckSpec] {
    SPECS
}

/// Runs the enabled family-F checks and appends deterministic lines.
pub fn run(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    tracing::debug!(
        checks = specs().len(),
        lines_before = out.len(),
        "performance family start"
    );
    for spec in specs() {
        if !ctx.enabled(spec) {
            tracing::trace!(
                id = spec.id,
                "performance check skipped (profile, ignore or facet)"
            );
            continue;
        }
        match spec.id {
            "content-visibility-without-intrinsic-size" => check_cv_intrinsic(ctx, out, spec),
            "content-visibility-above-the-fold" => check_cv_above_fold(ctx, out, spec),
            "cv-hidden-view-not-inert" => check_cv_hidden_inert(ctx, out, spec),
            "image-without-dimensions" => check_image_dimensions(ctx, out, spec),
            "srcset-without-sizes" => check_srcset_sizes(ctx, out, spec),
            "lazy-media-above-the-fold" => check_lazy_above_fold(ctx, out, spec),
            "lazy-media-without-dimensions-or-controls" => check_lazy_media_shape(ctx, out, spec),
            "fetchpriority-high-with-lazy" => check_fetch_conflict(ctx, out, spec),
            "excessive-will-change" => check_will_change(ctx, out, spec),
            "image-set-without-legacy-fallback" => check_image_set(ctx, out, spec),
            "render-blocking-script-misuse" => check_render_blocking(ctx, out, spec),
            "preload-priority-misuse" => check_preload(ctx, out, spec),
            "script-priority-misuse" => check_script_priority(ctx, out, spec),
            "speculation-rules-misuse" => check_speculation(ctx, out, spec),
            "streaming-live-region-scope" => check_live_region(ctx, out, spec),
            "animation-on-layout-props" => check_animation_layout(ctx, out, spec),
            "content-visibility-candidate" => check_cv_candidate(ctx, out, spec),
            "heavy-dom-region" => check_heavy_dom(ctx, out, spec),
            "image-without-lazy" => check_image_without_lazy(ctx, out, spec),
            "fetchpriority-missing" => check_fetchpriority_missing(ctx, out, spec),
            _ => tracing::warn!(id = spec.id, "unhandled performance check id"),
        }
    }
    tracing::debug!(lines_after = out.len(), "performance family done");
}

// --- emission helpers (the only place that logs a proxy line) -------------

fn emit_line(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
    node: &DiffNode,
    evidence: Evidence,
) {
    tracing::debug!(
        check = spec.id,
        selector = %node.selector,
        source = %evidence.source,
        "performance proxy emitted"
    );
    ctx.emit(out, spec, node, evidence);
}

fn emit_global(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
    selector: String,
    evidence: Evidence,
) {
    tracing::debug!(
        check = spec.id,
        selector = %selector,
        source = %evidence.source,
        "global performance proxy emitted"
    );
    ctx.emit_global(out, spec, &selector, evidence);
}

/// Prefixes every message so no line can ever be mistaken for field CWV.
fn proxy_text(text: impl AsRef<str>) -> String {
    format!("proxy: {}", text.as_ref())
}

fn proxy_ev(text: impl AsRef<str>, source: &str) -> Evidence {
    Evidence::new(proxy_text(text), source)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Evidence::numeric plus the proxy text needs seven inputs; kept in one helper"
)]
fn proxy_num(
    text: impl AsRef<str>,
    source: &str,
    metric: &str,
    observed: Value,
    threshold: Value,
    comparator: &str,
) -> Evidence {
    Evidence::numeric(
        proxy_text(text),
        source,
        metric,
        observed,
        threshold,
        comparator,
    )
}

// --- shared read-only helpers ---------------------------------------------

fn rect_coords(node: &DiffNode) -> Option<(f64, f64, f64, f64)> {
    let rect = node.rect.as_ref()?;
    let x = rect.get("x")?.as_f64()?;
    let y = rect.get("y")?.as_f64()?;
    let width = rect.get("width")?.as_f64()?;
    let height = rect.get("height")?.as_f64()?;
    (width > 0.0 && height > 0.0).then_some((x, y, width, height))
}

fn viewport_height(ctx: &ModernCtx<'_>) -> Option<f64> {
    if let Some((_, height)) = ctx.doc.viewport {
        return Some(height);
    }
    if let Some(device) = ctx.doc.env.as_ref().and_then(|env| env.device) {
        return Some(f64::from(device.height));
    }
    ctx.doc
        .matrix
        .iter()
        .find_map(|tag| tag.viewport)
        .map(|vp| f64::from(vp.height))
}

fn intersects_initial_viewport(y: f64, height: f64, viewport: f64) -> bool {
    y < viewport && y + height > 0.0
}

fn hidden(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    node.display_visible() == Some(false)
        || ctx
            .style_any(
                node,
                &[
                    ("layout", "display"),
                    ("box_model", "display"),
                    ("all", "display"),
                ],
            )
            .is_some_and(|value| value.eq_ignore_ascii_case("none"))
}

fn content_visibility<'b>(ctx: &ModernCtx<'_>, node: &'b DiffNode) -> Option<&'b str> {
    ctx.style_any(
        node,
        &[
            ("layout", "content-visibility"),
            ("all", "content-visibility"),
        ],
    )
}

fn has_length(value: &str) -> bool {
    value.bytes().any(|byte| byte.is_ascii_digit())
}

fn has_intrinsic_size(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    [
        "contain-intrinsic-size",
        "contain-intrinsic-width",
        "contain-intrinsic-height",
    ]
    .iter()
    .any(|prop| {
        ctx.style_any(node, &[("layout", prop), ("all", prop)])
            .is_some_and(has_length)
    })
}

fn has_cv_supports_fallback(ctx: &ModernCtx<'_>) -> bool {
    let Some(declared) = &ctx.doc.declared else {
        return false;
    };
    declared.at_rules.iter().any(|at| {
        at.condition_text.contains("content-visibility")
            && at.rules.iter().any(|rule| {
                rule.declarations
                    .values()
                    .any(|value| value.contains("layout") && value.contains("paint"))
            })
    })
}

fn attr_is(ctx: &ModernCtx<'_>, node: &DiffNode, name: &str, expected: &str) -> bool {
    ctx.attr(node, name)
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

/// Boolean HTML attributes are captured with an empty value; `ctx.attr`
/// filters those out, so presence must be tested on the raw map.
fn attr_present(_ctx: &ModernCtx<'_>, node: &DiffNode, name: &str) -> bool {
    node.attributes
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|attrs| attrs.contains_key(name))
}

fn has_js_lazy_fallback(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    ctx.attr(node, "data-src").is_some() || ctx.attr(node, "data-poster").is_some()
}

fn style_is_set(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        let trimmed = value.trim();
        !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("auto")
            && !trimmed.eq_ignore_ascii_case("none")
    })
}

fn css_sized(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    let width = ctx.style_any(
        node,
        &[
            ("box_model", "width"),
            ("layout", "width"),
            ("all", "width"),
        ],
    );
    let height = ctx.style_any(
        node,
        &[
            ("box_model", "height"),
            ("layout", "height"),
            ("all", "height"),
        ],
    );
    let ratio = ctx.style_any(
        node,
        &[
            ("box_model", "aspect-ratio"),
            ("layout", "aspect-ratio"),
            ("all", "aspect-ratio"),
        ],
    );
    style_is_set(ratio) || (style_is_set(width) && style_is_set(height))
}

fn integer_dim(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.trim().parse::<u32>().is_ok())
}

fn uses_w_descriptors(srcset: &str) -> bool {
    srcset.split(',').any(|candidate| {
        candidate.split_whitespace().last().is_some_and(|token| {
            token.strip_suffix('w').is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
            })
        })
    })
}

fn subtree_size(node: &DiffNode) -> usize {
    node.children
        .iter()
        .map(|child| subtree_size(child).saturating_add(1))
        .sum()
}

fn count_focusable(ctx: &ModernCtx<'_>, node: &DiffNode, total: &mut usize) {
    if ctx.aria_bool(node, "ariaHidden") || attr_present(ctx, node, "inert") || hidden(ctx, node) {
        return;
    }
    let disabled = attr_present(ctx, node, "disabled") || ctx.aria_bool(node, "disabled");
    let focusable = ctx.aria_bool(node, "focusable")
        || matches!(
            ctx.tag(node),
            "BUTTON" | "INPUT" | "SELECT" | "TEXTAREA" | "A" | "SUMMARY"
        );
    if focusable && !disabled {
        *total = total.saturating_add(1);
    }
    for child in &node.children {
        count_focusable(ctx, child, total);
    }
}

fn focusable_descendants(ctx: &ModernCtx<'_>, node: &DiffNode) -> usize {
    let mut total = 0;
    for child in &node.children {
        count_focusable(ctx, child, &mut total);
    }
    total
}

fn script_is_deferred(script: &ScriptInfo) -> bool {
    script.defer
        || script.async_
        || script
            .type_
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("module"))
}

fn script_node_is_deferred(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    attr_present(ctx, node, "async")
        || attr_present(ctx, node, "defer")
        || ctx
            .attr(node, "type")
            .is_some_and(|kind| kind.eq_ignore_ascii_case("module"))
}

fn blocking_render(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    ctx.attr(node, "blocking").is_some_and(|value| {
        value
            .split_whitespace()
            .any(|part| part.eq_ignore_ascii_case("render"))
    })
}

fn document_script_for_src<'a>(ctx: &'a ModernCtx<'_>, src: &str) -> Option<&'a ScriptInfo> {
    ctx.doc
        .document
        .as_ref()?
        .scripts
        .iter()
        .find(|script| script.src.as_deref() == Some(src))
}

fn node_for_href<'a>(ctx: &'a ModernCtx<'_>, tag: &str, href: &str) -> Option<&'a DiffNode> {
    ctx.flat.iter().copied().find(|node| {
        ctx.tag(node).eq_ignore_ascii_case(tag) && ctx.attr(node, "href") == Some(href)
    })
}

fn node_has_attr_for_href(ctx: &ModernCtx<'_>, tag: &str, href: &str, name: &str) -> bool {
    node_for_href(ctx, tag, href).is_some_and(|node| ctx.attr(node, name).is_some())
}

fn link_selector(href: Option<&str>) -> String {
    href.map_or_else(
        || "link[rel=preload]".to_string(),
        |href| format!("link[rel=preload][href=\"{href}\"]"),
    )
}

fn script_selector(src: Option<&str>) -> String {
    src.map_or_else(
        || "script[inline]".to_string(),
        |src| format!("script[src=\"{src}\"]"),
    )
}

fn positive_duration(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        value
            .split(',')
            .filter_map(|part| part.trim().strip_suffix('s'))
            .filter_map(|number| number.parse::<f64>().ok())
            .any(|seconds| seconds > 0.0)
    })
}

fn layout_keyframe_names(ctx: &ModernCtx<'_>) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let Some(declared) = &ctx.doc.declared else {
        return names;
    };
    for at in &declared.at_rules {
        let condition = at.condition_text.trim();
        let Some(prefix) = condition.get(..10) else {
            continue;
        };
        if !prefix.eq_ignore_ascii_case("@keyframes") {
            continue;
        }
        let Some(name) = condition.get(10..).map(str::trim) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let animates_layout = at.rules.iter().any(|rule| {
            rule.declarations.keys().any(|property| {
                LAYOUT_ANIMATION_PROPS
                    .iter()
                    .any(|candidate| property.eq_ignore_ascii_case(candidate))
            })
        });
        if animates_layout {
            names.insert(name.to_string());
        }
    }
    names
}

fn image_set_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        value
            .split_whitespace()
            .any(|token| token.to_ascii_lowercase().starts_with("image-set("))
    })
}

fn url_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        value
            .split_whitespace()
            .any(|token| token.to_ascii_lowercase().starts_with("url("))
    })
}

fn pseudo_image_set(node: &DiffNode) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let Some(pseudo) = &node.pseudo else {
        return found;
    };
    for (name, value) in pseudo {
        let styles = value.get("styles").unwrap_or(value);
        for category in ["visual", "accessibility", "all"] {
            let Some(map) = styles.get(category).and_then(Value::as_object) else {
                continue;
            };
            for property in ["background-image", "content"] {
                let raw = map.get(property).and_then(Value::as_str);
                if image_set_value(raw) {
                    found.push((name.clone(), property.to_string()));
                }
            }
        }
    }
    found
}

fn declared_has_url_fallback(ctx: &ModernCtx<'_>, selector: &str, property: &str) -> bool {
    let Some(declared) = &ctx.doc.declared else {
        return false;
    };
    declared.rules.iter().any(|rule| {
        rule.selector == selector && url_value(rule.declarations.get(property).map(String::as_str))
    })
}

// --- individual checks ----------------------------------------------------

fn check_cv_intrinsic(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let supports_fallback = has_cv_supports_fallback(ctx);
    let mut emitted = BTreeSet::new();
    for &node in &ctx.flat {
        let Some(cv) = content_visibility(ctx, node) else {
            continue;
        };
        if !cv.eq_ignore_ascii_case("auto")
            || hidden(ctx, node)
            || has_intrinsic_size(ctx, node)
            || supports_fallback
            || !emitted.insert(node.selector.clone())
        {
            continue;
        }
        emit_line(
            ctx,
            out,
            spec,
            node,
            proxy_ev(
                format!(
                    "content-visibility:auto on {} without contain-intrinsic-size: zero-height placeholder can shift layout (CLS proxy)",
                    node.selector
                ),
                "styles.layout",
            ),
        );
    }
}

fn check_cv_above_fold(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(viewport) = viewport_height(ctx) else {
        tracing::debug!("content-visibility-above-the-fold skipped: no viewport metadata");
        return;
    };
    for &node in &ctx.flat {
        let Some(cv) = content_visibility(ctx, node) else {
            continue;
        };
        if !cv.eq_ignore_ascii_case("auto") || hidden(ctx, node) {
            continue;
        }
        let Some((_, y, _, height)) = rect_coords(node) else {
            continue;
        };
        if !intersects_initial_viewport(y, height, viewport) {
            continue;
        }
        emit_line(
            ctx,
            out,
            spec,
            node,
            proxy_num(
                format!(
                    "content-visibility:auto on {} inside the initial viewport (y={y:.0}, vh={viewport:.0}): adds work to the critical render",
                    node.selector
                ),
                "styles.layout+rect+viewport",
                "rect_y_vs_viewport_height",
                Value::from(y),
                Value::from(viewport),
                "<",
            ),
        );
    }
}

fn check_cv_hidden_inert(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for &node in &ctx.flat {
        let Some(cv) = content_visibility(ctx, node) else {
            continue;
        };
        if !cv.eq_ignore_ascii_case("hidden") || hidden(ctx, node) {
            continue;
        }
        if ctx.aria_bool(node, "ariaHidden")
            || attr_present(ctx, node, "aria-hidden")
            || attr_present(ctx, node, "inert")
        {
            continue;
        }
        let focusable = focusable_descendants(ctx, node);
        if focusable == 0 {
            continue;
        }
        emit_line(
            ctx,
            out,
            spec,
            node,
            proxy_num(
                format!(
                    "view hidden via content-visibility without inert: {} keeps {focusable} focusable descendants in the tab order",
                    node.selector
                ),
                "styles.layout+attrs+aria",
                "focusable_descendants",
                Value::from(focusable),
                Value::from(0),
                ">",
            ),
        );
    }
}

fn check_image_dimensions(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    if ctx.dom_stats.images == 0 || ctx.dom_stats.images_without_dims == 0 {
        tracing::trace!("image-without-dimensions skipped: no undimensioned images in dom_stats");
        return;
    }
    for &node in &ctx.flat {
        if !ctx.tag(node).eq_ignore_ascii_case("img") {
            continue;
        }
        let width = ctx.attr(node, "width");
        let height = ctx.attr(node, "height");
        if width.is_some() && height.is_some() || css_sized(ctx, node) {
            continue;
        }
        emit_line(
            ctx,
            out,
            spec,
            node,
            proxy_num(
                format!(
                    "<img> {} has no width/height: aspect ratio is unknown until download (CLS proxy)",
                    node.selector
                ),
                "attrs+__dom_stats",
                "images_without_dims",
                Value::from(ctx.dom_stats.images_without_dims),
                Value::from(0),
                ">",
            ),
        );
    }
}

fn check_srcset_sizes(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for &node in &ctx.flat {
        if !matches!(ctx.tag(node), "IMG" | "SOURCE") {
            continue;
        }
        let Some(srcset) = ctx.attr(node, "srcset") else {
            continue;
        };
        if ctx.attr(node, "sizes").is_some() || !uses_w_descriptors(srcset) {
            continue;
        }
        emit_line(
            ctx,
            out,
            spec,
            node,
            proxy_ev(
                format!(
                    "srcset without sizes on {}: browser assumes 100vw and may fetch the largest resource",
                    node.selector
                ),
                "attrs",
            ),
        );
    }
}

fn check_lazy_above_fold(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(viewport) = viewport_height(ctx) else {
        tracing::debug!("lazy-media-above-the-fold skipped: no viewport metadata");
        return;
    };
    for &node in &ctx.flat {
        if !matches!(ctx.tag(node), "IMG" | "VIDEO" | "AUDIO" | "IFRAME")
            || !attr_is(ctx, node, "loading", "lazy")
        {
            continue;
        }
        let Some((_, y, width, height)) = rect_coords(node) else {
            continue;
        };
        if width.mul_add(height, 0.0) < MIN_PROXY_AREA
            || !intersects_initial_viewport(y, height, viewport)
        {
            continue;
        }
        emit_line(
            ctx,
            out,
            spec,
            node,
            proxy_num(
                format!(
                    "loading=lazy on {} {} inside the viewport ({y:.0}..{:.0}): delays first render",
                    ctx.tag(node),
                    node.selector,
                    y + height
                ),
                "attrs+rect+viewport",
                "rect_y_vs_viewport_height",
                Value::from(y),
                Value::from(viewport),
                "<",
            ),
        );
    }
}

fn check_lazy_media_shape(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for &node in &ctx.flat {
        if !attr_is(ctx, node, "loading", "lazy") || has_js_lazy_fallback(ctx, node) {
            continue;
        }
        let tag = ctx.tag(node);
        if tag.eq_ignore_ascii_case("video") {
            let missing_dims =
                !integer_dim(ctx.attr(node, "width")) || !integer_dim(ctx.attr(node, "height"));
            if missing_dims && !css_sized(ctx, node) {
                emit_line(
                    ctx,
                    out,
                    spec,
                    node,
                    proxy_ev(
                        format!(
                            "VIDEO {} with loading=lazy has no integer width/height: box falls back to 300x150 and may never intersect the viewport",
                            node.selector
                        ),
                        "attrs+tag",
                    ),
                );
            } else if attr_present(ctx, node, "autoplay") && !attr_present(ctx, node, "muted") {
                emit_line(
                    ctx,
                    out,
                    spec,
                    node,
                    proxy_ev(
                        format!(
                            "VIDEO {} autoplays without muted: audio may start unexpectedly (proxy; duration not measured)",
                            node.selector
                        ),
                        "attrs+tag",
                    ),
                );
            }
        } else if tag.eq_ignore_ascii_case("audio") && !attr_present(ctx, node, "controls") {
            emit_line(
                ctx,
                out,
                spec,
                node,
                proxy_ev(
                    format!(
                        "AUDIO {} with loading=lazy has no controls: it cannot load or play on demand",
                        node.selector
                    ),
                    "attrs+tag",
                ),
            );
        }
    }
}

fn check_fetch_conflict(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for &node in &ctx.flat {
        if matches!(ctx.tag(node), "IMG" | "SOURCE")
            && attr_is(ctx, node, "fetchpriority", "high")
            && attr_is(ctx, node, "loading", "lazy")
        {
            emit_line(
                ctx,
                out,
                spec,
                node,
                proxy_ev(
                    format!(
                        "fetchpriority=high combined with loading=lazy on {}: contradictory signals to the preload scanner",
                        node.selector
                    ),
                    "attrs",
                ),
            );
        }
    }
}

fn check_will_change(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let mut count = 0usize;
    let mut kinds = BTreeSet::new();
    for &node in &ctx.flat {
        let Some(value) = ctx.style_any(
            node,
            &[
                ("layout", "will-change"),
                ("visual", "will-change"),
                ("all", "will-change"),
            ],
        ) else {
            continue;
        };
        let trimmed = value.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("auto")
            || trimmed.eq_ignore_ascii_case("none")
        {
            continue;
        }
        count = count.saturating_add(1);
        kinds.insert(trimmed.to_ascii_lowercase());
    }
    if count <= WILL_CHANGE_LIMIT {
        tracing::debug!(
            count,
            limit = WILL_CHANGE_LIMIT,
            "will-change below proxy threshold"
        );
        return;
    }
    let values = kinds.into_iter().collect::<Vec<_>>().join(", ");
    emit_global(
        ctx,
        out,
        spec,
        "__styles.will-change".to_string(),
        proxy_num(
            format!(
                "{count} nodes declare will-change ({values}): excess VRAM; proxy suggests applying it only during hover/interaction"
            ),
            "styles",
            "will_change_nodes",
            Value::from(count),
            Value::from(WILL_CHANGE_LIMIT),
            ">",
        ),
    );
}

fn check_image_set(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = &ctx.doc.declared else {
        tracing::debug!("image-set-without-legacy-fallback skipped: no CSSOM facet");
        return;
    };
    let mut emitted = BTreeSet::new();
    for rule in &declared.rules {
        for (property, value) in &rule.declarations {
            if !image_set_value(Some(value))
                || url_value(rule.declarations.get(property).map(String::as_str))
            {
                continue;
            }
            if !emitted.insert(rule.selector.clone()) {
                continue;
            }
            emit_global(
                ctx,
                out,
                spec,
                rule.selector.clone(),
                proxy_ev(
                    format!(
                        "image-set() on {} without an earlier url() declaration for {property}: browsers without support lose the image",
                        rule.selector
                    ),
                    "declared",
                ),
            );
        }
    }
    for &node in &ctx.flat {
        for (pseudo, property) in pseudo_image_set(node) {
            if declared_has_url_fallback(ctx, &node.selector, &property) {
                continue;
            }
            let selector = format!("{}::{pseudo}", node.selector);
            if !emitted.insert(selector.clone()) {
                continue;
            }
            emit_global(
                ctx,
                out,
                spec,
                selector,
                proxy_ev(
                    format!(
                        "image-set() in {} for {property} without a url() fallback in CSSOM: older browsers lose the image",
                        node.selector
                    ),
                    "pseudo+declared",
                ),
            );
        }
    }
}

fn check_render_blocking(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    if !ctx.has_facet(Facet::Attrs) {
        tracing::debug!(
            "render-blocking-script-misuse: blocking attribute cannot be observed without attrs"
        );
    }
    for &node in &ctx.flat {
        if ctx.tag(node).eq_ignore_ascii_case("script") && blocking_render(ctx, node) {
            let deferred = script_node_is_deferred(ctx, node)
                || ctx
                    .attr(node, "src")
                    .and_then(|src| document_script_for_src(ctx, src))
                    .is_some_and(script_is_deferred);
            if deferred {
                continue;
            }
            emit_line(
                ctx,
                out,
                spec,
                node,
                proxy_ev(
                    format!(
                        "script {} has blocking=render without async/defer/module: parser-blocking head resource delays first paint",
                        node.selector
                    ),
                    "__document+attrs",
                ),
            );
            continue;
        }
        let loading_hack = ctx.attr(node, "class").is_some_and(|classes| {
            classes
                .split_whitespace()
                .any(|class| class.eq_ignore_ascii_case("ab-loading"))
        }) && ctx
            .style_any(node, &[("visual", "opacity"), ("all", "opacity")])
            .is_some_and(|opacity| opacity.trim() == "0");
        if loading_hack {
            emit_line(
                ctx,
                out,
                spec,
                node,
                proxy_ev(
                    format!(
                        "legacy ab-loading opacity:0 anti-flicker hack still active on {}: blocking=render path appears unfinished",
                        node.selector
                    ),
                    "attrs+styles.visual+__document",
                ),
            );
        }
    }
}

fn check_preload(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(document) = &ctx.doc.document else {
        return;
    };
    let preloads = document.links.iter().filter(|link| {
        link.rel
            .split_whitespace()
            .any(|rel| rel.eq_ignore_ascii_case("preload"))
    });
    let mut fonts = 0usize;
    let mut images = 0usize;
    let mut emitted = BTreeSet::new();
    for link in preloads {
        let kind = link.as_.as_deref().unwrap_or_default();
        let is_font = kind.eq_ignore_ascii_case("font");
        let is_image = kind.eq_ignore_ascii_case("image");
        if is_font {
            fonts = fonts.saturating_add(1);
        }
        if is_image {
            images = images.saturating_add(1);
        }
        let mut reasons = Vec::new();
        if link.as_.is_none() {
            reasons.push("missing as");
        }
        if is_font
            && link
                .fetchpriority
                .as_deref()
                .is_some_and(|priority| priority.eq_ignore_ascii_case("high"))
        {
            reasons.push("font preload with fetchpriority=high is redundant");
        }
        if is_font
            && ctx.has_facet(Facet::Attrs)
            && !link
                .href
                .as_deref()
                .is_some_and(|href| node_has_attr_for_href(ctx, "link", href, "crossorigin"))
        {
            reasons.push("font preload without crossorigin");
        }
        if reasons.is_empty() {
            continue;
        }
        let selector = link_selector(link.href.as_deref());
        if !emitted.insert(selector.clone()) {
            continue;
        }
        emit_global(
            ctx,
            out,
            spec,
            selector,
            proxy_ev(
                format!(
                    "preload {} has {}; check priority and crossorigin",
                    link.href.as_deref().unwrap_or("(no href)"),
                    reasons.join("; ")
                ),
                "__document.links",
            ),
        );
    }
    if fonts > PRELOAD_FONT_BUDGET || images > PRELOAD_IMAGE_BUDGET {
        emit_global(
            ctx,
            out,
            spec,
            "__document.links[rel=preload]".to_string(),
            proxy_num(
                format!(
                    "{fonts} font and {images} image preloads compete for bandwidth; proxy budget is {PRELOAD_FONT_BUDGET}/{PRELOAD_IMAGE_BUDGET}"
                ),
                "__document.links",
                "preload_count",
                Value::from(fonts.saturating_add(images)),
                Value::from(PRELOAD_FONT_BUDGET.saturating_add(PRELOAD_IMAGE_BUDGET)),
                ">",
            ),
        );
    }
}

fn check_script_priority(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(document) = &ctx.doc.document else {
        return;
    };
    let mut high = 0usize;
    for script in &document.scripts {
        let priority = script.fetchpriority.as_deref().unwrap_or_default();
        let selector = script_selector(script.src.as_deref());
        if priority.eq_ignore_ascii_case("high") {
            high = high.saturating_add(1);
        }
        if priority.eq_ignore_ascii_case("low") && !script_is_deferred(script) {
            emit_global(
                ctx,
                out,
                spec,
                selector,
                proxy_ev(
                    format!(
                        "script {} has fetchpriority=low without async/defer/module: still parser-blocking",
                        script.src.as_deref().unwrap_or("inline")
                    ),
                    "__document.scripts",
                ),
            );
        }
    }
    if high > HIGH_FETCHPRIORITY_BUDGET {
        emit_global(
            ctx,
            out,
            spec,
            "__document.scripts[fetchpriority=high]".to_string(),
            proxy_num(
                format!(
                    "{high} scripts request fetchpriority=high; priority is zero-sum beyond {HIGH_FETCHPRIORITY_BUDGET}"
                ),
                "__document.scripts",
                "high_priority_scripts",
                Value::from(high),
                Value::from(HIGH_FETCHPRIORITY_BUDGET),
                ">",
            ),
        );
    }
    for &node in &ctx.flat {
        if ctx.tag(node).eq_ignore_ascii_case("script") && ctx.attr(node, "importance").is_some() {
            emit_line(
                ctx,
                out,
                spec,
                node,
                proxy_ev(
                    format!(
                        "script {} uses the deprecated importance attribute; use fetchpriority instead",
                        node.selector
                    ),
                    "attrs+__document.scripts",
                ),
            );
        }
    }
}

fn check_speculation(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(document) = &ctx.doc.document else {
        return;
    };
    let rules = document
        .scripts
        .iter()
        .filter(|script| {
            script
                .type_
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("speculationrules"))
        })
        .count()
        + ctx
            .flat
            .iter()
            .filter(|node| {
                ctx.tag(node).eq_ignore_ascii_case("script")
                    && ctx
                        .attr(node, "type")
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("speculationrules"))
            })
            .count();
    if rules == 0 {
        tracing::debug!("speculation-rules-misuse skipped: no speculationrules script");
        return;
    }
    let mut targets = 0usize;
    let mut emitted = BTreeSet::new();
    for &node in &ctx.flat {
        let stateful_link = ctx.tag(node).eq_ignore_ascii_case("a")
            && ctx.attr(node, "href").is_some_and(|href| {
                let lower = href.to_ascii_lowercase();
                STATE_CHANGE_HINTS.iter().any(|hint| lower.contains(hint))
            });
        let stateful_form =
            ctx.tag(node).eq_ignore_ascii_case("form") && attr_is(ctx, node, "method", "post");
        if !stateful_link && !stateful_form {
            continue;
        }
        if attr_present(ctx, node, "data-no-speculate")
            || attr_present(ctx, node, "data-speculation-exclude")
        {
            continue;
        }
        targets = targets.saturating_add(1);
        if emitted.insert(node.selector.clone()) {
            emit_line(
                ctx,
                out,
                spec,
                node,
                proxy_ev(
                    format!(
                        "speculation rules coexist with state-changing target {}: verify eagerness and exclusions (JSON text not captured)",
                        node.selector
                    ),
                    "__document.scripts+attrs",
                ),
            );
        }
    }
    if targets == 0 {
        tracing::debug!("speculation-rules-misuse skipped: no state-changing navigation target");
    }
}

fn check_live_region(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for &node in &ctx.flat {
        if hidden(ctx, node) || ctx.aria_bool(node, "ariaHidden") {
            continue;
        }
        let live = ctx.attr(node, "aria-live").is_some()
            || ctx.aria(node, "ariaLive").is_some()
            || ctx.aria(node, "live").is_some();
        if !live {
            continue;
        }
        let size = subtree_size(node);
        let nested_busy = node
            .children
            .iter()
            .any(|child| ctx.aria_bool(child, "ariaBusy"));
        if size <= LIVE_REGION_NODES && !nested_busy {
            continue;
        }
        emit_line(
            ctx,
            out,
            spec,
            node,
            proxy_num(
                format!(
                    "live region on {} covers {size} nodes{}: guide requires aria-live only on the progress text",
                    node.selector,
                    if nested_busy {
                        " with nested aria-busy"
                    } else {
                        ""
                    }
                ),
                "attrs+aria+tree",
                "live_region_nodes",
                Value::from(size),
                Value::from(LIVE_REGION_NODES),
                ">",
            ),
        );
    }
}

fn check_animation_layout(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let keyframes = layout_keyframe_names(ctx);
    for &node in &ctx.flat {
        let mut property = ctx
            .style_any(
                node,
                &[
                    ("animation", "transition-property"),
                    ("all", "transition-property"),
                ],
            )
            .filter(|_| {
                positive_duration(ctx.style_any(
                    node,
                    &[
                        ("animation", "transition-duration"),
                        ("all", "transition-duration"),
                    ],
                ))
            })
            .and_then(|value| {
                value.split(',').map(str::trim).find(|part| {
                    part.eq_ignore_ascii_case("all")
                        || LAYOUT_ANIMATION_PROPS
                            .iter()
                            .any(|candidate| part.eq_ignore_ascii_case(candidate))
                })
            })
            .map(str::to_string);
        if property.is_none() {
            property = ctx
                .style_any(
                    node,
                    &[("animation", "animation-name"), ("all", "animation-name")],
                )
                .filter(|_| {
                    positive_duration(ctx.style_any(
                        node,
                        &[
                            ("animation", "animation-duration"),
                            ("all", "animation-duration"),
                        ],
                    ))
                })
                .and_then(|value| {
                    value
                        .split_whitespace()
                        .find(|name| keyframes.contains(*name))
                })
                .map(str::to_string);
        }
        let Some(property) = property else {
            continue;
        };
        emit_line(
            ctx,
            out,
            spec,
            node,
            proxy_ev(
                format!(
                    "{} animates layout property {property}: proxy suggests transform/opacity instead",
                    node.selector
                ),
                "styles.animation+declared",
            ),
        );
    }
}

fn check_cv_candidate(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for &node in &ctx.flat {
        if hidden(ctx, node)
            || content_visibility(ctx, node).is_some()
            || ctx
                .style_any(node, &[("layout", "contain"), ("all", "contain")])
                .is_some()
        {
            continue;
        }
        let size = subtree_size(node);
        if size < CV_CANDIDATE_NODES {
            continue;
        }
        let Some((_, _, width, height)) = rect_coords(node) else {
            continue;
        };
        if width.mul_add(height, 0.0) < CV_CANDIDATE_AREA {
            continue;
        }
        emit_line(
            ctx,
            out,
            spec,
            node,
            proxy_num(
                format!(
                    "{} contains {size} nodes in a large box: proxy candidate for content-visibility:auto with contain-intrinsic-size",
                    node.selector
                ),
                "styles+rect+tree",
                "subtree_nodes",
                Value::from(size),
                Value::from(CV_CANDIDATE_NODES),
                ">=",
            ),
        );
    }
}

fn check_heavy_dom(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let stats = &ctx.dom_stats;
    if stats.nodes < HEAVY_DOM_NODES && stats.max_depth < HEAVY_DOM_DEPTH {
        tracing::debug!(
            nodes = stats.nodes,
            depth = stats.max_depth,
            "heavy-dom-region below proxy budget"
        );
        return;
    }
    emit_global(
        ctx,
        out,
        spec,
        "__dom_stats".to_string(),
        proxy_num(
            format!(
                "captured {} nodes (max depth {}, {} interactive, {} images): heavy DOM is a proxy for slow style/layout work",
                stats.nodes, stats.max_depth, stats.interactive, stats.images
            ),
            "__dom_stats",
            "dom_nodes",
            Value::from(stats.nodes),
            Value::from(HEAVY_DOM_NODES),
            ">=",
        ),
    );
}

fn check_image_without_lazy(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(viewport) = viewport_height(ctx) else {
        tracing::debug!("image-without-lazy skipped: no viewport metadata");
        return;
    };
    for &node in &ctx.flat {
        if !ctx.tag(node).eq_ignore_ascii_case("img")
            || ctx.attr(node, "loading").is_some()
            || has_js_lazy_fallback(ctx, node)
            || attr_is(ctx, node, "fetchpriority", "high")
        {
            continue;
        }
        let Some((_, y, width, height)) = rect_coords(node) else {
            continue;
        };
        if y < viewport || width.mul_add(height, 0.0) < MIN_PROXY_AREA {
            continue;
        }
        emit_line(
            ctx,
            out,
            spec,
            node,
            proxy_num(
                format!(
                    "image {} sits below the fold (y={y:.0}, vh={viewport:.0}) without loading=lazy",
                    node.selector
                ),
                "attrs+rect+viewport",
                "rect_y_vs_viewport_height",
                Value::from(y),
                Value::from(viewport),
                ">=",
            ),
        );
    }
}

fn check_fetchpriority_missing(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(viewport) = viewport_height(ctx) else {
        tracing::debug!("fetchpriority-missing skipped: no viewport metadata");
        return;
    };
    let has_hero = ctx.flat.iter().any(|node| {
        ctx.tag(node).eq_ignore_ascii_case("img") && attr_is(ctx, node, "fetchpriority", "high")
    });
    if has_hero {
        tracing::debug!("fetchpriority-missing skipped: another image already has high priority");
        return;
    }
    for &node in &ctx.flat {
        if !ctx.tag(node).eq_ignore_ascii_case("img")
            || ctx.attr(node, "fetchpriority").is_some()
            || attr_is(ctx, node, "loading", "lazy")
        {
            continue;
        }
        let Some((_, y, width, height)) = rect_coords(node) else {
            continue;
        };
        let area = width.mul_add(height, 0.0);
        if !intersects_initial_viewport(y, height, viewport) || area < MIN_PROXY_AREA {
            continue;
        }
        emit_line(
            ctx,
            out,
            spec,
            node,
            proxy_num(
                format!(
                    "large in-viewport image {} has no fetchpriority hint and no image claims high priority: proxy candidate to review (LCP not measured)",
                    node.selector
                ),
                "attrs+rect+viewport",
                "proxy_area_px2",
                Value::from(area),
                Value::from(MIN_PROXY_AREA),
                ">=",
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Map, json};
    use sniff_core::modern::{DeclaredFacet, DocumentFacet};
    use sniff_css_diff::DiffDocument;

    fn node(id: u64, selector: &str, tag: &str) -> DiffNode {
        DiffNode {
            id,
            parent_id: None,
            selector: selector.to_string(),
            tag: Some(tag.to_string()),
            path: None,
            depth: Some(1),
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

    fn with_attrs(mut n: DiffNode, pairs: &[(&str, &str)]) -> DiffNode {
        let mut map = Map::new();
        for (key, value) in pairs {
            map.insert((*key).to_string(), Value::String((*value).to_string()));
        }
        n.attributes = Some(Value::Object(map));
        n
    }

    fn with_style(mut n: DiffNode, category: &str, pairs: &[(&str, &str)]) -> DiffNode {
        let map = n.styles.get_or_insert_with(Map::new);
        let category = map
            .entry(category)
            .or_insert_with(|| Value::Object(Map::new()));
        let props = category.as_object_mut().expect("synthetic style category");
        for (key, value) in pairs {
            props.insert((*key).to_string(), Value::String((*value).to_string()));
        }
        n
    }

    fn with_rect(mut n: DiffNode, x: f64, y: f64, width: f64, height: f64) -> DiffNode {
        n.rect = Some(json!({"x": x, "y": y, "width": width, "height": height}));
        n
    }

    fn with_visible(mut n: DiffNode, visible: bool) -> DiffNode {
        n.noticeable = Some(json!({"display_visible": visible}));
        n
    }

    fn run_doc(doc: &DiffDocument) -> Vec<CheckLine> {
        let opts = crate::modern::ModernOptions::default();
        let ctx = ModernCtx::new(doc, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        out
    }

    fn run_nodes(nodes: Vec<DiffNode>) -> Vec<CheckLine> {
        let doc = DiffDocument {
            nodes,
            viewport: Some((1280.0, 900.0)),
            ..DiffDocument::default()
        };
        run_doc(&doc)
    }

    fn ids(lines: &[CheckLine], check: &str) -> Vec<String> {
        lines
            .iter()
            .filter(|line| line.check == check)
            .map(|line| line.selector.clone())
            .collect()
    }

    fn has(lines: &[CheckLine], check: &str) -> bool {
        lines.iter().any(|line| line.check == check)
    }

    #[test]
    fn registry_has_all_family_ids_and_proxy_guides() {
        let specs = specs();
        assert_eq!(specs.len(), 20);
        let mut ids = BTreeSet::new();
        for spec in specs {
            assert!(ids.insert(spec.id), "duplicate id {}", spec.id);
            assert!(!spec.guides.is_empty(), "{} has no guide", spec.id);
            assert!(!spec.keep_when.is_empty(), "{} has no keepWhen", spec.id);
            assert_eq!(spec.severity, sniff_core::modern::Severity::Warn);
        }
        for alias in [
            "always-on-will-change",
            "resolution-optimized-pseudo",
            "script-blocking",
        ] {
            assert!(
                !ids.contains(alias),
                "alias must not be a second id: {alias}"
            );
        }
    }

    #[test]
    fn cv_without_intrinsic_size_positive_negative_keep_when() {
        let bad = with_style(
            node(1, "section.items", "SECTION"),
            "layout",
            &[("content-visibility", "auto")],
        );
        let good = with_style(
            node(2, "section.ok", "SECTION"),
            "layout",
            &[
                ("content-visibility", "auto"),
                ("contain-intrinsic-size", "auto 150px"),
            ],
        );
        let hidden = with_visible(
            with_style(
                node(3, "section.hidden", "SECTION"),
                "layout",
                &[("content-visibility", "auto")],
            ),
            false,
        );
        let visible = with_style(
            node(4, "div.plain", "DIV"),
            "layout",
            &[("content-visibility", "visible")],
        );
        let lines = run_nodes(vec![bad, good, hidden, visible]);
        assert_eq!(
            ids(&lines, "content-visibility-without-intrinsic-size"),
            vec!["section.items".to_string()]
        );
        assert!(
            lines
                .iter()
                .all(|line| line.evidence.text.starts_with("proxy:"))
        );
    }

    #[test]
    fn cv_intrinsic_size_respects_supports_fallback() {
        let mut fallback = DeclaredFacet::default();
        fallback.at_rules.push(sniff_core::modern::AtRuleInfo {
            condition_text: "@supports not (content-visibility: auto)".to_string(),
            index: 0,
            rules: vec![sniff_core::modern::DeclaredRule {
                selector: "section.items".to_string(),
                href: None,
                index: 0,
                origin: "author".to_string(),
                declarations: std::collections::BTreeMap::from([(
                    "contain".to_string(),
                    "layout style paint".to_string(),
                )]),
            }],
        });
        let mut doc = DiffDocument {
            nodes: vec![with_style(
                node(1, "section.items", "SECTION"),
                "layout",
                &[("content-visibility", "auto")],
            )],
            ..DiffDocument::default()
        };
        doc.declared = Some(fallback);
        assert!(!has(
            &run_doc(&doc),
            "content-visibility-without-intrinsic-size"
        ));
    }

    #[test]
    fn cv_above_fold_uses_rect_and_viewport() {
        let above = with_rect(
            with_style(
                node(1, "section.a", "SECTION"),
                "layout",
                &[("content-visibility", "auto")],
            ),
            0.0,
            20.0,
            800.0,
            300.0,
        );
        let below = with_rect(
            with_style(
                node(2, "section.b", "SECTION"),
                "layout",
                &[("content-visibility", "auto")],
            ),
            0.0,
            1200.0,
            800.0,
            300.0,
        );
        let lines = run_nodes(vec![above, below]);
        assert_eq!(
            ids(&lines, "content-visibility-above-the-fold"),
            vec!["section.a".to_string()]
        );
    }

    #[test]
    fn cv_hidden_view_requires_focusable_descendant_without_inert() {
        let view = |id: u64, selector: &str| {
            let mut parent = with_style(
                node(id, selector, "DIV"),
                "layout",
                &[("content-visibility", "hidden")],
            );
            let mut button = node(id + 100, "button.go", "BUTTON");
            button.aria = Some(json!({"focusable": true}));
            button.parent_id = Some(id);
            parent.children.push(button);
            parent
        };
        let mut inert_view = view(2, "div.inert-view");
        inert_view.attributes = Some(json!({"inert": ""}));
        let lines = run_nodes(vec![view(1, "div.view"), inert_view]);
        assert_eq!(
            ids(&lines, "cv-hidden-view-not-inert"),
            vec!["div.view".to_string()]
        );
    }

    #[test]
    fn image_dimensions_positive_negative_keep_when() {
        let missing = with_attrs(node(1, "img.a", "IMG"), &[("src", "a.jpg")]);
        let complete = with_attrs(
            node(2, "img.b", "IMG"),
            &[("src", "b.jpg"), ("width", "400"), ("height", "300")],
        );
        let css_sized = with_style(
            with_attrs(node(3, "img.c", "IMG"), &[("src", "c.jpg")]),
            "box_model",
            &[("width", "100%"), ("height", "50vh")],
        );
        let lines = run_nodes(vec![missing, complete, css_sized]);
        assert_eq!(
            ids(&lines, "image-without-dimensions"),
            vec!["img.a".to_string()]
        );
        assert!(
            lines
                .iter()
                .find(|line| line.check == "image-without-dimensions")
                .is_some_and(|line| line.evidence.metric.as_deref() == Some("images_without_dims"))
        );
    }

    #[test]
    fn srcset_without_sizes_ignores_density_descriptors() {
        let width = with_attrs(
            node(1, "img.w", "IMG"),
            &[("srcset", "a-400w.jpg 400w, a-800w.jpg 800w")],
        );
        let density = with_attrs(
            node(2, "img.d", "IMG"),
            &[("srcset", "a.jpg 1x, a-2x.jpg 2x")],
        );
        let sized = with_attrs(
            node(3, "img.s", "IMG"),
            &[("srcset", "a-400w.jpg 400w"), ("sizes", "50vw")],
        );
        let lines = run_nodes(vec![width, density, sized]);
        assert_eq!(
            ids(&lines, "srcset-without-sizes"),
            vec!["img.w".to_string()]
        );
    }

    #[test]
    fn lazy_above_fold_positive_negative_keep_when() {
        let above = with_rect(
            with_attrs(
                node(1, "img.hero", "IMG"),
                &[("loading", "lazy"), ("width", "800"), ("height", "400")],
            ),
            0.0,
            100.0,
            800.0,
            400.0,
        );
        let below = with_rect(
            with_attrs(
                node(2, "img.lower", "IMG"),
                &[("loading", "lazy"), ("width", "800"), ("height", "400")],
            ),
            0.0,
            950.0,
            800.0,
            400.0,
        );
        let eager = with_rect(
            with_attrs(node(3, "img.eager", "IMG"), &[("loading", "eager")]),
            0.0,
            100.0,
            800.0,
            400.0,
        );
        let lines = run_nodes(vec![above, below, eager]);
        assert_eq!(
            ids(&lines, "lazy-media-above-the-fold"),
            vec!["img.hero".to_string()]
        );
    }

    #[test]
    fn lazy_media_shape_and_fetchpriority_conflict() {
        let video = with_attrs(
            node(1, "video.a", "VIDEO"),
            &[("loading", "lazy"), ("poster", "p.jpg")],
        );
        let audio = with_attrs(
            node(2, "audio.a", "AUDIO"),
            &[("loading", "lazy"), ("src", "a.mp3")],
        );
        let good_audio = with_attrs(
            node(3, "audio.b", "AUDIO"),
            &[("loading", "lazy"), ("controls", "")],
        );
        let conflict = with_attrs(
            node(4, "img.conflict", "IMG"),
            &[("loading", "lazy"), ("fetchpriority", "high")],
        );
        let correct = with_attrs(node(5, "img.lcp", "IMG"), &[("fetchpriority", "high")]);
        let lines = run_nodes(vec![video, audio, good_audio, conflict, correct]);
        assert_eq!(
            ids(&lines, "lazy-media-without-dimensions-or-controls"),
            vec!["video.a".to_string(), "audio.a".to_string()]
        );
        assert_eq!(
            ids(&lines, "fetchpriority-high-with-lazy"),
            vec!["img.conflict".to_string()]
        );
    }

    #[test]
    fn will_change_threshold_and_keep_when() {
        let make = |id: u64, selector: &str| {
            with_style(
                node(id, selector, "DIV"),
                "layout",
                &[("will-change", "transform")],
            )
        };
        let few = run_nodes(vec![make(1, "div.a"), make(2, "div.b")]);
        assert!(!has(&few, "excessive-will-change"));
        let many = run_nodes(vec![
            make(1, "div.a"),
            make(2, "div.b"),
            make(3, "div.c"),
            make(4, "div.d"),
            make(5, "div.e"),
        ]);
        assert!(has(&many, "excessive-will-change"));
        let auto = run_nodes(vec![
            with_style(
                node(1, "div.1", "DIV"),
                "layout",
                &[("will-change", "auto")],
            ),
            with_style(
                node(2, "div.2", "DIV"),
                "layout",
                &[("will-change", "auto")],
            ),
            with_style(
                node(3, "div.3", "DIV"),
                "layout",
                &[("will-change", "auto")],
            ),
            with_style(
                node(4, "div.4", "DIV"),
                "layout",
                &[("will-change", "auto")],
            ),
        ]);
        assert!(!has(&auto, "excessive-will-change"));
    }

    #[test]
    fn image_set_requires_cssom_and_detects_missing_fallback() {
        let mut declared = DeclaredFacet::default();
        declared.rules.push(sniff_core::modern::DeclaredRule {
            selector: ".gallery".to_string(),
            href: None,
            index: 0,
            origin: "author".to_string(),
            declarations: std::collections::BTreeMap::from([(
                "background-image".to_string(),
                "image-set(url(a.avif) type(\"image/avif\") 1x)".to_string(),
            )]),
        });
        let mut doc = DiffDocument {
            nodes: Vec::new(),
            ..DiffDocument::default()
        };
        assert!(!has(&run_doc(&doc), "image-set-without-legacy-fallback"));
        doc.declared = Some(declared);
        assert_eq!(
            ids(&run_doc(&doc), "image-set-without-legacy-fallback"),
            vec![".gallery".to_string()]
        );
        doc.declared.as_mut().unwrap().rules[0]
            .declarations
            .insert("background-image".to_string(), "url(a.jpg)".to_string());
        assert!(!has(&run_doc(&doc), "image-set-without-legacy-fallback"));
    }

    #[test]
    fn document_head_checks_positive_and_negative() {
        let mut document = DocumentFacet::default();
        document.links.push(sniff_core::modern::LinkInfo {
            rel: "preload".to_string(),
            href: Some("/f.woff2".to_string()),
            as_: Some("font".to_string()),
            fetchpriority: Some("high".to_string()),
            integrity: None,
            crossorigin: None,
        });
        document.scripts.push(ScriptInfo {
            src: Some("/legacy.js".to_string()),
            type_: None,
            defer: false,
            async_: false,
            fetchpriority: Some("low".to_string()),
            integrity: None,
            blocking: None,
            in_head: Some(true),
            text: None,
            importance: None,
        });
        let mut doc = DiffDocument {
            nodes: Vec::new(),
            ..DiffDocument::default()
        };
        assert!(!has(&run_doc(&doc), "preload-priority-misuse"));
        doc.document = Some(document);
        let lines = run_doc(&doc);
        assert!(has(&lines, "preload-priority-misuse"));
        assert!(has(&lines, "script-priority-misuse"));
    }

    #[test]
    fn render_blocking_detects_attrs_hack_and_keeps_async() {
        let bad = with_style(
            with_attrs(
                node(1, "script.a", "SCRIPT"),
                &[("blocking", "render"), ("src", "/a.js")],
            ),
            "visual",
            &[],
        );
        let good = with_attrs(
            node(2, "script.b", "SCRIPT"),
            &[("blocking", "render"), ("async", "")],
        );
        let hack = with_style(
            with_attrs(node(3, "html.c", "HTML"), &[("class", "ab-loading")]),
            "visual",
            &[("opacity", "0")],
        );
        let mut doc = DiffDocument {
            nodes: vec![bad, good, hack],
            ..DiffDocument::default()
        };
        assert!(!has(&run_doc(&doc), "render-blocking-script-misuse"));
        doc.document = Some(DocumentFacet::default());
        assert_eq!(
            ids(&run_doc(&doc), "render-blocking-script-misuse"),
            vec!["script.a".to_string(), "html.c".to_string()]
        );
    }

    #[test]
    fn speculation_rules_require_stateful_target_and_exclusion() {
        let mut document = DocumentFacet::default();
        document.scripts.push(ScriptInfo {
            src: None,
            type_: Some("speculationrules".to_string()),
            defer: false,
            async_: false,
            fetchpriority: None,
            integrity: None,
            blocking: None,
            in_head: Some(true),
            text: Some("window.speculationRules".to_string()),
            importance: None,
        });
        let mut doc = DiffDocument {
            nodes: vec![
                with_attrs(node(1, "a.logout", "A"), &[("href", "/logout")]),
                with_attrs(node(2, "a.safe", "A"), &[("href", "/about")]),
                with_attrs(
                    node(3, "a.excluded", "A"),
                    &[("href", "/add-to-cart"), ("data-no-speculate", "")],
                ),
            ],
            ..DiffDocument::default()
        };
        assert!(!has(&run_doc(&doc), "speculation-rules-misuse"));
        doc.document = Some(document);
        assert_eq!(
            ids(&run_doc(&doc), "speculation-rules-misuse"),
            vec!["a.logout".to_string()]
        );
    }

    #[test]
    fn live_region_scope_uses_subtree_size() {
        let mut broad = with_attrs(
            node(1, "section.live", "SECTION"),
            &[("aria-live", "polite")],
        );
        for id in 10..17 {
            broad.children.push(node(id, "div.filler", "DIV"));
        }
        let leaf = with_attrs(node(20, "p.status", "P"), &[("aria-live", "polite")]);
        let lines = run_nodes(vec![broad, leaf]);
        assert_eq!(
            ids(&lines, "streaming-live-region-scope"),
            vec!["section.live".to_string()]
        );
    }

    #[test]
    fn animation_on_layout_props_detects_transition_and_keyframes() {
        let transition = with_style(
            node(1, "div.slide", "DIV"),
            "animation",
            &[
                ("transition-property", "width, opacity"),
                ("transition-duration", "0.3s"),
            ],
        );
        let opacity_only = with_style(
            node(2, "div.fade", "DIV"),
            "animation",
            &[
                ("transition-property", "opacity"),
                ("transition-duration", "0.3s"),
            ],
        );
        let lines = run_nodes(vec![transition, opacity_only]);
        assert_eq!(
            ids(&lines, "animation-on-layout-props"),
            vec!["div.slide".to_string()]
        );
    }

    #[test]
    fn heavy_dom_and_candidate_use_thresholds() {
        let small = run_nodes(vec![node(1, "div.a", "DIV")]);
        assert!(!has(&small, "heavy-dom-region"));
        let mut nodes = Vec::new();
        for id in 0..HEAVY_DOM_NODES {
            nodes.push(node(u64::try_from(id).expect("small id"), "div.x", "DIV"));
        }
        let big = run_nodes(nodes);
        assert!(has(&big, "heavy-dom-region"));

        let mut parent = with_style(node(1, "section.heavy", "SECTION"), "layout", &[]);
        for id in 10..(10 + CV_CANDIDATE_NODES) {
            parent.children.push(node(
                u64::try_from(id).expect("small id"),
                "div.child",
                "DIV",
            ));
        }
        let parent = with_rect(parent, 0.0, 1200.0, 800.0, 200.0);
        let lines = run_nodes(vec![parent]);
        assert_eq!(
            ids(&lines, "content-visibility-candidate"),
            vec!["section.heavy".to_string()]
        );
    }

    #[test]
    fn below_fold_images_get_lazy_but_not_high_priority_proxy() {
        let below = with_rect(
            with_attrs(node(1, "img.below", "IMG"), &[("src", "a.jpg")]),
            0.0,
            1000.0,
            800.0,
            400.0,
        );
        let lines = run_nodes(vec![below]);
        assert_eq!(
            ids(&lines, "image-without-lazy"),
            vec!["img.below".to_string()]
        );
        assert!(!has(&lines, "fetchpriority-missing"));
    }

    #[test]
    fn fetchpriority_missing_only_for_in_viewport_candidate() {
        let viewport = with_rect(
            with_attrs(node(1, "img.viewport", "IMG"), &[("src", "a.jpg")]),
            0.0,
            10.0,
            900.0,
            600.0,
        );
        let hero = with_attrs(
            node(2, "img.hero", "IMG"),
            &[("src", "b.jpg"), ("fetchpriority", "high")],
        );
        let without = run_nodes(vec![viewport.clone()]);
        assert_eq!(
            ids(&without, "fetchpriority-missing"),
            vec!["img.viewport".to_string()]
        );
        let with_hero = run_nodes(vec![viewport, hero]);
        assert!(!has(&with_hero, "fetchpriority-missing"));
    }

    #[test]
    fn deterministic_output_for_same_snapshot() {
        let nodes = vec![
            with_attrs(node(1, "img.a", "IMG"), &[("src", "a.jpg")]),
            with_rect(
                with_attrs(
                    node(2, "img.b", "IMG"),
                    &[("loading", "lazy"), ("width", "800"), ("height", "400")],
                ),
                0.0,
                100.0,
                800.0,
                400.0,
            ),
            with_style(
                node(3, "div.c", "DIV"),
                "layout",
                &[("content-visibility", "auto")],
            ),
        ];
        let doc = DiffDocument {
            nodes,
            viewport: Some((1280.0, 900.0)),
            ..DiffDocument::default()
        };
        let first = serde_json::to_string(&run_doc(&doc)).expect("serialize first run");
        let second = serde_json::to_string(&run_doc(&doc)).expect("serialize second run");
        let third = serde_json::to_string(&run_doc(&doc)).expect("serialize third run");
        assert_eq!(first, second);
        assert_eq!(second, third);
    }
}
