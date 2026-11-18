//! Família B — plataforma nativa em vez de reimplementação.
//!
//! Cada check observa apenas campos do snapshot: `aria`, `attrs`, `styles`,
//! `pseudo`, `rect` e as facetas globais `__document`/`__env`/`__declared`.
//! Nenhum check infere intenção do autor; `keep_when` vive no registro
//! ([`CheckSpec`]) e é exercitado pelos testes de negação no fim deste arquivo.
//!
//! Determinismo: a ordem de emissão segue o pré-ordem de [`ModernCtx::flat`]
//! e a ordem das chaves de `BTreeMap` em `pseudo`/`declared`. Nenhum relógio,
//! ambiente aleatório ou dado de snapshot inteiro entra nos logs: apenas o id
//! do check e o seletor do nó.

use crate::modern::{CheckSpec, Facet, ModernCtx};
use crate::rules::CheckLine;
use serde_json::Value;
use sniff_core::modern::{DeclaredRule, Evidence};
use sniff_css_diff::DiffNode;
use std::collections::BTreeMap;

/// Índice determinístico de nós por id, com o mapa de pais derivado da árvore.
struct TreeIndex<'a> {
    nodes: BTreeMap<u64, &'a DiffNode>,
    parent: BTreeMap<u64, u64>,
}

impl<'a> TreeIndex<'a> {
    fn new(flat: &[&'a DiffNode]) -> Self {
        let mut nodes = BTreeMap::new();
        for node in flat {
            nodes.insert(node.id, *node);
        }
        let mut parent = BTreeMap::new();
        for node in flat {
            for child in &node.children {
                parent.insert(child.id, node.id);
            }
            if let Some(parent_id) = node.parent_id {
                parent.entry(node.id).or_insert(parent_id);
            }
        }
        Self { nodes, parent }
    }

    /// Ancestrais do nó, do pai imediato até a raiz (com limite anti-ciclo).
    fn ancestors(&self, id: u64) -> impl Iterator<Item = &'a DiffNode> + '_ {
        let mut current = self.parent.get(&id).copied();
        let mut remaining = self.nodes.len();
        std::iter::from_fn(move || {
            if remaining == 0 {
                current = None;
                return None;
            }
            remaining -= 1;
            let next = current?;
            current = self.parent.get(&next).copied();
            self.nodes.get(&next).copied()
        })
    }
}

/// Todos os ids da família B, na ordem do catálogo (§4.4).
static SPECS: &[CheckSpec] = &[
    CheckSpec::new(
        "native-dialog-reimplemented",
        &["dialog"],
        &[Facet::Attrs],
        "keepWhen: popover=manual drawer/tour role=dialog is prescribed; native dialog without redundant aria-modal",
    ),
    CheckSpec::new(
        "native-checkbox-reimplemented",
        &["checkbox"],
        &[Facet::Attrs],
        "keepWhen: role=checkbox under menuitem/label with native input, or appearance reset without __declared origin",
    ),
    CheckSpec::new(
        "native-progress-reimplemented",
        &["progress", "progress-ring", "spinner"],
        &[Facet::Attrs],
        "keepWhen: role=progressbar inside menuitem, native PROGRESS in subtree, or aria-hidden decorative ring",
    ),
    CheckSpec::new(
        "native-range-control",
        &["slider", "switch"],
        &[Facet::Attrs],
        "keepWhen: input[type=range] with an accessible name, or a labeled native range in the subtree",
    ),
    CheckSpec::new(
        "switch-semantics",
        &["switch"],
        &[Facet::Attrs],
        "keepWhen: plain checkbox without switch, native AX role=switch, or AX facet absent for the polyfill signal",
    ),
    CheckSpec::new(
        "accordion-custom-instead-of-details",
        &["accordion"],
        &[],
        "keepWhen: single disclosure, no aria-controls panel, or details/summary present in the group",
    ),
    CheckSpec::new(
        "menu-composite-reimplemented",
        &["menu", "app-menus"],
        &[Facet::Attrs],
        "keepWhen: focusgroup present, role=menu inside NAV (site nav), or a single menuitem",
    ),
    CheckSpec::new(
        "tabs-reimplemented-with-roving-tabindex",
        &["tabs"],
        &[Facet::Attrs],
        "keepWhen: focusgroup on the tablist or a single tabpanel without manual tabindex",
    ),
    CheckSpec::new(
        "toolbar-roving-tabindex-broken",
        &["toolbar"],
        &[Facet::Attrs],
        "keepWhen: exactly one tabindex=0 control, or a toolbar with a single control",
    ),
    CheckSpec::new(
        "drawer-reimplemented-with-fixed-overlay",
        &["navigation-drawer", "persistent-app-tours"],
        &[Facet::Rect, Facet::Styles],
        "keepWhen: overlay below 95% of the viewport, no navigation links, or native dialog/popover in subtree",
    ),
    CheckSpec::new(
        "os-text-scale-optin",
        &["respect-os-text-scale"],
        &[Facet::Document],
        "keepWhen: no meta[name=text-scale]=scale, or font sizes keep a rem/em component",
    ),
    CheckSpec::new(
        "root-color-scheme-hardcoded",
        &["dark-mode-toggle"],
        &[Facet::Declared],
        "keepWhen: base rule is light dark or inside :has(meta); without __declared the computed value is ambiguous",
    ),
    CheckSpec::new(
        "color-scheme-without-background",
        &["component-specific-light-dark-theme"],
        &[],
        "keepWhen: HTML root, opaque background, or background-image present",
    ),
    CheckSpec::new(
        "masked-icon-missing-forced-color-adjust",
        &["icons"],
        &[Facet::Pseudo, Facet::Env],
        "keepWhen: pseudo already sets forced-color-adjust:none or has another forced-colors compensation",
    ),
    CheckSpec::new(
        "coarse-pointer-target-size",
        &["form-factor-adaptation"],
        &[Facet::Env, Facet::Rect],
        "keepWhen: fine pointer, link inside a sentence, checkbox/radio measured via label, or named scroll region",
    ),
];

#[must_use]
/// Registro dos checks da família B (fonte única de ids).
pub fn specs() -> &'static [CheckSpec] {
    SPECS
}

