//! Computed-style extraction via a single in-page JavaScript pass.

use serde_json::{Map, Value, json};
use sniff_cdp::session::CdpSession;
use sniff_core::properties::StyleCategory;
use sniff_core::types::{
    ComputedProperty, ComputedStyles, ElementMetrics, ElementSnapshot, PseudoStyles, Rect,
};
use sniff_core::{SniffConfig, SniffError, SniffResult};

/// Performs element matching, filtering, style extraction and DOM-tree
/// walk in one `Runtime.evaluate` call to minimize CDP round trips.
const EXTRACT_JS: &str = r#"
(args) => {
  const selector = args.selector;
  const maxDepth = args.depth;
  const categories = args.categories;
  const pseudo = args.pseudo || [];
  const filter = args.filter;
  const opts = args.opts;
  const stableKey = opts.stableKey || null;

  function hexColor(tok) {
    const m = /^rgba?\(([^)]+)\)$/.exec(tok.trim());
    if (!m) return tok;
    const parts = m[1].split(/[,\s/]+/).filter(Boolean);
    if (parts.length < 3) return tok;
    const r = parseInt(parts[0], 10);
    const g = parseInt(parts[1], 10);
    const b = parseInt(parts[2], 10);
    let a;
    if (parts[3] !== undefined) {
      a = parts[3].indexOf('%') >= 0 ? parseFloat(parts[3]) / 100 : parseFloat(parts[3]);
    }
    const h = (n) => n.toString(16).padStart(2, '0');
    if (a === undefined || a >= 1) return '#' + h(r) + h(g) + h(b);
    return '#' + h(r) + h(g) + h(b) + h(Math.round(a * 255));
  }
  function norm(value) {
    if (typeof value !== 'string' || value.indexOf('rgb') < 0) return value;
    return value.replace(/rgba?\([^)]*\)/g, hexColor);
  }

  function classesOf(el) {
    const cls = el.classList;
    return cls ? Array.prototype.slice.call(cls) : [];
  }
  function escAttr(v) {
    return String(v).replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  }
  function anchorOf(el) {
    if (el.id) return '#' + el.id;
    const sk = opts.stableKey;
    if (sk) {
      const v = el.getAttribute(sk);
      if (v) return '[' + sk + '="' + escAttr(v) + '"]';
    }
    return null;
  }
  function token(el) {
    const a = anchorOf(el);
    if (a) return el.tagName.toLowerCase() + a;
    const cls = classesOf(el);
    if (cls.length) return el.tagName.toLowerCase() + '.' + cls[0];
    return el.tagName.toLowerCase();
  }
  function hasAnchor(el) {
    return anchorOf(el) !== null;
  }
  function pathOf(el) {
    const parts = [];
    let cur = el;
    while (cur && cur.nodeType === 1) {
      parts.unshift(token(cur));
      if (hasAnchor(cur)) break;
      cur = cur.parentElement;
    }
    return parts.join(' > ');
  }
  function selectorOf(el) {
    const parts = [];
    let cur = el;
    let n = 0;
    while (cur && cur.nodeType === 1 && n < 32) {
      let tok = token(cur);
      if (hasAnchor(cur)) {
        parts.unshift(tok);
        break;
      }
      const parent = cur.parentElement;
      const siblings = parent ? Array.prototype.slice.call(parent.children) : [];
      if (siblings.length > 1) {
        tok += ':nth-child(' + (siblings.indexOf(cur) + 1) + ')';
      }
      parts.unshift(tok);
      cur = parent;
      n += 1;
    }
    return parts.join(' > ');
  }

  function passes(el) {
    if (filter.visible) {
      const cs = getComputedStyle(el);
      if (cs.display === 'none' || cs.visibility === 'hidden') return false;
      const r = el.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) return false;
    }
    if (filter.minWidth != null || filter.minHeight != null) {
      const r = el.getBoundingClientRect();
      if (filter.minWidth != null && r.width < filter.minWidth) return false;
      if (filter.minHeight != null && r.height < filter.minHeight) return false;
    }
    if (filter.excludeSelectors && filter.excludeSelectors.length) {
      for (let i = 0; i < filter.excludeSelectors.length; i++) {
        if (el.matches(filter.excludeSelectors[i])) return false;
      }
    }
    return true;
  }

  function stackingContext(cs) {
    if (cs.zIndex !== 'auto' && cs.position !== 'static') return true;
    if (cs.position === 'fixed' || cs.position === 'sticky') return true;
    if (parseFloat(cs.opacity) < 1) return true;
    if (cs.transform !== 'none') return true;
    if (cs.perspective !== 'none') return true;
    if (cs.filter !== 'none') return true;
    if (cs.backdropFilter !== 'none') return true;
    if (cs.mixBlendMode !== 'normal') return true;
    if (cs.isolation === 'isolate') return true;
    if (cs.willChange && /(transform|opacity|filter|perspective)/.test(cs.willChange)) return true;
    if (cs.contain && /(layout|paint|strict|content)/.test(cs.contain)) return true;
    if (cs.clipPath !== 'none') return true;
    if (cs.maskImage !== 'none') return true;
    if (cs.contentVisibility === 'auto') return true;
    return false;
  }

  function buildStyles(el, cs) {
    const result = {};
    const keys = Object.keys(categories);
    for (let ci = 0; ci < keys.length; ci++) {
      const key = keys[ci];
      const props = categories[key];
      const values = {};
      for (let pi = 0; pi < props.length; pi++) {
        const name = props[pi];
        let v = cs.getPropertyValue(name);
        if (opts.normalizeColors) v = norm(v);
        values[name] = v;
      }
      result[key] = values;
    }
    if (opts.customProps) {
      const values = {};
      for (let i = 0; i < cs.length; i++) {
        const name = cs[i];
        if (name.indexOf('--') === 0) {
          values[name] = cs.getPropertyValue(name);
        }
      }
      result['css_variables'] = values;
    }
    return result;
  }

  function buildPseudo(el) {
    const out = {};
    for (let i = 0; i < pseudo.length; i++) {
      const pname = pseudo[i];
      const cs = getComputedStyle(el, pname);
      const groups = {};
      const keys = Object.keys(categories);
      for (let ci = 0; ci < keys.length; ci++) {
        const props = categories[keys[ci]];
        const values = {};
        for (let pi = 0; pi < props.length; pi++) {
          const name = props[pi];
          let v = cs.getPropertyValue(name);
          if (opts.normalizeColors) v = norm(v);
          values[name] = v;
        }
        groups[keys[ci]] = values;
      }
      if (opts.customProps) {
        const values = {};
        for (let i = 0; i < cs.length; i++) {
          const name = cs[i];
          if (name.indexOf('--') === 0) {
            values[name] = cs.getPropertyValue(name);
          }
        }
        groups['css_variables'] = values;
      }
      out[pname] = groups;
    }
    return out;
  }

  function buildNode(el, depth, parentId) {
    const node = {};
    node.id = ++__nodeId;
    if (parentId) node.parentId = parentId;
    node.tag = el.tagName;
    node.selector = selectorOf(el);
    node.path = pathOf(el);
    node.depth = depth;
    const cs = getComputedStyle(el);
    let rect = null;
    if (opts.includeRect || opts.includeVisibility) {
      rect = el.getBoundingClientRect();
    }
    if (opts.includeRect) {
      node.rect = { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
    }
    if (opts.includeMetrics) {
      node.metrics = { z_index: cs.zIndex, stacking_context: stackingContext(cs) };
    }
    if (opts.includeVisibility) {
      node.isVisible = cs.display !== 'none' && cs.visibility !== 'hidden' &&
        parseFloat(cs.opacity) > 0 && (rect.width > 0 || rect.height > 0);
    }
    node.styles = buildStyles(el, cs);
    if (pseudo.length) {
      node.pseudo = buildPseudo(el);
    }
    if (depth < maxDepth) {
      const kids = Array.prototype.slice.call(el.children);
      const children = [];
      for (let i = 0; i < kids.length; i++) {
        if (!passes(kids[i])) continue;
        children.push(buildNode(kids[i], depth + 1, node.id));
      }
      node.children = children;
    } else {
      node.children = [];
    }
    return node;
  }

  const results = [];
  let __nodeId = 0;
  const roots = document.querySelectorAll(selector);
  for (let i = 0; i < roots.length; i++) {
    if (!passes(roots[i])) continue;
    results.push(buildNode(roots[i], 0, null));
  }

  let globalCssVars = null;
  if (opts.customProps) {
    const rootCs = getComputedStyle(document.documentElement);
    const g = {};
    for (let i = 0; i < rootCs.length; i++) {
      const n = rootCs[i];
      if (n.indexOf('--') === 0) g[n] = rootCs.getPropertyValue(n);
    }
    globalCssVars = g;
  }
  return { globalCssVars: globalCssVars, elements: results };
}
"#;

