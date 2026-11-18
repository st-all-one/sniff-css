//! Parser puro da faceta `__declared` (CSSOM declarado) para o contrato v2.
//!
//! A conversão é determinística: regras são ordenadas e deduplicadas por
//! `(href, index, selector)`, enquanto `blocked_sheets` e `at_rules` mantêm a
//! ordem original de DOM/folha. Nenhum campo ausente ou malformado causa pânico.

use serde_json::Value;
use sniff_core::modern::{AtRuleInfo, DeclaredFacet, DeclaredRule};
use std::collections::BTreeMap;

/// Parse the `__declared` facet emitted by the in-page extractor.
pub fn parse(value: &Value) -> DeclaredFacet {
    let rules = parse_rules(value.get("rules"));
    let blocked_sheets = value
        .get("blocked_sheets")
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        });
    let at_rules = value
        .get("at_rules")
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |entries| {
            entries.iter().map(parse_at_rule).collect()
        });
    let truncated = value
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    tracing::debug!(
        rules = rules.len(),
        blocked = blocked_sheets.len(),
        at_rules = at_rules.len(),
        truncated,
        "parsed __declared facet"
    );

    DeclaredFacet {
        rules,
        blocked_sheets,
        at_rules,
        truncated,
    }
}

/// Parse a `rules` array (also used for rules nested inside at-rules).
fn parse_rules(value: Option<&Value>) -> Vec<DeclaredRule> {
    let Some(entries) = value.and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut rules: Vec<DeclaredRule> = entries.iter().filter_map(parse_rule).collect();
    rules.sort_by(|left, right| {
        let left_key = (
            left.href.as_deref().unwrap_or(""),
            left.index,
            left.selector.as_str(),
        );
        let right_key = (
            right.href.as_deref().unwrap_or(""),
            right.index,
            right.selector.as_str(),
        );
        left_key.cmp(&right_key)
    });
    // A ordenação acima é estável e usa a mesma chave da deduplicação, de modo
    // que duplicatas ficam adjacentes e `dedup_by` preserva a primeira.
    rules.dedup_by(|left, right| {
        left.href == right.href && left.index == right.index && left.selector == right.selector
    });
    rules
}

/// Parse a single rule, skipping entries without a textual selector.
fn parse_rule(value: &Value) -> Option<DeclaredRule> {
    let selector = value.get("selector")?.as_str()?;
    let declarations = value
        .get("declarations")
        .and_then(Value::as_object)
        .map_or_else(BTreeMap::new, |object| {
            object
                .iter()
                .filter_map(|(name, value)| {
                    value.as_str().map(|text| (name.clone(), text.to_string()))
                })
                .collect()
        });

    Some(DeclaredRule {
        selector: selector.to_string(),
        href: value
            .get("href")
            .and_then(Value::as_str)
            .map(str::to_string),
        index: parse_index(value),
        origin: value
            .get("origin")
            .and_then(Value::as_str)
            .unwrap_or("author")
            .to_string(),
        declarations,
    })
}

/// Parse a single at-rule preamble, accepting the legacy `conditionText` key.
fn parse_at_rule(value: &Value) -> AtRuleInfo {
    AtRuleInfo {
        condition_text: value
            .get("condition_text")
            .and_then(Value::as_str)
            .or_else(|| value.get("conditionText").and_then(Value::as_str))
            .unwrap_or_default()
            .to_string(),
        index: parse_index(value),
        rules: parse_rules(value.get("rules")),
    }
}

