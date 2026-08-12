//! UI-effect mapping for user-interaction actions.
//!
//! After each action (click / hover / type) the engine captures a lightweight
//! snapshot of the *whole page* before and after the interaction and diffs the
//! two in Rust. The resulting report answers **what** happened at the UI level
//! (elements appeared/disappeared/changed) and **where** (rect, on-screen vs
//! out-of-view with px offsets, distance from the action point) — including the
//! "nothing happened" case (possible logic failure). Reports are emitted in the
//! reserved `__actions` output area.
//!
//! The per-element record is deliberately small: a stable key, tag, rect,
//! visibility, on-screen flag, a curated CSS *signature* (~38 visual/layout
//! properties) and the implicit ARIA role/name.
//!
//! Token hygiene is a contract: the full 38-property signature is emitted only
//! once per action entry as `css_keys`, and per-element records reference it by
//! index (`css_before_values`/`css_after_values` arrays). `changed` records
//! carry `css_diff` (only the properties that differ beyond tolerance, with
//! `before`/`after`), never the full maps. Root nodes (`html`/`body`) only
//! report theme/visual changes (geometry reflow is suppressed), and empty
//! fields are omitted entirely.

use serde_json::{Map, Value, json};
use sniff_core::{Action, SniffError, SniffResult};

use crate::action::ActionTarget;

/// Curated visual/layout properties captured for every element in a page
/// snapshot (the `css_keys` schema shared by all `css_*_values` arrays). Small
/// enough to keep the before-map cheap and the diff deterministic, large enough
/// to tell "a table appeared here" apart from "the page just reflowed".
pub const EFFECT_SIGNATURE_PROPS: &[&str] = &[
    "display",
    "position",
    "z-index",
    "top",
    "right",
    "bottom",
    "left",
    "width",
    "height",
    "min-width",
    "min-height",
    "max-width",
    "max-height",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "border-top-width",
    "border-right-width",
    "border-bottom-width",
    "border-left-width",
    "font-size",
    "font-weight",
    "line-height",
    "text-align",
    "color",
    "background-color",
    "background-image",
    "opacity",
    "visibility",
    "border-radius",
    "box-shadow",
    "transform",
    "overflow",
];

/// Tolerance for treating a rect move as a real "moved" change (px), matching
/// the main diff's default so subpixel jitter is ignored.
const RECT_TOLERANCE: f64 = 0.5;

/// Tolerance for treating two CSS signature values as equal (same unit, diff
/// within this magnitude, e.g. `16px` vs `16.2px`). Applied per property.
const SIGNATURE_TOLERANCE: f64 = 0.5;

/// Theme/visual subset of [`EFFECT_SIGNATURE_PROPS`] reported for the root
/// nodes `html`/`body`. Geometry, spacing and box-model props are excluded so a
/// page reflow (scrollbar, viewport growth, padding compensation) never floods
/// `changed` — roots only surface theme-level, stacking and visibility changes.
pub const ROOT_SIGNATURE_PROPS: &[&str] = &[
    "display",
    "position",
    "z-index",
    "font-family",
    "font-size",
    "font-weight",
    "line-height",
    "text-align",
    "color",
    "background-color",
    "background-image",
    "opacity",
    "visibility",
    "border-radius",
    "box-shadow",
    "transform",
    "overflow",
];