/// Executa todas as checks da família B habilitadas pelas opções e facetas.
pub fn run(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    let before = out.len();
    let index = TreeIndex::new(&ctx.flat);
    tracing::debug!(
        family = "platform",
        nodes = ctx.flat.len(),
        "modern platform family started"
    );
    for spec in specs() {
        if !ctx.enabled(spec) {
            tracing::debug!(
                check = spec.id,
                "modern platform check suppressed (facet missing or filtered)"
            );
            continue;
        }
        match spec.id {
            "native-dialog-reimplemented" => check_dialog(ctx, out, spec),
            "native-checkbox-reimplemented" => check_checkbox(ctx, out, spec, &index),
            "native-progress-reimplemented" => check_progress(ctx, out, spec, &index),
            "native-range-control" => check_range(ctx, out, spec),
            "switch-semantics" => check_switch(ctx, out, spec),
            "accordion-custom-instead-of-details" => check_accordion(ctx, out, spec),
            "menu-composite-reimplemented" => check_menu(ctx, out, spec, &index),
            "tabs-reimplemented-with-roving-tabindex" => check_tabs(ctx, out, spec),
            "toolbar-roving-tabindex-broken" => check_toolbar(ctx, out, spec),
            "drawer-reimplemented-with-fixed-overlay" => check_drawer(ctx, out, spec),
            "os-text-scale-optin" => check_text_scale(ctx, out, spec),
            "root-color-scheme-hardcoded" => check_root_color_scheme(ctx, out, spec),
            "color-scheme-without-background" => check_element_color_scheme(ctx, out, spec),
            "masked-icon-missing-forced-color-adjust" => check_masked_icon(ctx, out, spec),
            "coarse-pointer-target-size" => check_coarse_target(ctx, out, spec, &index),
            other => tracing::warn!(check = other, "no handler for registered platform id"),
        }
    }
    tracing::debug!(
        family = "platform",
        emitted = out.len() - before,
        "modern platform family finished"
    );
}

fn record(_ctx: &ModernCtx<'_>, spec: &CheckSpec) {
    tracing::debug!(check = spec.id, "modern platform check emitted");
}

fn tag_is(node: &DiffNode, tags: &[&str]) -> bool {
    let tag = node.tag.as_deref().unwrap_or("");
    tags.iter()
        .any(|candidate| tag.eq_ignore_ascii_case(candidate))
}

fn role_is(ctx: &ModernCtx<'_>, node: &DiffNode, roles: &[&str]) -> bool {
    ctx.aria(node, "role").is_some_and(|role| {
        roles
            .iter()
            .any(|candidate| role.eq_ignore_ascii_case(candidate))
    })
}

fn type_is(ctx: &ModernCtx<'_>, node: &DiffNode, types: &[&str]) -> bool {
    ctx.attr(node, "type").is_some_and(|value| {
        types
            .iter()
            .any(|candidate| value.eq_ignore_ascii_case(candidate))
    })
}

fn has_attr(node: &DiffNode, name: &str) -> bool {
    node.attributes
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|attrs| attrs.contains_key(name))
}

fn aria_value_present(ctx: &ModernCtx<'_>, node: &DiffNode, key: &str) -> bool {
    ctx.aria(node, key).is_some() || has_attr(node, key.replace('_', "-").as_str())
}

fn is_aria_hidden(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    ctx.aria(node, "aria_hidden")
        .is_some_and(|v| v.eq_ignore_ascii_case("true"))
        || ctx
            .attr(node, "aria-hidden")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

fn subtree_any(node: &DiffNode, predicate: &dyn Fn(&DiffNode) -> bool) -> bool {
    predicate(node)
        || node
            .children
            .iter()
            .any(|child| subtree_any(child, predicate))
}

fn subtree_count(node: &DiffNode, predicate: &dyn Fn(&DiffNode) -> bool) -> usize {
    usize::from(predicate(node))
        + node
            .children
            .iter()
            .map(|child| subtree_count(child, predicate))
            .sum::<usize>()
}

fn pseudo_style<'a>(
    node: &'a DiffNode,
    pseudo: &str,
    category: &str,
    property: &str,
) -> Option<&'a str> {
    let styles = node.pseudo.as_ref()?.get(pseudo)?;
    styles
        .get(category)
        .and_then(Value::as_object)
        .and_then(|group| group.get(property))
        .and_then(Value::as_str)
        .or_else(|| styles.get(property).and_then(Value::as_str))
}

fn env_media<'a>(ctx: &'a ModernCtx<'_>, key: &str) -> Option<&'a str> {
    let env = ctx.doc.env.as_ref()?;
    env.emulated
        .get(key)
        .map(String::as_str)
        .or_else(|| env.media.get(key).map(String::as_str))
}

fn rect_parts(node: &DiffNode) -> Option<(f64, f64, f64, f64)> {
    let rect = node.rect.as_ref()?;
    Some((
        rect.get("x")?.as_f64()?,
        rect.get("y")?.as_f64()?,
        rect.get("width")?.as_f64()?,
        rect.get("height")?.as_f64()?,
    ))
}

fn viewport_size(ctx: &ModernCtx<'_>) -> Option<(f64, f64)> {
    if let Some((width, height)) = ctx.doc.viewport {
        return Some((width, height));
    }
    ctx.doc
        .env
        .as_ref()
        .and_then(|env| env.device)
        .map(|device| (f64::from(device.width), f64::from(device.height)))
}

fn css_is_none(value: &str) -> bool {
    let value = value.split('!').next().unwrap_or(value).trim();
    matches!(value, "none" | "0" | "0px")
}

fn transparent_color(value: &str) -> bool {
    let value = value
        .split('!')
        .next()
        .unwrap_or(value)
        .trim()
        .to_ascii_lowercase();
    matches!(
        value.as_str(),
        "transparent" | "none" | "rgba(0, 0, 0, 0)" | "rgba(0,0,0,0)" | "rgb(0 0 0 / 0)"
    )
}

fn pinned_scheme(value: &str) -> Option<&'static str> {
    let value = value
        .split('!')
        .next()
        .unwrap_or(value)
        .trim()
        .to_ascii_lowercase();
    match value.as_str() {
        "dark" | "only dark" => Some("dark"),
        "light" | "only light" => Some("light"),
        _ => None,
    }
}

fn declared_reset_present(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    let Some(declared) = ctx.doc.declared.as_ref() else {
        return false;
    };
    let classes = ctx.attr(node, "class").unwrap_or("");
    let relevant = |rule: &DeclaredRule| {
        let selector = rule.selector.to_ascii_lowercase();
        selector.contains("input")
            || selector.contains("checkbox")
            || selector.contains("radio")
            || classes
                .split_ascii_whitespace()
                .any(|class| !class.is_empty() && selector.contains(&class.to_ascii_lowercase()))
    };
    let declares = |property: &str| {
        declared.rules.iter().any(|rule| {
            rule.origin != "ua"
                && relevant(rule)
                && rule
                    .declarations
                    .get(property)
                    .is_some_and(|value| css_is_none(value))
        })
    };
    declares("appearance") && (declares("outline") || declares("outline-style"))
}

fn check_dialog(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        let tag = ctx.tag(node);
        let role = ctx.aria(node, "role").unwrap_or("");
        if tag_is(node, &["DIV", "SPAN"])
            && (role.eq_ignore_ascii_case("dialog") || role.eq_ignore_ascii_case("alertdialog"))
            && !subtree_any(node, &|child| tag_is(child, &["DIALOG"]))
        {
            // O guia navigation-drawer prescreve popover=manual + role=dialog.
            if ctx
                .attr(node, "popover")
                .is_some_and(|v| v.eq_ignore_ascii_case("manual"))
            {
                tracing::debug!(check = spec.id, "keepWhen: prescribed dialog-role popover");
                continue;
            }
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!("{tag} role={role} reimplements native <dialog>"),
                    "aria+tag",
                ),
            );
            record(ctx, spec);
        }
        if tag_is(node, &["DIALOG"])
            && ctx
                .attr(node, "aria-modal")
                .is_some_and(|v| v.eq_ignore_ascii_case("true"))
        {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new("native dialog has redundant aria-modal=\"true\"", "attrs"),
            );
            record(ctx, spec);
        }
    }
}