/// Result of an extraction run: snapshots plus, when custom properties
/// were captured, the global `:root` variable map (once, not per node).
#[derive(Debug, Clone, Default)]
pub struct SniffOutcome {
    pub snapshots: Vec<ElementSnapshot>,
    pub global_css_variables: Option<Vec<(String, String)>>,
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
        "normalizeColors": config.output.normalize_colors,
        "customProps": config.include_custom_properties,
        "stableKey": config.stable_key,
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
    for cat in &config.categories {
        if has_all || *cat == StyleCategory::Custom {
            continue;
        }
        let props: Vec<String> = cat.properties().iter().map(|s| s.to_string()).collect();
        map.insert(
            cat.key().to_string(),
            Value::Array(props.into_iter().map(Value::String).collect()),
        );
    }
    if has_all {
        for cat in StyleCategory::all() {
            let props: Vec<String> = cat.properties().iter().map(|s| s.to_string()).collect();
            map.insert(
                cat.key().to_string(),
                Value::Array(props.into_iter().map(Value::String).collect()),
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

/// Parse the `{ globalCssVars, elements }` object returned by the page.
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

    Ok(SniffOutcome {
        snapshots,
        global_css_variables,
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
        depth: v.get("depth").and_then(Value::as_u64).unwrap_or(0) as usize,
        rect: v.get("rect").map(parse_rect),
        metrics: v.get("metrics").map(parse_metrics),
        is_visible: v.get("isVisible").and_then(Value::as_bool),
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
}
