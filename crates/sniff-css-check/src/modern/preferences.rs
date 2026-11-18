//! Família E — preferências do usuário e estado emulado.
//!
//! Os checks aqui cobrem movimento reduzido, dispensa de diálogo, foco após
//! scroll, live regions e transições de navegação. Todos são offline e
//! determinísticos: nenhuma decisão depende de relógio, ambiente, aleatoriedade
//! ou da ordem de um `HashMap`.
//!
//! `reduced-motion-guard-missing` precisa de `declared` **ou** de `__env` +
//! matriz. Como `CheckSpec::required` tem semântica de AND, a especificação
//! registra apenas `Styles` e a escolha da via é feita explicitamente em
//! [`check_reduced_motion_guard`]; sem nenhuma das duas vias o check silencia.
//!
//! Convenção para `__actions` (consumida de forma tolerante):
//! - ação de teclado: `{"action":"key","key":"Escape","selector":…,
//!   "open_after": true}` (aceita `key_name`/`keyName` e `after.open`);
//! - ação de foco: `{"focus_before":"…","focus_after":"BODY"}` (aceita
//!   `active_element_before`/`active_element_after` e `focus.before`/`after`).

use crate::modern::{CheckSpec, Facet, ModernCtx};
use crate::rules::CheckLine;
use serde_json::Value;
use sniff_core::modern::{DeclaredFacet, DocumentFacet, Evidence};
use sniff_css_diff::DiffNode;
use std::collections::{BTreeMap, BTreeSet};

/// Índice `id -> nó`, derivado da floresta já achatada pelo contexto.
type NodeIndex<'a> = BTreeMap<u64, &'a DiffNode>;

const MOTION_KEY: &str = "prefers-reduced-motion";
const ACTIVE_TYPE_MARKER: &str = ":active-view-transition-type(";
const VIEW_TRANSITION_MARKER: &str = "@view-transition";
const REDUCED_MOTION_MARKER: &str = "prefers-reduced-motion";

/// Todos os ids da família E, com guias, facets e `keepWhen` auditável.
static SPECS: &[CheckSpec] = &[
    CheckSpec::new(
        "reduced-motion-guard-missing",
        &[
            "parallax-scroll-effects",
            "scroll-entry-exit-effects",
            "scrollytelling",
        ],
        &[Facet::Styles],
        "Descarta movimento sem sinal espacial/contínuo, cor/background, regra \
             casada sob @media (prefers-reduced-motion), declaração truncada, e só usa \
             a via de emulação quando __env e matriz estão presentes e coerentes.",
    ),
    CheckSpec::new(
        "scroll-effect-under-reduced-motion",
        &["carousel-slide-effects"],
        &[Facet::Env, Facet::Styles],
        "Não dispara sem __env, quando o usuário não prefere reduce, quando a \
             emulação pedida diverge do estado reportado, quando animation-name é none \
             ou quando o efeito não está dentro de um scroller com scroll-snap.",
    ),
    CheckSpec::new(
        "dialog-dismiss-unverified",
        &["platform-controls-dismiss-dialog"],
        &[Facet::Attrs, Facet::Actions],
        "Só dispara para DIALOG aberto e não-light-dismiss após action key Escape \
             registrar explicitamente que o atributo open permaneceu; não dispara sem \
             a action, sem attrs ou quando o fechamento foi observado.",
    ),
    CheckSpec::new(
        "hidden-scroll-control-focus-loss",
        &["scroll-position-aware-elements"],
        &[Facet::Actions, Facet::Styles],
        "Exige controle sticky/fixed oculto e uma action de foco com activeElement \
             indo para BODY/HTML; não dispara quando o foco vai para o scroller ou \
             quando o controle não é ocultado.",
    ),
    CheckSpec::new(
        "toast-live-region-missing-at-load",
        &["persistent-toast-notifications"],
        &[Facet::Document, Facet::Attrs],
        "Não dispara quando há aria-live ativo ou role=status no agregado do DOM, \
             nem quando não há toast popover=manual/region de notificações visível.",
    ),
    CheckSpec::new(
        "cross-doc-transition-missing-render-block",
        &["consistent-cross-document-transitions"],
        &[Facet::Document, Facet::Declared],
        "Só dispara com @view-transition ativo; não dispara quando link \
             rel=expect aponta para id existente e todos os recursos críticos já \
             têm blocking=render na facet __document.",
    ),
    CheckSpec::new(
        "cross-doc-transition-not-opted-in",
        &["cross-document-transitions"],
        &[Facet::Declared],
        "Não dispara com at-rule @view-transition presente, com declared truncado \
             (sem certeza) ou sem a facet declared; PE sem suporte não é tratada como \
             falha.",
    ),
    CheckSpec::new(
        "directional-transition-missing-type-rule",
        &["directional-navigation-transitions"],
        &[Facet::Declared],
        "Não dispara quando todos os tipos declarados em `types:` possuem regra \
             :active-view-transition-type() correspondente, nem quando não há sinal \
             de tipos/keyframes direcionais na stylesheet.",
    ),
    CheckSpec::new(
        "scoped-vt-isolation-missing",
        &["scoped-component-transitions"],
        &[Facet::Declared, Facet::Styles],
        "Só dispara em escopo aninhado com match-element dentro de outro \
             participante de transição; não dispara com view-transition-scope:all, \
             com a alternativa view-transition-group:contain + children clip, ou sem \
             transição externa.",
    ),
    CheckSpec::new(
        "scroll-initial-target-multiple",
        &["scroll-target-on-load", "pull-to-reveal"],
        &[Facet::Styles],
        "Não dispara quando cada alvo pertence a um scroller distinto ou quando \
             não há ancestor com overflow auto/scroll; alvo único por scroller é \
             configuração válida.",
    ),
];

/// Registro dos checks da família E (fonte única de ids).
#[must_use]
pub fn specs() -> &'static [CheckSpec] {
    SPECS
}

/// Roda todos os checks habilitados da família E.
pub fn run(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    let index = index_nodes(ctx);
    for spec in specs() {
        if !ctx.enabled(spec) {
            tracing::debug!(
                check = spec.id,
                reason = "facet_absent_or_filtered",
                "family E check skipped"
            );
            continue;
        }
        match spec.id {
            "reduced-motion-guard-missing" => check_reduced_motion_guard(ctx, spec, out),
            "scroll-effect-under-reduced-motion" => {
                check_scroll_effect_under_reduced_motion(ctx, spec, out);
            }
            "dialog-dismiss-unverified" => check_dialog_dismiss(ctx, spec, out),
            "hidden-scroll-control-focus-loss" => check_focus_loss(ctx, spec, out),
            "toast-live-region-missing-at-load" => check_toast_live_region(ctx, spec, out),
            "cross-doc-transition-missing-render-block" => {
                check_cross_doc_render_block(ctx, spec, out, &index);
            }
            "cross-doc-transition-not-opted-in" => check_cross_doc_not_opted_in(ctx, spec, out),
            "directional-transition-missing-type-rule" => {
                check_directional_type_rule(ctx, spec, out);
            }
            "scoped-vt-isolation-missing" => {
                check_scoped_vt_isolation(ctx, spec, out, &index);
            }
            "scroll-initial-target-multiple" => {
                check_scroll_initial_target(ctx, spec, out, &index);
            }
            other => {
                tracing::warn!(
                    check = other,
                    reason = "unregistered_dispatch",
                    "family E check ignored"
                );
            }
        }
    }
}