fn check_checkbox(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
    index: &TreeIndex<'_>,
) {
    for node in &ctx.flat {
        let tag = ctx.tag(node);
        let role = ctx.aria(node, "role").unwrap_or("");
        let in_menu = index.ancestors(node.id).any(|ancestor| {
            role_is(
                ctx,
                ancestor,
                &["menuitem", "menuitemcheckbox", "menuitemradio", "menu"],
            )
        });
        let native_in_label = index.ancestors(node.id).any(|ancestor| {
            tag_is(ancestor, &["LABEL"])
                && subtree_any(ancestor, &|child| tag_is(child, &["INPUT"]))
        });
        if tag_is(node, &["DIV", "SPAN"])
            && (role.eq_ignore_ascii_case("checkbox")
                || role.eq_ignore_ascii_case("radio")
                || role.eq_ignore_ascii_case("switch"))
            && aria_value_present(ctx, node, "aria_checked")
            && !subtree_any(node, &|child| tag_is(child, &["INPUT", "PROGRESS"]))
            && !in_menu
            && !native_in_label
        {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!("{tag} role={role} reimplements native input"),
                    "aria+tag",
                ),
            );
            record(ctx, spec);
        }

        // A origem do reset de appearance só é certa com __declared.
        if tag_is(node, &["INPUT"])
            && type_is(ctx, node, &["checkbox", "radio"])
            && ctx
                .style(node, "interaction", "appearance")
                .is_some_and(|v| v.eq_ignore_ascii_case("none"))
            && ctx
                .style(node, "visual", "outline-style")
                .is_some_and(css_is_none)
            && ctx
                .style(node, "visual", "box-shadow")
                .is_none_or(css_is_none)
        {
            if ctx.has_facet(Facet::Declared) && declared_reset_present(ctx, node) {
                ctx.emit(
                    out,
                    spec,
                    node,
                    Evidence::new(
                        "appearance:none checkbox suppresses focus outline",
                        "__declared",
                    ),
                );
                record(ctx, spec);
            } else {
                tracing::debug!(
                    check = spec.id,
                    "appearance reset silenced: __declared origin unavailable"
                );
            }
        }
    }
}

fn check_progress(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
    index: &TreeIndex<'_>,
) {
    for node in &ctx.flat {
        if !tag_is(node, &["DIV", "SPAN", "SVG"])
            || !role_is(ctx, node, &["progressbar"])
            || is_aria_hidden(ctx, node)
        {
            continue;
        }
        let valued = ["aria-valuenow", "aria-valuemin", "aria-valuemax"]
            .iter()
            .any(|attr| has_attr(node, attr));
        let spinner = ctx
            .style(node, "animation", "animation-iteration-count")
            .is_some_and(|v| v.eq_ignore_ascii_case("infinite"));
        if !valued && !spinner {
            continue;
        }
        if subtree_any(node, &|child| tag_is(child, &["PROGRESS"])) {
            continue;
        }
        if index.ancestors(node.id).any(|ancestor| {
            role_is(
                ctx,
                ancestor,
                &["menuitem", "menuitemcheckbox", "menuitemradio", "menu"],
            )
        }) {
            continue;
        }
        let tag = ctx.tag(node);
        let detail = if spinner && !valued {
            "infinite spinner"
        } else {
            "ARIA value attributes"
        };
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!("{tag} role=progressbar reimplements native <progress> ({detail})"),
                "aria+attrs",
            ),
        );
        record(ctx, spec);
    }
}

fn check_range(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        let tag = ctx.tag(node);
        if tag_is(node, &["DIV", "SPAN"])
            && role_is(ctx, node, &["slider"])
            && (has_attr(node, "tabindex") || has_attr(node, "aria-valuenow"))
            && !subtree_any(node, &|child| {
                tag_is(child, &["INPUT"]) && type_is(ctx, child, &["range"])
            })
            && !is_aria_hidden(ctx, node)
        {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!("{tag} role=slider reimplements labeled input[type=range]"),
                    "aria+attrs",
                ),
            );
            record(ctx, spec);
        }
        if tag_is(node, &["INPUT"])
            && type_is(ctx, node, &["range"])
            && ctx.aria(node, "name").is_none()
            && !has_attr(node, "aria-label")
            && !aria_value_present(ctx, node, "aria_labelledby")
        {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new("input[type=range] has no accessible name", "aria+attrs"),
            );
            record(ctx, spec);
        }
    }
}

fn check_switch(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        if !tag_is(node, &["INPUT"])
            || !type_is(ctx, node, &["checkbox"])
            || !has_attr(node, "switch")
        {
            continue;
        }
        let native_role = ctx.has_facet(Facet::Ax)
            && node
                .ax
                .as_ref()
                .and_then(|ax| ax.get("role"))
                .and_then(Value::as_str)
                .is_some_and(|role| role.eq_ignore_ascii_case("switch"));
        let polyfill_role = role_is(ctx, node, &["switch"])
            || ctx
                .attr(node, "role")
                .is_some_and(|role| role.eq_ignore_ascii_case("switch"));
        let named = ctx.aria(node, "name").is_some()
            || has_attr(node, "aria-label")
            || aria_value_present(ctx, node, "aria_labelledby");
        if !native_role && !polyfill_role && ctx.has_facet(Facet::Ax) {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    "checkbox[switch] lacks AX role=switch (polyfill missing)",
                    "attrs+ax",
                ),
            );
            record(ctx, spec);
        } else if !native_role && !named {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new("checkbox[switch] has no accessible name", "aria+attrs"),
            );
            record(ctx, spec);
        }
    }
}

fn check_accordion(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for container in &ctx.flat {
        let triggers = container
            .children
            .iter()
            .filter(|child| aria_value_present(ctx, child, "aria_expanded"))
            .count();
        if triggers < 2 {
            continue;
        }
        let panels = container
            .children
            .iter()
            .filter(|child| aria_value_present(ctx, child, "aria_controls"))
            .count();
        if panels == 0 || subtree_any(container, &|child| tag_is(child, &["DETAILS", "SUMMARY"])) {
            tracing::debug!(
                check = spec.id,
                "keepWhen: native details or no controlled panel"
            );
            continue;
        }
        ctx.emit(
            out,
            spec,
            container,
            Evidence::numeric(
                format!(
                    "custom accordion: {triggers} aria-expanded triggers, 0 <details> in group"
                ),
                "aria+tree",
                "accordion_triggers",
                triggers,
                2,
                ">=",
            ),
        );
        record(ctx, spec);
    }
    for node in &ctx.flat {
        if tag_is(node, &["SUMMARY"])
            && subtree_any(node, &|child| {
                tag_is(child, &["H1", "H2", "H3", "H4", "H5", "H6"])
            })
        {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    "heading inside <summary> is dropped by JAWS/TalkBack",
                    "tag+tree",
                ),
            );
            record(ctx, spec);
        }
    }
}

