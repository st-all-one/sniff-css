//! Família A — estrutura, semântica e metadata.
//!
//! Todos os gatilhos vêm exclusivamente de campos do snapshot (`tag`, `tree`,
//! `attrs`, `aria`, `pseudo`, `styles`, `__document`). Nada infere intenção do
//! autor: quando o sinal é ambíguo, o check silencia (§7.1 do plano).
//!
//! Dois ids WCAG-normativos são registrados separadamente dos avisos
//! oportunísticos para que `failing()` não contamine sinais meramente
//! diagnósticos: `document-lang-missing` (WCAG 3.1.1) e
//! `focusable-tabindex-positive` (WCAG 2.4.3). Os demais sinais estruturais
//! do guia `html` (doctype, viewport, título, h1, hierarquia de headings,
//! landmarks e título de iframe) ficam agregados em
//! `document-metadata-basics`, que é `warn`.

use crate::modern::{CheckSpec, Facet, ModernCtx};
use crate::rules::CheckLine;
use serde_json::Value;
use sniff_core::modern::Evidence;
use sniff_css_diff::DiffNode;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// Metadados globais: doctype, viewport, título, headings e landmarks.
pub const DOCUMENT_METADATA: CheckSpec = CheckSpec::new(
    "document-metadata-basics",
    &["html", "respect-os-text-scale"],
    &[Facet::Document],
    "keepWhen: viewport presente com width=device-width e sem user-scalable=no; title com sufixo de site; h1 dentro de dialog não conta; documento sem texto legível não exige h1; main único.",
);
/// Idioma ausente no elemento raiz (critério normativo WCAG 3.1.1 A).
pub const DOCUMENT_LANG: CheckSpec = CheckSpec::new(
    "document-lang-missing",
    &["html"],
    &[Facet::Document],
    "keepWhen: lang presente e não vazio no __document ou no nó HTML; documento sem conteúdo linguístico (página puramente visual).",
).failing();
/// Semântica básica de formulários e agrupamentos.
pub const FORM_SEMANTICS: CheckSpec = CheckSpec::new(
    "form-semantics-basics",
    &["forms", "html"],
    &[Facet::Attrs],
    "keepWhen: button sem type fora de form; fieldset com role=group e nome; placeholder complementar a label persistente; controles hidden/aria-hidden.",
);
/// `tabindex` positivo em elemento focável (critério WCAG 2.4.3 A).
pub const FOCUSABLE_TABINDEX: CheckSpec = CheckSpec::new(
    "focusable-tabindex-positive",
    &["forms", "html"],
    &[Facet::Attrs],
    "keepWhen: tabindex ausente, zero ou negativo; valor não numérico; controle aria-hidden.",
)
.failing();
/// Trilha de navegação sem item corrente marcado.
pub const BREADCRUMB_CURRENT: CheckSpec = CheckSpec::new(
    "breadcrumb-current-page-missing",
    &["breadcrumbs"],
    &[Facet::Attrs],
    "keepWhen: nav sem nome que case com 'breadcrumb'; menos de dois links; algum descendente já traz aria-current=page.",
);
/// Texto de `content` gerado sem alternativa no nome acessível.
pub const GENERATED_CONTENT_ALT: CheckSpec = CheckSpec::new(
    "generated-content-alt",
    &["accessible-generated-content"],
    &[Facet::Pseudo],
    "keepWhen: conteúdo puramente decorativo (pontuação/setas) com alt vazio; alt presente e não duplicado; descrição diferente do nome oficial do emoji; mesmo texto no DOM sem duplicação.",
);
/// Múltiplas regiões vivas declaradas no documento.
pub const LIVE_REGION_ANNOUNCER: CheckSpec = CheckSpec::new(
    "live-region-announcer",
    &["accessible-error-announcement"],
    &[Facet::Attrs],
    "keepWhen: no máximo uma região aria-live/role=status; role=alert isolado (uso crítico prescrito); aria-live=off em contador passivo.",
);
/// `autofocus` sem `aria-describedby` resolvível.
pub const AUTOFOCUS_DESCRIBEDBY: CheckSpec = CheckSpec::new(
    "autofocus-without-describedby",
    &["passkey-authentication"],
    &[Facet::Attrs],
    "keepWhen: sem autofocus; aria-describedby presente e apontando apenas para ids existentes; nó não focável.",
);
/// Controle interativo aninhado dentro de link de card.
pub const CARD_INTERACTIVE_NESTED: CheckSpec = CheckSpec::new(
    "card-interactive-nested-in-link",
    &["card"],
    &[Facet::Attrs],
    "keepWhen: link só com conteúdo não interativo; controle dentro de LABEL (choice card); input hidden; descendente oculto ou aria-hidden.",
);
/// Navegação de site usando papéis de menu composto.
pub const SITE_NAV_MENU_ROLES: CheckSpec = CheckSpec::new(
    "site-nav-using-menu-roles",
    &["navbar"],
    &[Facet::Attrs],
    "keepWhen: sem NAV; papéis de menu fora de NAV (app-menus); menus de comando abaixo dos filhos diretos do NAV.",
);
/// Região rolável e focável sem papel/nome acessível.
pub const SCROLL_REGION_NAME: CheckSpec = CheckSpec::new(
    "scroll-region-without-name",
    &["responsive-table"],
    &[Facet::Styles, Facet::Attrs],
    "keepWhen: MAIN com papel implícito; aria-labelledby apontando para a caption; região sem tabindex=0; wrapper sem tabela.",
);
/// Elemento `<install>` sem manifesto vinculado.
pub const WEB_INSTALL_MANIFEST: CheckSpec = CheckSpec::new(
    "web-install-manifest-required",
    &["install-web-app"],
    &[Facet::Document, Facet::Attrs],
    "keepWhen: nenhum elemento <install>; link rel=manifest com href no __document ou nos nós link; instalação apenas por navigator.install() (não observável).",
);
/// `<form>` promovido a WebMCP tool sem descrição.
pub const WEBMCP_FORM_METADATA: CheckSpec = CheckSpec::new(
    "webmcp-form-tool-metadata",
    &["agentic-forms"],
    &[Facet::Attrs],
    "keepWhen: form comum sem atributos tool*; tooldescription presente e não vazio; heurística de toolautosubmit não entra no gatilho.",
);