/// Read an unsigned `index`, falling back to zero for missing or invalid values.
fn parse_index(value: &Value) -> usize {
    value
        .get("index")
        .and_then(Value::as_u64)
        .map_or(0, |index| usize::try_from(index).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::json;
    use sniff_core::modern::DeclaredFacet;

    #[test]
    fn parses_full_facet_with_author_and_inline_rules() {
        let value = json!({
            "rules": [
                {
                    "selector": "html",
                    "href": "/app.css",
                    "index": 12,
                    "origin": "author",
                    "declarations": {
                        "z-index": "1",
                        "font-size": "16px",
                        "color": "red"
                    }
                },
                {
                    "selector": ":root",
                    "href": null,
                    "index": 0,
                    "origin": "inline",
                    "declarations": {"color": "blue"}
                }
            ],
            "blocked_sheets": [
                "https://cdn.outro/theme.css: SecurityError: blocked by CORS",
                "/local.css"
            ],
            "at_rules": [
                {
                    "condition_text": "@supports (anchor-name: --x)",
                    "index": 3,
                    "rules": []
                }
            ],
            "truncated": true
        });

        let facet = parse(&value);

        assert_eq!(
            facet
                .rules
                .iter()
                .map(|rule| (
                    rule.selector.as_str(),
                    rule.href.as_deref(),
                    rule.origin.as_str()
                ))
                .collect::<Vec<_>>(),
            [
                (":root", None, "inline"),
                ("html", Some("/app.css"), "author")
            ]
        );
        let author = facet
            .rules
            .iter()
            .find(|rule| rule.selector == "html")
            .expect("author rule present");
        assert_eq!(author.index, 12);
        assert_eq!(
            author
                .declarations
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["color", "font-size", "z-index"]
        );
        assert_eq!(
            author.declarations.get("color").map(String::as_str),
            Some("red")
        );
        assert_eq!(
            author.declarations.get("font-size").map(String::as_str),
            Some("16px")
        );

        assert_eq!(
            facet.blocked_sheets,
            vec![
                String::from("https://cdn.outro/theme.css: SecurityError: blocked by CORS"),
                String::from("/local.css"),
            ]
        );
        assert_eq!(facet.at_rules.len(), 1);
        assert_eq!(
            facet.at_rules[0].condition_text,
            "@supports (anchor-name: --x)"
        );
        assert_eq!(facet.at_rules[0].index, 3);
        assert!(facet.truncated);
    }

    #[test]
    fn accepts_legacy_condition_text_and_prefers_preferred_key() {
        let value = json!({
            "at_rules": [
                {
                    "conditionText": "@media (width: 10px)",
                    "index": 7,
                    "rules": [
                        {"selector": "b", "href": "b.css", "index": 1, "declarations": {}},
                        {"selector": "a", "href": "a.css", "index": 1, "declarations": {}}
                    ]
                },
                {"condition_text": "@container (min-width: 1px)", "index": 8, "rules": []},
                {"condition_text": 42, "conditionText": "@supports (display: grid)", "rules": []},
                {"condition_text": "preferred", "conditionText": "legacy", "rules": []}
            ]
        });

        let facet = parse(&value);

        assert_eq!(
            facet
                .at_rules
                .iter()
                .map(|rule| rule.condition_text.as_str())
                .collect::<Vec<_>>(),
            [
                "@media (width: 10px)",
                "@container (min-width: 1px)",
                "@supports (display: grid)",
                "preferred"
            ]
        );
        assert_eq!(facet.at_rules[0].index, 7);
        assert_eq!(
            facet.at_rules[0]
                .rules
                .iter()
                .map(|rule| rule.selector.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn rule_order_is_deterministic_regardless_of_input_order() {
        let shuffled = json!({
            "rules": [
                {"selector": "c", "href": "z.css", "index": 2, "declarations": {}},
                {"selector": "a", "href": null, "index": 5, "declarations": {}},
                {"selector": "b", "href": "a.css", "index": 9, "declarations": {}}
            ]
        });
        let reordered = json!({
            "rules": [
                {"selector": "b", "href": "a.css", "index": 9, "declarations": {}},
                {"selector": "c", "href": "z.css", "index": 2, "declarations": {}},
                {"selector": "a", "href": null, "index": 5, "declarations": {}}
            ]
        });

        let first = parse(&shuffled);
        let second = parse(&reordered);

        assert_eq!(first.rules, second.rules);
        assert_eq!(
            first
                .rules
                .iter()
                .map(|rule| rule.selector.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn dedupes_rules_keeping_the_first_occurrence() {
        let value = json!({
            "rules": [
                {
                    "selector": "p",
                    "href": "s.css",
                    "index": 4,
                    "origin": "inline",
                    "declarations": {"color": "red"}
                },
                {
                    "selector": "p",
                    "href": "s.css",
                    "index": 4,
                    "origin": "author",
                    "declarations": {"color": "blue"}
                },
                {"selector": "div", "href": "s.css", "index": 4, "declarations": {}}
            ]
        });

        let facet = parse(&value);

        assert_eq!(facet.rules.len(), 2);
        let kept = facet
            .rules
            .iter()
            .find(|rule| rule.selector == "p")
            .expect("duplicated rule present");
        assert_eq!(kept.origin, "inline");
        assert_eq!(
            kept.declarations.get("color").map(String::as_str),
            Some("red")
        );
    }

    #[test]
    fn tolerates_malformed_entries() {
        let value = json!({
            "rules": "oops",
            "blocked_sheets": ["ok.css", 42, null, {"url": "bad"}],
            "at_rules": [
                "not-an-object",
                {"condition_text": 7, "index": "3", "rules": "oops"},
                {
                    "conditionText": "kept",
                    "index": 9.9,
                    "rules": [
                        "bad",
                        {"selector": 5, "declarations": []},
                        {
                            "selector": "ok",
                            "href": 4,
                            "index": -1,
                            "declarations": {"a": 1, "b": "two"}
                        }
                    ]
                }
            ],
            "truncated": "yes"
        });

        let facet = parse(&value);

        assert!(facet.rules.is_empty());
        assert_eq!(facet.blocked_sheets, vec![String::from("ok.css")]);
        assert_eq!(facet.at_rules.len(), 3);
        assert_eq!(facet.at_rules[0].condition_text, "");
        assert_eq!(facet.at_rules[0].index, 0);
        assert!(facet.at_rules[0].rules.is_empty());
        assert_eq!(facet.at_rules[1].condition_text, "");
        assert_eq!(facet.at_rules[1].index, 0);
        assert!(facet.at_rules[1].rules.is_empty());
        assert_eq!(facet.at_rules[2].condition_text, "kept");
        assert_eq!(facet.at_rules[2].index, 0);
        assert_eq!(facet.at_rules[2].rules.len(), 1);
        let rule = &facet.at_rules[2].rules[0];
        assert_eq!(rule.selector, "ok");
        assert_eq!(rule.href, None);
        assert_eq!(rule.index, 0);
        assert_eq!(rule.origin, "author");
        assert_eq!(rule.declarations.len(), 1);
        assert_eq!(rule.declarations.get("b").map(String::as_str), Some("two"));
        assert!(!facet.truncated);
    }

    #[test]
    fn empty_value_yields_default_facet() {
        assert_eq!(parse(&json!({})), DeclaredFacet::default());
        assert_eq!(parse(&json!(null)), DeclaredFacet::default());
    }
}