fn index_nodes<'a>(ctx: &ModernCtx<'a>) -> NodeIndex<'a> {
    ctx.flat.iter().map(|node| (node.id, *node)).collect()
}

fn has_attr(node: &DiffNode, name: &str) -> bool {
    node.attributes
        .as_ref()
        .and_then(|attrs| attrs.as_object())
        .is_some_and(|attrs| attrs.contains_key(name))
}

fn attr_equals(node: &DiffNode, name: &str, expected: &str) -> bool {
    node.attributes
        .as_ref()
        .and_then(|attrs| attrs.get(name))
        .and_then(Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn aria_role_is(node: &DiffNode, role: &str) -> bool {
    node.aria
        .as_ref()
        .and_then(|aria| aria.get("role"))
        .and_then(Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case(role))
}

fn is_one_of(value: Option<&str>, accepted: &[&str]) -> bool {
    value.is_some_and(|value| {
        accepted
            .iter()
            .any(|candidate| value.eq_ignore_ascii_case(candidate))
    })
}

fn contains_ignore_ascii(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn reduced_motion_state(ctx: &ModernCtx<'_>) -> Option<&'static str> {
    let env = ctx.doc.env.as_ref()?;
    let reported = env.media.get(MOTION_KEY).map(String::as_str)?;
    if !reported.eq_ignore_ascii_case("reduce") {
        return None;
    }
    // §F-C: `emulated` registra o pedido da CLI; `media`, o que o browser
    // reporta. Um pedido explícito divergente invalida a via baseada em estado.
    if let Some(requested) = env.emulated.get(MOTION_KEY).map(String::as_str)
        && !requested.eq_ignore_ascii_case("reduce")
    {
        return None;
    }
    Some(if env.emulated.contains_key(MOTION_KEY) {
        "__env.emulated"
    } else {
        "__env.media"
    })
}

fn matrix_emulates_reduce(ctx: &ModernCtx<'_>) -> bool {
    ctx.doc.matrix.iter().any(|tag| {
        tag.emulation
            .get(MOTION_KEY)
            .is_some_and(|v| v.eq_ignore_ascii_case("reduce"))
    })
}

fn has_reduced_motion_guard(declared: &DeclaredFacet) -> bool {
    declared
        .at_rules
        .iter()
        .any(|at| contains_ignore_ascii(&at.condition_text, REDUCED_MOTION_MARKER))
        || declared
            .rules
            .iter()
            .any(|rule| contains_ignore_ascii(&rule.selector, REDUCED_MOTION_MARKER))
}

fn motion_signal(ctx: &ModernCtx<'_>, node: &DiffNode) -> Option<&'static str> {
    let timeline = ctx
        .style(node, "animation", "animation-timeline")
        .unwrap_or("");
    let view = ctx
        .style(node, "animation", "view-timeline-name")
        .unwrap_or("");
    let count = ctx
        .style(node, "animation", "animation-iteration-count")
        .unwrap_or("");
    let name = ctx.style(node, "animation", "animation-name").unwrap_or("");
    if !timeline.is_empty()
        && !timeline.eq_ignore_ascii_case("auto")
        && !timeline.eq_ignore_ascii_case("none")
    {
        return Some("animation-timeline");
    }
    if !view.is_empty() && !view.eq_ignore_ascii_case("none") {
        return Some("view-timeline-name");
    }
    if count.eq_ignore_ascii_case("infinite") {
        return Some("animation-iteration-count");
    }
    let range = ctx
        .style(node, "animation", "animation-range-start")
        .or_else(|| ctx.style(node, "animation", "animation-range-end"))
        .unwrap_or("");
    if (contains_ignore_ascii(range, "entry") || contains_ignore_ascii(range, "exit"))
        && !name.is_empty()
        && !name.eq_ignore_ascii_case("none")
    {
        return Some("animation-range");
    }
    None
}

fn check_reduced_motion_guard(ctx: &ModernCtx<'_>, spec: &CheckSpec, out: &mut Vec<CheckLine>) {
    let declared = ctx.doc.declared.as_ref();
    let state = reduced_motion_state(ctx);
    let via_declared = declared.is_some();
    let via_matrix = state.is_some() && ctx.has_facet(Facet::Matrix) && matrix_emulates_reduce(ctx);
    if !via_declared && !via_matrix {
        tracing::debug!(
            check = spec.id,
            reason = "no_declared_no_matrix",
            "family E check skipped"
        );
        return;
    }
    if declared.is_some_and(|d| d.truncated) {
        tracing::debug!(
            check = spec.id,
            reason = "declared_truncated",
            "family E check skipped"
        );
        return;
    }
    if declared.is_some_and(has_reduced_motion_guard) {
        return;
    }
    let mut signals: Vec<(&DiffNode, &'static str)> = Vec::new();
    for node in &ctx.flat {
        if ctx.visible(node)
            && let Some(signal) = motion_signal(ctx, node)
        {
            signals.push((node, signal));
        }
    }
    if signals.is_empty() {
        return;
    }
    let emitted_count = signals.len();
    let source = if via_declared {
        "__declared"
    } else {
        "__env+__meta.matrix"
    };
    let blocked = declared.map_or(0_usize, |d| d.blocked_sheets.len());
    let limitation = if blocked > 0 {
        format!(" (origin check limited by {blocked} blocked sheet(s))")
    } else {
        String::new()
    };
    let state_note = state.map_or(String::new(), |_| format!(" under {source}"));
    for (node, signal) in signals {
        let text = format!(
            "{} node(s) keep scroll-driven/infinite motion via {signal}{state_note} with no @media (prefers-reduced-motion) guard{limitation}",
            ctx.flat
                .iter()
                .filter(|candidate| motion_signal(ctx, candidate) == Some(signal))
                .count()
        );
        ctx.emit(out, spec, node, Evidence::new(text, source));
    }
    tracing::debug!(
        check = spec.id,
        emitted = emitted_count,
        source,
        "family E check emitted"
    );
}

fn check_scroll_effect_under_reduced_motion(
    ctx: &ModernCtx<'_>,
    spec: &CheckSpec,
    out: &mut Vec<CheckLine>,
) {
    let Some(source) = reduced_motion_state(ctx) else {
        tracing::debug!(
            check = spec.id,
            reason = "reduce_not_confirmed",
            "family E check skipped"
        );
        return;
    };
    let mut emitted = 0_usize;
    for node in &ctx.flat {
        let timeline = ctx
            .style(node, "animation", "animation-timeline")
            .unwrap_or("");
        let view = ctx
            .style(node, "animation", "view-timeline-name")
            .unwrap_or("");
        let name = ctx.style(node, "animation", "animation-name").unwrap_or("");
        let in_scroller = ctx
            .style(node, "interaction", "scroll-snap-type")
            .is_some_and(|value| !value.eq_ignore_ascii_case("none"))
            || node.parent_id.is_some_and(|parent| {
                ctx.flat.iter().any(|candidate| {
                    candidate.id == parent
                        && ctx
                            .style(candidate, "interaction", "scroll-snap-type")
                            .is_some_and(|v| !v.eq_ignore_ascii_case("none"))
                })
            });
        let scroll_driven = contains_ignore_ascii(timeline, "view(")
            || (!view.is_empty() && !view.eq_ignore_ascii_case("none"));
        if !in_scroller || !scroll_driven || name.eq_ignore_ascii_case("none") {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!(
                    "scroll-driven slide effect still animates with {MOTION_KEY}: reduce ({source})"
                ),
                source,
            ),
        );
        emitted = emitted.saturating_add(1);
    }
    tracing::debug!(check = spec.id, emitted, source, "family E check emitted");
}

fn string_field<'a>(entry: &'a Value, names: &[&str]) -> Option<&'a str> {
    names
        .iter()
        .find_map(|name| entry.get(*name).and_then(Value::as_str))
}

fn nested_str<'a>(entry: &'a Value, outer: &str, inner: &str) -> Option<&'a str> {
    entry.get(outer)?.get(inner)?.as_str()
}

fn nested_bool(entry: &Value, outer: &str, inner: &str) -> Option<bool> {
    entry.get(outer)?.get(inner)?.as_bool()
}

fn bool_field(entry: &Value, names: &[&str]) -> Option<bool> {
    names
        .iter()
        .find_map(|name| entry.get(*name).and_then(Value::as_bool))
}

fn is_key_action(entry: &Value) -> bool {
    string_field(entry, &["action", "kind", "type"]).is_some_and(|kind| {
        kind.eq_ignore_ascii_case("key")
            || kind.eq_ignore_ascii_case("keydown")
            || kind.eq_ignore_ascii_case("press")
    })
}

fn is_escape_key(entry: &Value) -> bool {
    string_field(entry, &["key", "key_name", "keyName", "value"])
        .is_some_and(|key| key.eq_ignore_ascii_case("escape") || key.eq_ignore_ascii_case("esc"))
}

fn action_id(entry: &Value) -> Option<u64> {
    entry
        .get("node_id")
        .and_then(Value::as_u64)
        .or_else(|| nested_value(entry, "target", "node_id").and_then(Value::as_u64))
}

fn nested_value<'a>(entry: &'a Value, outer: &str, inner: &str) -> Option<&'a Value> {
    entry.get(outer)?.get(inner)
}