/// One pass over `document.querySelectorAll('*')` producing the compact
/// per-element records consumed by [`diff`]. Runs in a single
/// `Runtime.evaluate`; the resulting JSON is small (per element: key, tag,
/// rect, two flags, signature object, text/aria when visible).
const CAPTURE_JS: &str = r#"
(args) => {
  const stableKey = args.stableKey || null;
  const SIG = args.signature;
  const vw = window.innerWidth, vh = window.innerHeight;
  const esc = (v) => String(v).replace(/\\/g, '\\\\').replace(/"/g, '\\"');

  function anchorOf(el) {
    if (el.id) return '#' + el.id;
    if (stableKey) { const v = el.getAttribute(stableKey); if (v) return '[' + stableKey + '="' + esc(v) + '"]'; }
    return null;
  }
  function token(el) {
    const a = anchorOf(el);
    if (a) return el.tagName.toLowerCase() + a;
    const cls = el.classList;
    if (cls && cls.length) return el.tagName.toLowerCase() + '.' + cls[0];
    return el.tagName.toLowerCase();
  }
  function pathOf(el) {
    const parts = [];
    let cur = el;
    let n = 0;
    while (cur && cur.nodeType === 1 && n < 4) {
      let tok = token(cur);
      if (anchorOf(cur)) { parts.unshift(tok); break; }
      const parent = cur.parentElement;
      const sibs = parent ? Array.prototype.slice.call(parent.children) : [];
      if (sibs.length > 1) tok += ':nth-child(' + (sibs.indexOf(cur) + 1) + ')';
      parts.unshift(tok);
      cur = parent;
      n += 1;
    }
    return parts.join(' > ');
  }

  function normColor(value) {
    if (typeof value !== 'string' || value.indexOf('rgb') < 0) return value;
    return value.replace(/rgba?\(([^)]*)\)/g, function (m, inner) {
      const parts = inner.split(/[,\s/]+/).filter(Boolean);
      if (parts.length < 3) return m;
      const r = parseInt(parts[0], 10), g = parseInt(parts[1], 10), b = parseInt(parts[2], 10);
      let a;
      if (parts[3] !== undefined) a = parts[3].indexOf('%') >= 0 ? parseFloat(parts[3]) / 100 : parseFloat(parts[3]);
      const h = (v) => v.toString(16).padStart(2, '0');
      if (a === undefined || a >= 1) return '#' + h(r) + h(g) + h(b);
      return '#' + h(r) + h(g) + h(b) + h(Math.round(a * 255));
    });
  }
  function signature(cs) {
    const out = {};
    for (let i = 0; i < SIG.length; i++) {
      const name = SIG[i];
      out[name] = normColor(cs.getPropertyValue(name));
    }
    return out;
  }

  function accessibleName(el) {
    const lb = el.getAttribute('aria-labelledby');
    if (lb) {
      const parts = lb.split(/\s+/)
        .map((id) => { const n = document.getElementById(id); return n ? (n.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 60) : ''; })
        .filter(Boolean);
      if (parts.length) return parts.join(' ');
    }
    const l = el.getAttribute('aria-label');
    if (l && l.trim()) return l.trim();
    const alt = el.getAttribute('alt');
    if (alt !== null && el.tagName === 'IMG') return alt;
    const t = el.getAttribute('title');
    if (t) return t;
    return '';
  }
  function implicitRole(el) {
    switch (el.tagName) {
      case 'A': case 'AREA': return el.hasAttribute('href') ? 'link' : null;
      case 'BUTTON': return 'button';
      case 'INPUT': {
        const ty = (el.getAttribute('type') || 'text').toLowerCase();
        if (ty === 'button' || ty === 'submit' || ty === 'reset' || ty === 'image') return 'button';
        if (ty === 'checkbox') return 'checkbox';
        if (ty === 'radio') return 'radio';
        if (ty === 'range') return 'slider';
        if (ty === 'search') return 'searchbox';
        return 'textbox';
      }
      case 'SELECT': return (el.hasAttribute('multiple') || parseInt(el.getAttribute('size') || '0', 10) > 1) ? 'listbox' : 'combobox';
      case 'OPTION': return 'option';
      case 'TEXTAREA': return 'textbox';
      case 'NAV': return 'navigation';
      case 'MAIN': return 'main';
      case 'IMG': return (el.getAttribute('alt') === '') ? 'presentation' : 'img';
      case 'H1': case 'H2': case 'H3': case 'H4': case 'H5': case 'H6': return 'heading';
      case 'TABLE': return 'table';
      case 'TR': return 'row';
      case 'TD': return 'cell';
      case 'TH': return 'columnheader';
      case 'DIALOG': return 'dialog';
      default: return null;
    }
  }

  const all = document.querySelectorAll('*');
  const out = [];
  for (let i = 0; i < all.length; i++) {
    const el = all[i];
    if (el.ownerSVGElement || el.tagName === 'SVG') continue;
    const tag = el.tagName;
    if (tag === 'SCRIPT' || tag === 'STYLE' || tag === 'NOSCRIPT' || tag === 'TEMPLATE' || tag === 'TITLE') continue;
    const cs = getComputedStyle(el);
    const r = el.getBoundingClientRect();
    const visible = cs.display !== 'none' && cs.visibility !== 'hidden'
      && parseFloat(cs.opacity) > 0 && (r.width > 0 || r.height > 0);
    const onscreen = r.x < vw && r.y < vh && r.x + r.width > 0 && r.y + r.height > 0;
    const rec = {
      k: pathOf(el),
      t: tag,
      r: [r.x, r.y, r.width, r.height],
      v: visible,
      o: onscreen,
      s: signature(cs),
    };
    if (visible && r.width >= 2 && r.height >= 2) {
      const txt = (el.innerText || el.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 60);
      if (txt) rec.x = txt;
      const role = el.getAttribute('role') || implicitRole(el);
      const name = accessibleName(el);
      if (role || name) rec.a = { role: role || null, name: name || null };
    }
    out.push(rec);
  }
  return { viewport: [vw, vh], elements: out };
}
"#;

/// A parsed per-element record from a page snapshot.
#[derive(Debug, Clone)]
struct ElementRec {
    key: String,
    tag: String,
    rect: (f64, f64, f64, f64),
    visible: bool,
    signature: Map<String, Value>,
    text: Option<String>,
    aria: Option<Value>,
}

/// Capture the current page as a compact effect snapshot.
pub async fn capture(
    session: &sniff_cdp::session::CdpSession,
    stable_key: Option<&str>,
) -> SniffResult<Value> {
    let args = json!({
        "stableKey": stable_key,
        "signature": EFFECT_SIGNATURE_PROPS,
    });
    let args_json = serde_json::to_string(&args).map_err(SniffError::from)?;
    let expression = format!("({CAPTURE_JS})\n({args_json})");
    session
        .evaluate(&expression, false)
        .await
        .map_err(|e| SniffError::Cdp(e.to_string()))
}

