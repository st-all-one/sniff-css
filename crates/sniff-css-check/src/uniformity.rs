//! Uniformity check: find the "odd one out" among repeated instances.
//!
//! Given a snapshot forest whose roots are sibling instances of the same
//! selector (e.g. every `.card` in a grid), the group norm is computed per
//! property (median for numbers, mode otherwise) and instances that deviate
//! beyond the tolerance are reported as outliers — with the exact properties
//! and magnitudes that deviate. No LLM: this discovers *which* instance is
//! inconsistent and *in what*, deterministically.

use serde_json::Value;
use sniff_css_diff::DiffNode;

/// A single property where an instance deviates from the group norm.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Deviation {
    /// Property path, e.g. `box_model.width` or `rect.x`.
    pub property: String,
    /// The group norm value (median/mode) this instance differs from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub norm: Option<String>,
    /// This instance's value.
    pub value: String,
    /// Magnitude of the deviation, when numeric.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magnitude: Option<f64>,
}

/// One instance flagged as inconsistent.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Outlier {
    /// Selector of the deviating instance.
    pub selector: String,
    pub deviations: Vec<Deviation>,
}

/// Result of a uniformity run.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct UniformityReport {
    /// Number of sibling instances considered.
    pub instances: usize,
    /// Instances that deviate from the group norm.
    pub outliers: Vec<Outlier>,
}

/// A node paired with its flattened `(path, value)` property list.
type Instance<'a> = (&'a DiffNode, Vec<(String, Option<String>)>);

/// Run the uniformity check over a snapshot forest.
///
/// `tolerance` mirrors `sniffCSS-diff`: numeric differences below it (in the
/// same unit) are considered equal. With fewer than 2 instances the check
/// reports zero outliers (`instances <= 1`).
pub fn check_uniformity(nodes: &[DiffNode], tolerance: f64) -> UniformityReport {
    let instances: Vec<Instance<'_>> = nodes.iter().map(|n| (n, flat_props(n))).collect();
    let count = instances.len();
    if count < 2 {
        return UniformityReport {
            instances: count,
            outliers: Vec::new(),
        };
    }

    // Union of all property keys.
    let mut keys: Vec<String> = Vec::new();
    for (_, props) in &instances {
        for (k, _) in props {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
    }
    keys.sort_unstable();

    let mut outliers: Vec<Outlier> = Vec::new();
    for key in &keys {
        // Value of this property for every instance (index-aligned).
        let per_instance: Vec<Option<&String>> = instances
            .iter()
            .map(|(_, props)| {
                props
                    .iter()
                    .find(|(k, _)| k == key)
                    .and_then(|(_, v)| v.as_ref())
            })
            .collect();
        let present: Vec<&String> = per_instance.iter().filter_map(|v| *v).collect();
        if present.is_empty() {
            continue;
        }

        // Instance(s) missing the property deviate from those that have it.
        if per_instance.len() != present.len() {
            for (i, (node, _)) in instances.iter().enumerate() {
                if per_instance[i].is_some() {
                    continue;
                }
                push_deviation(&mut outliers, node, key, present[0], "(missing)", None);
            }
            continue;
        }

        if let Some((norm_value, numeric)) = group_norm(&present) {
            for (i, (node, _)) in instances.iter().enumerate() {
                let mine = per_instance[i].expect("present").as_str();
                let magnitude = if numeric {
                    match (strip_number(&norm_value), strip_number(mine)) {
                        (Some((nn, nu)), Some((mn, mu))) if nu == mu => Some((mn - nn).abs()),
                        _ => None,
                    }
                } else {
                    None
                };
                let within_tolerance = numeric
                    && magnitude.is_some_and(|m| m <= tolerance)
                    && strip_number(&norm_value).map(|(_, u)| u)
                        == strip_number(mine).map(|(_, u)| u);
                if !within_tolerance && mine != norm_value {
                    push_deviation(&mut outliers, node, key, &norm_value, mine, magnitude);
                }
            }
        }
    }

    UniformityReport {
        instances: count,
        outliers,
    }
}

/// Append (or extend) an outlier for `node` with a single deviation.
fn push_deviation(
    outliers: &mut Vec<Outlier>,
    node: &DiffNode,
    key: &str,
    norm: &str,
    value: &str,
    magnitude: Option<f64>,
) {
    let deviation = Deviation {
        property: key.to_string(),
        norm: Some(norm.to_string()),
        value: value.to_string(),
        magnitude,
    };
    match outliers.iter_mut().find(|o| o.selector == node.selector) {
        Some(outlier) => outlier.deviations.push(deviation),
        None => outliers.push(Outlier {
            selector: node.selector.clone(),
            deviations: vec![deviation],
        }),
    }
}

/// Flatten a node into a sorted `(path, value)` property list.
fn flat_props(node: &DiffNode) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    if let Some(styles) = &node.styles {
        for (cat, group) in styles {
            if let Some(group) = group.as_object() {
                for (prop, value) in group {
                    out.push((format!("{cat}.{prop}"), value.as_str().map(String::from)));
                }
            }
        }
    }
    if let Some(rect) = &node.rect
        && let Some(o) = rect.as_object()
    {
        // Compare the layout footprint (width/height), not the absolute
        // position (x/y) — siblings in normal flow always differ in y.
        for k in ["width", "height"] {
            if let Some(v) = o.get(k).and_then(Value::as_f64) {
                out.push((format!("rect.{k}"), Some(v.to_string())));
            }
        }
    }
    if let Some(v) = node.display_visible() {
        out.push((
            "is_user_noticeable.display_visible".into(),
            Some(v.to_string()),
        ));
    }
    if let Some(v) = node.accessibility_grade() {
        out.push((
            "is_user_noticeable.accessibility_grade".into(),
            Some(v.to_string()),
        ));
    }
    out.sort_unstable();
    out
}