fn entry_matches(entry: &Value, node: &DiffNode) -> bool {
    let selector = string_field(entry, &["selector", "node_selector"]);
    let path =
        string_field(entry, &["path", "node_path"]).or_else(|| nested_str(entry, "target", "path"));
    let has_identity = selector.is_some() || path.is_some() || action_id(entry).is_some();
    !has_identity
        || selector.is_some_and(|value| value == node.selector)
        || path.is_some_and(|value| node.path.as_deref() == Some(value))
        || action_id(entry).is_some_and(|value| value == node.id)
}

fn dialog_remains_open(entry: &Value, node: &DiffNode) -> bool {
    if let Some(open) = bool_field(entry, &["open_after", "still_open", "dialog_open_after"]) {
        return open;
    }
    if let Some(open) =
        nested_bool(entry, "after", "open").or_else(|| nested_bool(entry, "attrs_after", "open"))
    {
        return open;
    }
    let removed = entry.get("removed").and_then(Value::as_array);
    !removed.is_some_and(|records| records.iter().any(|record| entry_matches(record, node)))
}

fn check_dialog_dismiss(ctx: &ModernCtx<'_>, spec: &CheckSpec, out: &mut Vec<CheckLine>) {
    let dialogs: Vec<&DiffNode> = ctx
        .flat
        .iter()
        .filter(|node| {
            ctx.tag(node).eq_ignore_ascii_case("dialog")
                && has_attr(node, "open")
                && !attr_equals(node, "closedby", "any")
        })
        .copied()
        .collect();
    let escapes: Vec<&Value> = ctx
        .doc
        .actions
        .iter()
        .filter(|entry| is_key_action(entry) && is_escape_key(entry))
        .collect();
    if dialogs.is_empty() || escapes.is_empty() {
        tracing::debug!(
            check = spec.id,
            dialogs = dialogs.len(),
            escapes = escapes.len(),
            reason = "missing_signal",
            "family E check skipped"
        );
        return;
    }
    let mut emitted = 0_usize;
    for dialog in dialogs {
        if !escapes
            .iter()
            .any(|entry| entry_matches(entry, dialog) && dialog_remains_open(entry, dialog))
        {
            continue;
        }
        let text = if has_attr(dialog, "closedby") {
            "modal dialog stayed open after Escape close request (closedby present but not 'any')"
        } else {
            "modal dialog stayed open after Escape close request; native platform dismissal unavailable (closedby absent)"
        };
        ctx.emit(out, spec, dialog, Evidence::new(text, "attrs+__actions"));
        emitted = emitted.saturating_add(1);
    }
    tracing::debug!(check = spec.id, emitted, "family E check emitted");
}

fn focus_before(entry: &Value) -> Option<&str> {
    string_field(
        entry,
        &["focus_before", "focusBefore", "active_element_before"],
    )
    .or_else(|| nested_str(entry, "focus", "before"))
    .or_else(|| nested_str(entry, "active_element", "before"))
}

fn focus_after(entry: &Value) -> Option<&str> {
    string_field(
        entry,
        &["focus_after", "focusAfter", "active_element_after"],
    )
    .or_else(|| nested_str(entry, "focus", "after"))
    .or_else(|| nested_str(entry, "active_element", "after"))
}

fn is_body_target(focus: &str) -> bool {
    ["body", "html", "document.body", ""]
        .iter()
        .any(|candidate| focus.eq_ignore_ascii_case(candidate))
}

fn focus_refers_to(focus: &str, node: &DiffNode) -> bool {
    if focus == node.selector || node.path.as_deref() == Some(focus) {
        return true;
    }
    let bare = focus.strip_prefix('#').unwrap_or(focus);
    node.attributes
        .as_ref()
        .and_then(|attrs| attrs.get("id"))
        .and_then(Value::as_str)
        == Some(bare)
}

fn hidden_scroll_control(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    let hidden = is_one_of(
        ctx.style(node, "visual", "visibility"),
        &["hidden", "collapse"],
    ) || is_one_of(ctx.style(node, "visual", "opacity"), &["0", "0.0"]);
    let positioned = is_one_of(ctx.style(node, "layout", "position"), &["sticky", "fixed"]);
    hidden && positioned
}