/// Diff two page snapshots around one action and build the effect report.
///
/// The report is a JSON object ready to be pushed into the `__actions` output
/// area. Pure and deterministic (no I/O), so it is unit-testable.
pub fn diff(
    before: &Value,
    after: &Value,
    target: &ActionTarget,
    action: &Action,
    index: usize,
    limit: usize,
) -> Value {
    let viewport = parse_viewport(after);
    let (vw, vh) = viewport;

    let before_map = keyed(parse_elements(before));
    let after_map = keyed(parse_elements(after));

    let mut appeared: Vec<Value> = Vec::new();
    let mut removed: Vec<Value> = Vec::new();
    let mut changed: Vec<Value> = Vec::new();
    let mut n_revealed = 0usize;
    let mut n_hidden = 0usize;

    for (key, rec) in &after_map {
        match before_map.get(key) {
            None => {
                appeared.push(appeared_entry(rec, target, vw, vh));
            }
            Some(before_rec) => {
                let visible_changed = before_rec.visible != rec.visible;
                let root = is_root(rec);
                let changed_props =
                    signature_diff_keys(&before_rec.signature, &rec.signature, root);
                let sig_changed = !changed_props.is_empty();
                // Geometry reflow of the root nodes is noise (scrollbar width,
                // viewport growth, padding compensation) — never report it.
                let moved = !root && rect_delta(&before_rec.rect, &rec.rect);
                if visible_changed {
                    if rec.visible {
                        n_revealed += 1;
                    } else {
                        n_hidden += 1;
                    }
                }
                if (visible_changed || sig_changed || moved)
                    && let Some(entry) = changed_entry(
                        before_rec,
                        rec,
                        target,
                        vw,
                        vh,
                        visible_changed,
                        changed_props,
                        moved,
                    )
                {
                    changed.push(entry);
                }
            }
        }
    }
    for (key, rec) in &before_map {
        if !after_map.contains_key(key) {
            removed.push(removed_entry(rec, target, vw, vh));
        }
    }

    let sort_key = |v: &Value| {
        let area = v.get("rect").and_then(rect_of).map(area).unwrap_or(0.0);
        std::cmp::Reverse((
            area as i64,
            v.get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        ))
    };
    appeared.sort_by_key(sort_key);
    removed.sort_by_key(sort_key);
    // Semantic changes (css_diff/visibility) first, then largest area — so a
    // busy page opens with the entries that carry the most information.
    let changed_sort_key = |v: &Value| {
        let semantic = v
            .get("css_diff")
            .and_then(Value::as_object)
            .is_some_and(|o| !o.is_empty())
            || v.get("visible_before").is_some();
        let area = v.get("rect").and_then(rect_of).map(area).unwrap_or(0.0);
        std::cmp::Reverse((
            semantic as u8,
            area as i64,
            v.get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        ))
    };
    changed.sort_by_key(changed_sort_key);
    if limit > 0 {
        appeared.truncate(limit);
        removed.truncate(limit);
        changed.truncate(limit);
    }

    let effect = classify_effect(&appeared, &removed, &changed, n_revealed, n_hidden);
    let summary = build_summary(
        effect,
        appeared.len(),
        removed.len(),
        changed.len(),
        n_revealed,
        n_hidden,
        appeared.first(),
        target,
        viewport,
    );

    let mut entry = Map::new();
    entry.insert("index".into(), Value::from(index));
    entry.insert(
        "action".into(),
        Value::String(action_kind(action).to_string()),
    );
    entry.insert("selector".into(), Value::String(action_selector(action)));
    entry.insert("timeout_ms".into(), Value::from(action_timeout_ms(action)));
    entry.insert("settle_ms".into(), Value::from(action_settle_ms(action)));
    let mut target_obj = Map::new();
    target_obj.insert("path".into(), Value::String(target.path.clone()));
    target_obj.insert("rect".into(), rect_to_json(target.rect));
    target_obj.insert(
        "onscreen".into(),
        Value::Bool(rect_onscreen(target.rect, vw, vh)),
    );
    entry.insert("target".into(), Value::Object(target_obj));
    entry.insert("effect".into(), Value::String(effect.to_string()));
    entry.insert("summary".into(), Value::String(summary));
    // The 38-property signature schema is emitted once per entry that carries
    // appeared/removed records; per-node `css_*_values` arrays index into it.
    if !appeared.is_empty() || !removed.is_empty() {
        entry.insert(
            "css_keys".into(),
            Value::Array(
                EFFECT_SIGNATURE_PROPS
                    .iter()
                    .map(|p| Value::String((*p).to_string()))
                    .collect(),
            ),
        );
    }
    entry.insert("appeared".into(), Value::Array(appeared));
    entry.insert("removed".into(), Value::Array(removed));
    entry.insert("changed".into(), Value::Array(changed));
    Value::Object(entry)
}

// ---------------------------------------------------------------------------
// Classification & summaries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Effect {
    Revealed,
    Hidden,
    Changed,
    Moved,
    None,
}

impl std::fmt::Display for Effect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Effect::Revealed => "revealed",
            Effect::Hidden => "hidden",
            Effect::Changed => "changed",
            Effect::Moved => "moved",
            Effect::None => "no_effect",
        })
    }
}

fn classify_effect(
    appeared: &[Value],
    removed: &[Value],
    changed: &[Value],
    n_revealed: usize,
    n_hidden: usize,
) -> Effect {
    if !appeared.is_empty() || n_revealed > 0 {
        Effect::Revealed
    } else if !removed.is_empty() || n_hidden > 0 {
        Effect::Hidden
    } else if changed.iter().any(|c| {
        c.get("css_diff")
            .and_then(Value::as_object)
            .is_some_and(|o| !o.is_empty())
    }) {
        Effect::Changed
    } else if !changed.is_empty() {
        Effect::Moved
    } else {
        Effect::None
    }
}

#[allow(clippy::too_many_arguments)]
fn build_summary(
    effect: Effect,
    n_appeared: usize,
    n_removed: usize,
    n_changed: usize,
    n_revealed: usize,
    n_hidden: usize,
    biggest: Option<&Value>,
    target: &ActionTarget,
    viewport: (f64, f64),
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if n_appeared > 0 {
        parts.push(format!("{} element(s) appeared", n_appeared));
    }
    if n_revealed > 0 {
        parts.push(format!("{} became visible", n_revealed));
    }
    if n_removed > 0 {
        parts.push(format!("{} removed", n_removed));
    }
    if n_hidden > 0 {
        parts.push(format!("{} became hidden", n_hidden));
    }
    if n_changed > 0 {
        parts.push(format!("{} changed style/position", n_changed));
    }
    if effect == Effect::None {
        return "no DOM change within the settle window (the action may have had no handler, or the effect is async — raise settle_ms/--wait to observe it)".to_string();
    }
    let mut summary = if parts.is_empty() {
        "UI changed".to_string()
    } else {
        parts.join(" · ")
    };
    if let Some(rec) = biggest {
        let (x, y, w, h) = rec_rect(rec);
        let (vw, vh) = viewport;
        let where_note = if rect_onscreen((x, y, w, h), vw, vh) {
            "on-screen".to_string()
        } else {
            let oov = out_of_view((x, y, w, h), vw, vh);
            let dirs = oov_dirs(&oov);
            if dirs.is_empty() {
                "off-screen".to_string()
            } else {
                format!("{}px {}", dirs[0].1 as u64, dirs[0].0)
            }
        };
        let dist = distance_from_point((x, y, w, h), target.center.0, target.center.1) as u64;
        summary.push_str(&format!(
            " · biggest: {} {} — {}px from click",
            rec_tag(rec),
            where_note,
            dist,
        ));
    }
    summary
}

