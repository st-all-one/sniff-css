//! Deterministic WCAG contrast computation.
//!
//! Pure math: parses CSS colors, computes relative luminance and the
//! contrast ratio, and classifies AA/AAA compliance for normal vs. large
//! text. The effective background is resolved in-page (JS composites
//! transparent/semi-transparent layers over ancestors up to the page
//! canvas); only background images — where the color is genuinely unknown —
//! are reported as [`TriState::Unknown`] instead of guessing. Honesty over
//! false precision.

use crate::types::{ContrastInfo, ElementSnapshot, TriState};

/// Parse a CSS color into linear `[r, g, b]` (0..=1) plus an alpha (0..=1).
///
/// Accepts `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` and `rgb()`/`rgba()`
/// (comma or modern space/`/` syntax). Named colors are not resolved.
pub fn parse_color(input: &str) -> Option<([f64; 3], f64)> {
    let s = input.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Some(rest) = s.strip_prefix("rgba(").and_then(|r| r.strip_suffix(')')) {
        return parse_rgb_args(rest);
    }
    if let Some(rest) = s.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        return parse_rgb_args(rest);
    }
    None
}

fn parse_hex(hex: &str) -> Option<([f64; 3], f64)> {
    let h = hex.trim();
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok();
    let double = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    let (r, g, b, a) = match h.len() {
        3 => (byte(0)? * 17, byte(1)? * 17, byte(2)? * 17, 255),
        4 => (byte(0)? * 17, byte(1)? * 17, byte(2)? * 17, byte(3)? * 17),
        6 => (double(0)?, double(2)?, double(4)?, 255),
        8 => (double(0)?, double(2)?, double(4)?, double(6)?),
        _ => return None,
    };
    Some((
        [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0],
        a as f64 / 255.0,
    ))
}

/// Parse the comma-separated arguments of `rgb()`/`rgba()` (also handles
/// modern `255 255 255 / 0.5` syntax).
fn parse_rgb_args(rest: &str) -> Option<([f64; 3], f64)> {
    let parts: Vec<&str> = if rest.contains('/') {
        let (channels, alpha) = rest.split_once('/')?;
        channels
            .split_whitespace()
            .chain(std::iter::once(alpha))
            .collect()
    } else {
        rest.split([',', '/']).map(str::trim).collect()
    };
    let mut iter = parts.iter();
    let comp = |v: Option<&&str>| -> Option<f64> {
        let v = v?.trim();
        if let Some(pct) = v.strip_suffix('%') {
            pct.parse::<f64>().ok().map(|x| x / 100.0)
        } else {
            v.parse::<f64>().ok().map(|x| x / 255.0)
        }
    };
    let r = comp(iter.next())?;
    let g = comp(iter.next())?;
    let b = comp(iter.next())?;
    let a = match iter.next() {
        Some(raw) => {
            let raw = raw.trim();
            if let Some(pct) = raw.strip_suffix('%') {
                pct.parse::<f64>().ok()? / 100.0
            } else {
                raw.parse::<f64>().ok()?
            }
        }
        None => 1.0,
    };
    Some(([r, g, b], a))
}

