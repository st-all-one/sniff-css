//! Flutter color parsing and normalization.
//!
//! Flutter diagnostics serialize `Color` values in a few shapes across
//! versions: `Color(0xff2563eb)`, `Color.fromARGB(255, 37, 99, 235)`,
//! `Color.fromRGBO(...)` and — since Flutter 3.40+ — the explicit
//! `Color(alpha: 1.0000, red: 1.0000, green: 1.0000, blue: 1.0000, colorSpace:
//! ColorSpace.sRGB)`. The web contrast machinery
//! (`sniff_core::contrast::parse_color`) only understands `#rrggbb` /
//! `#rrggbbaa` / `rgb()`. This module normalizes Flutter color strings into
//! `#rrggbb` (opaque) / `#rrggbbaa` (semi-transparent) so the same contrast
//! derivation works for both backends. When the inspector exposes 0-255
//! channels via `valueProperties`, those are preferred (exact, no parsing).

/// Parse a Flutter `Color(...)` diagnostics string into `#rrggbb` /
/// `#rrggbbaa`. Returns `None` for anything unparseable.
pub fn parse_flutter_color(input: &str) -> Option<String> {
    let s = input.trim();
    if let Some(hex) = s.strip_prefix("Color(0x").and_then(|r| r.strip_suffix(')')) {
        let hex = hex.trim();
        let bytes = u32::from_str_radix(hex, 16).ok()?;
        // 0xAARRGGBB (8 hex digits) or 0xRRGGBB (6 hex digits).
        if hex.len() == 8 {
            return Some(color_from_argb(bytes));
        }
        if hex.len() == 6 {
            let rgb = bytes & 0x00FF_FFFF;
            return Some(format!("#{:06x}", rgb));
        }
        return None;
    }
    if let Some(rest) = s
        .strip_prefix("Color.fromARGB(")
        .and_then(|r| r.strip_suffix(')'))
    {
        return from_argb_args(rest);
    }
    if let Some(rest) = s
        .strip_prefix("Color.fromRGBO(")
        .and_then(|r| r.strip_suffix(')'))
    {
        return from_rgbo_args(rest);
    }
    if let Some(rest) = s.strip_prefix("Color(").and_then(|r| r.strip_suffix(')'))
        && let Some(out) = from_named_channels(rest)
    {
        return Some(out);
    }
    None
}

/// Normalize a color property from its raw diagnostic fields: prefer the
/// inspector's `valueProperties` 0-255 channels, then fall back to parsing the
/// `description` string. This mirrors what the real VM Service reports for
/// `ColorProperty` entries.
pub fn parse_color_value(
    value_properties: Option<&serde_json::Map<String, serde_json::Value>>,
    description: &str,
) -> Option<String> {
    if let Some(vp) = value_properties {
        let ch = |k: &str| vp.get(k).and_then(serde_json::Value::as_u64);
        if let (Some(r), Some(g), Some(b)) = (ch("red"), ch("green"), ch("blue")) {
            let a = ch("alpha").unwrap_or(255);
            return Some(color_from_argb(
                ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32,
            ));
        }
    }
    parse_flutter_color(description)
}

/// `Color(alpha: 1.0, red: 1.0, green: 1.0, blue: 1.0, colorSpace: ...)` —
/// 0.0-1.0 channels in any order.
fn from_named_channels(rest: &str) -> Option<String> {
    let mut alpha = 1.0f64;
    let mut red = 0.0f64;
    let mut green = 0.0f64;
    let mut blue = 0.0f64;
    for part in rest.split(',') {
        let part = part.trim();
        let (name, value) = part.split_once(':')?;
        if name == "colorSpace" {
            continue;
        }
        let parsed = value.trim().parse::<f64>().ok()?;
        match name.trim() {
            "alpha" => alpha = parsed,
            "red" => red = parsed,
            "green" => green = parsed,
            "blue" => blue = parsed,
            _ => {}
        }
    }
    let to = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    Some(color_from_argb(
        (to(alpha) << 24) | (to(red) << 16) | (to(green) << 8) | to(blue),
    ))
}

/// Format a 32-bit ARGB int as `#rrggbb` (opaque) or `#rrggbbaa`.
fn color_from_argb(argb: u32) -> String {
    let a = (argb >> 24) & 0xFF;
    let rgb = argb & 0x00FF_FFFF;
    if a == 0xFF {
        format!("#{rgb:06x}")
    } else {
        format!("#{rgb:06x}{a:02x}")
    }
}

