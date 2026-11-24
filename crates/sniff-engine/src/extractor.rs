//! Computed-style extraction via a single in-page JavaScript pass.

use crate::js::EXTRACT_JS;
use crate::{declared, document, env};
use serde_json::{Map, Value, json};
use sniff_cdp::session::CdpSession;
use sniff_core::properties::{SUMMARY_PROPS, StyleCategory};
use sniff_core::types::{
    AccessibilityGrade, AriaInfo, ComputedProperty, ComputedStyles, ElementMetrics,
    ElementSnapshot, Noticeability, PseudoStyles, Rect,
};
use sniff_core::{SniffConfig, SniffError, SniffResult};

/// Result of an extraction run: snapshots plus, when custom properties
/// were captured, the global `:root` variable map (once, not per node).
#[derive(Debug, Clone, Default)]
pub struct SniffOutcome {
    /// The extracted snapshot forest.
    pub snapshots: Vec<ElementSnapshot>,
    /// Global `:root` CSS custom properties, captured once when requested.
    pub global_css_variables: Option<Vec<(String, String)>>,
    /// Global `__document` facet (title/lang/meta/links/scripts/sheets).
    pub document_facet: Option<sniff_core::modern::DocumentFacet>,
    /// Global `__env` facet (normalized media prefs, emulation, device).
    pub env_facet: Option<sniff_core::modern::EnvFacet>,
    /// Global `__declared` facet (CSSOM rules, opt-in via `--declared`).
    pub declared_facet: Option<sniff_core::modern::DeclaredFacet>,
    /// Matrix state tags when more than one viewport/emulation was captured.
    pub matrix: Vec<sniff_core::modern::MatrixTag>,
    /// Open shadow roots traversed during extraction.
    pub shadow_roots: usize,
    /// Full accessibility subtree (CDP `Accessibility` domain) for the
    /// matched elements, when `ax_tree` capture was requested.
    pub ax_tree: Option<Value>,
    /// UI-effect map for the configured interaction actions (the reserved
    /// `__actions` output area): per action, what appeared/disappeared/
    /// changed and where. `None` when there were no actions or `effects`
    /// is disabled.
    pub actions: Option<Value>,
    /// Decoded PNG bytes of the page when `config.screenshot` is set.
    pub screenshot: Option<Vec<u8>>,
}

/// Run the extraction pass and convert the returned JSON to snapshots.
pub async fn extract(session: &CdpSession, config: &SniffConfig) -> SniffResult<SniffOutcome> {
    let args = build_args(config);
    let args_json = serde_json::to_string(&args).map_err(SniffError::from)?;
    let expression = format!("({EXTRACT_JS})\n({args_json})");

    let value = session
        .evaluate(&expression, false)
        .await
        .map_err(|e| SniffError::Cdp(e.to_string()))?;

    parse_results(&value, config)
}

/// Build the argument object consumed by the in-page script.
///
/// NOTE: keys mirror the camelCase field names read by `EXTRACT_JS`.
fn build_args(config: &SniffConfig) -> Value {
    let categories = build_categories(config);
    let filter = json!({
        "visible": config.filter.visible,
        "minWidth": config.filter.min_width,
        "minHeight": config.filter.min_height,
        "excludeSelectors": config.filter.exclude_selectors,
    });
    let opts = json!({
        "includeRect": config.output.include_rect,
        "includeMetrics": config.output.include_metrics,
        "includeVisibility": config.output.include_visibility,
        "includeAria": config.output.include_aria,
        "includeContrast": config.output.include_contrast,
        "normalizeColors": config.output.normalize_colors,
        "customProps": config.include_custom_properties,
        "stableKey": config.stable_key,
        "attributes": config.effective_attributes(),
        "allAttributes": config.all_attributes(),
        "documentFacet": config.document_enabled(),
        "envFacet": config.env_enabled(),
        "declaredFacet": config.declared_facet,
        "declaredLimit": config.declared_limit,
        "emulate": config.emulate,
    });
    json!({
        "selector": config.selector,
        "depth": config.depth,
        "categories": categories,
        "pseudo": config.pseudo_elements,
        "filter": filter,
        "opts": opts,
    })
}