/// The group norm: `Some((norm_value, is_numeric))` when the group is not
/// uniform; `None` when every value is equal.
///
/// Numeric values (same unit) use the median as the norm; categorical
/// values use the mode.
fn group_norm(values: &[&String]) -> Option<(String, bool)> {
    if values.iter().all(|v| v == &values[0]) {
        return None;
    }
    // Try numeric: all values parse as a number with the same unit.
    let nums: Vec<(f64, &str)> = values.iter().filter_map(|v| strip_number(v)).collect();
    if nums.len() == values.len() {
        let unit = nums[0].1;
        if nums.iter().all(|(_, u)| *u == unit) {
            let mut sorted: Vec<f64> = nums.iter().map(|(n, _)| *n).collect();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = median(&sorted);
            return Some((format!("{median}{unit}"), true));
        }
    }
    // Categorical: modal value.
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for v in values {
        *counts.entry(v.as_str()).or_insert(0) += 1;
    }
    let mode = counts
        .iter()
        .max_by_key(|(_, c)| *c)
        .map(|(k, _)| *k)
        .unwrap_or(values[0]);
    Some((mode.to_string(), false))
}

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Split a CSS value into a leading number and the unit remainder.
fn strip_number(s: &str) -> Option<(f64, &str)> {
    let s = s.trim();
    let idx = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(s.len());
    if idx == 0 {
        return None;
    }
    let (num, rest) = s.split_at(idx);
    num.parse().ok().map(|n| (n, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(selector: &str, width: &str, height: &str) -> DiffNode {
        DiffNode {
            id: 0,
            parent_id: None,
            selector: selector.to_string(),
            tag: Some("DIV".into()),
            path: Some(selector.to_string()),
            depth: Some(0),
            rect: None,
            metrics: None,
            noticeable: Some(serde_json::json!({
                "display_visible": true, "accessibility_grade": "AAA"
            })),
            hash: None,
            styles: Some(
                serde_json::json!({
                    "box_model": {"width": width, "height": height},
                    "typography": {"font-size": "16px"}
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            pseudo: None,
            aria: None,
            contrast: None,
            ax: None,
            children: vec![],
        }
    }

    #[test]
    fn uniform_group_has_no_outliers() {
        let nodes = vec![
            node("div.card:nth-child(1)", "300px", "120px"),
            node("div.card:nth-child(2)", "300px", "120px"),
            node("div.card:nth-child(3)", "300px", "120px"),
        ];
        let report = check_uniformity(&nodes, 0.5);
        assert_eq!(report.instances, 3);
        assert!(report.outliers.is_empty());
    }

    #[test]
    fn odd_card_out_is_detected() {
        let nodes = vec![
            node("div.card:nth-child(1)", "300px", "120px"),
            node("div.card:nth-child(2)", "300px", "120px"),
            node("div.card:nth-child(3)", "300px", "80px"),
        ];
        let report = check_uniformity(&nodes, 0.5);
        assert_eq!(report.outliers.len(), 1);
        let outlier = &report.outliers[0];
        assert_eq!(outlier.selector, "div.card:nth-child(3)");
        let height = outlier
            .deviations
            .iter()
            .find(|d| d.property == "box_model.height")
            .expect("height deviation");
        assert_eq!(height.norm.as_deref(), Some("120px"));
        assert_eq!(height.value, "80px");
        assert!((height.magnitude.unwrap() - 40.0).abs() < 1e-9);
    }

    #[test]
    fn subpixel_jitter_within_tolerance_is_ignored() {
        let mut a = node("div.a", "300px", "120px");
        let mut b = node("div.b", "300px", "120px");
        a.styles = Some(
            serde_json::json!({"box_model": {"width": "300px", "height": "120.4px"}})
                .as_object()
                .unwrap()
                .clone(),
        );
        b.styles = Some(
            serde_json::json!({"box_model": {"width": "300px", "height": "120px"}})
                .as_object()
                .unwrap()
                .clone(),
        );
        let report = check_uniformity(&[a, b], 0.5);
        assert!(report.outliers.is_empty());
    }

    #[test]
    fn missing_property_is_a_deviation() {
        let nodes = vec![
            node("div.a", "300px", "120px"),
            node("div.b", "300px", "120px"),
            node("div.c", "300px", "120px"),
        ];
        let mut custom = nodes.clone();
        custom[2].styles = Some(
            serde_json::json!({"box_model": {"width": "300px"}})
                .as_object()
                .unwrap()
                .clone(),
        );
        let report = check_uniformity(&custom, 0.5);
        assert_eq!(report.outliers.len(), 1);
        assert!(
            report.outliers[0]
                .deviations
                .iter()
                .any(|d| d.property == "box_model.height" && d.value == "(missing)")
        );
    }

    #[test]
    fn single_instance_reports_zero_outliers() {
        let report = check_uniformity(&[node("div.card", "300px", "120px")], 0.5);
        assert_eq!(report.instances, 1);
        assert!(report.outliers.is_empty());
    }

    #[test]
    fn categorical_deviation_uses_mode() {
        let nodes = vec![
            node("div.a", "300px", "120px"),
            node("div.b", "300px", "120px"),
            node("div.c", "300px", "120px"),
        ];
        let mut custom = nodes;
        custom[1].noticeable = Some(serde_json::json!({
            "display_visible": false, "accessibility_grade": "NONE"
        }));
        let report = check_uniformity(&custom, 0.5);
        assert_eq!(report.outliers.len(), 1);
        assert_eq!(report.outliers[0].selector, "div.b");
        assert!(
            report.outliers[0]
                .deviations
                .iter()
                .any(|d| d.property == "is_user_noticeable.display_visible")
        );
    }
}