/// Out-of-view amounts as `(direction_label, px)` sorted by magnitude desc.
fn oov_dirs(oov: &Value) -> Vec<(&str, f64)> {
    let mut dirs: Vec<(&str, f64)> = [
        (
            "above",
            oov.get("above").and_then(Value::as_f64).unwrap_or(0.0),
        ),
        (
            "below",
            oov.get("below").and_then(Value::as_f64).unwrap_or(0.0),
        ),
        (
            "left",
            oov.get("left").and_then(Value::as_f64).unwrap_or(0.0),
        ),
        (
            "right",
            oov.get("right").and_then(Value::as_f64).unwrap_or(0.0),
        ),
    ]
    .to_vec();
    dirs.retain(|(_, v)| *v > 0.5);
    dirs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    dirs
}

// ---------------------------------------------------------------------------
// Entry builders
// ---------------------------------------------------------------------------

fn base_entry(rec: &ElementRec, target: &ActionTarget, vw: f64, vh: f64) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("tag".into(), Value::String(rec.tag.clone()));
    m.insert("path".into(), Value::String(rec.key.clone()));
    m.insert("rect".into(), rect_to_json(rec.rect));
    // Geometry is recomputed here (not trusted from the JS flag) so the
    // report's on-screen/out-of-view math is consistent with viewport.
    m.insert(
        "onscreen".into(),
        Value::Bool(rect_onscreen(rec.rect, vw, vh)),
    );
    let oov = out_of_view(rec.rect, vw, vh);
    if oov.as_object().is_some_and(|o| !o.is_empty()) {
        m.insert("out_of_view".into(), oov);
    }
    let dist = distance_from_point(rec.rect, target.center.0, target.center.1);
    m.insert("distance_from_action".into(), Value::from(dist.round()));
    m.insert(
        "direction".into(),
        Value::String(direction_from_point(rec.rect, target.center.0, target.center.1).to_string()),
    );
    if let Some(z) = rec.signature.get("z-index") {
        m.insert("z_index".into(), z.clone());
    }
    if let Some(text) = &rec.text {
        m.insert("text".into(), Value::String(text.clone()));
    }
    if let Some(aria) = &rec.aria {
        m.insert("aria".into(), aria.clone());
    }
    m
}

fn appeared_entry(rec: &ElementRec, target: &ActionTarget, vw: f64, vh: f64) -> Value {
    let mut m = base_entry(rec, target, vw, vh);
    m.insert("css_after_values".into(), signature_values(&rec.signature));
    Value::Object(m)
}

fn removed_entry(rec: &ElementRec, target: &ActionTarget, vw: f64, vh: f64) -> Value {
    let mut m = base_entry(rec, target, vw, vh);
    m.insert("css_before_values".into(), signature_values(&rec.signature));
    Value::Object(m)
}

/// The 38-property signature as a value array aligned to `css_keys` (missing
/// props padded with `null`, so every array has the same length/order).
fn signature_values(sig: &Map<String, Value>) -> Value {
    Value::Array(
        EFFECT_SIGNATURE_PROPS
            .iter()
            .map(|p| sig.get(*p).cloned().unwrap_or(Value::Null))
            .collect(),
    )
}

/// Property names (in canonical order) whose signature value differs beyond
/// [`SIGNATURE_TOLERANCE`]. For root nodes only [`ROOT_SIGNATURE_PROPS`] are
/// compared, so pure geometry reflow never marks `html`/`body` as changed.
fn signature_diff_keys(
    before: &Map<String, Value>,
    after: &Map<String, Value>,
    root: bool,
) -> Vec<String> {
    let props: &[&str] = if root {
        ROOT_SIGNATURE_PROPS
    } else {
        EFFECT_SIGNATURE_PROPS
    };
    props
        .iter()
        .filter(|p| !css_value_equal(before.get(**p), after.get(**p)))
        .map(|p| p.to_string())
        .collect()
}

fn is_root(rec: &ElementRec) -> bool {
    rec.tag == "HTML" || rec.tag == "BODY"
}

/// Signature-value equality applying [`SIGNATURE_TOLERANCE`] to numbers (and
/// strings carrying numbers), so subpixel jitter doesn't show up in `css_diff`.
fn css_value_equal(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(Value::String(x)), Some(Value::String(y))) => {
            x == y || numeric_close(x, y, SIGNATURE_TOLERANCE)
        }
        (Some(Value::Number(x)), Some(Value::Number(y))) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => x == y || (x - y).abs() <= SIGNATURE_TOLERANCE,
            _ => x == y,
        },
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn changed_entry(
    before: &ElementRec,
    after: &ElementRec,
    target: &ActionTarget,
    vw: f64,
    vh: f64,
    visible_changed: bool,
    changed_props: Vec<String>,
    moved: bool,
) -> Option<Value> {
    // Skip nodes that only moved (roots never get here with `moved`, geometry
    // reflow being suppressed) — a pure reflow carries no UI-level signal.
    if !visible_changed && changed_props.is_empty() && !moved {
        return None;
    }
    let mut m = base_entry(after, target, vw, vh);
    if moved {
        m.insert("rect_before".into(), rect_to_json(before.rect));
        m.insert("rect_after".into(), rect_to_json(after.rect));
    }
    if visible_changed {
        m.insert("visible_before".into(), Value::Bool(before.visible));
        m.insert("visible_after".into(), Value::Bool(after.visible));
    }
    if !changed_props.is_empty() {
        let mut diff = Map::new();
        for k in &changed_props {
            let mut pair = Map::new();
            pair.insert(
                "before".into(),
                before.signature.get(k).cloned().unwrap_or(Value::Null),
            );
            pair.insert(
                "after".into(),
                after.signature.get(k).cloned().unwrap_or(Value::Null),
            );
            diff.insert(k.clone(), Value::Object(pair));
        }
        m.insert("css_diff".into(), Value::Object(diff));
    }
    Some(Value::Object(m))
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

fn parse_viewport(v: &Value) -> (f64, f64) {
    let arr = v.get("viewport").and_then(Value::as_array);
    match arr {
        Some(a) if a.len() >= 2 => (a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0)),
        _ => (0.0, 0.0),
    }
}