/// `Color.fromARGB(a, r, g, b)` with 0-255 channels.
fn from_argb_args(rest: &str) -> Option<String> {
    let mut it = rest.split(',').map(str::trim);
    let a = it.next()?.parse::<u32>().ok()?;
    let r = it.next()?.parse::<u32>().ok()?;
    let g = it.next()?.parse::<u32>().ok()?;
    let b = it.next()?.parse::<u32>().ok()?;
    Some(color_from_argb((a << 24) | (r << 16) | (g << 8) | b))
}

/// `Color.fromRGBO(r, g, b, opacity)` with 0-255 channels and 0.0-1.0 opacity.
fn from_rgbo_args(rest: &str) -> Option<String> {
    let mut it = rest.split(',').map(str::trim);
    let r = it.next()?.parse::<u32>().ok()?;
    let g = it.next()?.parse::<u32>().ok()?;
    let b = it.next()?.parse::<u32>().ok()?;
    let opacity = it.next()?.parse::<f64>().ok()?;
    let a = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
    Some(color_from_argb((a << 24) | (r << 16) | (g << 8) | b))
}

/// Property names that carry a color value.
pub fn is_color_property(name: &str) -> bool {
    matches!(
        name,
        "color"
            | "backgroundColor"
            | "background"
            | "foregroundColor"
            | "shadowColor"
            | "borderColor"
            | "iconColor"
            | "surfaceTintColor"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_constructor() {
        assert_eq!(
            parse_flutter_color("Color(0xff2563eb)"),
            Some("#2563eb".into())
        );
        assert_eq!(
            parse_flutter_color("Color(0xFF2563EB)"),
            Some("#2563eb".into())
        );
        assert_eq!(
            parse_flutter_color("Color(0x80ff0000)"),
            Some("#ff000080".into())
        );
    }

    #[test]
    fn parses_argb_and_rgbo() {
        assert_eq!(
            parse_flutter_color("Color.fromARGB(255, 37, 99, 235)"),
            Some("#2563eb".into())
        );
        assert_eq!(
            parse_flutter_color("Color.fromRGBO(37, 99, 235, 1.0)"),
            Some("#2563eb".into())
        );
        assert_eq!(
            parse_flutter_color("Color.fromRGBO(0, 0, 0, 0.5)"),
            Some("#00000080".into())
        );
    }

    #[test]
    fn parses_named_channel_constructor() {
        assert_eq!(
            parse_flutter_color(
                "Color(alpha: 1.0000, red: 1.0000, green: 1.0000, blue: 1.0000, colorSpace: ColorSpace.sRGB)"
            ),
            Some("#ffffff".into())
        );
        assert_eq!(
            parse_flutter_color(
                "Color(alpha: 1.0000, red: 0.1451, green: 0.3882, blue: 0.9216, colorSpace: ColorSpace.sRGB)"
            ),
            Some("#2563eb".into())
        );
        assert_eq!(
            parse_flutter_color(
                "Color(alpha: 0.5019, red: 0.0, green: 0.0, blue: 1.0, colorSpace: ColorSpace.extendedSRGB)"
            ),
            Some("#0000ff80".into())
        );
    }

    #[test]
    fn value_properties_are_preferred() {
        use serde_json::{Map, json};
        let vp: Map<String, serde_json::Value> =
            json!({"red": 255, "green": 255, "blue": 255, "alpha": 255})
                .as_object()
                .unwrap()
                .clone();
        assert_eq!(
            parse_color_value(
                Some(&vp),
                "Color(alpha: 1.0000, red: 1.0000, green: 1.0000, blue: 1.0000, colorSpace: ColorSpace.sRGB)"
            ),
            Some("#ffffff".into())
        );
        let opaque: Map<String, serde_json::Value> = json!({"red": 37, "green": 99, "blue": 235})
            .as_object()
            .unwrap()
            .clone();
        assert_eq!(parse_color_value(Some(&opaque), ""), Some("#2563eb".into()));
        assert_eq!(
            parse_color_value(None, "Color(0xff2563eb)"),
            Some("#2563eb".into())
        );
    }

    #[test]
    fn rejects_non_colors() {
        assert!(parse_flutter_color("TextAlign.start").is_none());
        assert!(parse_flutter_color("EdgeInsets.all(8.0)").is_none());
        assert!(parse_flutter_color("").is_none());
    }

    #[test]
    fn color_property_detection() {
        assert!(is_color_property("color"));
        assert!(is_color_property("backgroundColor"));
        assert!(!is_color_property("fontSize"));
        assert!(!is_color_property("alignment"));
    }
}