fn check_focus_loss(ctx: &ModernCtx<'_>, spec: &CheckSpec, out: &mut Vec<CheckLine>) {
    let controls: Vec<&DiffNode> = ctx
        .flat
        .iter()
        .filter(|node| hidden_scroll_control(ctx, node))
        .copied()
        .collect();
    if controls.is_empty() {
        return;
    }
    let mut emitted = 0_usize;
    for entry in &ctx.doc.actions {
        let (Some(before), Some(after)) = (focus_before(entry), focus_after(entry)) else {
            continue;
        };
        if !is_body_target(after) {
            continue;
        }
        for control in &controls {
            if !focus_refers_to(before, control) {
                continue;
            }
            ctx.emit(
                out,
                spec,
                control,
                Evidence::new(
                    format!(
                        "focus lost to <body> when scroll-position-aware control {} became hidden",
                        control.selector
                    ),
                    "__actions+styles.visual+styles.layout",
                ),
            );
            emitted = emitted.saturating_add(1);
        }
    }
    tracing::debug!(check = spec.id, emitted, "family E check emitted");
}

fn is_live_region(_ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    (has_attr(node, "aria-live") && !attr_equals(node, "aria-live", "off"))
        || aria_role_is(node, "status")
}

fn is_toast_like(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    if is_live_region(ctx, node) {
        return false;
    }
    attr_equals(node, "popover", "manual")
        || (aria_role_is(node, "region")
            && ctx
                .aria(node, "name")
                .is_some_and(|name| contains_ignore_ascii(name, "notification")))
}

fn check_toast_live_region(ctx: &ModernCtx<'_>, spec: &CheckSpec, out: &mut Vec<CheckLine>) {
    let live_regions = ctx
        .flat
        .iter()
        .filter(|node| is_live_region(ctx, node))
        .count();
    let mut emitted = 0_usize;
    if live_regions > 0 {
        tracing::debug!(
            check = spec.id,
            live_regions,
            "pre-populated live region keeps toast check silent"
        );
        return;
    }
    for node in &ctx.flat {
        if !ctx.visible(node) || !is_toast_like(ctx, node) {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::numeric(
                format!(
                    "no pre-populated aria-live region at load ({live_regions} found); toast announcement unreliable"
                ),
                "__document+attrs",
                "live_regions_at_load",
                Value::from(0_u64),
                Value::from(1_u64),
                ">=",
            ),
        );
        emitted = emitted.saturating_add(1);
    }
    tracing::debug!(
        check = spec.id,
        emitted,
        live_regions,
        "family E check emitted"
    );
}

fn view_transition_active(declared: &DeclaredFacet) -> bool {
    declared
        .at_rules
        .iter()
        .any(|at| contains_ignore_ascii(&at.condition_text, VIEW_TRANSITION_MARKER))
        || declared
            .rules
            .iter()
            .any(|rule| contains_ignore_ascii(&rule.selector, VIEW_TRANSITION_MARKER))
}

fn render_blocked<T: serde::Serialize>(item: &T) -> bool {
    serde_json::to_value(item).is_ok_and(|value| {
        value
            .get("blocking")
            .and_then(Value::as_str)
            .is_some_and(|blocking| blocking.eq_ignore_ascii_case("render"))
    })
}

fn expect_link_covers(
    ctx: &ModernCtx<'_>,
    index: &NodeIndex<'_>,
    document: &DocumentFacet,
) -> bool {
    document.links.iter().any(|link| {
        link.rel.eq_ignore_ascii_case("expect")
            && link
                .href
                .as_deref()
                .and_then(|href| href.strip_prefix('#'))
                .is_some_and(|id| {
                    index
                        .values()
                        .any(|node| ctx.attr(node, "id") == Some(id) && ctx.visible(node))
                })
    })
}

fn check_cross_doc_render_block(
    ctx: &ModernCtx<'_>,
    spec: &CheckSpec,
    out: &mut Vec<CheckLine>,
    index: &NodeIndex<'_>,
) {
    let Some(declared) = ctx.doc.declared.as_ref() else {
        return;
    };
    if !view_transition_active(declared) {
        return;
    }
    let Some(document) = ctx.doc.document.as_ref() else {
        return;
    };
    let covered = expect_link_covers(ctx, index, document);
    let critical_unblocked = document.scripts.iter().any(|script| {
        !render_blocked(script)
            && (script.src.is_some() || script.type_.as_deref() == Some("module"))
    }) || document
        .style_sheets
        .iter()
        .any(|sheet| !sheet.blocked && !sheet.media.eq_ignore_ascii_case("print"));
    if covered && !critical_unblocked {
        return;
    }
    let text = if covered {
        "@view-transition is active but critical head resources are not blocking=render — snapshot may capture an incomplete page"
    } else {
        "@view-transition is active but no link[rel=expect][blocking=render] targets a rendered #id — snapshot may capture an incomplete page"
    };
    ctx.emit_global(
        out,
        spec,
        "head",
        Evidence::new(text, "__document+__declared"),
    );
    tracing::debug!(
        check = spec.id,
        covered,
        critical_unblocked,
        "family E check emitted"
    );
}

fn check_cross_doc_not_opted_in(ctx: &ModernCtx<'_>, spec: &CheckSpec, out: &mut Vec<CheckLine>) {
    let Some(declared) = ctx.doc.declared.as_ref() else {
        return;
    };
    if declared.truncated || view_transition_active(declared) {
        tracing::debug!(
            check = spec.id,
            reason = "certain_or_unverifiable",
            "family E check skipped"
        );
        return;
    }
    let limitation = if declared.blocked_sheets.is_empty() {
        String::new()
    } else {
        format!(
            " ({} sheet(s) blocked; styles may be incomplete)",
            declared.blocked_sheets.len()
        )
    };
    ctx.emit_global(
        out,
        spec,
        "document",
        Evidence::new(
            format!(
                "cross-document view transitions are not opted in — no {VIEW_TRANSITION_MARKER} rule with navigation:auto{limitation}"
            ),
            "__declared",
        ),
    );
    tracing::debug!(check = spec.id, "family E check emitted");
}

fn collect_types_at(text: &str, marker: &str, out: &mut BTreeSet<String>) {
    let lower = text.to_ascii_lowercase();
    let mut rest = lower.as_str();
    while let Some(position) = rest.find(marker) {
        let Some(tail) = rest.get(position.saturating_add(marker.len())..) else {
            break;
        };
        let segment = tail.split([';', '}', ')']).next().unwrap_or("");
        for token in segment.split(',') {
            let name = token.trim().trim_matches(['"', '\'']).trim();
            if !name.is_empty()
                && name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
            {
                out.insert(name.to_string());
            }
        }
        rest = tail;
    }
}

fn collect_active_types(text: &str, out: &mut BTreeSet<String>) {
    let lower = text.to_ascii_lowercase();
    let mut rest = lower.as_str();
    while let Some(position) = rest.find(ACTIVE_TYPE_MARKER) {
        let Some(tail) = rest.get(position.saturating_add(ACTIVE_TYPE_MARKER.len())..) else {
            break;
        };
        let Some(end) = tail.find(')') else {
            break;
        };
        let Some(name) = tail.get(..end) else {
            break;
        };
        let name = name.trim().trim_matches(['"', '\'']).trim();
        if !name.is_empty() {
            out.insert(name.to_string());
        }
        rest = tail.get(end.saturating_add(1)..).unwrap_or("");
    }
}