/// Todos os ids da família A registrados (ordem estável do catálogo).
pub fn specs() -> &'static [CheckSpec] {
    &[
        DOCUMENT_METADATA,
        DOCUMENT_LANG,
        FORM_SEMANTICS,
        FOCUSABLE_TABINDEX,
        BREADCRUMB_CURRENT,
        GENERATED_CONTENT_ALT,
        LIVE_REGION_ANNOUNCER,
        AUTOFOCUS_DESCRIBEDBY,
        CARD_INTERACTIVE_NESTED,
        SITE_NAV_MENU_ROLES,
        SCROLL_REGION_NAME,
        WEB_INSTALL_MANIFEST,
        WEBMCP_FORM_METADATA,
    ]
}

/// Roda a família A em ordem determinística, respeitando facets e filtros.
pub fn run(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    let meta = ctx.enabled(&DOCUMENT_METADATA);
    let lang = ctx.enabled(&DOCUMENT_LANG);
    if meta || lang {
        tracing::debug!(
            document_facet = meta,
            lang_check = lang,
            "structure: document checks"
        );
        document_metadata(ctx, out, meta, lang);
    }

    let semantics = ctx.enabled(&FORM_SEMANTICS);
    let tabindex = ctx.enabled(&FOCUSABLE_TABINDEX);
    if semantics || tabindex {
        tracing::debug!(semantics, tabindex, "structure: form checks");
        form_semantics(ctx, out, semantics, tabindex);
    }

    if ctx.enabled(&BREADCRUMB_CURRENT) {
        breadcrumb(ctx, out);
    }
    if ctx.enabled(&GENERATED_CONTENT_ALT) {
        generated_content(ctx, out);
    }
    if ctx.enabled(&LIVE_REGION_ANNOUNCER) {
        live_region(ctx, out);
    }
    if ctx.enabled(&AUTOFOCUS_DESCRIBEDBY) {
        autofocus_check(ctx, out);
    }
    if ctx.enabled(&CARD_INTERACTIVE_NESTED) {
        card_links(ctx, out);
    }
    if ctx.enabled(&SITE_NAV_MENU_ROLES) {
        site_nav(ctx, out);
    }
    if ctx.enabled(&SCROLL_REGION_NAME) {
        scroll_region(ctx, out);
    }
    if ctx.enabled(&WEB_INSTALL_MANIFEST) {
        web_install(ctx, out);
    }
    if ctx.enabled(&WEBMCP_FORM_METADATA) {
        webmcp_form(ctx, out);
    }
}

fn emit(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
    node: &DiffNode,
    evidence: Evidence,
) {
    tracing::debug!(check = spec.id, selector = %node.selector, "structure check emitted");
    ctx.emit(out, spec, node, evidence);
}

fn emit_global(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
    selector: &str,
    evidence: Evidence,
) {
    tracing::debug!(check = spec.id, selector, "structure check emitted");
    ctx.emit_global(out, spec, selector, evidence);
}

fn attr_present(node: &DiffNode, name: &str) -> bool {
    node.attributes
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|m| m.contains_key(name))
}

fn submittable_control(tag: &str, node: &DiffNode, ctx: &ModernCtx<'_>) -> bool {
    if !matches!(tag, "INPUT" | "SELECT" | "TEXTAREA") {
        return false;
    }
    let excluded = ["hidden", "submit", "reset", "button", "image"];
    !ctx.attr(node, "type")
        .is_some_and(|t| excluded.iter().any(|e| t.eq_ignore_ascii_case(e)))
}

/// Percorre a cadeia de `parent_id` sem alocar nem tocar em `HashMap`.
fn ancestor_has(
    ctx: &ModernCtx<'_>,
    node: &DiffNode,
    parents: &BTreeMap<u64, u64>,
    tag: &str,
) -> bool {
    let mut current = node.parent_id;
    while let Some(parent_id) = current {
        let Some(parent) = ctx.flat.iter().find(|n| n.id == parent_id) else {
            return false;
        };
        if ctx.tag(parent) == tag {
            return true;
        }
        current = parents.get(&parent_id).copied();
    }
    false
}

// ---------------------------------------------------------------------------
// document-metadata-basics + document-lang-missing
// ---------------------------------------------------------------------------

#[derive(Default)]
struct HeadingState {
    h1_count: usize,
    last_level: Option<u8>,
    first_jump: Option<String>,
    main_count: usize,
    untitled_iframes: usize,
}

fn heading_level(tag: &str) -> Option<u8> {
    match tag {
        "H1" => Some(1),
        "H2" => Some(2),
        "H3" => Some(3),
        "H4" => Some(4),
        "H5" => Some(5),
        "H6" => Some(6),
        _ => None,
    }
}

fn walk_headings<'a>(
    node: &'a DiffNode,
    ctx: &ModernCtx<'a>,
    inside_dialog: bool,
    state: &mut HeadingState,
) {
    let tag = ctx.tag(node);
    let hidden = ctx.aria_bool(node, "aria_hidden") || !ctx.visible(node);
    let nested_dialog = inside_dialog || tag == "DIALOG";

    if !hidden {
        if ctx
            .aria(node, "role")
            .is_some_and(|r| r.eq_ignore_ascii_case("main"))
            || tag == "MAIN"
        {
            state.main_count = state.main_count.saturating_add(1);
        }
        if tag == "IFRAME" && ctx.attr(node, "title").is_none() && ctx.aria(node, "title").is_none()
        {
            state.untitled_iframes = state.untitled_iframes.saturating_add(1);
        }
        if !nested_dialog {
            if tag == "H1" {
                state.h1_count = state.h1_count.saturating_add(1);
            }
            if let Some(level) = heading_level(tag) {
                if let Some(previous) = state.last_level
                    && level > previous.saturating_add(1)
                    && state.first_jump.is_none()
                {
                    state.first_jump = Some(format!(
                        "heading jump H{}->H{} at {}",
                        previous, level, node.selector
                    ));
                }
                state.last_level = Some(level);
            }
        }
    }

    for child in &node.children {
        walk_headings(child, ctx, nested_dialog, state);
    }
}