fn parse_elements(v: &Value) -> Vec<ElementRec> {
    let mut out = Vec::new();
    let Some(arr) = v.get("elements").and_then(Value::as_array) else {
        return out;
    };
    for el in arr {
        let key = el
            .get("k")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let tag = el
            .get("t")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let rect = parse_rect(el.get("r"));
        let visible = el.get("v").and_then(Value::as_bool).unwrap_or(false);
        let signature = el
            .get("s")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let text = el.get("x").and_then(Value::as_str).map(String::from);
        let aria = el.get("a").cloned().filter(|v| !v.is_null());
        out.push(ElementRec {
            key,
            tag,
            rect,
            visible,
            signature,
            text,
            aria,
        });
    }
    out
}

fn parse_rect(v: Option<&Value>) -> (f64, f64, f64, f64) {
    let Some(v) = v else {
        return (0.0, 0.0, 0.0, 0.0);
    };
    let Some(arr) = v.as_array() else {
        return (0.0, 0.0, 0.0, 0.0);
    };
    let get = |i: usize| arr.get(i).and_then(Value::as_f64).unwrap_or(0.0);
    (get(0), get(1), get(2), get(3))
}

fn keyed(recs: Vec<ElementRec>) -> std::collections::HashMap<String, ElementRec> {
    let mut m = std::collections::HashMap::new();
    for rec in recs {
        if rec.key.is_empty() {
            continue;
        }
        m.insert(rec.key.clone(), rec);
    }
    m
}

fn rect_to_json(rect: (f64, f64, f64, f64)) -> Value {
    json!({ "x": rect.0, "y": rect.1, "width": rect.2, "height": rect.3 })
}

fn rect_of(v: &Value) -> Option<(f64, f64, f64, f64)> {
    Some((
        v.get("x")?.as_f64()?,
        v.get("y")?.as_f64()?,
        v.get("width")?.as_f64()?,
        v.get("height")?.as_f64()?,
    ))
}

fn rec_rect(v: &Value) -> (f64, f64, f64, f64) {
    v.get("rect")
        .and_then(rect_of)
        .unwrap_or((0.0, 0.0, 0.0, 0.0))
}