/// Resolve categories (including `All` and custom properties) into the
/// `{ category_key: [props] }` map consumed by the page.
fn build_categories(config: &SniffConfig) -> Map<String, Value> {
    let mut map = Map::new();
    let has_all = config.categories.contains(&StyleCategory::All);
    // Summary output emits only the curated `SUMMARY_PROPS` subset. Keep the
    // entire catalog for `check`/`diff` consumers (jsonl/json) but avoid
    // asking the browser for hundreds of properties the digest discards.
    if config.output.format == sniff_core::OutputFormat::Summary {
        let props = Value::Array(
            SUMMARY_PROPS
                .iter()
                .map(|p| Value::String((*p).to_string()))
                .collect(),
        );
        for cat in StyleCategory::all() {
            map.insert(cat.key().to_string(), props.clone());
        }
        return map;
    }
    for cat in &config.categories {
        if has_all || *cat == StyleCategory::Custom {
            continue;
        }
        map.insert(
            cat.key().to_string(),
            Value::Array(
                cat.properties()
                    .iter()
                    .map(|p| Value::String(p.to_string()))
                    .collect(),
            ),
        );
    }
    if has_all {
        for cat in StyleCategory::all() {
            map.insert(
                cat.key().to_string(),
                Value::Array(
                    cat.properties()
                        .iter()
                        .map(|p| Value::String(p.to_string()))
                        .collect(),
                ),
            );
        }
    }
    if !config.custom_properties.is_empty() {
        map.insert(
            "custom".to_string(),
            Value::Array(
                config
                    .custom_properties
                    .iter()
                    .map(|s| Value::String(s.clone()))
                    .collect(),
            ),
        );
    }
    map
}

/// Map a JSON category key back to its `StyleCategory`.
fn category_from_key(key: &str) -> StyleCategory {
    for cat in StyleCategory::all() {
        if cat.key() == key {
            return cat;
        }
    }
    if key == "custom" {
        return StyleCategory::Custom;
    }
    if key == "css_variables" {
        return StyleCategory::Variables;
    }
    StyleCategory::All
}

