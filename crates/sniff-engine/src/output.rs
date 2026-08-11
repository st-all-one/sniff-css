//! Serialization of snapshots into the AI-friendly output stream.
//!
//! Two fidelity modes:
//! - **Full** (default): every captured property, `css_variables` per node.
//! - **Compact** (`--compact`): drops redundant logical/default properties
//!   and scopes `css_variables` to a single global `__meta` map plus
//!   per-node overrides, maximizing token efficiency for AI consumption.

use serde_json::{Map, Value};
use sniff_core::properties::StyleCategory;
use sniff_core::types::{ComputedProperty, ComputedStyles, ElementSnapshot};
use sniff_core::{OutputConfig, OutputFormat, SniffError, SniffResult};
use std::collections::HashMap;
use std::io::Write;
use xxhash_rust::xxh3::{xxh3_64, xxh3_64_with_seed};

use crate::extractor::SniffOutcome;

/// Render a snapshot (with its subtree) as a JSON object.
pub fn snapshot_to_json(
    snap: &ElementSnapshot,
    config: &OutputConfig,
    global_vars: Option<&HashMap<String, String>>,
) -> Value {
    node_to_json(snap, config, global_vars, true)
}

/// Render a single node as a JSON object; when `tree`, includes children.
fn node_to_json(
    snap: &ElementSnapshot,
    config: &OutputConfig,
    global_vars: Option<&HashMap<String, String>>,
    tree: bool,
) -> Value {
    let mut obj = Map::new();
    obj.insert("id".into(), Value::from(snap.id));
    if let Some(pid) = snap.parent_id {
        obj.insert("parent_id".into(), Value::from(pid));
    }
    obj.insert("tag".into(), Value::String(snap.tag.clone()));
    obj.insert("selector".into(), Value::String(snap.selector.clone()));
    if config.include_path {
        obj.insert("path".into(), Value::String(snap.path.clone()));
    }
    obj.insert("depth".into(), Value::from(snap.depth));
    if config.include_rect {
        if let Some(rect) = snap.rect {
            obj.insert(
                "rect".into(),
                json_rect(rect.x, rect.y, rect.width, rect.height),
            );
        }
    }
    if config.include_metrics {
        if let Some(metrics) = &snap.metrics {
            obj.insert(
                "metrics".into(),
                Value::Object({
                    let mut m = Map::new();
                    m.insert("z_index".into(), Value::String(metrics.z_index.clone()));
                    m.insert(
                        "stacking_context".into(),
                        Value::Bool(metrics.stacking_context),
                    );
                    m
                }),
            );
        }
    }
    if config.include_visibility {
        if let Some(visible) = snap.is_visible {
            obj.insert("is_visible".into(), Value::Bool(visible));
        }
    }
    if let Some(aria) = &snap.aria {
        obj.insert(
            "aria".into(),
            serde_json::to_value(aria).unwrap_or(Value::Null),
        );
    }
    if let Some(contrast) = &snap.contrast {
        obj.insert(
            "contrast".into(),
            serde_json::to_value(contrast).unwrap_or(Value::Null),
        );
    }
    if let Some(ax) = &snap.ax {
        obj.insert("ax".into(), serde_json::to_value(ax).unwrap_or(Value::Null));
    }
    let pseudo_value = if snap.pseudo.is_empty() {
        None
    } else {
        Some(Value::Object(
            snap.pseudo
                .iter()
                .map(|p| {
                    (
                        p.name.clone(),
                        styles_to_json(&p.styles, config, global_vars),
                    )
                })
                .collect::<Map<_, _>>(),
        ))
    };
    let styles_value = styles_to_json(&snap.styles, config, global_vars);
    if config.include_style_hash {
        obj.insert(
            "computed_style_hash".into(),
            Value::String(style_hash(&styles_value, pseudo_value.as_ref())),
        );
    }
    obj.insert("styles".into(), styles_value);
    if let Some(pseudo) = pseudo_value {
        obj.insert("pseudo".into(), pseudo);
    }
    if tree && !snap.children.is_empty() {
        obj.insert(
            "children".into(),
            Value::Array(
                snap.children
                    .iter()
                    .map(|c| node_to_json(c, config, global_vars, true))
                    .collect(),
            ),
        );
    }
    Value::Object(obj)
}

