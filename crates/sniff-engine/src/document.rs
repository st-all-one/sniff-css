//! Parser for the `__document` facet emitted by the in-page extractor.
//!
//! The extractor emits one JSON object per capture with the document-wide
//! context: `<title>`, `lang`, doctype, metatags, `<link>`/`<script>`/stylesheet
//! inventories and `window.isSecureContext`. This module turns that raw JSON
//! into the shared [`DocumentFacet`] type.
//!
//! The parser is pure and deterministic: it reads no clock, environment or
//! randomness, preserves DOM/array order, sorts metatags by key via
//! [`BTreeMap`], and falls back to defaults for missing or malformed fields
//! instead of panicking.

use serde_json::Value;
use sniff_core::modern::{DocumentFacet, LinkInfo, ScriptInfo, StyleSheetInfo};

use std::collections::BTreeMap;

/// Parse the `__document` facet emitted by the in-page extractor.
///
/// Missing, null or wrong-typed fields fall back to empty/default values, so
/// an incomplete or partially malformed extractor payload never panics.
#[must_use]
pub fn parse(value: &Value) -> DocumentFacet {
    let link_count = value
        .get("links")
        .and_then(|links| links.as_array())
        .map_or(0, Vec::len);
    tracing::debug!(
        has_title = value.get("title").is_some(),
        links = link_count,
        "parsed __document facet"
    );

    let meta = parse_meta(value);
    let links = parse_array(value, "links", |item| LinkInfo {
        rel: string_or_none(item, "rel").unwrap_or_default(),
        href: string_or_none(item, "href"),
        as_: string_or_none(item, "as"),
        fetchpriority: string_or_none(item, "fetchpriority"),
        integrity: string_or_none(item, "integrity"),
        crossorigin: string_or_none(item, "crossorigin"),
    });
    let scripts = parse_array(value, "scripts", |item| ScriptInfo {
        src: string_or_none(item, "src"),
        type_: string_or_none(item, "type"),
        defer: bool_or_false(item, "defer"),
        async_: bool_or_false(item, "async"),
        fetchpriority: string_or_none(item, "fetchpriority"),
        integrity: string_or_none(item, "integrity"),
        blocking: string_or_none(item, "blocking"),
        in_head: bool_or_none(item, "in_head"),
        text: string_or_none(item, "text"),
        importance: string_or_none(item, "importance"),
    });
    let style_sheets = parse_array(value, "style_sheets", |item| StyleSheetInfo {
        href: string_or_none(item, "href"),
        media: media_or_all(item),
        blocked: bool_or_false(item, "blocked"),
    });

    DocumentFacet {
        title: string_or_none(value, "title"),
        lang: string_or_none(value, "lang"),
        origin: string_or_none(value, "origin"),
        doctype: string_or_none(value, "doctype"),
        meta,
        links,
        scripts,
        style_sheets,
        is_secure_context: value.get("is_secure_context").and_then(Value::as_bool),
    }
}