fn directional_keyframes(declared: &DeclaredFacet) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for at in &declared.at_rules {
        let lower = at.condition_text.to_ascii_lowercase();
        let marker = "@keyframes";
        let mut rest = lower.as_str();
        while let Some(position) = rest.find(marker) {
            let Some(tail) = rest.get(position.saturating_add(marker.len())..) else {
                break;
            };
            let name = tail
                .trim_start()
                .split(['{', ' ', '\n', ','])
                .next()
                .unwrap_or("");
            if ["forward", "backward", "slide", "direction"]
                .iter()
                .any(|keyword| name.contains(keyword))
            {
                out.insert(name.to_string());
            }
            rest = tail;
        }
    }
    out
}

fn check_directional_type_rule(ctx: &ModernCtx<'_>, spec: &CheckSpec, out: &mut Vec<CheckLine>) {
    let Some(declared) = ctx.doc.declared.as_ref() else {
        return;
    };
    let mut used = BTreeSet::new();
    let mut covered = BTreeSet::new();
    for at in &declared.at_rules {
        collect_types_at(&at.condition_text, "types:", &mut used);
        collect_active_types(&at.condition_text, &mut covered);
    }
    for rule in &declared.rules {
        collect_active_types(&rule.selector, &mut covered);
    }
    if used.is_empty() {
        // SPA usam `startViewTransition({types})` em JS; o sinal CSS disponível
        // são keyframes direcionais sem nenhuma regra de tipo correspondente.
        let keyframes = directional_keyframes(declared);
        if !keyframes.is_empty() && covered.is_empty() {
            used = keyframes;
        }
    }
    let missing: Vec<&String> = used.difference(&covered).collect();
    if missing.is_empty() {
        return;
    }
    let names = missing
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    ctx.emit_global(
        out,
        spec,
        "document",
        Evidence::new(
            format!(
                "directional transition uses type(s) [{names}] but no :active-view-transition-type() rule defines slide animations"
            ),
            "__declared",
        ),
    );
    tracing::debug!(
        check = spec.id,
        missing = missing.len(),
        "family E check emitted"
    );
}

fn declared_scope_is_all(declared: &DeclaredFacet, node: &DiffNode) -> bool {
    declared.rules.iter().any(|rule| {
        rule.selector
            .split(',')
            .any(|selector| selector.trim() == node.selector)
            && rule
                .declarations
                .get("view-transition-scope")
                .is_some_and(|value| contains_ignore_ascii(value, "all"))
    })
}

fn alternate_isolation(declared: &DeclaredFacet) -> bool {
    let text = declared
        .at_rules
        .iter()
        .map(|at| at.condition_text.as_str())
        .chain(declared.rules.iter().map(|rule| rule.selector.as_str()))
        .fold(String::new(), |mut acc, part| {
            acc.push_str(part);
            acc
        });
    let declarations = declared.rules.iter().fold(String::new(), |mut acc, rule| {
        for (name, value) in &rule.declarations {
            acc.push_str(name);
            acc.push(':');
            acc.push_str(value);
            acc.push(';');
        }
        acc
    });
    contains_ignore_ascii(&text, "view-transition-group-children")
        && contains_ignore_ascii(&declarations, "view-transition-group:contain")
}

fn has_outer_vt_participant(ctx: &ModernCtx<'_>, index: &NodeIndex<'_>, node: &DiffNode) -> bool {
    let mut parent = node.parent_id.and_then(|id| index.get(&id).copied());
    while let Some(ancestor) = parent {
        let name = ctx
            .style(ancestor, "animation", "view-transition-name")
            .unwrap_or("");
        if !name.is_empty() && !name.eq_ignore_ascii_case("none") {
            return true;
        }
        parent = ancestor.parent_id.and_then(|id| index.get(&id).copied());
    }
    false
}

fn check_scoped_vt_isolation(
    ctx: &ModernCtx<'_>,
    spec: &CheckSpec,
    out: &mut Vec<CheckLine>,
    index: &NodeIndex<'_>,
) {
    let Some(declared) = ctx.doc.declared.as_ref() else {
        return;
    };
    if declared.truncated || alternate_isolation(declared) {
        return;
    }
    let mut emitted = 0_usize;
    for node in &ctx.flat {
        if !attr_equals(node, "view-transition-name", "match-element")
            && !ctx
                .style(node, "animation", "view-transition-name")
                .is_some_and(|value| value.eq_ignore_ascii_case("match-element"))
        {
            continue;
        }
        if ctx
            .style(node, "animation", "view-transition-scope")
            .is_some_and(|value| contains_ignore_ascii(value, "all"))
            || declared_scope_is_all(declared, node)
            || !has_outer_vt_participant(ctx, index, node)
        {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                "nested transition scope lacks view-transition-scope:all — inner names leak into the outer transition",
                "styles.animation+__declared",
            ),
        );
        emitted = emitted.saturating_add(1);
    }
    tracing::debug!(check = spec.id, emitted, "family E check emitted");
}

fn is_scroller(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    ["overflow-x", "overflow-y"]
        .iter()
        .any(|property| is_one_of(ctx.style(node, "box_model", property), &["auto", "scroll"]))
}

fn nearest_scroller<'a>(
    ctx: &ModernCtx<'a>,
    index: &NodeIndex<'a>,
    node: &DiffNode,
) -> Option<&'a DiffNode> {
    let mut parent = node.parent_id.and_then(|id| index.get(&id).copied());
    while let Some(ancestor) = parent {
        if is_scroller(ctx, ancestor) {
            return Some(ancestor);
        }
        parent = ancestor.parent_id.and_then(|id| index.get(&id).copied());
    }
    None
}

fn scroll_initial_target(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    ctx.style_any(
        node,
        &[
            ("interaction", "scroll-initial-target"),
            ("custom", "scroll-initial-target"),
        ],
    )
    .is_some_and(|value| value.eq_ignore_ascii_case("nearest"))
}