/// 64-bit checksum of the effective styles (and pseudo-elements), computed
/// over the canonical serialized form so a change in output equals a change
/// in hash. Non-cryptographic (xxh3): collisions are negligible for
/// change-detection across pages.
fn style_hash(styles: &Value, pseudo: Option<&Value>) -> String {
    let mut hash = xxh3_64(serde_json::to_vec(styles).unwrap_or_default().as_slice());
    if let Some(p) = pseudo {
        hash = xxh3_64_with_seed(serde_json::to_vec(p).unwrap_or_default().as_slice(), hash);
    }
    format!("{hash:016x}")
}

fn json_rect(x: f64, y: f64, width: f64, height: f64) -> Value {
    Value::Object({
        let mut m = Map::new();
        m.insert("x".into(), Value::from(x));
        m.insert("y".into(), Value::from(y));
        m.insert("width".into(), Value::from(width));
        m.insert("height".into(), Value::from(height));
        m
    })
}

/// Convert computed styles to a JSON object. In compact mode, redundant
/// logical/default properties are removed and `css_variables` is reduced
/// to overrides against the global map.
fn styles_to_json(
    styles: &ComputedStyles,
    config: &OutputConfig,
    global_vars: Option<&HashMap<String, String>>,
) -> Value {
    let mut obj = Map::new();
    for (category, props) in &styles.groups {
        if *category == StyleCategory::Variables {
            let vars: Vec<ComputedProperty> = if config.compact {
                scope_variables(props, global_vars)
            } else {
                props.clone()
            };
            if vars.is_empty() {
                continue;
            }
            if config.group_by_category {
                let group = props_map(&vars);
                obj.insert(category.key().to_string(), Value::Object(group));
            } else {
                for p in vars {
                    obj.insert(p.name, Value::String(p.value));
                }
            }
            continue;
        }

        let props = if config.compact {
            compact_group(*category, props)
        } else {
            props.clone()
        };
        if props.is_empty() {
            continue;
        }
        if config.group_by_category {
            obj.insert(category.key().to_string(), Value::Object(props_map(&props)));
        } else {
            for p in props {
                obj.insert(p.name, Value::String(p.value));
            }
        }
    }
    Value::Object(obj)
}

fn props_map(props: &[ComputedProperty]) -> Map<String, Value> {
    let mut group = Map::new();
    for p in props {
        group.insert(p.name.clone(), Value::String(p.value.clone()));
    }
    group
}

/// Keep only variables whose value differs from the inherited global map.
fn scope_variables(
    props: &[ComputedProperty],
    global_vars: Option<&HashMap<String, String>>,
) -> Vec<ComputedProperty> {
    match global_vars {
        Some(global) => props
            .iter()
            .filter(|p| global.get(&p.name).is_none_or(|g| *g != p.value))
            .cloned()
            .collect(),
        None => props.to_vec(),
    }
}

/// Reduce a category group in compact mode: drop logical properties that
/// duplicate their physical counterpart, and suppress default/noise values.
fn compact_group(category: StyleCategory, props: &[ComputedProperty]) -> Vec<ComputedProperty> {
    let map: HashMap<&str, &str> = props
        .iter()
        .map(|p| (p.name.as_str(), p.value.as_str()))
        .collect();

    props
        .iter()
        .filter(|p| {
            // Logical/physical dedup (keep the physical, drop the logical).
            if let Some(phys) = physical_equivalent(&p.name) {
                if map
                    .get(phys.as_str())
                    .is_some_and(|&v| v == p.value.as_str())
                {
                    return false;
                }
            }
            // Default/noise suppression (skip allow-listed properties).
            if !KEEP_DEFAULTS.contains(&p.name.as_str()) && is_noise_value(p.value.trim()) {
                return false;
            }
            let _ = category;
            true
        })
        .cloned()
        .collect()
}