/// Parse the `{ globalCssVars, elements, ...facets }` object returned by the page.
fn parse_results(value: &Value, _config: &SniffConfig) -> SniffResult<SniffOutcome> {
    let elements = value
        .get("elements")
        .and_then(Value::as_array)
        .ok_or_else(|| SniffError::Other("extraction returned no `elements` array".into()))?;
    let snapshots = elements.iter().map(parse_node).collect();

    let global_css_variables = value
        .get("globalCssVars")
        .and_then(Value::as_object)
        .map(|obj| {
            obj.iter()
                .map(|(name, val)| (name.clone(), val.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .filter(|v: &Vec<(String, String)>| !v.is_empty());

    let parse_facet = |key: &str| value.get(key).filter(|v| !v.is_null());
    Ok(SniffOutcome {
        snapshots,
        global_css_variables,
        document_facet: parse_facet("documentFacet").map(document::parse),
        env_facet: parse_facet("envFacet").map(env::parse),
        declared_facet: parse_facet("declaredFacet").map(declared::parse),
        shadow_roots: value
            .get("shadowRoots")
            .and_then(Value::as_u64)
            .map_or(0, |n| usize::try_from(n).unwrap_or(0)),
        ..SniffOutcome::default()
    })
}

fn parse_node(v: &Value) -> ElementSnapshot {
    ElementSnapshot {
        id: v.get("id").and_then(Value::as_u64).unwrap_or(0),
        parent_id: v.get("parentId").and_then(Value::as_u64),
        tag: v
            .get("tag")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        selector: v
            .get("selector")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        path: v
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        depth: v
            .get("depth")
            .and_then(Value::as_u64)
            .map_or(0, |d| usize::try_from(d).unwrap_or(usize::MAX)),
        rect: v.get("rect").map(parse_rect),
        metrics: v.get("metrics").map(parse_metrics),
        noticeable: v.get("isUserNoticeable").and_then(parse_noticeable),
        effective_background: v
            .get("effectiveBackground")
            .and_then(Value::as_str)
            .map(String::from),
        aria: v.get("aria").map(parse_aria),
        contrast: None,
        ax: None,
        attributes: v.get("attributes").and_then(Value::as_object).map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        }),
        styles: parse_styles(v.get("styles").unwrap_or(&Value::Null)),
        pseudo: v
            .get("pseudo")
            .and_then(Value::as_object)
            .map(|obj| {
                obj.iter()
                    .map(|(name, styles)| PseudoStyles {
                        name: name.clone(),
                        styles: parse_styles(styles),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        children: v
            .get("children")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().map(parse_node).collect())
            .unwrap_or_default(),
    }
}

fn parse_aria(v: &Value) -> AriaInfo {
    let str_opt = |key: &str| v.get(key).and_then(Value::as_str).map(String::from);
    let flag = |key: &str| v.get(key).and_then(Value::as_bool).unwrap_or(false);
    AriaInfo {
        role: str_opt("role"),
        name: str_opt("name"),
        focusable: flag("focusable"),
        aria_hidden: str_opt("ariaHidden"),
        aria_expanded: str_opt("ariaExpanded"),
        aria_checked: str_opt("ariaChecked"),
        aria_controls: str_opt("ariaControls"),
        aria_labelledby: str_opt("ariaLabelledby"),
        aria_describedby: str_opt("ariaDescribedby"),
        lang: str_opt("lang"),
        alt: str_opt("alt"),
        title: str_opt("title"),
        html_hidden: flag("htmlHidden"),
        disabled: flag("disabled"),
        has_text: flag("hasText"),
    }
}

fn parse_rect(v: &Value) -> Rect {
    Rect {
        x: v.get("x").and_then(Value::as_f64).unwrap_or(0.0),
        y: v.get("y").and_then(Value::as_f64).unwrap_or(0.0),
        width: v.get("width").and_then(Value::as_f64).unwrap_or(0.0),
        height: v.get("height").and_then(Value::as_f64).unwrap_or(0.0),
    }
}

fn parse_metrics(v: &Value) -> ElementMetrics {
    ElementMetrics {
        z_index: v
            .get("z_index")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        stacking_context: v
            .get("stacking_context")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn parse_noticeable(v: &Value) -> Option<Noticeability> {
    let display_visible = v.get("display_visible").and_then(Value::as_bool)?;
    let grade = match v
        .get("accessibility_grade")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_uppercase()
        .as_str()
    {
        "NONE" => AccessibilityGrade::None,
        "AA" => AccessibilityGrade::Aa,
        "AAA" => AccessibilityGrade::Aaa,
        _ => return None,
    };
    Some(Noticeability {
        display_visible,
        accessibility_grade: grade,
    })
}

fn parse_styles(v: &Value) -> ComputedStyles {
    let Some(obj) = v.as_object() else {
        return ComputedStyles::default();
    };
    let groups = obj
        .iter()
        .map(|(key, props)| {
            let props = props
                .as_object()
                .map(|p| {
                    p.iter()
                        .map(|(name, value)| ComputedProperty {
                            name: name.clone(),
                            value: value.as_str().unwrap_or_default().to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            (category_from_key(key), props)
        })
        .collect();
    ComputedStyles { groups }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_key_round_trip() {
        assert_eq!(category_from_key("box_model"), StyleCategory::BoxModel);
        assert_eq!(category_from_key("custom"), StyleCategory::Custom);
        assert_eq!(category_from_key("css_variables"), StyleCategory::Variables);
        assert_eq!(category_from_key("unknown"), StyleCategory::All);
    }

    #[test]
    fn build_categories_resolves_all() {
        let cfg = SniffConfig {
            categories: vec![StyleCategory::All],
            custom_properties: vec!["--my-prop".into()],
            ..Default::default()
        };
        let map = build_categories(&cfg);
        assert!(map.contains_key("box_model"));
        assert!(map.contains_key("animation"));
        assert_eq!(map.get("custom").unwrap().as_array().unwrap().len(), 1);
        assert_eq!(
            map.get("box_model").unwrap().as_array().unwrap()[0],
            Value::String("width".into())
        );
    }

    #[test]
    fn build_categories_honors_selection_and_custom() {
        let cfg = SniffConfig {
            categories: vec![StyleCategory::Typography, StyleCategory::Custom],
            custom_properties: vec!["--x".into()],
            ..Default::default()
        };
        let map = build_categories(&cfg);
        assert!(map.contains_key("typography"));
        assert!(!map.contains_key("box_model"));
        assert!(map.contains_key("custom"));
    }

    #[test]
    fn parse_node_full_tree() {
        let json = json!({
            "tag": "DIV",
            "selector": "div.a",
            "path": "body > div.a",
            "depth": 0,
            "rect": {"x": 1.0, "y": 2.0, "width": 100.0, "height": 50.0},
            "metrics": {"z_index": "10", "stacking_context": true},
            "aria": {"role": "link", "name": "Go", "focusable": true, "ariaHidden": "false"},
            "styles": {
                "box_model": {"width": "100px"},
                "layout": {"display": "block"}
            },
            "children": [
                {"tag": "SPAN", "selector": "div.a > span", "path": "body > div.a > span",
                 "depth": 1, "styles": {"layout": {"display": "inline"}}, "children": []}
            ]
        });
        let snap = parse_node(&json);
        assert_eq!(snap.tag, "DIV");
        assert_eq!(snap.rect.unwrap().width, 100.0);
        assert_eq!(snap.metrics.unwrap().z_index, "10");
        assert_eq!(snap.styles.groups.len(), 2);
        assert_eq!(snap.children.len(), 1);
        assert_eq!(snap.children[0].depth, 1);
        let aria = snap.aria.expect("aria parsed");
        assert_eq!(aria.role.as_deref(), Some("link"));
        assert_eq!(aria.name.as_deref(), Some("Go"));
        assert!(aria.focusable);
        assert_eq!(aria.aria_hidden.as_deref(), Some("false"));
        assert_eq!(snap.contrast, None);
        assert_eq!(snap.ax, None);
    }

    #[test]
    fn parse_aria_missing_fields_default() {
        let snap = parse_node(&json!({
            "tag": "DIV", "selector": "div",
            "aria": {"focusable": false, "htmlHidden": false, "disabled": false},
            "styles": {}, "children": []
        }));
        let aria = snap.aria.expect("aria facet parsed");
        assert_eq!(aria.role, None);
        assert!(!aria.focusable);
        assert!(!aria.html_hidden);
        assert!(!aria.disabled);
    }

    #[test]
    fn parse_styles_empty_is_default() {
        let styles = parse_styles(&Value::Null);
        assert!(styles.is_empty());
    }

    #[test]
    fn build_args_keys_match_js_reads() {
        let cfg = SniffConfig {
            url: "http://x".into(),
            selector: ".card".into(),
            depth: 2,
            categories: vec![StyleCategory::BoxModel],
            custom_properties: vec!["--x".into()],
            pseudo_elements: vec!["::before".into()],
            filter: sniff_core::ElementFilter {
                visible: false,
                min_width: Some(10.0),
                min_height: Some(20.0),
                exclude_selectors: vec![".skip".into()],
            },
            ..Default::default()
        };
        let args = build_args(&cfg);
        // Keys the JS reads must be present with camelCase spelling.
        let obj = args.as_object().unwrap();
        assert_eq!(obj["selector"], ".card");
        assert_eq!(obj["depth"], 2);
        assert_eq!(obj["pseudo"][0], "::before");
        let filter = obj["filter"].as_object().unwrap();
        assert_eq!(filter["visible"], false);
        assert_eq!(filter["minWidth"], 10.0);
        assert_eq!(filter["minHeight"], 20.0);
        assert_eq!(filter["excludeSelectors"][0], ".skip");
        let opts = obj["opts"].as_object().unwrap();
        assert!(opts.contains_key("includeRect"));
        assert!(opts.contains_key("includeMetrics"));
        assert!(opts.contains_key("normalizeColors"));
        assert_eq!(opts["includeVisibility"], true);
        assert_eq!(opts["stableKey"], Value::Null);
        // JS must reference the same spellings (guards against drift).
        let js = EXTRACT_JS.to_string();
        assert!(js.contains("opts.includeRect"));
        assert!(js.contains("opts.includeMetrics"));
        assert!(js.contains("opts.normalizeColors"));
        assert!(js.contains("opts.includeVisibility"));
        assert!(js.contains("opts.stableKey"));
        assert!(js.contains("opts.includeAria"));
        assert!(js.contains("ariaOf(el)"));
        assert!(js.contains("filter.minWidth"));
        assert!(js.contains("filter.minHeight"));
        assert!(js.contains("filter.excludeSelectors"));
    }

    #[test]
    fn build_args_include_custom_props_flag() {
        let cfg = SniffConfig {
            include_custom_properties: true,
            ..Default::default()
        };
        let args = build_args(&cfg);
        assert_eq!(args["opts"]["customProps"], Value::Bool(true));
        // The JS side must collect `--*` into a `css_variables` group.
        let js = EXTRACT_JS.to_string();
        assert!(js.contains("opts.customProps"));
        assert!(js.contains("'css_variables'"));
    }

    #[test]
    fn build_args_carries_stable_key() {
        let cfg = SniffConfig {
            stable_key: Some("data-testid".into()),
            ..Default::default()
        };
        let args = build_args(&cfg);
        assert_eq!(args["opts"]["stableKey"], "data-testid");
        // JS must prefer the stable attribute as the selector anchor.
        let js = EXTRACT_JS.to_string();
        assert!(js.contains("opts.stableKey"));
        assert!(js.contains("el.getAttribute(sk)"));
    }

    #[test]
    fn implicit_role_covers_arria_spec_tags() {
        let js = EXTRACT_JS.to_string();
        // The implicit-role switch must map the HTML-ARIA implicit roles the
        // accessibility guide documents. Guards against drift when the map
        // is enriched.
        let expected: &[(&str, &str)] = &[
            ("A", "case 'A': case 'AREA':"),
            ("BUTTON", "case 'BUTTON': return 'button'"),
            ("INPUT types", "'checkbox'"),
            ("SELECT multiple -> listbox", "'listbox' : 'combobox'"),
            ("OPTION", "case 'OPTION': return 'option'"),
            ("OPTGROUP", "case 'OPTGROUP': return 'group'"),
            (
                "HEADER/FOOTER scoping",
                "el.closest('article,aside,main,nav,section')",
            ),
            ("NAV", "case 'NAV': return 'navigation'"),
            ("MAIN", "case 'MAIN': return 'main'"),
            ("ASIDE", "case 'ASIDE': return 'complementary'"),
            ("FORM", "case 'FORM': return 'form'"),
            ("SEARCH", "case 'SEARCH': return 'search'"),
            ("ARTICLE", "case 'ARTICLE': return 'article'"),
            ("SECTION->region", "'region' : null"),
            (
                "UL/OL/MENU",
                "case 'UL': case 'OL': case 'MENU': return 'list'",
            ),
            ("LI", "case 'LI': return 'listitem'"),
            ("DL", "case 'DL': return 'list'"),
            ("DT->term", "case 'DT': return 'term'"),
            ("DD->definition", "case 'DD': return 'definition'"),
            ("DFN->term", "case 'DFN': return 'term'"),
            ("TABLE", "case 'TABLE': return 'table'"),
            (
                "COLGROUP/THEAD/TBODY/TFOOT->rowgroup",
                "case 'COLGROUP': case 'THEAD': case 'TBODY': case 'TFOOT': return 'rowgroup'",
            ),
            ("COL->column", "case 'COL': return 'column'"),
            ("TR->row", "case 'TR': return 'row'"),
            ("TD->cell", "case 'TD': return 'cell'"),
            (
                "TH scope",
                "el.getAttribute('scope') === 'row' ? 'rowheader' : 'columnheader'",
            ),
            ("IMG alt=\"\"->presentation", "'presentation' : 'img'"),
            (
                "H1-H6->heading",
                "case 'H1': case 'H2': case 'H3': case 'H4': case 'H5': case 'H6': return 'heading'",
            ),
            ("FIGURE->figure", "case 'FIGURE': return 'figure'"),
            ("FIELDSET->group", "case 'FIELDSET': return 'group'"),
            ("DIALOG", "case 'DIALOG': return 'dialog'"),
            ("SUMMARY->button", "case 'SUMMARY': return 'button'"),
            ("DETAILS->group", "case 'DETAILS': return 'group'"),
            ("HR->separator", "case 'HR': return 'separator'"),
            (
                "PROGRESS->progressbar",
                "case 'PROGRESS': return 'progressbar'",
            ),
            ("METER->meter", "case 'METER': return 'meter'"),
            ("OUTPUT->status", "case 'OUTPUT': return 'status'"),
            ("MARK->mark", "case 'MARK': return 'mark'"),
            ("STRONG->strong", "case 'STRONG': return 'strong'"),
            ("EM->emphasis", "case 'EM': return 'emphasis'"),
            ("SUB->subscript", "case 'SUB': return 'subscript'"),
            ("SUP->superscript", "case 'SUP': return 'superscript'"),
            ("CODE->code", "case 'CODE': return 'code'"),
            (
                "BLOCKQUOTE->blockquote",
                "case 'BLOCKQUOTE': return 'blockquote'",
            ),
            ("TIME->time", "case 'TIME': return 'time'"),
            ("P->paragraph", "case 'P': return 'paragraph'"),
            ("MATH->math", "case 'MATH': return 'math'"),
            ("CANVAS fallback", "case 'CANVAS':"),
            ("IFRAME title->document", "case 'IFRAME':"),
            ("EMBED->embedded", "case 'EMBED': return 'embedded'"),
            ("AUDIO/VIDEO controls->group", "case 'AUDIO': case 'VIDEO':"),
        ];
        for (label, needle) in expected {
            assert!(
                js.contains(needle),
                "implicitRole must map {label} (missing `{needle}`)"
            );
        }
    }
}