fn check_menu(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
    index: &TreeIndex<'_>,
) {
    for node in &ctx.flat {
        if !role_is(ctx, node, &["menu"]) {
            continue;
        }
        // Navegação de site não deve usar papéis de menu (guia navbar).
        if index
            .ancestors(node.id)
            .any(|ancestor| tag_is(ancestor, &["NAV"]))
        {
            tracing::debug!(check = spec.id, "keepWhen: role=menu inside NAV");
            continue;
        }
        let manual = node
            .children
            .iter()
            .filter(|child| role_is(ctx, child, &["menuitem"]) && has_attr(child, "tabindex"))
            .count();
        let reason = if ctx.attr(node, "focusgroup").is_none() && manual >= 2 {
            Some("role=menu lacks focusgroup and uses manual roving tabindex")
        } else if ctx.attr(node, "focusgroup").is_none() && tag_is(node, &["MENU"]) {
            Some("HTML <menu> element used instead of focusgroup menu container")
        } else if ctx.attr(node, "popovertarget").is_some() {
            Some("menu trigger uses popovertarget instead of command")
        } else if node
            .children
            .iter()
            .any(|child| role_is(ctx, child, &["separator"]) && tag_is(child, &["DIV"]))
        {
            Some("menu separator uses DIV instead of HR")
        } else {
            None
        };
        if let Some(reason) = reason {
            ctx.emit(out, spec, node, Evidence::new(reason, "aria+attrs+tag"));
            record(ctx, spec);
        }
    }
}

fn check_tabs(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        if !role_is(ctx, node, &["tablist"]) || ctx.attr(node, "focusgroup").is_some() {
            continue;
        }
        let manual = node
            .children
            .iter()
            .filter(|child| role_is(ctx, child, &["tab"]) && has_attr(child, "tabindex"))
            .count();
        let aria_controls = node.children.iter().any(|child| {
            role_is(ctx, child, &["tab"]) && aria_value_present(ctx, child, "aria_controls")
        });
        let visible_panels = subtree_count(node, &|child| {
            role_is(ctx, child, &["tabpanel"]) && !has_attr(child, "hidden")
        });
        if manual < 2 && !aria_controls && visible_panels < 2 {
            continue;
        }
        let reason = if manual >= 2 {
            format!("role=tablist uses manual roving tabindex on {manual} tabs")
        } else if aria_controls {
            "tabs add aria-controls (guide prefers focusgroup tablist)".to_string()
        } else {
            format!("{visible_panels} role=tabpanel without hidden")
        };
        ctx.emit(out, spec, node, Evidence::new(reason, "aria+attrs"));
        record(ctx, spec);
    }
}

fn check_toolbar(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        if !role_is(ctx, node, &["toolbar"]) {
            continue;
        }
        let zeroes = node
            .children
            .iter()
            .filter(|child| ctx.attr(child, "tabindex") == Some("0"))
            .count();
        let manual = node
            .children
            .iter()
            .filter(|child| has_attr(child, "tabindex"))
            .count();
        let reason = if ctx.attr(node, "focusgroup").is_some() {
            if zeroes > 1 {
                Some(format!(
                    "role=toolbar has {zeroes} tabindex=0 controls (expected 1)"
                ))
            } else {
                None
            }
        } else if zeroes > 1 {
            Some(format!(
                "role=toolbar has {zeroes} tabindex=0 controls (expected 1)"
            ))
        } else if manual >= 2 {
            Some("role=toolbar lacks focusgroup and uses manual roving tabindex".to_string())
        } else {
            None
        };
        if let Some(reason) = reason {
            ctx.emit(out, spec, node, Evidence::new(reason, "aria+attrs"));
            record(ctx, spec);
        }
    }
}

fn check_drawer(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some((viewport_width, viewport_height)) = viewport_size(ctx) else {
        tracing::debug!(
            check = spec.id,
            "viewport unavailable; fixed-overlay check silenced"
        );
        return;
    };
    if viewport_width <= 0.0 || viewport_height <= 0.0 {
        return;
    }
    for node in &ctx.flat {
        if ctx.style(node, "layout", "position") != Some("fixed") {
            continue;
        }
        let z_index = ctx
            .style(node, "layout", "z-index")
            .and_then(|value| value.trim().parse::<f64>().ok())
            .or_else(|| {
                node.metrics
                    .as_ref()
                    .and_then(|metrics| metrics.get("z_index").or_else(|| metrics.get("z-index")))
                    .and_then(Value::as_f64)
            })
            .unwrap_or(0.0);
        if z_index < 10.0 {
            continue;
        }
        let Some((_, _, width, height)) = rect_parts(node) else {
            continue;
        };
        let coverage = (width / viewport_width).min(height / viewport_height);
        if coverage < 0.95
            || subtree_any(node, &|child| {
                tag_is(child, &["DIALOG"]) || has_attr(child, "popover")
            })
        {
            continue;
        }
        let has_nav = subtree_any(node, &|child| {
            (tag_is(child, &["A"]) && has_attr(child, "href"))
                || role_is(ctx, child, &["navigation"])
        });
        if !has_nav {
            tracing::debug!(
                check = spec.id,
                "keepWhen: full-viewport overlay without nav links"
            );
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::numeric(
                "full-viewport fixed overlay reimplements dialog/popover top layer",
                "rect+styles+__meta",
                "viewport_coverage",
                coverage,
                0.95,
                ">=",
            ),
        );
        record(ctx, spec);
    }
}

fn non_scaling_font_size(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    (value.contains("vw") || value.contains("vh"))
        && !value.contains("rem")
        && !value.contains("em")
}

fn check_text_scale(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(scale) = ctx
        .doc
        .document
        .as_ref()
        .and_then(|document| document.meta.get("text-scale"))
    else {
        tracing::debug!(
            check = spec.id,
            "__document unavailable; text-scale check silenced"
        );
        return;
    };
    if !scale.eq_ignore_ascii_case("scale") {
        return;
    }
    let mut total = 0_usize;
    let mut shown = Vec::new();
    for node in &ctx.flat {
        let tag = ctx.tag(node);
        let font_size = ctx.style(node, "typography", "font-size").unwrap_or("");
        let mut detail = None;
        if tag_is(node, &["HTML"]) && !font_size.is_empty() {
            detail = Some(format!("{tag} font-size={font_size}"));
        } else if non_scaling_font_size(font_size) {
            detail = Some(format!("{} font-size={font_size}", node.selector));
        } else {
            let nowrap = ctx
                .style_any(
                    node,
                    &[("typography", "white-space"), ("layout", "white-space")],
                )
                .is_some_and(|value| value.eq_ignore_ascii_case("nowrap"));
            let fixed_height = ctx
                .style_any(node, &[("box_model", "height"), ("layout", "height")])
                .is_some_and(|value| !value.eq_ignore_ascii_case("auto"));
            if ctx.aria_bool(node, "has_text") && (nowrap || fixed_height) {
                detail = Some(format!("{} text in fixed-height/nowrap box", node.selector));
            }
        }
        if let Some(detail) = detail {
            total += 1;
            if shown.len() < 8 {
                shown.push(detail);
            }
        }
    }
    if total == 0 {
        return;
    }
    ctx.emit_global(
        out,
        spec,
        "meta[name=text-scale]",
        Evidence::new(
            format!(
                "meta text-scale=scale but {total} non-scaling rules: {}",
                shown.join("; ")
            ),
            "__document+styles",
        ),
    );
    record(ctx, spec);
}