/// Map a logical (block/inline) property to its physical counterpart.
fn physical_equivalent(logical: &str) -> Option<String> {
    const MAP: &[(&str, &str)] = &[
        ("margin-block-start", "margin-top"),
        ("margin-block-end", "margin-bottom"),
        ("margin-inline-start", "margin-left"),
        ("margin-inline-end", "margin-right"),
        ("padding-block-start", "padding-top"),
        ("padding-block-end", "padding-bottom"),
        ("padding-inline-start", "padding-left"),
        ("padding-inline-end", "padding-right"),
        ("block-size", "height"),
        ("inline-size", "width"),
        ("min-block-size", "min-height"),
        ("max-block-size", "max-height"),
        ("min-inline-size", "min-width"),
        ("max-inline-size", "max-width"),
        ("overflow-block", "overflow-y"),
        ("overflow-inline", "overflow-x"),
        ("inset-block-start", "top"),
        ("inset-block-end", "bottom"),
        ("inset-inline-start", "left"),
        ("inset-inline-end", "right"),
        ("scroll-margin-block-start", "scroll-margin-top"),
        ("scroll-margin-block-end", "scroll-margin-bottom"),
        ("scroll-margin-inline-start", "scroll-margin-left"),
        ("scroll-margin-inline-end", "scroll-margin-right"),
        ("scroll-padding-block-start", "scroll-padding-top"),
        ("scroll-padding-block-end", "scroll-padding-bottom"),
        ("scroll-padding-inline-start", "scroll-padding-left"),
        ("scroll-padding-inline-end", "scroll-padding-right"),
        ("overscroll-behavior-block", "overscroll-behavior-y"),
        ("overscroll-behavior-inline", "overscroll-behavior-x"),
        ("grid-column-gap", "column-gap"),
        ("grid-row-gap", "row-gap"),
    ];
    if let Some(&(_, phys)) = MAP.iter().find(|(l, _)| *l == logical) {
        return Some(phys.to_string());
    }
    // Border logical variants: border-block-start-* -> border-top-*, etc.
    const BORDER: &[(&str, &str)] = &[
        ("border-block-start-", "border-top-"),
        ("border-block-end-", "border-bottom-"),
        ("border-inline-start-", "border-left-"),
        ("border-inline-end-", "border-right-"),
    ];
    for (prefix, phys_prefix) in BORDER {
        if let Some(suffix) = logical.strip_prefix(prefix) {
            return Some(format!("{phys_prefix}{suffix}"));
        }
    }
    None
}

/// Properties whose default values are always meaningful and must survive
/// compact default-suppression.
const KEEP_DEFAULTS: &[&str] = &[
    "display",
    "position",
    "z-index",
    "opacity",
    "visibility",
    "box-sizing",
    "float",
    "clear",
    "cursor",
    "pointer-events",
    "user-select",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "line-height",
    "letter-spacing",
    "word-spacing",
    "text-align",
    "text-decoration-line",
    "color",
    "width",
    "height",
    "min-width",
    "max-width",
    "min-height",
    "max-height",
    "block-size",
    "inline-size",
    "min-block-size",
    "max-block-size",
    "min-inline-size",
    "max-inline-size",
    "aspect-ratio",
    "content",
    "writing-mode",
    "direction",
    "isolation",
    "mix-blend-mode",
    "accent-color",
    "color-scheme",
    "flex-grow",
    "flex-shrink",
    "flex-basis",
    "transform",
    "transform-origin",
    "perspective",
    "backface-visibility",
    "filter",
    "backdrop-filter",
    "clip-path",
    "box-shadow",
    "background-color",
    "background-image",
    "border-top-color",
    "border-right-color",
    "border-bottom-color",
    "border-left-color",
    "outline-color",
    "caret-color",
    "gap",
    "row-gap",
    "column-gap",
    "grid-template-columns",
    "grid-template-rows",
    "grid-auto-rows",
    "grid-auto-columns",
    "grid-column-start",
    "grid-column-end",
    "grid-row-start",
    "grid-row-end",
    "object-fit",
    "object-position",
    "white-space",
    "overflow-x",
    "overflow-y",
    "word-break",
    "overflow-wrap",
    "hyphens",
    "scroll-behavior",
    "scroll-snap-type",
    "touch-action",
    "animation-name",
    "animation-duration",
    "animation-timing-function",
    "animation-delay",
    "animation-iteration-count",
    "transition-property",
    "transition-duration",
    "transition-timing-function",
    "transition-delay",
];

