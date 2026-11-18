//! Parser for the `__env` facet emitted by the in-page extractor.
//!
//! The conversion is pure: the same input always produces the same
//! [`sniff_core::modern::EnvFacet`]. Media preferences are validated against
//! closed value sets and every canonical key is always present in the output,
//! so consumers never need to distinguish "absent" from "default".

use std::collections::BTreeMap;

/// `(chave, valores aceitos, valor padrão)` de uma preferência normalizada.
type MediaSpec = (&'static str, &'static [&'static str], &'static str);
/// Canonical media keys, their accepted values and the value used when the
/// extractor omits the key or reports something outside the closed set.
const MEDIA_SPECS: &[MediaSpec] = &[
    (
        "prefers-color-scheme",
        &["dark", "light", "no-preference"],
        "no-preference",
    ),
    (
        "prefers-reduced-motion",
        &["reduce", "no-preference"],
        "no-preference",
    ),
    (
        "prefers-contrast",
        &["more", "less", "custom", "no-preference"],
        "no-preference",
    ),
    ("forced-colors", &["active", "none"], "none"),
    ("pointer", &["fine", "coarse", "none"], "none"),
    ("hover", &["hover", "none"], "none"),
    ("any-pointer", &["fine", "coarse", "none"], "none"),
    ("any-hover", &["hover", "none"], "none"),
];

/// Parse the `__env` facet emitted by the in-page extractor.
pub fn parse(value: &serde_json::Value) -> sniff_core::modern::EnvFacet {
    let source_media = value.get("media").and_then(serde_json::Value::as_object);
    let mut media = BTreeMap::new();
    for &(key, accepted, default) in MEDIA_SPECS {
        let canonical = source_media
            .and_then(|obj| obj.get(key))
            .and_then(serde_json::Value::as_str)
            .filter(|value| accepted.contains(value))
            .unwrap_or(default);
        media.insert(key.to_owned(), canonical.to_owned());
    }

    let mut emulated = BTreeMap::new();
    if let Some(source) = value.get("emulated").and_then(serde_json::Value::as_object) {
        for (key, entry) in source {
            if let Some(text) = entry.as_str() {
                emulated.insert(key.clone(), text.to_owned());
            }
        }
    }

    let device = parse_device(value.get("device"));

    tracing::debug!(
        media_keys = media.len(),
        emulated_keys = emulated.len(),
        has_device = device.is_some(),
        "parsed __env facet"
    );

    sniff_core::modern::EnvFacet {
        media,
        emulated,
        device,
    }
}

/// Accept a JSON number or a numeric string as a finite `f64`.
fn as_number(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Normalize the device pixel ratio: two decimal places, never negative.
fn parse_dpr(value: Option<&serde_json::Value>) -> Option<f64> {
    let raw = as_number(value?)?;
    if !raw.is_finite() {
        return None;
    }
    let rounded = (raw * 100.0).round() / 100.0;
    if !rounded.is_finite() {
        return None;
    }
    Some(rounded.max(0.0))
}

/// Normalize a viewport dimension: a positive integer clamped to `u32`.
fn parse_dimension(value: Option<&serde_json::Value>) -> Option<u32> {
    let raw = as_number(value?)?;
    if !raw.is_finite() || raw <= 0.0 || raw.fract().abs() > f64::EPSILON {
        return None;
    }
    if raw > f64::from(u32::MAX) {
        return Some(u32::MAX);
    }
    // O valor já foi validado como inteiro: parsear a forma decimal evita
    // casts silenciosos de f64 para u32.
    raw.to_string().parse::<u32>().ok()
}

/// Build a device facet only when all three fields are valid.
fn parse_device(value: Option<&serde_json::Value>) -> Option<sniff_core::modern::DeviceFacet> {
    let object = value?.as_object()?;
    let dpr = parse_dpr(object.get("dpr"))?;
    let width = parse_dimension(object.get("width"))?;
    let height = parse_dimension(object.get("height"))?;
    Some(sniff_core::modern::DeviceFacet { dpr, width, height })
}

#[cfg(test)]
mod tests {
    use super::{MEDIA_SPECS, parse};
    use serde_json::json;

    #[test]
    fn empty_object_yields_all_closed_keys_with_defaults() {
        let facet = parse(&json!({}));
        assert_eq!(facet.media.len(), 8);
        for &(key, _accepted, default) in MEDIA_SPECS {
            assert_eq!(facet.media.get(key).map(String::as_str), Some(default));
        }
        assert!(facet.emulated.is_empty());
        assert_eq!(facet.device, None);
    }

    #[test]
    fn every_canonical_media_value_round_trips() {
        for &(key, accepted, _default) in MEDIA_SPECS {
            for &value in accepted {
                let mut input = json!({});
                input["media"] = json!({});
                input["media"][key] = json!(value);
                let facet = parse(&input);
                assert_eq!(
                    facet.media.get(key).map(String::as_str),
                    Some(value),
                    "key {key}, value {value}"
                );
            }
        }
    }

    #[test]
    fn invalid_and_unknown_media_values_fall_back_to_defaults() {
        for &(key, _accepted, default) in MEDIA_SPECS {
            for bad in [json!("not-a-value"), json!(42), json!(null)] {
                let mut input = json!({});
                input["media"] = json!({});
                input["media"][key] = bad;
                let facet = parse(&input);
                assert_eq!(
                    facet.media.get(key).map(String::as_str),
                    Some(default),
                    "key {key}"
                );
            }
        }

        // A value canonical for a different key is still out of this key's set.
        let facet = parse(&json!({"media": {"prefers-color-scheme": "coarse"}}));
        assert_eq!(facet.media["prefers-color-scheme"], "no-preference");
    }

    #[test]
    fn unknown_media_keys_are_ignored() {
        let facet = parse(&json!({
            "media": {"unknown-key": "x", "prefers-color-scheme": "dark"}
        }));
        assert_eq!(facet.media.len(), 8);
        assert!(!facet.media.contains_key("unknown-key"));
        assert_eq!(facet.media["prefers-color-scheme"], "dark");
    }

    #[test]
    fn emulated_is_sorted_and_skips_non_strings() {
        let facet = parse(&json!({
            "emulated": {"zeta": "z", "alpha": "a", "count": 3, "nil": null}
        }));
        assert_eq!(
            facet
                .emulated
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["alpha", "zeta"]
        );
        assert_eq!(facet.emulated["alpha"], "a");
        assert_eq!(facet.emulated["zeta"], "z");
    }

    #[test]
    fn device_normalizes_numbers_and_numeric_strings() {
        let facet = parse(&json!({
            "device": {"dpr": 1.256, "width": "360", "height": 768}
        }));
        let device = facet.device.expect("valid device");
        assert!((device.dpr - 1.26).abs() < 1e-9);
        assert_eq!(device.width, 360);
        assert_eq!(device.height, 768);
    }

    #[test]
    fn device_rounds_and_clamps() {
        let facet = parse(&json!({
            "device": {"dpr": -2.5, "width": 5_000_000_000u64, "height": "768"}
        }));
        let device = facet.device.expect("valid device");
        assert_eq!(device.dpr, 0.0);
        assert_eq!(device.width, u32::MAX);
        assert_eq!(device.height, 768);
    }

    #[test]
    fn device_rejects_incomplete_or_malformed_fields() {
        for bad in [
            json!({}),
            json!({"dpr": 2, "width": 0, "height": 10}),
            json!({"dpr": 2, "width": -1, "height": 10}),
            json!({"dpr": 2, "width": 10.5, "height": 10}),
            json!({"dpr": "not-a-number", "width": 10, "height": 10}),
            json!({"dpr": 2, "width": 10}),
            json!({"dpr": "NaN", "width": 10, "height": 10}),
            json!({"dpr": "inf", "width": 10, "height": 10}),
        ] {
            assert_eq!(parse(&json!({"device": bad})).device, None);
        }
    }

    #[test]
    fn malformed_input_never_panics() {
        let inputs = [
            json!(null),
            json!(42),
            json!("text"),
            json!([]),
            json!({"media": []}),
            json!({"emulated": 7}),
            json!({"device": "wide"}),
            json!({
                "media": {"prefers-color-scheme": {"unexpected": true}},
                "emulated": [],
                "device": {"dpr": [2]}
            }),
        ];
        for input in &inputs {
            let facet = parse(input);
            assert_eq!(facet.media.len(), 8);
            assert!(facet.emulated.is_empty());
            assert_eq!(facet.device, None);
        }
    }

    #[test]
    fn repeated_parses_are_identical() {
        let input = json!({
            "media": {"pointer": "coarse", "hover": "none"},
            "emulated": {"prefers-reduced-motion": "reduce"},
            "device": {"dpr": "1.5", "width": 360, "height": 640}
        });
        assert_eq!(parse(&input), parse(&input));
    }
}
