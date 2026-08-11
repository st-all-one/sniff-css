//! Deterministic WCAG contrast computation.
//!
//! Pure math: parses CSS colors, computes relative luminance and the
//! contrast ratio, and classifies AA/AAA compliance for normal vs. large
//! text. Transparent/gradient/image backgrounds are reported as
//! [`TriState::Unknown`] instead of guessing — honesty over false precision.

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

/// Attach a contrast facet to a snapshot in place, reading the already
/// captured `color`, `background-color`, `background-image`, `font-size`
/// and `font-weight` properties.
pub fn apply_contrast(snap: &mut ElementSnapshot) {
    let info = derive_contrast_values(
        snap.styles.get("color"),
        snap.styles.get("background-color"),
        snap.styles.get("background-image"),
        snap.styles.get("font-size"),
        snap.styles.get("font-weight"),
    );
    snap.contrast = info;
}

/// Apply contrast derivation to every node in a snapshot forest.
pub fn apply_contrast_all(snaps: &mut [ElementSnapshot]) {
    for snap in snaps {
        apply_contrast(snap);
        apply_contrast_all(&mut snap.children);
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
            is_visible: Some(true),
            aria: None,
            contrast: None,
            ax: None,
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
        apply_contrast(&mut snap);
        let c = snap.contrast.expect("contrast derived");
        assert_eq!(c.aa, TriState::Pass);
        assert_eq!(c.foreground, "#2563eb");
    }
}