fn is_noise_value(value: &str) -> bool {
    matches!(
        value,
        "" | "0px"
            | "0s"
            | "0%"
            | "0deg"
            | "0"
            | "none"
            | "normal"
            | "auto"
            | "100%"
            | "visible"
            | "scroll"
            | "repeat"
            | "add"
            | "stretch"
            | "ease"
            | "balance"
            | "slice"
            | "clip"
    )
}

/// Pre-order flattening of a snapshot tree.
fn flatten<'a>(snap: &'a ElementSnapshot, out: &mut Vec<&'a ElementSnapshot>) {
    out.push(snap);
    for child in &snap.children {
        flatten(child, out);
    }
}

/// Write the outcome to `writer` using the configured format.
pub fn write_output<W: Write>(
    writer: &mut W,
    outcome: &SniffOutcome,
    config: &OutputConfig,
) -> SniffResult<()> {
    let global = outcome
        .global_css_variables
        .as_ref()
        .map(|vars| vars.iter().cloned().collect::<HashMap<_, _>>());

    match config.format {
        OutputFormat::JsonLines => {
            if config.compact {
                emit_meta_line(writer, &global)?;
            }
            emit_ax_tree_line(writer, outcome.ax_tree.as_ref())?;
            for snap in &outcome.snapshots {
                let json = snapshot_to_json(snap, config, global.as_ref());
                writeln!(
                    writer,
                    "{}",
                    serde_json::to_string(&json).map_err(SniffError::from)?
                )
                .map_err(io_err)?;
            }
        }
        OutputFormat::JsonLinesFlat => {
            if config.compact {
                emit_meta_line(writer, &global)?;
            }
            emit_ax_tree_line(writer, outcome.ax_tree.as_ref())?;
            let mut nodes = Vec::new();
            for snap in &outcome.snapshots {
                flatten(snap, &mut nodes);
            }
            for node in nodes {
                let json = node_to_json(node, config, global.as_ref(), false);
                writeln!(
                    writer,
                    "{}",
                    serde_json::to_string(&json).map_err(SniffError::from)?
                )
                .map_err(io_err)?;
            }
        }
        OutputFormat::Json => {
            let compact_meta = config.compact && global.as_ref().is_some_and(|g| !g.is_empty());
            let array: Vec<Value> = outcome
                .snapshots
                .iter()
                .map(|s| snapshot_to_json(s, config, global.as_ref()))
                .collect();
            let document = if compact_meta || outcome.ax_tree.is_some() {
                let mut root = Map::new();
                let mut meta = Map::new();
                if let Some(vars) = global.as_ref() {
                    if !vars.is_empty() {
                        meta.insert("css_variables".into(), vars_to_json(vars));
                    }
                }
                if !meta.is_empty() {
                    root.insert("__meta".into(), Value::Object(meta));
                }
                if let Some(ax_tree) = &outcome.ax_tree {
                    root.insert("__ax_tree".into(), ax_tree.clone());
                }
                root.insert("elements".into(), Value::Array(array));
                Value::Object(root)
            } else {
                Value::Array(array)
            };
            if config.pretty {
                serde_json::to_writer_pretty(&mut *writer, &document).map_err(SniffError::from)?;
            } else {
                serde_json::to_writer(&mut *writer, &document).map_err(SniffError::from)?;
            }
            writeln!(writer).map_err(io_err)?;
        }
    }
    writer.flush().map_err(io_err)?;
    Ok(())
}

fn vars_to_json(vars: &HashMap<String, String>) -> Value {
    Value::Object(
        vars.iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect(),
    )
}

/// In compact mode with custom properties, emit the global `css_variables`
/// map once as a `__meta` line so descendant nodes can omit it.
fn emit_meta_line<W: Write>(
    writer: &mut W,
    global: &Option<HashMap<String, String>>,
) -> SniffResult<()> {
    if let Some(vars) = global {
        if !vars.is_empty() {
            let mut meta = Map::new();
            meta.insert("css_variables".into(), vars_to_json(vars));
            let line = serde_json::to_string(&Value::Object({
                let mut m = Map::new();
                m.insert("__meta".into(), Value::Object(meta));
                m
            }))
            .map_err(SniffError::from)?;
            writeln!(writer, "{line}").map_err(io_err)?;
        }
    }
    Ok(())
}