fn check_scroll_initial_target(
    ctx: &ModernCtx<'_>,
    spec: &CheckSpec,
    out: &mut Vec<CheckLine>,
    index: &NodeIndex<'_>,
) {
    let mut groups: BTreeMap<u64, Vec<&DiffNode>> = BTreeMap::new();
    for node in &ctx.flat {
        if scroll_initial_target(ctx, node)
            && let Some(scroller) = nearest_scroller(ctx, index, node)
        {
            groups.entry(scroller.id).or_default().push(node);
        }
    }
    let mut emitted = 0_usize;
    for targets in groups.values() {
        if targets.len() < 2 {
            continue;
        }
        let count = targets.len();
        for node in targets {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::numeric(
                    format!(
                        "{count} elements declare scroll-initial-target:nearest in the same scroller — only the first in DOM order is used"
                    ),
                    "styles.interaction+path",
                    "scroll_initial_targets_per_scroller",
                    Value::from(u64::try_from(count).unwrap_or(0_u64)),
                    Value::from(2_u64),
                    ">=",
                ),
            );
            emitted = emitted.saturating_add(1);
        }
    }
    tracing::debug!(check = spec.id, emitted, "family E check emitted");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modern::ModernOptions;
    use sniff_core::modern::{AtRuleInfo, DeclaredRule, EnvFacet, MatrixTag};
    use sniff_css_diff::DiffDocument;

    fn node(
        id: u64,
        selector: &str,
        tag: &str,
        styles: Value,
        attrs: Value,
        aria: Value,
    ) -> DiffNode {
        DiffNode {
            id,
            parent_id: None,
            selector: selector.to_string(),
            tag: Some(tag.to_string()),
            path: None,
            depth: None,
            rect: Some(serde_json::json!({"x": 0, "y": 0, "width": 0, "height": 0})),
            metrics: None,
            noticeable: Some(serde_json::json!({"display_visible": true})),
            hash: None,
            styles: styles.as_object().cloned(),
            pseudo: Some(serde_json::Map::new()),
            aria: Some(aria),
            contrast: None,
            ax: Some(Value::Object(serde_json::Map::new())),
            attributes: Some(attrs),
            effective_background: None,
            children: Vec::new(),
        }
    }

    fn doc(
        nodes: Vec<DiffNode>,
        actions: Vec<Value>,
        document: Option<DocumentFacet>,
        env: Option<EnvFacet>,
        declared: Option<DeclaredFacet>,
        matrix: Vec<MatrixTag>,
    ) -> DiffDocument {
        DiffDocument {
            nodes,
            actions,
            document,
            env,
            declared,
            matrix,
            ..DiffDocument::default()
        }
    }

    fn declared_with(at_rules: Vec<AtRuleInfo>, rules: Vec<DeclaredRule>) -> DeclaredFacet {
        DeclaredFacet {
            rules,
            blocked_sheets: Vec::new(),
            at_rules,
            truncated: false,
        }
    }

    fn at(condition: &str) -> AtRuleInfo {
        AtRuleInfo {
            condition_text: condition.to_string(),
            index: 0,
            rules: Vec::new(),
        }
    }

    fn run_all(document: &DiffDocument) -> Vec<CheckLine> {
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        out
    }

    fn ids(lines: &[CheckLine]) -> Vec<&str> {
        lines.iter().map(|line| line.check.as_str()).collect()
    }
    fn ids_for<'a>(lines: &'a [CheckLine], id: &str) -> Vec<&'a str> {
        lines
            .iter()
            .filter(|line| line.check == id)
            .map(|line| line.check.as_str())
            .collect()
    }

    #[test]
    fn registry_covers_all_family_e_ids() {
        let expected = [
            "reduced-motion-guard-missing",
            "scroll-effect-under-reduced-motion",
            "dialog-dismiss-unverified",
            "hidden-scroll-control-focus-loss",
            "toast-live-region-missing-at-load",
            "cross-doc-transition-missing-render-block",
            "cross-doc-transition-not-opted-in",
            "directional-transition-missing-type-rule",
            "scoped-vt-isolation-missing",
            "scroll-initial-target-multiple",
        ];
        let actual: BTreeSet<&str> = specs().iter().map(|spec| spec.id).collect();
        assert_eq!(actual, expected.iter().copied().collect());
        assert!(specs().iter().all(|spec| !spec.guides.is_empty()));
        assert!(specs().iter().all(|spec| !spec.keep_when.is_empty()));
    }

    #[test]
    fn reduced_motion_guard_declared_positive_negative_keep_when() {
        let motion = node(
            1,
            ".layer",
            "DIV",
            serde_json::json!({"animation": {"animation-timeline": "view()"}}),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        let no_guard = doc(
            vec![motion.clone()],
            Vec::new(),
            None,
            None,
            Some(declared_with(Vec::new(), Vec::new())),
            Vec::new(),
        );
        assert_eq!(
            ids_for(&run_all(&no_guard), "reduced-motion-guard-missing"),
            vec!["reduced-motion-guard-missing"]
        );

        let guarded = doc(
            vec![motion.clone()],
            Vec::new(),
            None,
            None,
            Some(declared_with(
                vec![at("@media (prefers-reduced-motion: no-preference)")],
                Vec::new(),
            )),
            Vec::new(),
        );
        assert!(ids_for(&run_all(&guarded), "reduced-motion-guard-missing").is_empty());

        let static_animation = node(
            2,
            ".fade",
            "DIV",
            serde_json::json!({"animation": {"animation-name": "fade"}}),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        let no_motion = doc(
            vec![static_animation],
            Vec::new(),
            None,
            None,
            Some(declared_with(Vec::new(), Vec::new())),
            Vec::new(),
        );
        assert!(ids_for(&run_all(&no_motion), "reduced-motion-guard-missing").is_empty());

        let truncated = DeclaredFacet {
            truncated: true,
            ..declared_with(Vec::new(), Vec::new())
        };
        let unknown = doc(
            vec![motion],
            Vec::new(),
            None,
            None,
            Some(truncated),
            Vec::new(),
        );
        assert!(ids_for(&run_all(&unknown), "reduced-motion-guard-missing").is_empty());
    }

    #[test]
    fn reduced_motion_guard_matrix_path_and_stabilization_precedence() {
        let motion = node(
            1,
            ".layer",
            "DIV",
            serde_json::json!({"animation": {"view-timeline-name": "--tl"}}),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        let env_reduce = EnvFacet {
            media: BTreeMap::from([(MOTION_KEY.to_string(), "reduce".to_string())]),
            emulated: BTreeMap::new(),
            device: None,
        };
        let matrix = vec![MatrixTag {
            index: 0,
            total: 2,
            viewport: None,
            emulation: BTreeMap::from([(MOTION_KEY.to_string(), "reduce".to_string())]),
            node_id_start: None,
            node_id_end: None,
        }];
        let with_matrix = doc(
            vec![motion.clone()],
            Vec::new(),
            None,
            Some(env_reduce.clone()),
            None,
            matrix.clone(),
        );
        assert_eq!(
            ids(&run_all(&with_matrix)),
            vec!["reduced-motion-guard-missing"]
        );

        let env_only = doc(
            vec![motion.clone()],
            Vec::new(),
            None,
            Some(env_reduce.clone()),
            None,
            Vec::new(),
        );
        assert!(run_all(&env_only).is_empty(), "__env sozinho não basta");

        let explicit_no_preference = EnvFacet {
            emulated: BTreeMap::from([(MOTION_KEY.to_string(), "no-preference".to_string())]),
            ..env_reduce
        };
        let stabilized = doc(
            vec![motion],
            Vec::new(),
            None,
            Some(explicit_no_preference),
            None,
            matrix,
        );
        assert!(
            run_all(&stabilized).is_empty(),
            "emulação explícita vence a estabilização (§F-C)"
        );
    }

    #[test]
    fn scroll_effect_requires_reduce_and_snapping() {
        let slide = node(
            1,
            ".slide",
            "DIV",
            serde_json::json!({
                "animation": {"animation-timeline": "view(inline)", "animation-name": "slide"},
                "interaction": {"scroll-snap-type": "x mandatory"}
            }),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        let env = |value: &str| EnvFacet {
            media: BTreeMap::from([(MOTION_KEY.to_string(), value.to_string())]),
            emulated: BTreeMap::new(),
            device: None,
        };
        let reduce = doc(
            vec![slide.clone()],
            Vec::new(),
            None,
            Some(env("reduce")),
            None,
            Vec::new(),
        );
        assert_eq!(
            ids(&run_all(&reduce)),
            vec!["scroll-effect-under-reduced-motion"]
        );

        let no_preference = doc(
            vec![slide.clone()],
            Vec::new(),
            None,
            Some(env("no-preference")),
            None,
            Vec::new(),
        );
        assert!(run_all(&no_preference).is_empty());

        let disabled = node(
            1,
            ".slide",
            "DIV",
            serde_json::json!({
                "animation": {"animation-timeline": "view(inline)", "animation-name": "none"},
                "interaction": {"scroll-snap-type": "x mandatory"}
            }),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        let guarded = doc(
            vec![disabled],
            Vec::new(),
            None,
            Some(env("reduce")),
            None,
            Vec::new(),
        );
        assert!(run_all(&guarded).is_empty());
    }

    #[test]
    fn dialog_dismiss_positive_negative_keep_when() {
        let dialog = node(
            1,
            "dialog#d",
            "DIALOG",
            serde_json::json!({}),
            serde_json::json!({"open": "", "closedby": "closerequest"}),
            serde_json::json!({}),
        );
        let escape = |open_after: bool| {
            serde_json::json!({
                "action": "key",
                "key": "Escape",
                "selector": "dialog#d",
                "open_after": open_after
            })
        };
        let stuck = doc(
            vec![dialog.clone()],
            vec![escape(true)],
            None,
            None,
            None,
            Vec::new(),
        );
        assert_eq!(ids(&run_all(&stuck)), vec!["dialog-dismiss-unverified"]);

        let closed = doc(
            vec![dialog.clone()],
            vec![escape(false)],
            None,
            None,
            None,
            Vec::new(),
        );
        assert!(run_all(&closed).is_empty());

        let no_action = doc(vec![dialog], Vec::new(), None, None, None, Vec::new());
        assert!(run_all(&no_action).is_empty());

        let light_dismiss = node(
            2,
            "dialog#light",
            "DIALOG",
            serde_json::json!({}),
            serde_json::json!({"open": "", "closedby": "any"}),
            serde_json::json!({}),
        );
        let light = doc(
            vec![light_dismiss],
            vec![serde_json::json!({
                "action": "key", "key": "Escape", "selector": "dialog#light", "open_after": true
            })],
            None,
            None,
            None,
            Vec::new(),
        );
        assert!(run_all(&light).is_empty());
    }

    #[test]
    fn hidden_scroll_control_focus_loss_positive_negative() {
        let control = node(
            1,
            "button.back-to-top",
            "BUTTON",
            serde_json::json!({
                "visual": {"visibility": "hidden", "opacity": "0"},
                "layout": {"position": "sticky"}
            }),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        let action = |after: &str| {
            serde_json::json!({
                "focus_before": "button.back-to-top",
                "focus_after": after
            })
        };
        let lost = doc(
            vec![control.clone()],
            vec![action("BODY")],
            None,
            None,
            None,
            Vec::new(),
        );
        assert_eq!(
            ids(&run_all(&lost)),
            vec!["hidden-scroll-control-focus-loss"]
        );

        let moved_to_scroller = doc(
            vec![control.clone()],
            vec![action("div.scroller")],
            None,
            None,
            None,
            Vec::new(),
        );
        assert!(run_all(&moved_to_scroller).is_empty());

        let visible = node(
            2,
            "button.back-to-top",
            "BUTTON",
            serde_json::json!({
                "visual": {"visibility": "visible"},
                "layout": {"position": "sticky"}
            }),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        let no_hidden_control = doc(
            vec![visible],
            vec![action("BODY")],
            None,
            None,
            None,
            Vec::new(),
        );
        assert!(run_all(&no_hidden_control).is_empty());
    }

    #[test]
    fn toast_live_region_positive_negative_keep_when() {
        let toast = node(
            1,
            "div.toast",
            "DIV",
            serde_json::json!({}),
            serde_json::json!({"popover": "manual"}),
            serde_json::json!({}),
        );
        let document = Some(DocumentFacet::default());
        let missing = doc(
            vec![toast.clone()],
            Vec::new(),
            document.clone(),
            None,
            None,
            Vec::new(),
        );
        assert_eq!(
            ids(&run_all(&missing)),
            vec!["toast-live-region-missing-at-load"]
        );

        let live = node(
            2,
            "div.announcer",
            "DIV",
            serde_json::json!({}),
            serde_json::json!({"aria-live": "polite"}),
            serde_json::json!({}),
        );
        let present = doc(
            vec![toast.clone(), live],
            Vec::new(),
            document.clone(),
            None,
            None,
            Vec::new(),
        );
        assert!(ids_for(&run_all(&present), "toast-live-region-missing-at-load").is_empty());

        let without_document = doc(vec![toast], Vec::new(), None, None, None, Vec::new());
        assert!(
            ids_for(
                &run_all(&without_document),
                "toast-live-region-missing-at-load"
            )
            .is_empty()
        );
    }

    #[test]
    fn cross_doc_render_block_and_opt_in_positive_negative() {
        let target = node(
            1,
            "#main",
            "MAIN",
            serde_json::json!({}),
            serde_json::json!({"id": "main"}),
            serde_json::json!({}),
        );
        let mut document = DocumentFacet::default();
        document.scripts.push(sniff_core::modern::ScriptInfo {
            src: Some("/app.js".to_string()),
            type_: Some("module".to_string()),
            defer: true,
            async_: false,
            fetchpriority: None,
            integrity: None,
            blocking: None,
            in_head: Some(true),
            text: None,
            importance: None,
        });
        let active = declared_with(
            vec![at("@view-transition { navigation: auto }")],
            Vec::new(),
        );
        let unblocked = doc(
            vec![target.clone()],
            Vec::new(),
            Some(document.clone()),
            None,
            Some(active.clone()),
            Vec::new(),
        );
        assert_eq!(
            ids(&run_all(&unblocked)),
            vec!["cross-doc-transition-missing-render-block"]
        );

        let expect = sniff_core::modern::LinkInfo {
            rel: "expect".to_string(),
            href: Some("#main".to_string()),
            as_: None,
            fetchpriority: None,
            integrity: None,
            crossorigin: None,
        };
        document.links.push(expect);
        // Sem campo `blocking` na facet, cobertura do expect reduz o ruído:
        // o recurso crítico ainda é sinalizado, então o check permanece.
        let with_expect = doc(
            vec![target.clone()],
            Vec::new(),
            Some(document.clone()),
            None,
            Some(active.clone()),
            Vec::new(),
        );
        assert_eq!(
            ids(&run_all(&with_expect)),
            vec!["cross-doc-transition-missing-render-block"]
        );

        let opted_out = doc(
            vec![target.clone()],
            Vec::new(),
            Some(document),
            None,
            Some(declared_with(Vec::new(), Vec::new())),
            Vec::new(),
        );
        assert_eq!(
            ids(&run_all(&opted_out)),
            vec!["cross-doc-transition-not-opted-in"]
        );

        let truncated = DeclaredFacet {
            truncated: true,
            ..declared_with(Vec::new(), Vec::new())
        };
        let unverifiable = doc(
            vec![target],
            Vec::new(),
            None,
            None,
            Some(truncated),
            Vec::new(),
        );
        assert!(run_all(&unverifiable).is_empty());
    }

    #[test]
    fn directional_transition_type_rule_positive_negative() {
        let declared = declared_with(
            vec![
                at("@view-transition { types: forward, back }"),
                at("@keyframes slide-forward { from { transform: translateX(100%) } }"),
            ],
            Vec::new(),
        );
        let missing = doc(
            Vec::new(),
            Vec::new(),
            None,
            None,
            Some(declared),
            Vec::new(),
        );
        let lines = run_all(&missing);
        assert_eq!(
            ids(&lines),
            vec!["directional-transition-missing-type-rule"]
        );
        assert!(
            lines[0].evidence.text.contains("back") && !lines[0].evidence.text.contains("[forward")
        );

        let covered = declared_with(
            vec![
                at("@view-transition { types: forward, back }"),
                at("@keyframes slide-forward { from { transform: translateX(100%) } }"),
            ],
            vec![
                DeclaredRule {
                    selector: ":active-view-transition-type(forward)::view-transition-old(root)"
                        .to_string(),
                    href: None,
                    index: 0,
                    origin: "author".to_string(),
                    declarations: BTreeMap::new(),
                },
                DeclaredRule {
                    selector: ":active-view-transition-type(back)::view-transition-new(root)"
                        .to_string(),
                    href: None,
                    index: 1,
                    origin: "author".to_string(),
                    declarations: BTreeMap::new(),
                },
            ],
        );
        let ok = doc(
            Vec::new(),
            Vec::new(),
            None,
            None,
            Some(covered),
            Vec::new(),
        );
        assert!(run_all(&ok).is_empty());
    }

    #[test]
    fn scoped_vt_isolation_positive_negative_keep_when() {
        let mut wrapper = node(
            1,
            ".card-wrapper",
            "DIV",
            serde_json::json!({"animation": {"view-transition-name": "match-element"}}),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        let mut inner = node(
            2,
            ".card ul li",
            "LI",
            serde_json::json!({"animation": {"view-transition-name": "match-element"}}),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        inner.parent_id = Some(wrapper.id);
        let declared = declared_with(Vec::new(), Vec::new());
        let missing = doc(
            vec![wrapper.clone(), inner.clone()],
            Vec::new(),
            None,
            None,
            Some(declared.clone()),
            Vec::new(),
        );
        assert_eq!(
            ids_for(&run_all(&missing), "scoped-vt-isolation-missing"),
            vec!["scoped-vt-isolation-missing"]
        );

        inner.styles = Some(
            serde_json::json!({"animation": {
                "view-transition-name": "match-element",
                "view-transition-scope": "all"
            }})
            .as_object()
            .cloned()
            .unwrap(),
        );
        let scoped = doc(
            vec![wrapper.clone(), inner.clone()],
            Vec::new(),
            None,
            None,
            Some(declared.clone()),
            Vec::new(),
        );
        assert!(ids_for(&run_all(&scoped), "scoped-vt-isolation-missing").is_empty());

        inner.styles = Some(
            serde_json::json!({"animation": {"view-transition-name": "match-element"}})
                .as_object()
                .cloned()
                .unwrap(),
        );
        wrapper.id = 99;
        inner.parent_id = Some(77);
        let isolated_component = doc(
            vec![wrapper, inner],
            Vec::new(),
            None,
            None,
            Some(declared),
            Vec::new(),
        );
        assert!(ids_for(&run_all(&isolated_component), "scoped-vt-isolation-missing").is_empty());
    }

    #[test]
    fn scroll_initial_target_multiple_positive_negative() {
        let scroller_styles = || {
            serde_json::json!({"box_model": {"overflow-y": "auto"}})
                .as_object()
                .cloned()
                .unwrap()
        };
        let mut scroller = node(
            1,
            "div.feed",
            "DIV",
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        scroller.styles = Some(scroller_styles());
        let mut first = node(
            2,
            ".item.a",
            "DIV",
            serde_json::json!({"interaction": {"scroll-initial-target": "nearest"}}),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        first.parent_id = Some(scroller.id);
        let mut second = node(
            3,
            ".item.b",
            "DIV",
            serde_json::json!({"interaction": {"scroll-initial-target": "nearest"}}),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        second.parent_id = Some(scroller.id);
        let multiple = doc(
            vec![scroller.clone(), first.clone(), second.clone()],
            Vec::new(),
            None,
            None,
            None,
            Vec::new(),
        );
        let lines = run_all(&multiple);
        assert_eq!(
            ids(&lines),
            vec![
                "scroll-initial-target-multiple",
                "scroll-initial-target-multiple"
            ]
        );
        assert_eq!(lines[0].evidence.observed, Some(Value::from(2_u64)));

        let mut other = node(
            4,
            "div.other",
            "DIV",
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        other.styles = Some(scroller_styles());
        second.parent_id = Some(other.id);
        let separate = doc(
            vec![scroller, first, other, second],
            Vec::new(),
            None,
            None,
            None,
            Vec::new(),
        );
        assert!(run_all(&separate).is_empty());
    }

    #[test]
    fn v1_snapshot_without_new_facets_emits_nothing() {
        let motion = node(
            1,
            ".layer",
            "DIV",
            serde_json::json!({"animation": {"animation-timeline": "view()"}}),
            serde_json::json!({"open": ""}),
            serde_json::json!({}),
        );
        let empty = doc(vec![motion], Vec::new(), None, None, None, Vec::new());
        assert!(run_all(&empty).is_empty());
    }

    #[test]
    fn output_is_byte_for_byte_deterministic() {
        let motion = node(
            1,
            ".layer",
            "DIV",
            serde_json::json!({"animation": {"animation-timeline": "view()"}}),
            serde_json::json!({}),
            serde_json::json!({}),
        );
        let document = DocumentFacet::default();
        let snapshot = doc(
            vec![motion],
            Vec::new(),
            Some(document),
            None,
            Some(declared_with(Vec::new(), Vec::new())),
            Vec::new(),
        );
        let first = serde_json::to_string(&run_all(&snapshot)).unwrap();
        let second = serde_json::to_string(&run_all(&snapshot)).unwrap();
        assert_eq!(first, second);
    }
}