fn check_root_color_scheme(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(declared) = ctx.doc.declared.as_ref() else {
        tracing::debug!(
            check = spec.id,
            "__declared unavailable; root color-scheme check silenced"
        );
        return;
    };
    let Some(root) = ctx.flat.iter().find(|node| tag_is(node, &["HTML"])) else {
        return;
    };
    let Some(scheme) = ctx
        .style(root, "accessibility", "color-scheme")
        .and_then(pinned_scheme)
    else {
        return;
    };
    let base_is_pinned = declared.rules.iter().any(|rule| {
        let selector = rule.selector.to_ascii_lowercase();
        (selector.contains(":root") || selector.contains("html"))
            && !selector.contains(":has(")
            && rule
                .declarations
                .get("color-scheme")
                .and_then(|value| pinned_scheme(value))
                == Some(scheme)
    });
    if !base_is_pinned {
        tracing::debug!(
            check = spec.id,
            "keepWhen: base rule is light dark or meta-conditional"
        );
        return;
    }
    ctx.emit(
        out,
        spec,
        root,
        Evidence::new(
            format!("root color-scheme base is pinned to '{scheme}'; system preference cannot win"),
            "__declared+styles",
        ),
    );
    record(ctx, spec);
}

fn check_element_color_scheme(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        if tag_is(node, &["HTML"]) {
            continue;
        }
        let Some(scheme) = ctx
            .style(node, "accessibility", "color-scheme")
            .and_then(pinned_scheme)
        else {
            continue;
        };
        let Some(background) = ctx.style(node, "visual", "background-color") else {
            continue;
        };
        if !transparent_color(background) {
            continue;
        }
        if ctx
            .style(node, "visual", "background-image")
            .is_some_and(|value| !value.eq_ignore_ascii_case("none"))
        {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!("color-scheme: {scheme} on element with transparent background"),
                "styles.accessibility+styles.visual",
            ),
        );
        record(ctx, spec);
    }
}

fn compensates_in_forced_colors(ctx: &ModernCtx<'_>, node: &DiffNode, pseudo: &str) -> bool {
    let filter = ctx.style(node, "visual", "filter").unwrap_or("");
    if !filter.is_empty() && !filter.eq_ignore_ascii_case("none") {
        return true;
    }
    pseudo_style(node, pseudo, "visual", "background-color")
        .is_some_and(|value| !transparent_color(value))
}

fn check_masked_icon(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    for node in &ctx.flat {
        let Some(pseudo) = node.pseudo.as_ref() else {
            continue;
        };
        for name in pseudo.keys() {
            let mask = pseudo_style(node, name, "visual", "mask-image").unwrap_or("");
            if mask.is_empty() || mask.eq_ignore_ascii_case("none") {
                continue;
            }
            let adjust = pseudo_style(node, name, "accessibility", "forced-color-adjust")
                .or_else(|| ctx.style(node, "accessibility", "forced-color-adjust"));
            if adjust.is_some_and(|value| value.eq_ignore_ascii_case("none")) {
                continue;
            }
            let forced = env_media(ctx, "forced-colors").unwrap_or("none");
            if forced.eq_ignore_ascii_case("active")
                && compensates_in_forced_colors(ctx, node, name)
            {
                continue;
            }
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!("{name} mask-image without forced-color-adjust: none"),
                    "pseudo+__env",
                ),
            );
            record(ctx, spec);
            break;
        }
    }
}

fn inline_link_in_sentence(ctx: &ModernCtx<'_>, index: &TreeIndex<'_>, node: &DiffNode) -> bool {
    ctx.style(node, "layout", "display")
        .is_some_and(|v| v.eq_ignore_ascii_case("inline"))
        && index
            .ancestors(node.id)
            .any(|ancestor| ctx.aria_bool(ancestor, "has_text"))
}

fn interactive_target(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    let tag = ctx.tag(node);
    tag_is(node, &["BUTTON", "SELECT", "TEXTAREA"])
        || (tag_is(node, &["A"]) && has_attr(node, "href"))
        || (tag_is(node, &["INPUT"]) && !type_is(ctx, node, &["hidden"]))
        || role_is(
            ctx,
            node,
            &[
                "button", "link", "checkbox", "radio", "switch", "slider", "menuitem", "tab",
            ],
        )
        || tag.eq("__never__")
}