/// Relative luminance of a color, per WCAG 2.x (sRGB linearization).
pub fn relative_luminance([r, g, b]: [f64; 3]) -> f64 {
    let channel = |c: f64| {
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// Contrast ratio between two colors (1.0..=21.0).
pub fn contrast_ratio(fg: [f64; 3], bg: [f64; 3]) -> f64 {
    let (l1, l2) = (relative_luminance(fg), relative_luminance(bg));
    let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

/// Derive a [`ContrastInfo`] from raw captured values (no snapshot needed).
pub fn derive_contrast_values(
    fg: Option<&str>,
    bg: Option<&str>,
    bg_image: Option<&str>,
    font_size: Option<&str>,
    font_weight: Option<&str>,
) -> Option<ContrastInfo> {
    let fg_raw = fg?;
    let bg_raw = bg?;
    let (fg, fg_alpha) = parse_color(fg_raw)?;
    let (bg, bg_alpha) = parse_color(bg_raw)?;

    let unknown = |reason: &str| ContrastInfo {
        ratio: 0.0,
        foreground: fg_raw.to_string(),
        background: bg_raw.to_string(),
        large: false,
        aa: TriState::Unknown,
        aaa: TriState::Unknown,
        unknown_reason: Some(reason.to_string()),
    };

    let image = bg_image.unwrap_or("none").trim();
    if !image.is_empty() && image != "none" {
        return Some(unknown("background image"));
    }
    if fg_alpha < 1.0 {
        return Some(unknown("transparent foreground"));
    }
    if bg_alpha < 1.0 {
        return Some(unknown("transparent background"));
    }

    let large = is_large_text(font_size, font_weight);
    let ratio = contrast_ratio(fg, bg);
    let (aa_th, aaa_th) = if large { (3.0, 4.5) } else { (4.5, 7.0) };
    Some(ContrastInfo {
        ratio: (ratio * 100.0).round() / 100.0,
        foreground: fg_raw.to_string(),
        background: bg_raw.to_string(),
        large,
        aa: classify(ratio, aa_th),
        aaa: classify(ratio, aaa_th),
        unknown_reason: None,
    })
}

fn classify(ratio: f64, threshold: f64) -> TriState {
    if ratio >= threshold {
        TriState::Pass
    } else {
        TriState::Fail
    }
}

/// WCAG "large text": >=24px, or >=18.66px bold (>=700 weight).
fn is_large_text(font_size: Option<&str>, font_weight: Option<&str>) -> bool {
    let size = font_size.and_then(parse_px).unwrap_or(0.0);
    if size >= 24.0 {
        return true;
    }
    let bold = font_weight
        .and_then(|w| {
            w.split_whitespace()
                .next()
                .and_then(|n| n.parse::<f64>().ok())
        })
        .is_some_and(|w| w >= 700.0);
    size >= 18.66 && bold
}

fn parse_px(value: &str) -> Option<f64> {
    let v = value.trim();
    if let Some(px) = v.strip_suffix("px") {
        return px.parse().ok();
    }
    if let Some(pt) = v.strip_suffix("pt") {
        return pt.parse::<f64>().ok().map(|x| x * 96.0 / 72.0);
    }
    v.parse().ok()
}

/// The effective background behind a node's text, resolved by walking the
/// ancestor chain: transparent/semi-transparent backgrounds composite
/// over the nearest opaque ancestor, while a background image anywhere in
/// the chain makes the value unmeasurable.
#[derive(Debug, Clone, PartialEq)]
enum EffectiveBackground {
    /// A composited opaque color, plus the raw value it was derived from.
    Solid { color: [f64; 3], raw: String },
    /// Some ancestor (or the node itself) paints an image underneath.
    Image,
    /// No ancestor has a resolvable background (all transparent to the
    /// capture root) — unknown, manual review.
    Unknown,
}

impl EffectiveBackground {
    fn raw(&self) -> Option<&str> {
        match self {
            Self::Solid { raw, .. } => Some(raw),
            Self::Image | Self::Unknown => None,
        }
    }
}

/// Resolve the effective background of `snap` given the background
/// inherited from its ancestors.
///
/// - An opaque `background-color` becomes the new effective background.
/// - A `background-image` (this node) paints over everything below it:
///   the node itself and all descendants become unmeasurable.
/// - A fully transparent background keeps the inherited one.
/// - A semi-transparent background composites over the inherited one
///   (when the inherited one is a solid color).
fn resolve_background(
    snap: &ElementSnapshot,
    inherited: &EffectiveBackground,
) -> EffectiveBackground {
    let bg = snap.styles.get("background-color");
    let bg_image = snap.styles.get("background-image").unwrap_or("none").trim();
    let has_image = !bg_image.is_empty() && bg_image != "none";

    let Some(bg) = bg.and_then(parse_color) else {
        // Unparseable background-color (named color, `currentcolor`, ...)
        // or none captured: keep the inherited background unless this
        // node paints an image.
        return if has_image {
            EffectiveBackground::Image
        } else {
            inherited.clone()
        };
    };
    let (color, alpha) = bg;

    if has_image {
        // An image paints over both the color and anything below.
        return EffectiveBackground::Image;
    }
    if alpha >= 1.0 {
        return EffectiveBackground::Solid {
            color,
            raw: snap
                .styles
                .get("background-color")
                .unwrap_or("")
                .to_string(),
        };
    }
    if alpha <= 0.0 {
        // Fully transparent: the inherited background shows through.
        return inherited.clone();
    }
    // Semi-transparent: blend over the inherited solid color.
    match inherited {
        EffectiveBackground::Solid { color: under, raw } => {
            let composite = [
                color[0] * alpha + under[0] * (1.0 - alpha),
                color[1] * alpha + under[1] * (1.0 - alpha),
                color[2] * alpha + under[2] * (1.0 - alpha),
            ];
            EffectiveBackground::Solid {
                color: composite,
                raw: raw.clone(),
            }
        }
        EffectiveBackground::Image | EffectiveBackground::Unknown => inherited.clone(),
    }
}

/// Apply contrast derivation to every node in a snapshot forest, resolving
/// the effective background (transparent/semi-transparent colors composite
/// over the nearest opaque ancestor; background images make the value
/// unmeasurable) top-down.
pub fn apply_contrast_all(snaps: &mut [ElementSnapshot]) {
    for snap in snaps {
        apply_contrast_node(snap, &EffectiveBackground::Unknown);
    }
}

fn apply_contrast_node(snap: &mut ElementSnapshot, inherited: &EffectiveBackground) {
    // Prefer the effective background composited in-page (JS climbs the real
    // DOM, so even a transparent capture root resolves to the page color).
    // Fall back to the tree-walk for snapshots without the field.
    let eff = match snap.effective_background.as_deref() {
        Some("image") => EffectiveBackground::Image,
        Some(color) => match parse_color(color) {
            Some((rgb, alpha)) if alpha >= 1.0 => EffectiveBackground::Solid {
                color: rgb,
                raw: color.to_string(),
            },
            _ => EffectiveBackground::Unknown,
        },
        None => resolve_background(snap, inherited),
    };
    match &eff {
        EffectiveBackground::Solid {
            color: bg_color, ..
        } => {
            let fg = snap.styles.get("color");
            match fg.and_then(parse_color) {
                Some((fg_color, fg_alpha)) => {
                    // Composite a translucent foreground over the resolved
                    // background before measuring (e.g. `rgba(0,0,0,.5)`
                    // text on white is effectively gray).
                    let fg_eff = if fg_alpha < 1.0 {
                        [
                            fg_color[0] * fg_alpha + bg_color[0] * (1.0 - fg_alpha),
                            fg_color[1] * fg_alpha + bg_color[1] * (1.0 - fg_alpha),
                            fg_color[2] * fg_alpha + bg_color[2] * (1.0 - fg_alpha),
                        ]
                    } else {
                        fg_color
                    };
                    let large =
                        is_large_text(snap.styles.get("font-size"), snap.styles.get("font-weight"));
                    let (aa_th, aaa_th) = if large { (3.0, 4.5) } else { (4.5, 7.0) };
                    let ratio = contrast_ratio(fg_eff, *bg_color);
                    snap.contrast = Some(ContrastInfo {
                        ratio: (ratio * 100.0).round() / 100.0,
                        foreground: fg.unwrap_or("").to_string(),
                        background: eff.raw().unwrap_or("").to_string(),
                        large,
                        aa: classify(ratio, aa_th),
                        aaa: classify(ratio, aaa_th),
                        unknown_reason: None,
                    });
                }
                None => snap.contrast = Some(unknown_contrast(snap, "unparseable foreground")),
            }
        }
        EffectiveBackground::Image => {
            snap.contrast = Some(unknown_contrast(snap, "background image"));
        }
        EffectiveBackground::Unknown => {
            snap.contrast = Some(unknown_contrast(snap, "transparent background"));
        }
    }
    let children = &mut snap.children;
    for child in children.iter_mut() {
        apply_contrast_node(child, &eff);
    }
}

fn unknown_contrast(snap: &ElementSnapshot, reason: &str) -> ContrastInfo {
    ContrastInfo {
        ratio: 0.0,
        foreground: snap.styles.get("color").unwrap_or("").to_string(),
        background: snap
            .styles
            .get("background-color")
            .unwrap_or("")
            .to_string(),
        large: false,
        aa: TriState::Unknown,
        aaa: TriState::Unknown,
        unknown_reason: Some(reason.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ComputedProperty, ComputedStyles};

    fn prop(name: &str, value: &str) -> ComputedProperty {
        ComputedProperty {
            name: name.into(),
            value: value.into(),
        }
    }

    #[test]
    fn parse_hex_variants() {
        assert_eq!(parse_color("#fff"), Some(([1.0, 1.0, 1.0], 1.0)));
        let (rgb, a) = parse_color("#2563eb").unwrap();
        assert!((rgb[0] - 0x25 as f64 / 255.0).abs() < 1e-9);
        assert_eq!(a, 1.0);
        let (_, a) = parse_color("#2563eb80").unwrap();
        assert!((a - 0x80 as f64 / 255.0).abs() < 1e-9);
        assert!(parse_color("#xyz").is_none());
    }

    #[test]
    fn parse_rgb_and_alpha() {
        let (rgb, a) = parse_color("rgb(37, 99, 235)").unwrap();
        assert!((rgb[0] - 37.0 / 255.0).abs() < 1e-9);
        assert_eq!(a, 1.0);
        let (_, a) = parse_color("rgba(255, 255, 255, 0.2)").unwrap();
        assert!((a - 0.2).abs() < 1e-9);
        // Modern syntax.
        let (_, a) = parse_color("rgb(255 255 255 / 50%)").unwrap();
        assert!((a - 0.5).abs() < 1e-9);
    }

    #[test]
    fn white_on_black_meets_aaa() {
        let ratio = contrast_ratio([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]);
        assert!(ratio > 20.9 && ratio <= 21.0, "ratio was {ratio}");
    }

    #[test]
    fn black_on_white_ratio_is_identical() {
        let a = contrast_ratio([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let b = contrast_ratio([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]);
        assert!((a - b).abs() < 1e-9);
    }

    #[test]
    fn derive_classifies_normal_and_large() {
        // #2563eb on #ffffff: ratio ~5.17 -> AA passes, AAA fails.
        let normal = derive_contrast_values(
            Some("#2563eb"),
            Some("#ffffff"),
            None,
            Some("16px"),
            Some("400"),
        )
        .unwrap();
        assert!((normal.ratio - 5.17).abs() < 0.05, "ratio {}", normal.ratio);
        assert_eq!(normal.aa, TriState::Pass);
        assert_eq!(normal.aaa, TriState::Fail);
        assert!(!normal.large);

        // Same colors but 24px text: AA threshold drops to 3.0, AAA to 4.5.
        let large = derive_contrast_values(
            Some("#2563eb"),
            Some("#ffffff"),
            None,
            Some("24px"),
            Some("400"),
        )
        .unwrap();
        assert!(large.large);
        assert_eq!(large.aa, TriState::Pass);
        assert_eq!(large.aaa, TriState::Pass);
    }

    #[test]
    fn derive_reports_unknown_backgrounds() {
        let transparent = derive_contrast_values(
            Some("#000000"),
            Some("rgba(255, 255, 255, 0.5)"),
            None,
            Some("16px"),
            Some("400"),
        )
        .unwrap();
        assert_eq!(transparent.aa, TriState::Unknown);
        assert_eq!(
            transparent.unknown_reason.as_deref(),
            Some("transparent background")
        );

        let image = derive_contrast_values(
            Some("#000000"),
            Some("#ffffff"),
            Some("url(x.png)"),
            Some("16px"),
            Some("400"),
        )
        .unwrap();
        assert_eq!(image.aa, TriState::Unknown);
    }

    #[test]
    fn apply_contrast_reads_snapshot_styles() {
        let mut snap = ElementSnapshot {
            id: 1,
            parent_id: None,
            tag: "DIV".into(),
            selector: ".card".into(),
            path: ".card".into(),
            depth: 0,
            rect: None,
            metrics: None,
            noticeable: None,
            aria: None,
            effective_background: None,
            contrast: None,
            ax: None,
            attributes: None,
            styles: ComputedStyles {
                groups: vec![(
                    crate::properties::StyleCategory::Typography,
                    vec![
                        prop("font-size", "16px"),
                        prop("font-weight", "400"),
                        prop("color", "#2563eb"),
                        prop("background-color", "#ffffff"),
                    ],
                )],
            },
            pseudo: vec![],
            children: vec![],
        };
        apply_contrast_all(std::slice::from_mut(&mut snap));
        let c = snap.contrast.expect("contrast derived");
        assert_eq!(c.aa, TriState::Pass);
        assert_eq!(c.foreground, "#2563eb");
    }

    #[test]
    fn apply_contrast_resolves_transparent_through_ancestors() {
        // Parent paints an opaque white; the child (text) has a transparent
        // background. The old behaviour reported `unknown`; now the child's
        // effective background is resolved to the ancestor's white.
        let child = ElementSnapshot {
            id: 2,
            parent_id: Some(1),
            tag: "P".into(),
            selector: ".card > p".into(),
            path: ".card > p".into(),
            depth: 1,
            rect: None,
            metrics: None,
            noticeable: None,
            aria: None,
            effective_background: None,
            contrast: None,
            ax: None,
            attributes: None,
            styles: ComputedStyles {
                groups: vec![(
                    crate::properties::StyleCategory::Typography,
                    vec![
                        prop("font-size", "16px"),
                        prop("font-weight", "400"),
                        prop("color", "#2563eb"),
                        prop("background-color", "rgba(0, 0, 0, 0)"),
                    ],
                )],
            },
            pseudo: vec![],
            children: vec![],
        };
        let mut parent = ElementSnapshot {
            id: 1,
            parent_id: None,
            tag: "DIV".into(),
            selector: ".card".into(),
            path: ".card".into(),
            depth: 0,
            rect: None,
            metrics: None,
            noticeable: None,
            aria: None,
            effective_background: None,
            contrast: None,
            ax: None,
            attributes: None,
            styles: ComputedStyles {
                groups: vec![(
                    crate::properties::StyleCategory::Typography,
                    vec![
                        prop("font-size", "16px"),
                        prop("font-weight", "400"),
                        prop("color", "#212529"),
                        prop("background-color", "#ffffff"),
                    ],
                )],
            },
            pseudo: vec![],
            children: vec![child],
        };
        apply_contrast_all(std::slice::from_mut(&mut parent));
        let c = parent.children[0]
            .contrast
            .as_ref()
            .expect("child contrast");
        assert_eq!(c.aa, TriState::Pass);
        assert_eq!(c.background, "#ffffff");
        assert_eq!(c.foreground, "#2563eb");
    }

    #[test]
    fn apply_contrast_composites_semitransparent_over_ancestor() {
        // Text sits on a 50% black overlay that itself sits on white: the
        // effective background is mid-gray (~#808080), a real measurable
        // value, not `unknown`.
        let text = ElementSnapshot {
            id: 3,
            parent_id: Some(2),
            tag: "SPAN".into(),
            selector: ".overlay > span".into(),
            path: ".overlay > span".into(),
            depth: 2,
            rect: None,
            metrics: None,
            noticeable: None,
            aria: None,
            effective_background: None,
            contrast: None,
            ax: None,
            attributes: None,
            styles: ComputedStyles {
                groups: vec![(
                    crate::properties::StyleCategory::Typography,
                    vec![
                        prop("font-size", "16px"),
                        prop("font-weight", "400"),
                        prop("color", "#ffffff"),
                        prop("background-color", "rgba(0, 0, 0, 0)"),
                    ],
                )],
            },
            pseudo: vec![],
            children: vec![],
        };
        let overlay = ElementSnapshot {
            id: 2,
            parent_id: Some(1),
            tag: "DIV".into(),
            selector: ".card > .overlay".into(),
            path: ".card > .overlay".into(),
            depth: 1,
            rect: None,
            metrics: None,
            noticeable: None,
            aria: None,
            effective_background: None,
            contrast: None,
            ax: None,
            attributes: None,
            styles: ComputedStyles {
                groups: vec![(
                    crate::properties::StyleCategory::Visual,
                    vec![
                        prop("color", "#000000"),
                        prop("background-color", "rgba(0, 0, 0, 0.5)"),
                        prop("background-image", "none"),
                    ],
                )],
            },
            pseudo: vec![],
            children: vec![text],
        };
        let mut card = ElementSnapshot {
            id: 1,
            parent_id: None,
            tag: "DIV".into(),
            selector: ".card".into(),
            path: ".card".into(),
            depth: 0,
            rect: None,
            metrics: None,
            noticeable: None,
            aria: None,
            effective_background: None,
            contrast: None,
            ax: None,
            attributes: None,
            styles: ComputedStyles {
                groups: vec![(
                    crate::properties::StyleCategory::Visual,
                    vec![
                        prop("color", "#000000"),
                        prop("background-color", "#ffffff"),
                        prop("background-image", "none"),
                    ],
                )],
            },
            pseudo: vec![],
            children: vec![overlay],
        };
        apply_contrast_all(std::slice::from_mut(&mut card));
        let c = card.children[0].children[0]
            .contrast
            .as_ref()
            .expect("child contrast");
        assert!(c.aa != TriState::Unknown, "expected measurable, got {c:?}");
        assert!(c.ratio > 1.0 && c.ratio < 21.0);
    }

    #[test]
    fn apply_contrast_prefers_inpage_effective_background() {
        // The capture root is transparent, but the in-page composited
        // background (climbed by JS over the real DOM) is #f8f9fa. The
        // measured muted text (~4.4:1) is a genuine AA failure.
        let mut snap = ElementSnapshot {
            id: 1,
            parent_id: None,
            tag: "P".into(),
            selector: "main > p".into(),
            path: "main > p".into(),
            depth: 0,
            rect: None,
            metrics: None,
            noticeable: None,
            aria: None,
            contrast: None,
            effective_background: Some("#f8f9fa".into()),
            ax: None,
            attributes: None,
            styles: ComputedStyles {
                groups: vec![(
                    crate::properties::StyleCategory::Typography,
                    vec![
                        prop("font-size", "16px"),
                        prop("font-weight", "400"),
                        prop("color", "#6c757d"),
                        prop("background-color", "rgba(0, 0, 0, 0)"),
                    ],
                )],
            },
            pseudo: vec![],
            children: vec![],
        };
        apply_contrast_all(std::slice::from_mut(&mut snap));
        let c = snap.contrast.expect("contrast derived");
        assert_eq!(c.background, "#f8f9fa");
        assert_eq!(c.aa, TriState::Fail, "4.4:1 muted-on-light must fail AA");
        assert!(c.ratio > 4.0 && c.ratio < 4.5, "ratio {}", c.ratio);
    }

    #[test]
    fn apply_contrast_image_background_propagates_unknown() {
        let child = ElementSnapshot {
            id: 2,
            parent_id: Some(1),
            tag: "P".into(),
            selector: ".hero > p".into(),
            path: ".hero > p".into(),
            depth: 1,
            rect: None,
            metrics: None,
            noticeable: None,
            aria: None,
            effective_background: None,
            contrast: None,
            ax: None,
            attributes: None,
            styles: ComputedStyles {
                groups: vec![(
                    crate::properties::StyleCategory::Typography,
                    vec![
                        prop("font-size", "16px"),
                        prop("font-weight", "400"),
                        prop("color", "#ffffff"),
                        prop("background-color", "rgba(0, 0, 0, 0)"),
                    ],
                )],
            },
            pseudo: vec![],
            children: vec![],
        };
        let mut hero = ElementSnapshot {
            id: 1,
            parent_id: None,
            tag: "DIV".into(),
            selector: ".hero".into(),
            path: ".hero".into(),
            depth: 0,
            rect: None,
            metrics: None,
            noticeable: None,
            aria: None,
            effective_background: None,
            contrast: None,
            ax: None,
            attributes: None,
            styles: ComputedStyles {
                groups: vec![(
                    crate::properties::StyleCategory::Visual,
                    vec![
                        prop("color", "#000000"),
                        prop("background-color", "#223d73"),
                        prop("background-image", "url(hero.png)"),
                    ],
                )],
            },
            pseudo: vec![],
            children: vec![child],
        };
        apply_contrast_all(std::slice::from_mut(&mut hero));
        let c = hero.children[0].contrast.as_ref().expect("child contrast");
        assert_eq!(c.aa, TriState::Unknown);
        assert_eq!(c.unknown_reason.as_deref(), Some("background image"));
    }
}