/// Emit the captured accessibility subtree as a single `__ax_tree` line
/// (only when an AX tree capture was requested).
fn emit_ax_tree_line<W: Write>(writer: &mut W, ax_tree: Option<&Value>) -> SniffResult<()> {
    if let Some(tree) = ax_tree {
        let line = serde_json::to_string(&Value::Object({
            let mut m = Map::new();
            m.insert("__ax_tree".into(), tree.clone());
            m
        }))
        .map_err(SniffError::from)?;
        writeln!(writer, "{line}").map_err(io_err)?;
    }
    Ok(())
}

fn io_err(e: std::io::Error) -> SniffError {
    SniffError::Other(format!("io error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sniff_core::config::OutputFormat;
    use sniff_core::types::{ComputedStyles, ElementMetrics, Rect};

    fn cp(name: &str, value: &str) -> ComputedProperty {
        ComputedProperty {
            name: name.into(),
            value: value.into(),
        }
    }

    fn sample_snapshot() -> ElementSnapshot {
        ElementSnapshot {
            id: 1,
            parent_id: None,
            tag: "SPAN".into(),
            selector: "span.icon".into(),
            path: "body > button.btn > span.icon".into(),
            depth: 1,
            rect: Some(Rect {
                x: 12.0,
                y: 20.0,
                width: 32.0,
                height: 32.0,
            }),
            metrics: Some(ElementMetrics {
                z_index: "auto".into(),
                stacking_context: false,
            }),
            is_visible: Some(true),
            aria: None,
            contrast: None,
            ax: None,
            styles: ComputedStyles {
                groups: vec![
                    (
                        StyleCategory::BoxModel,
                        vec![
                            cp("width", "32px"),
                            cp("margin-block-start", "0px"),
                            cp("margin-top", "0px"),
                            cp("margin-left", "0px"),
                            cp("padding-block-start", "8px"),
                            cp("padding-top", "8px"),
                        ],
                    ),
                    (
                        StyleCategory::Typography,
                        vec![cp("font-size", "16px"), cp("text-transform", "none")],
                    ),
                    (
                        StyleCategory::Variables,
                        vec![cp("--tenant-primary", "#1a3c6e"), cp("--local", "red")],
                    ),
                ],
            },
            pseudo: vec![],
            children: vec![],
        }
    }

    #[test]
    fn full_json_has_ids_and_readable_names() {
        let snap = sample_snapshot();
        let cfg = OutputConfig::default();
        let json = snapshot_to_json(&snap, &cfg, None);
        assert_eq!(json["id"], 1);
        assert_eq!(json["tag"], "SPAN");
        assert_eq!(json["depth"], 1);
        assert_eq!(json["styles"]["box_model"]["margin-block-start"], "0px");
        assert_eq!(json["styles"]["typography"]["font-size"], "16px");
    }

    #[test]
    fn compact_drops_logical_duplicates_and_noise() {
        let snap = sample_snapshot();
        let cfg = OutputConfig {
            compact: true,
            ..OutputConfig::default()
        };
        let json = snapshot_to_json(&snap, &cfg, None);
        let box_model = &json["styles"]["box_model"];
        // Logical duplicates of physical ones are removed.
        assert!(box_model.get("margin-block-start").is_none());
        assert!(box_model.get("padding-block-start").is_none());
        // margin-top: 0px is noise and dropped; padding-top: 8px survives.
        assert!(box_model.get("margin-top").is_none());
        assert_eq!(box_model["padding-top"], "8px");
        assert_eq!(box_model["width"], "32px");
        // Noise value suppressed on non-allow-listed property.
        assert!(json["styles"]["typography"].get("text-transform").is_none());
        // Allow-listed keep-default property survives.
        assert_eq!(json["styles"]["typography"]["font-size"], "16px");
    }

    #[test]
    fn compact_scopes_css_variables_against_global() {
        let snap = sample_snapshot();
        let global: HashMap<String, String> = [("--tenant-primary".into(), "#1a3c6e".into())]
            .into_iter()
            .collect();
        let cfg = OutputConfig {
            compact: true,
            ..OutputConfig::default()
        };
        let json = snapshot_to_json(&snap, &cfg, Some(&global));
        let vars = &json["styles"]["css_variables"];
        // Inherited value equal to global is dropped; override stays.
        assert!(vars.get("--tenant-primary").is_none());
        assert_eq!(vars["--local"], "red");
    }

    #[test]
    fn jsonl_output_emits_meta_line_in_compact_mode() {
        let snaps = vec![sample_snapshot()];
        let global = Some(vec![(
            "--tenant-primary".to_string(),
            "#1a3c6e".to_string(),
        )]);
        let outcome = crate::extractor::SniffOutcome {
            snapshots: snaps,
            global_css_variables: global,
            ax_tree: None,
        };

        let cfg = OutputConfig {
            compact: true,
            ..OutputConfig::default()
        };
        let mut buf = Vec::new();
        write_output(&mut buf, &outcome, &cfg).unwrap();
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        let meta: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(
            meta["__meta"]["css_variables"]["--tenant-primary"],
            "#1a3c6e"
        );
        let node: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(node["tag"], "SPAN");
    }

    #[test]
    fn flat_jsonl_emits_one_line_per_node_with_parent_id() {
        let child = ElementSnapshot {
            id: 2,
            parent_id: Some(1),
            tag: "B".into(),
            selector: "span > b".into(),
            path: "body > span.icon > b".into(),
            depth: 2,
            rect: None,
            metrics: None,
            is_visible: None,
            aria: None,
            contrast: None,
            ax: None,
            styles: ComputedStyles::default(),
            pseudo: vec![],
            children: vec![],
        };
        let mut snap = sample_snapshot();
        snap.children.push(child);
        let outcome = crate::extractor::SniffOutcome {
            snapshots: vec![snap],
            global_css_variables: None,
            ax_tree: None,
        };
        let cfg = OutputConfig {
            format: OutputFormat::JsonLinesFlat,
            ..OutputConfig::default()
        };
        let mut buf = Vec::new();
        write_output(&mut buf, &outcome, &cfg).unwrap();
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        let root: Value = serde_json::from_str(lines[0]).unwrap();
        let child_v: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(root["id"], 1);
        assert!(root.get("children").is_none());
        assert_eq!(child_v["parent_id"], 1);
        assert_eq!(child_v["tag"], "B");
    }

    #[test]
    fn json_array_wraps_global_meta_in_compact_mode() {
        let outcome = crate::extractor::SniffOutcome {
            snapshots: vec![sample_snapshot()],
            global_css_variables: Some(vec![("--x".to_string(), "1".to_string())]),
            ax_tree: None,
        };
        let cfg = OutputConfig {
            format: OutputFormat::Json,
            compact: true,
            ..OutputConfig::default()
        };
        let mut buf = Vec::new();
        write_output(&mut buf, &outcome, &cfg).unwrap();
        let v: Value = serde_json::from_slice(&buf).unwrap();
        assert!(v.get("__meta").is_some());
        assert_eq!(v["elements"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn emits_is_visible_when_present() {
        let mut snap = sample_snapshot();
        snap.is_visible = Some(true);
        let cfg = OutputConfig::default();
        let json = snapshot_to_json(&snap, &cfg, None);
        assert_eq!(json["is_visible"], true);
        assert_eq!(json["computed_style_hash"].as_str().unwrap().len(), 16);
    }

    #[test]
    fn omits_hash_and_visibility_when_disabled() {
        let mut snap = sample_snapshot();
        snap.is_visible = Some(true);
        let cfg = OutputConfig {
            include_style_hash: false,
            include_visibility: false,
            ..OutputConfig::default()
        };
        let json = snapshot_to_json(&snap, &cfg, None);
        assert!(json.get("computed_style_hash").is_none());
        assert!(json.get("is_visible").is_none());
    }

    #[test]
    fn style_hash_is_deterministic_and_sensitive_to_changes() {
        let a = sample_snapshot();
        let cfg = OutputConfig::default();
        let h1 = snapshot_to_json(&a, &cfg, None)["computed_style_hash"].clone();
        let h2 = snapshot_to_json(&a, &cfg, None)["computed_style_hash"].clone();
        assert_eq!(h1, h2, "identical styles must hash identically");

        let mut b = a;
        b.styles = ComputedStyles {
            groups: vec![(
                StyleCategory::BoxModel,
                vec![cp("width", "33px"), cp("padding-top", "8px")],
            )],
        };
        let hb = snapshot_to_json(&b, &cfg, None)["computed_style_hash"].clone();
        assert_ne!(h1, hb, "style change must change the hash");

        let mut c = sample_snapshot();
        c.pseudo.push(sniff_core::types::PseudoStyles {
            name: "::before".into(),
            styles: ComputedStyles {
                groups: vec![(StyleCategory::BoxModel, vec![cp("content", "\"x\"")])],
            },
        });
        let hc = snapshot_to_json(&c, &cfg, None)["computed_style_hash"].clone();
        assert_ne!(h1, hc, "pseudo styles must be covered by the hash");
    }

    #[test]
    fn compact_and_full_hashes_differ_by_mode() {
        let snap = sample_snapshot();
        let full = snapshot_to_json(&snap, &OutputConfig::default(), None);
        let compact = snapshot_to_json(
            &snap,
            &OutputConfig {
                compact: true,
                ..OutputConfig::default()
            },
            None,
        );
        assert_ne!(full["computed_style_hash"], compact["computed_style_hash"]);
    }

    #[test]
    fn serializes_aria_contrast_and_ax_facets() {
        use sniff_core::types::{AriaInfo, AxInfo, ContrastInfo, TriState};
        let mut snap = sample_snapshot();
        snap.aria = Some(AriaInfo {
            role: Some("button".into()),
            name: Some("Confirmar".into()),
            focusable: true,
            ..Default::default()
        });
        snap.contrast = Some(ContrastInfo {
            ratio: 4.54,
            foreground: "#2563eb".into(),
            background: "#ffffff".into(),
            large: false,
            aa: TriState::Pass,
            aaa: TriState::Fail,
            unknown_reason: None,
        });
        snap.ax = Some(AxInfo {
            role: Some("button".into()),
            name: Some("Confirmar".into()),
            ignored: false,
            ..Default::default()
        });
        let cfg = OutputConfig {
            include_aria: true,
            include_contrast: true,
            include_ax: true,
            ..OutputConfig::default()
        };
        let json = snapshot_to_json(&snap, &cfg, None);
        assert_eq!(json["aria"]["role"], "button");
        assert_eq!(json["aria"]["focusable"], true);
        assert_eq!(json["contrast"]["ratio"], 4.54);
        assert_eq!(json["contrast"]["aa"], "pass");
        assert_eq!(json["contrast"]["aaa"], "fail");
        assert_eq!(json["ax"]["role"], "button");
        assert_eq!(json["ax"]["ignored"], false);
    }

    #[test]
    fn ax_tree_emitted_as_dedicated_line_in_jsonl() {
        let outcome = crate::extractor::SniffOutcome {
            snapshots: vec![sample_snapshot()],
            global_css_variables: None,
            ax_tree: Some(serde_json::json!([
                {"role": "group", "children": [{"role": "button"}]}
            ])),
        };
        let cfg = OutputConfig::default();
        let mut buf = Vec::new();
        write_output(&mut buf, &outcome, &cfg).unwrap();
        let text = String::from_utf8(buf).unwrap();
        let first: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(first["__ax_tree"][0]["role"], "group");
        assert_eq!(first["__ax_tree"][0]["children"][0]["role"], "button");
    }

    #[test]
    fn json_mode_embeds_ax_tree_document() {
        let outcome = crate::extractor::SniffOutcome {
            snapshots: vec![sample_snapshot()],
            global_css_variables: None,
            ax_tree: Some(serde_json::json!([{"role": "main"}])),
        };
        let cfg = OutputConfig {
            format: OutputFormat::Json,
            ..OutputConfig::default()
        };
        let mut buf = Vec::new();
        write_output(&mut buf, &outcome, &cfg).unwrap();
        let v: Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(v["__ax_tree"][0]["role"], "main");
        assert_eq!(v["elements"].as_array().unwrap().len(), 1);
    }
}