fn document_metadata(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, meta: bool, lang: bool) {
    let mut signals = Vec::new();
    if let Some(document) = &ctx.doc.document {
        if document
            .doctype
            .as_deref()
            .is_none_or(|d| d.trim().is_empty())
        {
            signals.push("doctype missing".to_string());
        }
        match document.meta.get("viewport") {
            None => signals.push("meta[name=viewport] missing".to_string()),
            Some(content) => {
                if !content.contains("width=device-width") {
                    signals.push("viewport lacks width=device-width".to_string());
                }
                if content.contains("user-scalable=no") {
                    signals.push("viewport disables zoom (user-scalable=no)".to_string());
                }
            }
        }
        if document
            .title
            .as_deref()
            .is_none_or(|t| t.trim().is_empty())
        {
            signals.push("title missing".to_string());
        }
    }

    let mut headings = HeadingState::default();
    for root in &ctx.doc.nodes {
        walk_headings(root, ctx, false, &mut headings);
    }
    if headings.untitled_iframes > 0 {
        signals.push(format!(
            "iframe without title: {}",
            headings.untitled_iframes
        ));
    }
    let has_text = ctx
        .flat
        .iter()
        .any(|node| ctx.aria_bool(node, "has_text") || heading_level(ctx.tag(node)).is_some());
    if has_text && headings.h1_count == 0 {
        signals.push("no h1 outside dialog".to_string());
    }
    if let Some(jump) = &headings.first_jump {
        signals.push(jump.clone());
    }
    if headings.main_count > 1 {
        signals.push(format!("duplicate main landmark: {}", headings.main_count));
    }

    if meta && !signals.is_empty() {
        emit_global(
            ctx,
            out,
            &DOCUMENT_METADATA,
            "__document",
            Evidence::new(signals.join("; "), "__document"),
        );
    } else if meta {
        tracing::trace!(
            check = DOCUMENT_METADATA.id,
            "structure: metadata keepWhen matched"
        );
    }

    if lang {
        let declared = ctx.doc.document.as_ref().is_some_and(|d| {
            d.lang
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        }) || ctx
            .flat
            .iter()
            .any(|node| ctx.tag(node) == "HTML" && ctx.aria(node, "lang").is_some());
        if !declared && has_text {
            emit_global(
                ctx,
                out,
                &DOCUMENT_LANG,
                "html",
                Evidence::new("html lang missing (WCAG 3.1.1 Level A)", "__document"),
            );
        } else if !has_text {
            tracing::trace!(
                check = DOCUMENT_LANG.id,
                reason = "no readable text",
                "structure: keepWhen matched"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// form-semantics-basics + focusable-tabindex-positive
// ---------------------------------------------------------------------------

fn form_semantics(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, warn: bool, fail: bool) {
    let parents: BTreeMap<u64, u64> = ctx
        .flat
        .iter()
        .filter_map(|n| n.parent_id.map(|p| (n.id, p)))
        .collect();
    let labels: BTreeSet<&str> = ctx
        .flat
        .iter()
        .filter(|n| ctx.tag(n) == "LABEL")
        .filter_map(|n| ctx.attr(n, "for"))
        .collect();

    for node in &ctx.flat {
        let tag = ctx.tag(node);
        let hidden = ctx.aria_bool(node, "aria_hidden") || !ctx.visible(node);
        let in_form = ancestor_has(ctx, node, &parents, "FORM");
        let in_label = ancestor_has(ctx, node, &parents, "LABEL");

        if fail && !hidden {
            let positive_tabindex = ctx
                .attr(node, "tabindex")
                .and_then(|value| value.trim().parse::<i32>().ok())
                .is_some_and(|value| value > 0);
            if positive_tabindex {
                emit(
                    ctx,
                    out,
                    &FOCUSABLE_TABINDEX,
                    node,
                    Evidence::new(
                        format!(
                            "tabindex={} is greater than 0 (WCAG 2.4.3)",
                            0_i32.saturating_add(1)
                        ),
                        "attrs",
                    ),
                );
            }
        }

        if !warn || hidden {
            continue;
        }

        if tag == "FORM" && in_form {
            emit(
                ctx,
                out,
                &FORM_SEMANTICS,
                node,
                Evidence::new("nested form: FORM inside another FORM", "tree"),
            );
            continue;
        }
        if in_form && submittable_control(tag, node, ctx) && ctx.attr(node, "name").is_none() {
            emit(
                ctx,
                out,
                &FORM_SEMANTICS,
                node,
                Evidence::new(format!("{} without name inside form", tag), "attrs"),
            );
            continue;
        }
        if in_form && tag == "BUTTON" && ctx.attr(node, "type").is_none() {
            emit(
                ctx,
                out,
                &FORM_SEMANTICS,
                node,
                Evidence::new("submit button without type inside form", "attrs"),
            );
            continue;
        }
        if tag == "FIELDSET" {
            let has_legend = node
                .children
                .first()
                .is_some_and(|child| ctx.tag(child) == "LEGEND");
            let grouped = ctx
                .aria(node, "role")
                .is_some_and(|r| r.eq_ignore_ascii_case("group"))
                && ctx.aria(node, "name").is_some();
            if !has_legend && !grouped {
                emit(
                    ctx,
                    out,
                    &FORM_SEMANTICS,
                    node,
                    Evidence::new("fieldset without leading legend", "tree"),
                );
            }
            continue;
        }
        if matches!(tag, "INPUT" | "TEXTAREA")
            && ctx.attr(node, "placeholder").is_some()
            && ctx.aria(node, "name").is_none()
            && !in_label
            && ctx.attr(node, "id").is_none_or(|id| !labels.contains(id))
        {
            emit(
                ctx,
                out,
                &FORM_SEMANTICS,
                node,
                Evidence::new("placeholder used as only accessible label", "attrs"),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// breadcrumb-current-page-missing
// ---------------------------------------------------------------------------

fn collect_descendants<'a>(node: &'a DiffNode, out: &mut Vec<&'a DiffNode>) {
    for child in &node.children {
        out.push(child);
        collect_descendants(child, out);
    }
}

fn breadcrumb(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    for node in &ctx.flat {
        if ctx.tag(node) != "NAV"
            || !ctx
                .aria(node, "name")
                .is_some_and(|name| name.to_ascii_lowercase().contains("breadcrumb"))
        {
            continue;
        }
        let mut descendants = Vec::new();
        collect_descendants(node, &mut descendants);
        let link_count = descendants
            .iter()
            .filter(|child| ctx.tag(child) == "A")
            .count();
        let marked = descendants.iter().any(|child| {
            ctx.attr(child, "aria-current")
                .is_some_and(|value| value.eq_ignore_ascii_case("page"))
        });
        if link_count >= 2 && !marked {
            emit(
                ctx,
                out,
                &BREADCRUMB_CURRENT,
                node,
                Evidence::new(
                    format!("breadcrumb nav has {link_count} links but no aria-current=\"page\""),
                    "attrs",
                ),
            );
        } else {
            tracing::trace!(
                check = BREADCRUMB_CURRENT.id,
                links = link_count,
                "structure: breadcrumb keepWhen matched"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// generated-content-alt
// ---------------------------------------------------------------------------

/// Category order used to find the `content` declaration inside a pseudo
/// style object; `content` lives in `accessibility` in the extended catalog.
fn pseudo_content(styles: &Value) -> Option<&str> {
    ["accessibility", "visual", "custom", "box_model"]
        .iter()
        .find_map(|category| {
            styles
                .get(*category)
                .and_then(|group| group.get("content"))
                .and_then(Value::as_str)
        })
}

fn split_content(raw: &str) -> (&str, Option<&str>) {
    match raw.split_once(" / ") {
        Some((source, alt)) => (source.trim(), Some(alt.trim())),
        None => (raw.trim(), None),
    }
}

fn unquote(value: &str) -> &str {
    value.trim().trim_matches(|c| c == '"' || c == '\'').trim()
}

fn decorative_content(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_whitespace() || "-–—/\\|+*→←↑↓•·<>\"'`^~!?.,;:…()[]{}".contains(c))
}

const EMOJI_OFFICIAL_NAMES: &[(&str, &[&str])] = &[
    ("🎉", &["celebration", "tada", "party popper"]),
    ("✅", &["check mark", "checkmark", "white heavy check mark"]),
    ("❌", &["cross mark", "x"]),
];

fn repeats_official_emoji_name(source: &str, alt: &str) -> bool {
    EMOJI_OFFICIAL_NAMES.iter().any(|(emoji, names)| {
        *emoji == source && names.iter().any(|name| alt.eq_ignore_ascii_case(name))
    })
}

fn generated_content(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    for node in &ctx.flat {
        let Some(pseudo) = &node.pseudo else {
            continue;
        };
        let dom_name = ctx.aria(node, "name").map(str::to_ascii_lowercase);
        let mut reason: Option<String> = None;
        for (pseudo_name, styles) in pseudo {
            let Some(raw) = pseudo_content(styles) else {
                continue;
            };
            let (source_raw, alt_raw) = split_content(raw);
            let source = unquote(source_raw);
            // Only the bare CSS keyword means "no generated content"; a quoted
            // `"none"` renders literal text and must stay visible to the check.
            if source.is_empty()
                || matches!(
                    source_raw.trim().to_ascii_lowercase().as_str(),
                    "none" | "normal"
                )
            {
                continue;
            }
            let alt = alt_raw.map(unquote);
            let source_lower = source.to_ascii_lowercase();
            let is_url = source.starts_with("url(");
            let in_dom = dom_name
                .as_deref()
                .is_some_and(|name| name.contains(&source_lower));

            let found = if is_url && alt.is_some_and(|a| a.is_empty()) {
                Some("generated image with empty alt")
            } else if !is_url && decorative_content(source) {
                tracing::trace!(check = GENERATED_CONTENT_ALT.id, pseudo = %pseudo_name, "structure: decorative content ignored");
                None
            } else if let Some(a) = alt.filter(|a| !a.is_empty()) {
                if repeats_official_emoji_name(source, a) {
                    Some("alt repeats emoji's official name")
                } else if dom_name
                    .as_deref()
                    .is_some_and(|name| name == a.to_ascii_lowercase())
                {
                    Some("generated alt duplicates accessible name")
                } else {
                    None
                }
            } else if in_dom {
                None
            } else if is_url {
                None
            } else if alt.is_some() {
                Some("meaningful generated text has empty alt")
            } else {
                Some("meaningful generated text missing from accessible name")
            };
            if let Some(found) = found {
                reason = Some(format!("{} content={} — {}", pseudo_name, raw, found));
                break;
            }
        }
        if let Some(reason) = reason {
            emit(
                ctx,
                out,
                &GENERATED_CONTENT_ALT,
                node,
                Evidence::new(reason, "pseudo"),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// live-region-announcer
// ---------------------------------------------------------------------------

fn live_region(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    let mut regions = Vec::new();
    for node in &ctx.flat {
        if ctx.aria_bool(node, "aria_hidden") {
            continue;
        }
        let explicit = ctx.attr(node, "aria-live").is_some_and(|value| {
            value.eq_ignore_ascii_case("polite") || value.eq_ignore_ascii_case("assertive")
        });
        let status = ctx
            .aria(node, "role")
            .is_some_and(|role| role.eq_ignore_ascii_case("status"));
        if explicit || status {
            regions.push(node);
        }
    }
    if let Some(first) = regions.first()
        && regions.len() > 1
    {
        emit(
            ctx,
            out,
            &LIVE_REGION_ANNOUNCER,
            first,
            Evidence::new(
                format!(
                    "{} live regions declared (aria-live/role=status); guide recommends a single polite announcer",
                    regions.len()
                ),
                "attrs",
            ),
        );
    } else {
        tracing::trace!(
            check = LIVE_REGION_ANNOUNCER.id,
            count = regions.len(),
            "structure: live region keepWhen matched"
        );
    }
}

// ---------------------------------------------------------------------------
// autofocus-without-describedby
// ---------------------------------------------------------------------------

fn autofocus_check(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    let ids: BTreeSet<&str> = ctx
        .flat
        .iter()
        .filter_map(|node| ctx.attr(node, "id"))
        .collect();
    for node in &ctx.flat {
        if !attr_present(node, "autofocus") {
            continue;
        }
        let focusable = ctx.aria_bool(node, "focusable")
            || matches!(
                ctx.tag(node),
                "INPUT" | "SELECT" | "TEXTAREA" | "BUTTON" | "A"
            );
        if !focusable {
            continue;
        }
        let describedby = ctx
            .aria(node, "aria_describedby")
            .or_else(|| ctx.attr(node, "aria-describedby"));
        let valid = describedby.is_some_and(|value| {
            let mut ids_seen = 0_usize;
            for id in value.split_whitespace() {
                if !ids.contains(id) {
                    return false;
                }
                ids_seen = ids_seen.saturating_add(1);
            }
            ids_seen > 0
        });
        if !valid {
            let webauthn = ctx
                .attr(node, "autocomplete")
                .is_some_and(|value| value.split_whitespace().any(|token| token == "webauthn"));
            emit(
                ctx,
                out,
                &AUTOFOCUS_DESCRIBEDBY,
                node,
                Evidence::new(
                    format!(
                        "autofocus without valid aria-describedby{}",
                        if webauthn {
                            " (autocomplete~=webauthn)"
                        } else {
                            ""
                        }
                    ),
                    "attrs",
                ),
            );
        } else {
            tracing::trace!(
                check = AUTOFOCUS_DESCRIBEDBY.id,
                "structure: autofocus keepWhen matched"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// card-interactive-nested-in-link
// ---------------------------------------------------------------------------

fn interactive_descendant<'a>(
    root: &'a DiffNode,
    inside_label: bool,
    ctx: &ModernCtx<'a>,
) -> Option<&'a DiffNode> {
    for child in &root.children {
        let tag = ctx.tag(child);
        let in_label = inside_label || tag == "LABEL";
        let hidden = ctx.aria_bool(child, "aria_hidden") || !ctx.visible(child);
        let native = matches!(tag, "A" | "BUTTON" | "INPUT" | "SELECT" | "TEXTAREA")
            && !(tag == "INPUT" && ctx.attr(child, "type").is_some_and(|t| t == "hidden"));
        if !hidden && !in_label && native {
            return Some(child);
        }
        if let Some(found) = interactive_descendant(child, in_label, ctx) {
            return Some(found);
        }
    }
    None
}

fn card_links(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    for node in &ctx.flat {
        if ctx.tag(node) != "A" || ctx.aria_bool(node, "aria_hidden") || !ctx.visible(node) {
            continue;
        }
        if let Some(found) = interactive_descendant(node, false, ctx) {
            emit(
                ctx,
                out,
                &CARD_INTERACTIVE_NESTED,
                node,
                Evidence::new(
                    format!("card link wraps interactive descendant {}", ctx.tag(found)),
                    "tree",
                ),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// site-nav-using-menu-roles
// ---------------------------------------------------------------------------

fn site_nav(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    for node in &ctx.flat {
        if ctx.tag(node) != "NAV" {
            continue;
        }
        let offenders = std::iter::once(*node)
            .chain(node.children.iter())
            .filter(|candidate| {
                let role = ctx.aria(candidate, "role").unwrap_or("");
                let menu_role = role.eq_ignore_ascii_case("menubar")
                    || role.eq_ignore_ascii_case("menu")
                    || role.eq_ignore_ascii_case("menuitem");
                menu_role
                    || ctx.attr(candidate, "aria-haspopup").is_some()
                    || attr_present(candidate, "focusgroup")
            })
            .count();
        if offenders > 0 {
            emit(
                ctx,
                out,
                &SITE_NAV_MENU_ROLES,
                node,
                Evidence::new(
                    format!(
                        "site navigation uses ARIA menu roles on {offenders} node(s) instead of links in the Tab order"
                    ),
                    "attrs",
                ),
            );
        } else {
            tracing::trace!(
                check = SITE_NAV_MENU_ROLES.id,
                "structure: site nav keepWhen matched"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// scroll-region-without-name
// ---------------------------------------------------------------------------

fn contains_table(node: &DiffNode) -> bool {
    node.children.iter().any(|child| ctx_has_table(child))
}

fn ctx_has_table(node: &DiffNode) -> bool {
    node.tag.as_deref() == Some("TABLE") || contains_table(node)
}

fn scroll_region(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    for node in &ctx.flat {
        if ctx.tag(node) == "MAIN" || ctx.attr(node, "tabindex") != Some("0") {
            continue;
        }
        let scrollable = [
            ctx.style(node, "box_model", "overflow-x"),
            ctx.style(node, "box_model", "overflow-y"),
        ]
        .into_iter()
        .flatten()
        .any(|value| value.eq_ignore_ascii_case("auto") || value.eq_ignore_ascii_case("scroll"));
        if !scrollable || ctx.aria(node, "aria_labelledby").is_some() || !contains_table(node) {
            tracing::trace!(
                check = SCROLL_REGION_NAME.id,
                "structure: scroll region keepWhen matched"
            );
            continue;
        }
        let named_region = ctx
            .aria(node, "role")
            .is_some_and(|role| role.eq_ignore_ascii_case("region"))
            && ctx.aria(node, "name").is_some();
        if named_region {
            continue;
        }
        emit(
            ctx,
            out,
            &SCROLL_REGION_NAME,
            node,
            Evidence::new(
                "scrollable table wrapper is tabbable but has no role=region / accessible name",
                "styles+attrs",
            ),
        );
    }
}

// ---------------------------------------------------------------------------
// web-install-manifest-required
// ---------------------------------------------------------------------------

fn is_manifest_rel(rel: &str) -> bool {
    rel.split_whitespace()
        .any(|token| token.eq_ignore_ascii_case("manifest"))
}

fn has_manifest(ctx: &ModernCtx<'_>) -> bool {
    if let Some(document) = &ctx.doc.document
        && document.links.iter().any(|link| {
            is_manifest_rel(&link.rel)
                && link
                    .href
                    .as_deref()
                    .is_some_and(|href| !href.trim().is_empty())
        })
    {
        return true;
    }
    ctx.flat.iter().any(|node| {
        ctx.tag(node) == "LINK"
            && ctx.attr(node, "rel").is_some_and(is_manifest_rel)
            && ctx.attr(node, "href").is_some()
    })
}

fn web_install(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    let manifest = has_manifest(ctx);
    for node in &ctx.flat {
        if ctx.tag(node) != "INSTALL" {
            continue;
        }
        if manifest {
            tracing::trace!(
                check = WEB_INSTALL_MANIFEST.id,
                "structure: manifest linked, keepWhen matched"
            );
            continue;
        }
        emit(
            ctx,
            out,
            &WEB_INSTALL_MANIFEST,
            node,
            Evidence::new(
                "<install> without <link rel=manifest> — install would fail with DataError",
                "__document",
            ),
        );
    }
}

// ---------------------------------------------------------------------------
// webmcp-form-tool-metadata
// ---------------------------------------------------------------------------

fn webmcp_form(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    for node in &ctx.flat {
        if ctx.tag(node) != "FORM" {
            continue;
        }
        let Some(toolname) = ctx.attr(node, "toolname") else {
            tracing::trace!(
                check = WEBMCP_FORM_METADATA.id,
                "structure: no tool* attributes"
            );
            continue;
        };
        let described = ctx
            .attr(node, "tooldescription")
            .is_some_and(|value| !value.trim().is_empty());
        if !described {
            emit(
                ctx,
                out,
                &WEBMCP_FORM_METADATA,
                node,
                Evidence::new(
                    format!("form[toolname=\"{toolname}\"] without tooldescription"),
                    "attrs",
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Map, json};
    use sniff_core::modern::DocumentFacet;
    use sniff_css_diff::DiffDocument;

    fn mk(tag: &str, selector: &str) -> DiffNode {
        DiffNode {
            id: 0,
            parent_id: None,
            selector: selector.to_string(),
            tag: Some(tag.to_string()),
            path: None,
            depth: None,
            rect: Some(json!({"x": 0, "y": 0, "width": 0, "height": 0})),
            metrics: None,
            noticeable: None,
            hash: None,
            styles: Some(Map::new()),
            pseudo: Some(Map::new()),
            aria: Some(Value::Object(Map::new())),
            contrast: None,
            ax: Some(Value::Object(Map::new())),
            attributes: Some(Value::Object(Map::new())),
            effective_background: None,
            children: Vec::new(),
        }
    }

    fn with_id(mut node: DiffNode, id: u64) -> DiffNode {
        node.id = id;
        node
    }

    fn with_parent(mut node: DiffNode, parent_id: u64) -> DiffNode {
        node.parent_id = Some(parent_id);
        node
    }

    fn with_attrs(mut node: DiffNode, attributes: Value) -> DiffNode {
        node.attributes = Some(attributes);
        node
    }

    fn with_aria(mut node: DiffNode, aria: Value) -> DiffNode {
        node.aria = Some(aria);
        node
    }

    fn with_styles(mut node: DiffNode, styles: Value) -> DiffNode {
        node.styles = styles.as_object().cloned();
        node
    }

    fn with_pseudo(mut node: DiffNode, pseudo: Value) -> DiffNode {
        node.pseudo = pseudo.as_object().cloned();
        node
    }

    fn with_kids(mut node: DiffNode, children: Vec<DiffNode>) -> DiffNode {
        node.children = children;
        node
    }

    fn document(nodes: Vec<DiffNode>, facet: Option<DocumentFacet>) -> DiffDocument {
        DiffDocument {
            nodes,
            document: facet,
            ..DiffDocument::default()
        }
    }

    fn lines(doc: &DiffDocument) -> Vec<CheckLine> {
        let opts = crate::modern::ModernOptions::default();
        let ctx = ModernCtx::new(doc, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        out
    }

    fn ids(doc: &DiffDocument) -> Vec<String> {
        lines(doc).into_iter().map(|line| line.check).collect()
    }

    fn assert_emits(doc: &DiffDocument, check: &str) {
        assert!(ids(doc).contains(&check.to_string()), "missing {check}");
    }

    fn assert_silent(doc: &DiffDocument, check: &str) {
        assert!(!ids(doc).contains(&check.to_string()), "unexpected {check}");
    }

    fn meta(title: &str, lang: &str) -> DocumentFacet {
        let mut facet = DocumentFacet::default();
        facet.doctype = Some("html".to_string());
        facet.title = Some(title.to_string());
        facet.lang = Some(lang.to_string());
        facet.meta.insert(
            "viewport".to_string(),
            "width=device-width, initial-scale=1.0".to_string(),
        );
        facet
    }

    #[test]
    fn document_metadata_positive_negative_keepwhen() {
        let positive = document(
            vec![with_aria(
                mk("HTML", "html"),
                serde_json::json!({"lang": null, "has_text": true}),
            )],
            Some(DocumentFacet::default()),
        );
        assert_emits(&positive, "document-metadata-basics");
        assert_emits(&positive, "document-lang-missing");

        let negative = document(
            vec![
                with_aria(mk("H1", "h1"), serde_json::json!({"has_text": true})),
                with_aria(mk("MAIN", "main"), serde_json::json!({"role": "main"})),
            ],
            Some(meta("Page | Site", "pt-BR")),
        );
        assert_silent(&negative, "document-metadata-basics");
        assert_silent(&negative, "document-lang-missing");

        let no_text = document(vec![mk("HTML", "html")], Some(DocumentFacet::default()));
        assert_silent(&no_text, "document-lang-missing");
    }

    #[test]
    fn document_metadata_heading_and_landmark_signals() {
        let h1 = with_aria(mk("H1", "h1"), serde_json::json!({"has_text": true}));
        let h3 = with_aria(mk("H3", "h3"), serde_json::json!({"has_text": true}));
        let main_a = with_aria(mk("MAIN", "main#a"), serde_json::json!({"role": "main"}));
        let main_b = with_aria(mk("MAIN", "main#b"), serde_json::json!({"role": "main"}));
        let dialog_h1 = with_kids(mk("DIALOG", "dialog"), vec![mk("H1", "dialog h1")]);
        let doc = document(
            vec![h1, h3, main_a, main_b, dialog_h1],
            Some(meta("T", "en")),
        );
        let evidence = lines(&doc)
            .into_iter()
            .find(|line| line.check == "document-metadata-basics")
            .map(|line| line.evidence.text);
        let text = evidence.unwrap_or_default();
        assert!(text.contains("heading jump H1->H3"), "{text}");
        assert!(text.contains("duplicate main landmark: 2"), "{text}");
        assert!(
            !text.contains("no h1"),
            "dialog h1 must not be counted as page h1"
        );
    }

    #[test]
    fn form_semantics_positive_negative_keepwhen() {
        let inner = with_attrs(
            with_parent(mk("FORM", "form#inner"), 1),
            serde_json::json!({"id": "inner"}),
        );
        let button = with_attrs(
            with_parent(mk("BUTTON", "button#go"), 1),
            serde_json::json!({}),
        );
        let outer = with_attrs(
            with_id(mk("FORM", "form#outer"), 1),
            serde_json::json!({"id": "outer"}),
        );
        let positive = document(
            vec![with_kids(outer, vec![inner, button])],
            Some(meta("T", "en")),
        );
        assert_emits(&positive, "form-semantics-basics");

        let typed = with_attrs(
            mk("BUTTON", "button#go"),
            serde_json::json!({"type": "button"}),
        );
        let negative = document(
            vec![with_kids(
                with_attrs(mk("FORM", "form"), serde_json::json!({})),
                vec![typed],
            )],
            Some(meta("T", "en")),
        );
        assert_silent(&negative, "form-semantics-basics");

        let standalone = with_attrs(mk("BUTTON", "button#standalone"), serde_json::json!({}));
        let grouped = with_attrs(
            mk("FIELDSET", "fieldset"),
            serde_json::json!({"role": "group"}),
        )
        .into();
        let grouped = with_aria(
            grouped,
            serde_json::json!({"role": "group", "name": "Group"}),
        );
        let keep = document(vec![standalone, grouped], Some(meta("T", "en")));
        assert_silent(&keep, "form-semantics-basics");
    }

    #[test]
    fn tabindex_fail_and_keepwhen() {
        let positive = with_attrs(mk("DIV", "div#top"), serde_json::json!({"tabindex": "3"}));
        assert_emits(
            &document(vec![positive], None),
            "focusable-tabindex-positive",
        );

        let zero = with_attrs(mk("DIV", "div#zero"), serde_json::json!({"tabindex": "0"}));
        assert_silent(&document(vec![zero], None), "focusable-tabindex-positive");

        let bad = with_attrs(
            mk("DIV", "div#bad"),
            serde_json::json!({"tabindex": "many"}),
        );
        assert_silent(&document(vec![bad], None), "focusable-tabindex-positive");
    }

    #[test]
    fn breadcrumb_positive_negative_keepwhen() {
        let links = vec![
            with_attrs(mk("A", "a.one"), serde_json::json!({"href": "/"})),
            with_attrs(mk("A", "a.two"), serde_json::json!({"href": "/docs"})),
        ];
        let nav = |name: &str, marked: bool| {
            let mut links = links.clone();
            if marked {
                links[1].attributes =
                    Some(serde_json::json!({"href": "/docs", "aria-current": "page"}));
            }
            with_aria(
                with_kids(mk("NAV", "nav.bc"), links),
                serde_json::json!({"name": name}),
            )
        };
        let positive = document(vec![nav("Breadcrumbs", false)], None);
        assert_emits(&positive, "breadcrumb-current-page-missing");

        let negative = document(vec![nav("Breadcrumbs", true)], None);
        assert_silent(&negative, "breadcrumb-current-page-missing");

        let keep = document(vec![nav("Primary", false)], None);
        assert_silent(&keep, "breadcrumb-current-page-missing");
    }

    #[test]
    fn generated_content_positive_negative_keepwhen() {
        let button = |pseudo: Value| {
            with_aria(
                with_pseudo(mk("BUTTON", "button.save"), pseudo),
                serde_json::json!({"name": "Save"}),
            )
        };
        let duplicate = button(serde_json::json!({
            "::after": {"accessibility": {"content": "\"Save\" / \"Save\""}}
        }));
        assert_emits(&document(vec![duplicate], None), "generated-content-alt");

        let decorative = button(serde_json::json!({
            "::after": {"accessibility": {"content": "\" →\" / \"\""}}
        }));
        assert_silent(&document(vec![decorative], None), "generated-content-alt");

        let image_with_alt = button(serde_json::json!({
            "::before": {"visual": {"content": "url(/icon.svg) / \"Details icon\""}}
        }));
        assert_silent(
            &document(vec![image_with_alt], None),
            "generated-content-alt",
        );
    }

    #[test]
    fn generated_content_absent_content_is_silent() {
        let absent = with_aria(
            with_pseudo(
                mk("DIV", "div.wrapper"),
                serde_json::json!({"::before": {"accessibility": {"content": "none"}}}),
            ),
            serde_json::json!({"name": ""}),
        );
        assert_silent(&document(vec![absent], None), "generated-content-alt");
    }

    #[test]
    fn live_region_positive_negative_keepwhen() {
        let region = |selector: &str| {
            with_attrs(
                mk("DIV", selector),
                serde_json::json!({"aria-live": "polite"}),
            )
        };
        let positive = document(
            vec![region("div#email-error"), region("div#password-error")],
            None,
        );
        assert_emits(&positive, "live-region-announcer");

        let single = document(vec![region("div#announcer")], None);
        assert_silent(&single, "live-region-announcer");

        let alerts = vec![
            with_aria(
                mk("DIV", "div#alert-1"),
                serde_json::json!({"role": "alert"}),
            ),
            with_aria(
                mk("DIV", "div#alert-2"),
                serde_json::json!({"role": "alert"}),
            ),
        ];
        let keep = document(alerts, None);
        assert_silent(&keep, "live-region-announcer");
    }

    #[test]
    fn autofocus_positive_negative_keepwhen() {
        let positive = with_attrs(
            mk("INPUT", "input#user"),
            serde_json::json!({"autofocus": "true", "autocomplete": "username webauthn"}),
        );
        assert_emits(
            &document(vec![positive], None),
            "autofocus-without-describedby",
        );

        let no_autofocus = with_attrs(
            mk("INPUT", "input#user"),
            serde_json::json!({"autocomplete": "username webauthn"}),
        );
        assert_silent(
            &document(vec![no_autofocus], None),
            "autofocus-without-describedby",
        );

        let hint = with_attrs(mk("P", "p#hint"), serde_json::json!({"id": "hint"}));
        let described = with_attrs(
            mk("INPUT", "input#user"),
            serde_json::json!({"autofocus": "true", "aria-describedby": "hint"}),
        );
        let keep = document(vec![described, hint], None);
        assert_silent(&keep, "autofocus-without-describedby");
    }

    #[test]
    fn card_links_positive_negative_keepwhen() {
        let button = with_kids(
            with_attrs(mk("A", "a.card"), serde_json::json!({"href": "/x"})),
            vec![mk("BUTTON", "a.card button")],
        );
        assert_emits(
            &document(vec![button], None),
            "card-interactive-nested-in-link",
        );

        let plain = with_kids(
            with_attrs(mk("A", "a.card"), serde_json::json!({"href": "/x"})),
            vec![mk("ARTICLE", "article")],
        );
        assert_silent(
            &document(vec![plain], None),
            "card-interactive-nested-in-link",
        );

        let label = with_kids(
            mk("LABEL", "label.choice"),
            vec![mk("INPUT", "label.choice input")],
        );
        let choice = with_kids(
            with_attrs(mk("A", "a.choice"), serde_json::json!({"href": "/x"})),
            vec![label],
        );
        assert_silent(
            &document(vec![choice], None),
            "card-interactive-nested-in-link",
        );
    }

    #[test]
    fn site_nav_positive_negative_keepwhen() {
        let menu = with_aria(mk("DIV", "nav div"), serde_json::json!({"role": "menubar"}));
        let positive = with_kids(
            with_aria(
                mk("NAV", "nav.primary"),
                serde_json::json!({"name": "Primary"}),
            ),
            vec![menu],
        );
        assert_emits(&document(vec![positive], None), "site-nav-using-menu-roles");

        let link = with_attrs(mk("A", "nav a"), serde_json::json!({"href": "/"}));
        let negative = with_kids(mk("NAV", "nav.primary"), vec![link]);
        assert_silent(&document(vec![negative], None), "site-nav-using-menu-roles");

        let outside = with_aria(
            mk("DIV", "div.menu"),
            serde_json::json!({"role": "menubar"}),
        );
        let keep = document(vec![outside], None);
        assert_silent(&keep, "site-nav-using-menu-roles");
    }

    #[test]
    fn scroll_region_positive_negative_keepwhen() {
        let wrapper = |attrs: Value, style: Value| {
            with_styles(with_attrs(mk("DIV", "div.wrapper"), attrs), style)
        };
        let overflow = serde_json::json!({"box_model": {"overflow-x": "auto"}});
        let table_wrapper = with_kids(
            wrapper(serde_json::json!({"tabindex": "0"}), overflow.clone()),
            vec![mk("TABLE", "table")],
        );
        assert_emits(
            &document(vec![table_wrapper], None),
            "scroll-region-without-name",
        );

        let named = with_aria(
            wrapper(serde_json::json!({"tabindex": "0"}), overflow.clone()),
            serde_json::json!({"role": "region", "name": "Employees"}),
        );
        let negative = with_kids(named, vec![mk("TABLE", "table")]);
        assert_silent(
            &document(vec![negative], None),
            "scroll-region-without-name",
        );

        let main = with_styles(
            wrapper(serde_json::json!({"tabindex": "0"}), overflow),
            serde_json::json!({"box_model": {"overflow-x": "scroll"}}),
        );
        let keep = with_kids(mk("MAIN", "main"), vec![main]);
        assert_silent(&document(vec![keep], None), "scroll-region-without-name");
    }

    #[test]
    fn web_install_positive_negative_keepwhen() {
        let install = mk("INSTALL", "install");
        let positive = document(vec![install], Some(meta("T", "en")));
        assert_emits(&positive, "web-install-manifest-required");

        let mut with_link = meta("T", "en");
        with_link.links.push(sniff_core::modern::LinkInfo {
            rel: "manifest".to_string(),
            href: Some("/manifest.webmanifest".to_string()),
            as_: None,
            fetchpriority: None,
            integrity: None,
            crossorigin: None,
        });
        let negative = document(vec![mk("INSTALL", "install")], Some(with_link));
        assert_silent(&negative, "web-install-manifest-required");

        let keep = document(vec![mk("LINK", "link.manifest")], Some(meta("T", "en")));
        assert_silent(&keep, "web-install-manifest-required");
    }

    #[test]
    fn webmcp_positive_negative_keepwhen() {
        let positive = with_attrs(
            mk("FORM", "form#search"),
            serde_json::json!({"toolname": "search-cars"}),
        );
        assert_emits(&document(vec![positive], None), "webmcp-form-tool-metadata");

        let negative = with_attrs(
            mk("FORM", "form#search"),
            serde_json::json!({"toolname": "search-cars", "tooldescription": "Search cars"}),
        );
        assert_silent(&document(vec![negative], None), "webmcp-form-tool-metadata");

        let keep = with_attrs(
            mk("FORM", "form#plain"),
            serde_json::json!({"action": "/x"}),
        );
        assert_silent(&document(vec![keep], None), "webmcp-form-tool-metadata");
    }

    #[test]
    fn determinism_byte_for_byte() {
        let nodes = vec![
            with_attrs(mk("FORM", "form#a"), serde_json::json!({"toolname": "go"})),
            with_attrs(
                mk("INPUT", "input#q"),
                serde_json::json!({"autofocus": "true", "tabindex": "2"}),
            ),
            with_aria(mk("H3", "h3"), serde_json::json!({"has_text": true})),
        ];
        let doc = document(nodes, Some(meta("T", "en")));
        let first: Vec<String> = lines(&doc)
            .iter()
            .map(|line| serde_json::to_string(line).unwrap_or_default())
            .collect();
        let second: Vec<String> = lines(&doc)
            .iter()
            .map(|line| serde_json::to_string(line).unwrap_or_default())
            .collect();
        assert_eq!(first, second);
        assert!(!first.is_empty());
    }

    #[test]
    fn registry_shape() {
        let all = specs();
        assert_eq!(all.len(), 13);
        let mut seen = BTreeSet::new();
        for spec in all {
            assert!(seen.insert(spec.id), "duplicate id {}", spec.id);
            assert!(!spec.guides.is_empty());
            assert!(!spec.keep_when.is_empty());
        }
        assert_eq!(DOCUMENT_LANG.severity, sniff_core::modern::Severity::Fail);
        assert_eq!(
            FOCUSABLE_TABINDEX.severity,
            sniff_core::modern::Severity::Fail
        );
    }
}