fn rec_tag(v: &Value) -> String {
    v.get("tag")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn area(rect: (f64, f64, f64, f64)) -> f64 {
    (rect.2).max(0.0) * (rect.3).max(0.0)
}

fn rect_delta(a: &(f64, f64, f64, f64), b: &(f64, f64, f64, f64)) -> bool {
    (a.0 - b.0).abs() > RECT_TOLERANCE
        || (a.1 - b.1).abs() > RECT_TOLERANCE
        || (a.2 - b.2).abs() > RECT_TOLERANCE
        || (a.3 - b.3).abs() > RECT_TOLERANCE
}

/// True when two CSS strings are numbers with the same unit and within `tol`
/// (e.g. `16px` vs `16.2px`).
fn numeric_close(a: &str, b: &str, tol: f64) -> bool {
    let (Some((x, ru)), Some((y, rv))) = (leading_number(a), leading_number(b)) else {
        return false;
    };
    ru == rv && (x - y).abs() <= tol
}

/// Split a CSS value into a leading number and the unit remainder.
fn leading_number(s: &str) -> Option<(f64, &str)> {
    let s = s.trim();
    let idx = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(s.len());
    if idx == 0 {
        return None;
    }
    let (num, rest) = s.split_at(idx);
    Some((num.parse().ok()?, rest))
}

fn rect_onscreen(rect: (f64, f64, f64, f64), vw: f64, vh: f64) -> bool {
    rect.0 < vw && rect.1 < vh && rect.0 + rect.2 > 0.0 && rect.1 + rect.3 > 0.0
}

/// px beyond each viewport edge (only nonzero amounts, keyed `above`/`below`/
/// `left`/`right`).
fn out_of_view(rect: (f64, f64, f64, f64), vw: f64, vh: f64) -> Value {
    let mut m = Map::new();
    let above = (-rect.1).max(0.0);
    let below = (rect.1 + rect.3 - vh).max(0.0);
    let left = (-rect.0).max(0.0);
    let right = (rect.0 + rect.2 - vw).max(0.0);
    if above > 0.5 {
        m.insert("above".into(), Value::from(above.round()));
    }
    if below > 0.5 {
        m.insert("below".into(), Value::from(below.round()));
    }
    if left > 0.5 {
        m.insert("left".into(), Value::from(left.round()));
    }
    if right > 0.5 {
        m.insert("right".into(), Value::from(right.round()));
    }
    Value::Object(m)
}

/// Nearest distance from a point to a rect (0 when the point is inside).
fn distance_from_point(rect: (f64, f64, f64, f64), x: f64, y: f64) -> f64 {
    let (rx, ry, rw, rh) = rect;
    let dx = (rx - x).max(0.0).max(x - (rx + rw));
    let dy = (ry - y).max(0.0).max(y - (ry + rh));
    (dx * dx + dy * dy).sqrt()
}

/// Compass direction from a point to a rect center.
fn direction_from_point(rect: (f64, f64, f64, f64), x: f64, y: f64) -> &'static str {
    let (rx, ry, rw, rh) = rect;
    let cx = rx + rw / 2.0;
    let cy = ry + rh / 2.0;
    let dx = cx - x;
    let dy = cy - y;
    let horiz = if dx.abs() > 0.5 {
        if dx > 0.0 { "right" } else { "left" }
    } else {
        ""
    };
    let vert = if dy.abs() > 0.5 {
        if dy > 0.0 { "below" } else { "above" }
    } else {
        ""
    };
    match (horiz, vert) {
        ("", "") => "at-action-point",
        (h, "") => h,
        ("", v) => v,
        (h, v) => {
            // dominant axis first, secondary appended.
            if dx.abs() >= dy.abs() {
                match (h, v) {
                    ("right", "below") => "below-right",
                    ("right", "above") => "above-right",
                    ("left", "below") => "below-left",
                    _ => "above-left",
                }
            } else {
                match (v, h) {
                    ("below", "right") => "below-right",
                    ("below", "left") => "below-left",
                    ("above", "right") => "above-right",
                    _ => "above-left",
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Action accessors
// ---------------------------------------------------------------------------

fn action_kind(action: &Action) -> &'static str {
    match action {
        Action::Click { .. } => "click",
        Action::Hover { .. } => "hover",
        Action::Type { .. } => "type",
        Action::Upload { .. } => "upload",
    }
}

fn action_selector(action: &Action) -> String {
    match action {
        Action::Click { selector, .. }
        | Action::Hover { selector, .. }
        | Action::Type { selector, .. }
        | Action::Upload { selector, .. } => selector.clone(),
    }
}

fn action_timeout_ms(action: &Action) -> u64 {
    match action {
        Action::Click { timeout_ms, .. }
        | Action::Hover { timeout_ms, .. }
        | Action::Type { timeout_ms, .. }
        | Action::Upload { timeout_ms, .. } => *timeout_ms,
    }
}

fn action_settle_ms(action: &Action) -> u64 {
    match action {
        Action::Click { settle_ms, .. }
        | Action::Hover { settle_ms, .. }
        | Action::Type { settle_ms, .. }
        | Action::Upload { settle_ms, .. } => *settle_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(
        key: &str,
        tag: &str,
        rect: (f64, f64, f64, f64),
        visible: bool,
        display: &str,
    ) -> Value {
        json!({
            "k": key, "t": tag, "r": [rect.0, rect.1, rect.2, rect.3],
            "v": visible, "o": true,
            "s": {"display": display, "position": "static", "z-index": "auto",
                  "width": format!("{}px", rect.2), "height": format!("{}px", rect.3)},
        })
    }

    fn snapshot(recs: Vec<Value>) -> Value {
        json!({ "viewport": [1366.0, 768.0], "elements": recs })
    }

    fn target(x: f64, y: f64) -> ActionTarget {
        ActionTarget {
            path: "button#open".into(),
            rect: (x, y, 100.0, 40.0),
            center: (x + 50.0, y + 20.0),
        }
    }

    fn click_action() -> Action {
        Action::Click {
            selector: "#open".into(),
            timeout_ms: 10_000,
            settle_ms: 150,
        }
    }

    /// Read a per-element signature value by property name from the compact
    /// `css_*_values` array, using the entry's `css_keys` header.
    fn sig_value<'a>(
        report: &'a Value,
        rec: &'a Value,
        field: &str,
        prop: &str,
    ) -> Option<&'a Value> {
        let keys = report["css_keys"].as_array()?;
        let idx = keys.iter().position(|k| k.as_str() == Some(prop))?;
        rec.get(field)?.as_array()?.get(idx)
    }

    fn rec_with_sig(key: &str, tag: &str, sig: Value) -> Value {
        json!({
            "k": key, "t": tag, "r": [0.0, 0.0, 100.0, 40.0],
            "v": true, "o": true,
            "s": sig,
        })
    }

    #[test]
    fn diff_detects_appeared_element_with_off_view() {
        let before = snapshot(vec![rec(
            "button#open",
            "BUTTON",
            (100.0, 100.0, 100.0, 40.0),
            true,
            "block",
        )]);
        let after = snapshot(vec![
            rec(
                "button#open",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
            rec(
                "body > div#panel > table",
                "TABLE",
                (0.0, 1000.0, 600.0, 120.0),
                true,
                "table",
            ),
        ]);
        let report = diff(
            &before,
            &after,
            &target(125.0, 120.0),
            &click_action(),
            0,
            10,
        );
        assert_eq!(report["effect"], "revealed");
        assert_eq!(report["appeared"].as_array().unwrap().len(), 1);
        let appeared = &report["appeared"][0];
        assert_eq!(appeared["tag"], "TABLE");
        assert_eq!(appeared["onscreen"], false);
        assert_eq!(appeared["out_of_view"]["below"], 352.0);
        assert!(appeared["distance_from_action"].as_f64().unwrap() > 0.0);
        assert!(report["summary"].as_str().unwrap().contains("appeared"));
        // The signature schema is shared once per entry; per-node arrays align.
        let keys = report["css_keys"].as_array().unwrap();
        assert_eq!(keys.len(), EFFECT_SIGNATURE_PROPS.len());
        assert_eq!(
            appeared["css_after_values"].as_array().unwrap().len(),
            EFFECT_SIGNATURE_PROPS.len(),
            "value array must align with css_keys"
        );
        assert_eq!(
            sig_value(&report, appeared, "css_after_values", "display"),
            Some(&Value::String("table".into()))
        );
        assert!(appeared.get("css_before_values").is_none());
    }

    #[test]
    fn diff_reports_no_effect_when_nothing_changed() {
        let before = snapshot(vec![rec(
            "button#open",
            "BUTTON",
            (100.0, 100.0, 100.0, 40.0),
            true,
            "block",
        )]);
        let after = snapshot(vec![rec(
            "button#open",
            "BUTTON",
            (100.0, 100.0, 100.0, 40.0),
            true,
            "block",
        )]);
        let report = diff(
            &before,
            &after,
            &target(125.0, 120.0),
            &click_action(),
            0,
            10,
        );
        assert_eq!(report["effect"], "no_effect");
        assert!(report["appeared"].as_array().unwrap().is_empty());
        assert!(report["changed"].as_array().unwrap().is_empty());
    }

    #[test]
    fn diff_tracks_visible_transition_as_changed() {
        let before = snapshot(vec![
            rec(
                "button#open",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
            rec("body > #modal", "DIV", (0.0, 0.0, 0.0, 0.0), false, "none"),
        ]);
        let after = snapshot(vec![
            rec(
                "button#open",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
            rec(
                "body > #modal",
                "DIV",
                (100.0, 200.0, 400.0, 300.0),
                true,
                "block",
            ),
        ]);
        let report = diff(
            &before,
            &after,
            &target(125.0, 120.0),
            &click_action(),
            0,
            10,
        );
        assert_eq!(report["effect"], "revealed");
        let changed = report["changed"].as_array().unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0]["visible_before"], false);
        assert_eq!(changed[0]["visible_after"], true);
        let css_diff = changed[0]["css_diff"].as_object().unwrap();
        assert!(css_diff.contains_key("display"), "got: {css_diff:?}");
    }

    #[test]
    fn diff_detects_removed_element() {
        let before = snapshot(vec![
            rec(
                "button#open",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
            rec(
                "body > .spinner",
                "DIV",
                (10.0, 10.0, 30.0, 30.0),
                true,
                "block",
            ),
        ]);
        let after = snapshot(vec![rec(
            "button#open",
            "BUTTON",
            (100.0, 100.0, 100.0, 40.0),
            true,
            "block",
        )]);
        let report = diff(
            &before,
            &after,
            &target(125.0, 120.0),
            &click_action(),
            0,
            10,
        );
        assert_eq!(report["effect"], "hidden");
        assert_eq!(report["removed"].as_array().unwrap().len(), 1);
        assert_eq!(
            sig_value(
                &report,
                &report["removed"][0],
                "css_before_values",
                "display"
            ),
            Some(&Value::String("block".into()))
        );
        assert!(
            report["removed"][0].get("css_after_values").is_none(),
            "removed nodes carry no after signature"
        );
    }

    #[test]
    fn diff_applies_limit_and_sorts_by_area() {
        let mut before_recs = vec![rec(
            "button#open",
            "BUTTON",
            (100.0, 100.0, 100.0, 40.0),
            true,
            "block",
        )];
        let mut after_recs = vec![rec(
            "button#open",
            "BUTTON",
            (100.0, 100.0, 100.0, 40.0),
            true,
            "block",
        )];
        for i in 0..5 {
            let key = format!("body > .a{i}");
            let w = 100.0 + i as f64 * 50.0;
            before_recs.push(rec(&key, "DIV", (0.0, 0.0, 0.0, 0.0), false, "none"));
            after_recs.push(rec(&key, "DIV", (10.0, 10.0, w, 40.0), true, "block"));
        }
        let report = diff(
            &snapshot(before_recs),
            &snapshot(after_recs),
            &target(125.0, 120.0),
            &click_action(),
            0,
            3,
        );
        let changed = report["changed"].as_array().unwrap();
        assert_eq!(changed.len(), 3, "limit must cap the list");
        // Largest area first (width grows with index).
        assert!(
            changed[0]["rect"]["width"].as_f64().unwrap()
                >= changed[1]["rect"]["width"].as_f64().unwrap()
        );
    }

    #[test]
    fn signature_change_is_reported_as_changed_effect() {
        let before = snapshot(vec![
            rec(
                "button#open",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
            rec(
                "body > .btn",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
        ]);
        let mut after = before.clone();
        after["elements"][1]["s"]["background-color"] = Value::from("#ff0000");
        let report = diff(
            &before,
            &after,
            &target(125.0, 120.0),
            &click_action(),
            0,
            10,
        );
        assert_eq!(report["effect"], "changed");
        let changed = report["changed"].as_array().unwrap();
        assert_eq!(changed.len(), 1);
        let css_diff = changed[0]["css_diff"].as_object().unwrap();
        assert!(css_diff.contains_key("background-color"));
        assert_eq!(css_diff["background-color"]["before"], Value::Null);
        assert_eq!(css_diff["background-color"]["after"], "#ff0000");
    }

    #[test]
    fn root_geometry_reflow_is_suppressed_but_theme_changes_surface() {
        let root_sig = |width: &str, height: &str, bg: &str| {
            json!({
                "display": "block", "position": "static", "z-index": "auto",
                "width": width, "height": height, "padding-top": "20px",
                "background-color": bg, "background-image": "none",
            })
        };
        let before = snapshot(vec![
            rec_with_sig("html", "HTML", root_sig("1366px", "768px", "#00000000")),
            rec_with_sig(
                "html > body",
                "BODY",
                root_sig("1366px", "768px", "#00000000"),
            ),
            rec(
                "button#open",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
        ]);
        let after = snapshot(vec![
            // Geometry reflow only: viewport width shrink + content growth.
            rec_with_sig("html", "HTML", root_sig("1351px", "1884px", "#00000000")),
            rec_with_sig(
                "html > body",
                "BODY",
                root_sig("1351px", "1884px", "#00000000"),
            ),
            rec(
                "button#open",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
        ]);
        let report = diff(
            &before,
            &after,
            &target(125.0, 120.0),
            &click_action(),
            0,
            10,
        );
        assert_eq!(
            report["changed"].as_array().unwrap().len(),
            0,
            "geometry-only root reflow must not flood changed"
        );
        assert_eq!(report["effect"], "no_effect");

        // Now add a real theme change on BODY: background-color flips.
        let after2 = snapshot(vec![
            rec_with_sig("html", "HTML", root_sig("1351px", "1884px", "#00000000")),
            rec_with_sig(
                "html > body",
                "BODY",
                root_sig("1351px", "1884px", "#ffffff"),
            ),
            rec(
                "button#open",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
        ]);
        let report = diff(
            &before,
            &after2,
            &target(125.0, 120.0),
            &click_action(),
            0,
            10,
        );
        let changed = report["changed"].as_array().unwrap();
        assert_eq!(changed.len(), 1, "only BODY surfaces a theme change");
        assert_eq!(changed[0]["tag"], "BODY");
        let css_diff = changed[0]["css_diff"].as_object().unwrap();
        assert!(
            css_diff.contains_key("background-color"),
            "theme diff must be reported: {css_diff:?}"
        );
        assert!(
            !css_diff.contains_key("width") && !css_diff.contains_key("height"),
            "root geometry must stay out of css_diff: {css_diff:?}"
        );
        assert!(
            changed[0].get("rect_before").is_none(),
            "root nodes never report rect movement"
        );
    }

    #[test]
    fn css_diff_ignores_subpixel_jitter_beyond_tolerance() {
        let sig_a = json!({
            "display": "block", "position": "static", "z-index": "auto",
            "font-size": "16px", "width": "340px", "height": "58px",
        });
        let mut sig_b = sig_a.clone();
        sig_b["font-size"] = Value::String("16.2px".into());
        let before = snapshot(vec![
            rec(
                "button#open",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
            rec_with_sig("body > .field", "INPUT", sig_a.clone()),
        ]);
        let after = snapshot(vec![
            rec(
                "button#open",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
            rec_with_sig("body > .field", "INPUT", sig_b),
        ]);
        let report = diff(
            &before,
            &after,
            &target(125.0, 120.0),
            &click_action(),
            0,
            10,
        );
        assert_eq!(
            report["changed"].as_array().unwrap().len(),
            0,
            "subpixel jitter within tolerance must not mark a node changed"
        );

        // A real size change (unit-consistent) still surfaces.
        let mut sig_c = sig_a.clone();
        sig_c["font-size"] = Value::String("20px".into());
        let after2 = snapshot(vec![
            rec(
                "button#open",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
            rec_with_sig("body > .field", "INPUT", sig_c),
        ]);
        let report = diff(
            &before,
            &after2,
            &target(125.0, 120.0),
            &click_action(),
            0,
            10,
        );
        let changed = report["changed"].as_array().unwrap();
        assert_eq!(changed.len(), 1);
        assert!(changed[0]["css_diff"]["font-size"]["before"] == "16px");
        assert!(changed[0]["css_diff"]["font-size"]["after"] == "20px");
    }

    #[test]
    fn no_effect_entries_omit_css_keys() {
        let before = snapshot(vec![rec(
            "button#open",
            "BUTTON",
            (100.0, 100.0, 100.0, 40.0),
            true,
            "block",
        )]);
        let report = diff(
            &before,
            &before.clone(),
            &target(125.0, 120.0),
            &click_action(),
            0,
            10,
        );
        assert_eq!(report["effect"], "no_effect");
        assert!(
            report.get("css_keys").is_none(),
            "no appeared/removed records -> no css_keys header"
        );
    }

    #[test]
    fn entry_carries_action_metadata() {
        let before = snapshot(vec![rec(
            "button#open",
            "BUTTON",
            (100.0, 100.0, 100.0, 40.0),
            true,
            "block",
        )]);
        let after = snapshot(vec![
            rec(
                "button#open",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
            rec(
                "body > #panel",
                "DIV",
                (0.0, 0.0, 200.0, 100.0),
                true,
                "block",
            ),
        ]);
        let report = diff(
            &before,
            &after,
            &target(125.0, 120.0),
            &click_action(),
            3,
            10,
        );
        assert_eq!(report["index"], 3);
        assert_eq!(report["action"], "click");
        assert_eq!(report["selector"], "#open");
        assert_eq!(report["target"]["path"], "button#open");
        assert_eq!(report["target"]["onscreen"], true);
    }

    #[test]
    fn direction_and_distance_math() {
        // Point to the right of a small rect at origin: the rect is LEFT of
        // the point.
        let d = distance_from_point((0.0, 0.0, 10.0, 10.0), 100.0, 5.0);
        assert!((d - 90.0).abs() < 0.001);
        assert_eq!(
            direction_from_point((0.0, 0.0, 10.0, 10.0), 100.0, 5.0),
            "left"
        );
        assert_eq!(
            direction_from_point((0.0, 0.0, 10.0, 10.0), 5.0, 5.0),
            "at-action-point"
        );
        // Rect below-right of the point.
        assert_eq!(
            direction_from_point((100.0, 100.0, 10.0, 10.0), 50.0, 50.0),
            "below-right"
        );
    }

    #[test]
    fn out_of_view_reports_only_nonzero_edges() {
        let v = out_of_view((0.0, 900.0, 600.0, 120.0), 1366.0, 768.0);
        assert_eq!(v.get("above"), None);
        assert_eq!(v.get("below").and_then(Value::as_f64), Some(252.0));
        assert_eq!(v.get("left"), None);
        assert_eq!(v.get("right"), None);
    }

    #[test]
    fn summary_mentions_biggest_off_screen_element() {
        let before = snapshot(vec![rec(
            "button#open",
            "BUTTON",
            (100.0, 100.0, 100.0, 40.0),
            true,
            "block",
        )]);
        let after = snapshot(vec![
            rec(
                "button#open",
                "BUTTON",
                (100.0, 100.0, 100.0, 40.0),
                true,
                "block",
            ),
            rec(
                "body > table",
                "TABLE",
                (0.0, 2000.0, 500.0, 100.0),
                true,
                "table",
            ),
        ]);
        let report = diff(
            &before,
            &after,
            &target(125.0, 120.0),
            &click_action(),
            0,
            10,
        );
        let summary = report["summary"].as_str().unwrap();
        assert!(summary.contains("appeared"), "got: {summary}");
        assert!(summary.contains("TABLE"), "got: {summary}");
        assert!(summary.contains("px below"), "got: {summary}");
    }
}