/// Read a string-or-null key; missing/null/wrong-typed values become `None`.
fn string_or_none(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Read a boolean key; missing/null/wrong-typed values become `false`.
fn bool_or_false(value: &Value, key: &str) -> bool {
    matches!(value.get(key), Some(Value::Bool(true)))
}

/// Read a boolean-or-null key; missing/null/wrong-typed values become `None`.
fn bool_or_none(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

/// Read `media`, defaulting to `"all"` when missing, null, empty or non-string.
fn media_or_all(value: &Value) -> String {
    value
        .get("media")
        .and_then(Value::as_str)
        .filter(|media| !media.is_empty())
        .map_or_else(|| "all".to_owned(), str::to_owned)
}

/// Collect string-valued entries into a key-sorted map; skip everything else.
fn parse_meta(value: &Value) -> BTreeMap<String, String> {
    value
        .get("meta")
        .and_then(Value::as_object)
        .map_or_else(BTreeMap::new, |obj| {
            obj.iter()
                .filter_map(|(key, entry)| {
                    entry.as_str().map(|text| (key.clone(), text.to_owned()))
                })
                .collect()
        })
}

/// Parse an array in input order; a missing/wrong-typed field yields an empty
/// vector. Each element is passed to `build` even when it is not an object,
/// letting the field helpers fall back to defaults.
fn parse_array<T>(value: &Value, key: &str, build: impl Fn(&Value) -> T) -> Vec<T> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |items| items.iter().map(build).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_full_document() {
        let value = json!({
            "title": "Dashboard | Exemplo",
            "lang": "pt-BR",
            "doctype": "html",
            "origin": "https://site.example",
            "meta": {"charset": "utf-8", "viewport": "width=device-width"},
            "links": [{
                "rel": "preload",
                "href": "/hero.avif",
                "as": "image",
                "fetchpriority": "high",
                "integrity": null,
                "crossorigin": "anonymous"
            }],
            "scripts": [{
                "src": "/app.js",
                "type": "module",
                "defer": true,
                "async": false,
                "fetchpriority": null,
                "integrity": null,
                "blocking": "render",
                "in_head": true,
                "text": "window.__x=1",
                "importance": "high"
            }],
            "style_sheets": [{"href": "/app.css", "media": "all", "blocked": false}],
            "is_secure_context": true
        });

        let expected = DocumentFacet {
            title: Some("Dashboard | Exemplo".to_owned()),
            lang: Some("pt-BR".to_owned()),
            origin: Some("https://site.example".to_owned()),
            doctype: Some("html".to_owned()),
            meta: BTreeMap::from([
                ("charset".to_owned(), "utf-8".to_owned()),
                ("viewport".to_owned(), "width=device-width".to_owned()),
            ]),
            links: vec![LinkInfo {
                rel: "preload".to_owned(),
                href: Some("/hero.avif".to_owned()),
                as_: Some("image".to_owned()),
                fetchpriority: Some("high".to_owned()),
                integrity: None,
                crossorigin: Some("anonymous".to_owned()),
            }],
            scripts: vec![ScriptInfo {
                src: Some("/app.js".to_owned()),
                type_: Some("module".to_owned()),
                defer: true,
                async_: false,
                fetchpriority: None,
                integrity: None,
                blocking: Some("render".to_owned()),
                in_head: Some(true),
                text: Some("window.__x=1".to_owned()),
                importance: Some("high".to_owned()),
            }],
            style_sheets: vec![StyleSheetInfo {
                href: Some("/app.css".to_owned()),
                media: "all".to_owned(),
                blocked: false,
            }],
            is_secure_context: Some(true),
        };

        assert_eq!(parse(&value), expected);
    }

    #[test]
    fn empty_object_yields_default() {
        assert_eq!(parse(&json!({})), DocumentFacet::default());
    }

    #[test]
    fn meta_keys_are_sorted_regardless_of_insertion_order() {
        let first = json!({"meta": {"zeta": "3", "alpha": "1", "mid": "2"}});
        let second = json!({"meta": {"mid": "2", "zeta": "3", "alpha": "1"}});
        let expected = BTreeMap::from([
            ("alpha".to_owned(), "1".to_owned()),
            ("mid".to_owned(), "2".to_owned()),
            ("zeta".to_owned(), "3".to_owned()),
        ]);

        assert_eq!(parse(&first).meta, expected);
        assert_eq!(parse(&second).meta, expected);
        assert_eq!(parse(&first), parse(&second));
    }

    #[test]
    fn blocked_cross_origin_stylesheet_is_preserved() {
        let value = json!({
            "style_sheets": [{
                "href": "https://cdn.example.com/app.css",
                "media": "print",
                "blocked": true
            }]
        });

        assert_eq!(
            parse(&value).style_sheets,
            vec![StyleSheetInfo {
                href: Some("https://cdn.example.com/app.css".to_owned()),
                media: "print".to_owned(),
                blocked: true,
            }]
        );
    }

    #[test]
    fn bare_style_sheet_with_null_href_defaults_media_to_all() {
        let with_empty_media = json!({
            "style_sheets": [{"href": null, "media": "", "blocked": false}]
        });
        let with_missing_media = json!({
            "style_sheets": [{"href": null, "blocked": false}]
        });
        let expected = vec![StyleSheetInfo {
            href: None,
            media: "all".to_owned(),
            blocked: false,
        }];

        assert_eq!(parse(&with_empty_media).style_sheets, expected);
        assert_eq!(parse(&with_missing_media).style_sheets, expected);
    }

    #[test]
    fn null_optional_fields_and_unknown_keys_are_handled() {
        let value = json!({
            "links": [{
                "rel": "stylesheet",
                "href": null,
                "as": null,
                "fetchpriority": null,
                "integrity": null,
                "unknown": "ignored"
            }],
            "scripts": [{
                "src": null,
                "type": null,
                "defer": false,
                "async": true,
                "fetchpriority": null,
                "integrity": null,
                "blocking": null,
                "in_head": null,
                "text": null,
                "importance": null,
                "unknown": 42
            }]
        });

        let facet = parse(&value);
        assert_eq!(
            facet.links,
            vec![LinkInfo {
                rel: "stylesheet".to_owned(),
                href: None,
                as_: None,
                fetchpriority: None,
                integrity: None,
                crossorigin: None,
            }]
        );
        assert_eq!(
            facet.scripts,
            vec![ScriptInfo {
                src: None,
                type_: None,
                defer: false,
                async_: true,
                fetchpriority: None,
                integrity: None,
                blocking: None,
                in_head: None,
                text: None,
                importance: None,
            }]
        );
    }

    #[test]
    fn malformed_scalar_types_fall_back_without_panicking() {
        let value = json!({
            "title": 42,
            "lang": false,
            "doctype": ["html"],
            "meta": {"ok": "yes", "bad": 7, "nil": null},
            "links": "oops",
            "scripts": {"src": "/x.js"},
            "style_sheets": [42, "bad", null, {"href": "/ok.css"}],
            "is_secure_context": "yes"
        });

        let expected = DocumentFacet {
            title: None,
            lang: None,
            origin: None,
            doctype: None,
            meta: BTreeMap::from([("ok".to_owned(), "yes".to_owned())]),
            links: Vec::new(),
            scripts: Vec::new(),
            style_sheets: vec![
                StyleSheetInfo {
                    href: None,
                    media: "all".to_owned(),
                    blocked: false,
                },
                StyleSheetInfo {
                    href: None,
                    media: "all".to_owned(),
                    blocked: false,
                },
                StyleSheetInfo {
                    href: None,
                    media: "all".to_owned(),
                    blocked: false,
                },
                StyleSheetInfo {
                    href: Some("/ok.css".to_owned()),
                    media: "all".to_owned(),
                    blocked: false,
                },
            ],
            is_secure_context: None,
        };

        assert_eq!(parse(&value), expected);
    }
}