fn check_coarse_target(
    ctx: &ModernCtx<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
    index: &TreeIndex<'_>,
) {
    let Some(pointer) = env_media(ctx, "pointer") else {
        tracing::debug!(
            check = spec.id,
            "__env unavailable; coarse-pointer check silenced"
        );
        return;
    };
    if !pointer.eq_ignore_ascii_case("coarse") {
        return;
    }
    for node in &ctx.flat {
        if !ctx.visible(node) || !interactive_target(ctx, node) || is_aria_hidden(ctx, node) {
            continue;
        }
        if tag_is(node, &["INPUT"]) && type_is(ctx, node, &["checkbox", "radio", "hidden"]) {
            continue;
        }
        if tag_is(node, &["A"]) && inline_link_in_sentence(ctx, index, node) {
            continue;
        }
        if ctx.attr(node, "tabindex") == Some("0")
            && role_is(ctx, node, &["region"])
            && ctx.aria(node, "name").is_some()
        {
            continue;
        }
        let Some((_, _, width, height)) = rect_parts(node) else {
            continue;
        };
        if width <= 0.0 || height <= 0.0 {
            continue;
        }
        let side = width.min(height);
        if side >= 44.0 {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::numeric(
                format!(
                    "interactive {} is {width:0.0}x{height:0.0}px under pointer=coarse (target 44px)",
                    ctx.tag(node)
                ),
                "__env+rect",
                "target_min_side_px",
                side,
                44.0_f64,
                "<",
            ),
        );
        record(ctx, spec);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modern::ModernOptions;
    use serde_json::{Map, json};
    use sniff_core::modern::{DeclaredFacet, DocumentFacet, EnvFacet};
    use sniff_css_diff::DiffDocument;

    fn base(id: u64, tag: &str) -> DiffNode {
        DiffNode {
            id,
            parent_id: None,
            selector: format!("{tag}#{id}"),
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

    fn with_attrs(mut node: DiffNode, pairs: &[(&str, &str)]) -> DiffNode {
        let mut attrs = Map::new();
        for (key, value) in pairs {
            attrs.insert((*key).to_string(), Value::String((*value).to_string()));
        }
        node.attributes = Some(Value::Object(attrs));
        node
    }

    fn with_aria(mut node: DiffNode, pairs: &[(&str, Value)]) -> DiffNode {
        let mut aria = Map::new();
        for (key, value) in pairs {
            aria.insert((*key).to_string(), value.clone());
        }
        node.aria = Some(Value::Object(aria));
        node
    }

    fn with_ax(mut node: DiffNode, pairs: &[(&str, Value)]) -> DiffNode {
        let mut ax = Map::new();
        for (key, value) in pairs {
            ax.insert((*key).to_string(), value.clone());
        }
        node.ax = Some(Value::Object(ax));
        node
    }

    fn with_styles(mut node: DiffNode, groups: &[(&str, &[(&str, &str)])]) -> DiffNode {
        let mut styles = Map::new();
        for (category, props) in groups {
            let mut group = Map::new();
            for (name, value) in *props {
                group.insert((*name).to_string(), Value::String((*value).to_string()));
            }
            styles.insert((*category).to_string(), Value::Object(group));
        }
        node.styles = Some(styles);
        node
    }

    fn with_pseudo(mut node: DiffNode, name: &str, groups: &[(&str, &[(&str, &str)])]) -> DiffNode {
        let mut pseudo = node.pseudo.unwrap_or_default();
        let mut styles = Map::new();
        for (category, props) in groups {
            let mut group = Map::new();
            for (key, value) in *props {
                group.insert((*key).to_string(), Value::String((*value).to_string()));
            }
            styles.insert((*category).to_string(), Value::Object(group));
        }
        pseudo.insert(name.to_string(), Value::Object(styles));
        node.pseudo = Some(pseudo);
        node
    }

    fn with_rect(mut node: DiffNode, x: f64, y: f64, width: f64, height: f64) -> DiffNode {
        node.rect = Some(json!({"x": x, "y": y, "width": width, "height": height}));
        node
    }

    fn with_children(mut node: DiffNode, children: Vec<DiffNode>) -> DiffNode {
        node.children = children
            .into_iter()
            .map(|mut child| {
                child.parent_id = Some(node.id);
                child
            })
            .collect();
        node
    }

    fn doc(nodes: Vec<DiffNode>) -> DiffDocument {
        DiffDocument {
            nodes,
            ..DiffDocument::default()
        }
    }

    fn run_ids(document: &DiffDocument) -> Vec<CheckLine> {
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(document, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        out
    }

    fn count(out: &[CheckLine], id: &str) -> usize {
        out.iter().filter(|line| line.check == id).count()
    }

    #[test]
    fn dialog_positive_negative_keep() {
        let positive = doc(vec![with_aria(
            base(1, "DIV"),
            &[("role", json!("dialog")), ("name", json!("Delete?"))],
        )]);
        let out = run_ids(&positive);
        assert_eq!(count(&out, "native-dialog-reimplemented"), 1);
        assert!(
            out[0]
                .evidence
                .text
                .contains("reimplements native <dialog>")
        );

        let native = doc(vec![with_attrs(
            base(2, "DIALOG"),
            &[("aria-modal", "true")],
        )]);
        let out = run_ids(&native);
        assert_eq!(count(&out, "native-dialog-reimplemented"), 1);
        assert!(out[0].evidence.text.contains("redundant aria-modal"));

        let negative = doc(vec![with_aria(
            base(3, "DIALOG"),
            &[("role", json!("dialog"))],
        )]);
        assert_eq!(count(&run_ids(&negative), "native-dialog-reimplemented"), 0);

        let keep = doc(vec![with_attrs(
            with_aria(base(4, "DIV"), &[("role", json!("dialog"))]),
            &[("popover", "manual")],
        )]);
        assert_eq!(count(&run_ids(&keep), "native-dialog-reimplemented"), 0);
    }

    #[test]
    fn checkbox_positive_negative_keep() {
        let positive = doc(vec![with_aria(
            base(1, "DIV"),
            &[
                ("role", json!("checkbox")),
                ("aria_checked", json!("false")),
            ],
        )]);
        assert_eq!(
            count(&run_ids(&positive), "native-checkbox-reimplemented"),
            1
        );

        let negative = doc(vec![with_aria(
            base(2, "DIV"),
            &[("role", json!("checkbox"))],
        )]);
        assert_eq!(
            count(&run_ids(&negative), "native-checkbox-reimplemented"),
            0
        );

        let keep = doc(vec![with_children(
            base(10, "LABEL"),
            vec![
                with_attrs(base(11, "INPUT"), &[("type", "checkbox")]),
                with_aria(
                    base(12, "DIV"),
                    &[("role", json!("checkbox")), ("aria_checked", json!("true"))],
                ),
            ],
        )]);
        assert_eq!(count(&run_ids(&keep), "native-checkbox-reimplemented"), 0);

        let mut with_declared = doc(vec![with_styles(
            with_attrs(
                base(3, "INPUT"),
                &[("type", "checkbox"), ("class", "custom")],
            ),
            &[
                ("interaction", &[("appearance", "none")]),
                ("visual", &[("outline-style", "none")]),
            ],
        )]);
        assert_eq!(
            count(&run_ids(&with_declared), "native-checkbox-reimplemented"),
            0
        );
        let rule = DeclaredRule {
            selector: "input.custom".to_string(),
            href: None,
            index: 0,
            origin: "author".to_string(),
            declarations: BTreeMap::from([
                ("appearance".to_string(), "none".to_string()),
                ("outline-style".to_string(), "none".to_string()),
            ]),
        };
        with_declared.declared = Some(DeclaredFacet {
            rules: vec![rule],
            ..DeclaredFacet::default()
        });
        let out = run_ids(&with_declared);
        assert_eq!(count(&out, "native-checkbox-reimplemented"), 1);
        assert!(out[0].evidence.text.contains("suppresses focus outline"));
    }

    #[test]
    fn progress_positive_negative_keep() {
        let positive = doc(vec![with_attrs(
            with_aria(base(1, "DIV"), &[("role", json!("progressbar"))]),
            &[("aria-valuenow", "75")],
        )]);
        assert_eq!(
            count(&run_ids(&positive), "native-progress-reimplemented"),
            1
        );

        let spinner = doc(vec![with_styles(
            with_aria(base(2, "DIV"), &[("role", json!("progressbar"))]),
            &[("animation", &[("animation-iteration-count", "infinite")])],
        )]);
        let out = run_ids(&spinner);
        assert_eq!(count(&out, "native-progress-reimplemented"), 1);
        assert!(out[0].evidence.text.contains("spinner"));

        let negative = doc(vec![with_aria(
            base(3, "DIV"),
            &[("role", json!("progressbar"))],
        )]);
        assert_eq!(
            count(&run_ids(&negative), "native-progress-reimplemented"),
            0
        );

        let keep = doc(vec![with_children(
            with_attrs(
                with_aria(base(4, "DIV"), &[("role", json!("progressbar"))]),
                &[("aria-valuenow", "10")],
            ),
            vec![base(5, "PROGRESS")],
        )]);
        assert_eq!(count(&run_ids(&keep), "native-progress-reimplemented"), 0);
    }

    #[test]
    fn accordion_positive_negative_keep() {
        let trigger = |id: u64, controls: &str| {
            with_attrs(
                with_aria(base(id, "BUTTON"), &[("aria_expanded", json!("false"))]),
                &[("aria-controls", controls)],
            )
        };
        let positive = doc(vec![with_children(
            base(1, "SECTION"),
            vec![
                trigger(2, "p1"),
                base(3, "DIV"),
                trigger(4, "p2"),
                base(5, "DIV"),
            ],
        )]);
        let out = run_ids(&positive);
        assert_eq!(count(&out, "accordion-custom-instead-of-details"), 1);
        assert!(out[0].evidence.text.contains("0 <details>"));

        let negative = doc(vec![with_children(
            base(6, "SECTION"),
            vec![trigger(7, "p1"), base(8, "DIV")],
        )]);
        assert_eq!(
            count(&run_ids(&negative), "accordion-custom-instead-of-details"),
            0
        );

        let keep = doc(vec![with_children(
            base(9, "SECTION"),
            vec![
                with_children(base(10, "DETAILS"), vec![base(11, "SUMMARY")]),
                trigger(12, "p1"),
                trigger(13, "p2"),
            ],
        )]);
        assert_eq!(
            count(&run_ids(&keep), "accordion-custom-instead-of-details"),
            0
        );

        let heading_in_summary = doc(vec![with_children(
            base(14, "DETAILS"),
            vec![with_children(base(15, "SUMMARY"), vec![base(16, "H3")])],
        )]);
        let out = run_ids(&heading_in_summary);
        assert_eq!(count(&out, "accordion-custom-instead-of-details"), 1);
        assert!(out[0].evidence.text.contains("JAWS/TalkBack"));
    }

    #[test]
    fn tabs_positive_negative_keep() {
        let tab = |id: u64| {
            with_attrs(
                with_aria(base(id, "BUTTON"), &[("role", json!("tab"))]),
                &[("tabindex", "0")],
            )
        };
        let positive = doc(vec![with_children(
            base(1, "DIV"),
            vec![
                with_children(
                    with_aria(base(2, "DIV"), &[("role", json!("tablist"))]),
                    vec![tab(3), tab(4)],
                ),
                with_aria(base(5, "DIV"), &[("role", json!("tabpanel"))]),
                with_aria(base(6, "DIV"), &[("role", json!("tabpanel"))]),
            ],
        )]);
        let out = run_ids(&positive);
        assert_eq!(count(&out, "tabs-reimplemented-with-roving-tabindex"), 1);

        let keep = doc(vec![with_children(
            base(7, "DIV"),
            vec![
                with_attrs(
                    with_aria(base(8, "DIV"), &[("role", json!("tablist"))]),
                    &[("focusgroup", "tablist")],
                ),
                tab(9),
                tab(10),
            ],
        )]);
        assert_eq!(
            count(&run_ids(&keep), "tabs-reimplemented-with-roving-tabindex"),
            0
        );

        let single_panel = doc(vec![with_children(
            base(11, "DIV"),
            vec![
                with_aria(base(12, "DIV"), &[("role", json!("tablist"))]),
                with_aria(base(13, "BUTTON"), &[("role", json!("tab"))]),
                with_aria(base(14, "DIV"), &[("role", json!("tabpanel"))]),
            ],
        )]);
        assert_eq!(
            count(
                &run_ids(&single_panel),
                "tabs-reimplemented-with-roving-tabindex"
            ),
            0
        );
    }

    #[test]
    fn remaining_checks_have_positive_and_keep_cases() {
        // range
        let range = doc(vec![with_attrs(
            with_aria(base(1, "DIV"), &[("role", json!("slider"))]),
            &[("tabindex", "0")],
        )]);
        assert_eq!(count(&run_ids(&range), "native-range-control"), 1);
        let range_keep = doc(vec![with_aria(
            with_attrs(base(2, "INPUT"), &[("type", "range")]),
            &[("name", json!("Volume"))],
        )]);
        assert_eq!(count(&run_ids(&range_keep), "native-range-control"), 0);

        // switch
        let switch = with_attrs(
            with_styles(
                base(3, "INPUT"),
                &[("interaction", &[("appearance", "auto")])],
            ),
            &[("type", "checkbox"), ("switch", "")],
        );
        let switch = with_ax(switch, &[("role", json!("checkbox"))]);
        assert_eq!(count(&run_ids(&doc(vec![switch])), "switch-semantics"), 1);
        let switch_ok = with_attrs(base(4, "INPUT"), &[("type", "checkbox"), ("switch", "")]);
        let switch_ok = with_ax(switch_ok, &[("role", json!("switch"))]);
        assert_eq!(
            count(&run_ids(&doc(vec![switch_ok])), "switch-semantics"),
            0
        );

        // menu
        let menu = doc(vec![with_children(
            with_aria(base(5, "DIV"), &[("role", json!("menu"))]),
            vec![
                with_attrs(
                    with_aria(base(6, "BUTTON"), &[("role", json!("menuitem"))]),
                    &[("tabindex", "0")],
                ),
                with_attrs(
                    with_aria(base(7, "BUTTON"), &[("role", json!("menuitem"))]),
                    &[("tabindex", "-1")],
                ),
            ],
        )]);
        assert_eq!(count(&run_ids(&menu), "menu-composite-reimplemented"), 1);
        let menu_keep = doc(vec![with_children(
            with_attrs(
                with_aria(base(8, "DIV"), &[("role", json!("menu"))]),
                &[("focusgroup", "menu nomemory")],
            ),
            vec![with_attrs(
                with_aria(base(9, "BUTTON"), &[("role", json!("menuitem"))]),
                &[("tabindex", "0")],
            )],
        )]);
        assert_eq!(
            count(&run_ids(&menu_keep), "menu-composite-reimplemented"),
            0
        );

        // toolbar
        let toolbar = doc(vec![with_children(
            with_aria(base(10, "DIV"), &[("role", json!("toolbar"))]),
            vec![
                with_attrs(base(11, "BUTTON"), &[("tabindex", "0")]),
                with_attrs(base(12, "BUTTON"), &[("tabindex", "0")]),
            ],
        )]);
        assert_eq!(
            count(&run_ids(&toolbar), "toolbar-roving-tabindex-broken"),
            1
        );
        let toolbar_keep = doc(vec![with_children(
            with_attrs(
                with_aria(base(13, "DIV"), &[("role", json!("toolbar"))]),
                &[("focusgroup", "toolbar wrap")],
            ),
            vec![with_attrs(base(14, "BUTTON"), &[("tabindex", "0")])],
        )]);
        assert_eq!(
            count(&run_ids(&toolbar_keep), "toolbar-roving-tabindex-broken"),
            0
        );

        // drawer
        let drawer = with_children(
            with_styles(
                with_rect(base(15, "DIV"), 0.0, 0.0, 1000.0, 800.0),
                &[("layout", &[("position", "fixed"), ("z-index", "9999")])],
            ),
            vec![with_attrs(base(16, "A"), &[("href", "/x")])],
        );
        let mut drawer_doc = doc(vec![drawer]);
        drawer_doc.viewport = Some((1000.0, 800.0));
        assert_eq!(
            count(
                &run_ids(&drawer_doc),
                "drawer-reimplemented-with-fixed-overlay"
            ),
            1
        );
        let small = with_styles(
            with_rect(base(17, "DIV"), 0.0, 0.0, 400.0, 200.0),
            &[("layout", &[("position", "fixed"), ("z-index", "9999")])],
        );
        let mut small_doc = doc(vec![with_children(
            small,
            vec![with_attrs(base(18, "A"), &[("href", "/x")])],
        )]);
        small_doc.viewport = Some((1000.0, 800.0));
        assert_eq!(
            count(
                &run_ids(&small_doc),
                "drawer-reimplemented-with-fixed-overlay"
            ),
            0
        );

        // os text scale
        let mut text_doc = doc(vec![with_styles(
            base(19, "HTML"),
            &[("typography", &[("font-size", "16px")])],
        )]);
        text_doc.document = Some(DocumentFacet {
            meta: BTreeMap::from([("text-scale".to_string(), "scale".to_string())]),
            ..DocumentFacet::default()
        });
        assert_eq!(count(&run_ids(&text_doc), "os-text-scale-optin"), 1);
        let mut text_keep = doc(vec![with_styles(
            base(20, "H1"),
            &[(
                "typography",
                &[("font-size", "clamp(2rem, 1.25rem + 2vw, 4rem)")],
            )],
        )]);
        text_keep.document = Some(DocumentFacet {
            meta: BTreeMap::from([("text-scale".to_string(), "scale".to_string())]),
            ..DocumentFacet::default()
        });
        assert_eq!(count(&run_ids(&text_keep), "os-text-scale-optin"), 0);

        // root color scheme
        let mut scheme_doc = doc(vec![with_styles(
            base(21, "HTML"),
            &[("accessibility", &[("color-scheme", "dark")])],
        )]);
        scheme_doc.declared = Some(DeclaredFacet {
            rules: vec![DeclaredRule {
                selector: ":root".to_string(),
                href: None,
                index: 0,
                origin: "author".to_string(),
                declarations: BTreeMap::from([("color-scheme".to_string(), "dark".to_string())]),
            }],
            ..DeclaredFacet::default()
        });
        assert_eq!(
            count(&run_ids(&scheme_doc), "root-color-scheme-hardcoded"),
            1
        );
        let mut scheme_keep = scheme_doc.clone();
        if let Some(declared) = scheme_keep.declared.as_mut() {
            declared.rules[0]
                .declarations
                .insert("color-scheme".to_string(), "light dark".to_string());
        }
        assert_eq!(
            count(&run_ids(&scheme_keep), "root-color-scheme-hardcoded"),
            0
        );
        scheme_keep.declared = None;
        assert_eq!(
            count(&run_ids(&scheme_keep), "root-color-scheme-hardcoded"),
            0
        );

        // element color scheme
        let element = doc(vec![with_styles(
            base(22, "DIV"),
            &[
                ("accessibility", &[("color-scheme", "dark")]),
                ("visual", &[("background-color", "rgba(0, 0, 0, 0)")]),
            ],
        )]);
        assert_eq!(
            count(&run_ids(&element), "color-scheme-without-background"),
            1
        );
        let element_keep = doc(vec![with_styles(
            base(23, "DIV"),
            &[
                ("accessibility", &[("color-scheme", "dark")]),
                ("visual", &[("background-color", "rgb(0, 0, 0)")]),
            ],
        )]);
        assert_eq!(
            count(&run_ids(&element_keep), "color-scheme-without-background"),
            0
        );

        // masked icon
        let icon = doc(vec![with_pseudo(
            base(24, "SPAN"),
            "::before",
            &[("visual", &[("mask-image", "url(icon.svg)")])],
        )]);
        let mut icon_doc = icon;
        icon_doc.env = Some(EnvFacet {
            media: BTreeMap::from([("forced-colors".to_string(), "none".to_string())]),
            ..EnvFacet::default()
        });
        assert_eq!(
            count(
                &run_ids(&icon_doc),
                "masked-icon-missing-forced-color-adjust"
            ),
            1
        );
        let icon_fixed = with_pseudo(
            base(25, "SPAN"),
            "::before",
            &[
                ("visual", &[("mask-image", "url(icon.svg)")]),
                ("accessibility", &[("forced-color-adjust", "none")]),
            ],
        );
        let mut icon_fixed_doc = doc(vec![icon_fixed]);
        icon_fixed_doc.env = Some(EnvFacet {
            media: BTreeMap::from([("forced-colors".to_string(), "none".to_string())]),
            ..EnvFacet::default()
        });
        assert_eq!(
            count(
                &run_ids(&icon_fixed_doc),
                "masked-icon-missing-forced-color-adjust"
            ),
            0
        );

        // coarse pointer
        let coarse = with_rect(base(26, "BUTTON"), 0.0, 0.0, 36.0, 32.0);
        let mut coarse_doc = doc(vec![coarse]);
        coarse_doc.env = Some(EnvFacet {
            media: BTreeMap::from([("pointer".to_string(), "coarse".to_string())]),
            ..EnvFacet::default()
        });
        let out = run_ids(&coarse_doc);
        assert_eq!(count(&out, "coarse-pointer-target-size"), 1);
        assert_eq!(
            out[0].evidence.metric.as_deref(),
            Some("target_min_side_px")
        );
        let fine_doc = doc(vec![with_rect(base(27, "BUTTON"), 0.0, 0.0, 36.0, 32.0)]);
        assert_eq!(count(&run_ids(&fine_doc), "coarse-pointer-target-size"), 0);
    }

    #[test]
    fn specs_are_unique_and_deterministic() {
        let ids: Vec<&str> = specs().iter().map(|spec| spec.id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len());
        assert_eq!(ids.len(), 15);
        for spec in specs() {
            assert!(!spec.guides.is_empty(), "{} must list a guide", spec.id);
            assert!(
                !spec.keep_when.is_empty(),
                "{} must document keepWhen",
                spec.id
            );
        }
        let document = doc(vec![with_aria(
            base(1, "DIV"),
            &[("role", json!("dialog"))],
        )]);
        let first = run_ids(&document);
        let second = run_ids(&document);
        assert_eq!(
            first
                .iter()
                .map(|line| (line.check.clone(), line.evidence.text.clone()))
                .collect::<Vec<_>>(),
            second
                .iter()
                .map(|line| (line.check.clone(), line.evidence.text.clone()))
                .collect::<Vec<_>>()
        );
    }
}
