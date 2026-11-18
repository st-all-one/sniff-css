//! Checks da família G — Web Components (F2, walker de shadow root).
//!
//! Todos os sinais vêm de campos do snapshot: `tag`, `aria`, `ax`, `attrs`,
//! `styles` e as variáveis CSS globais. O walker do motor marca a raiz de cada
//! shadow root **aberta** com `DiffNode.shadow = true` e conta as raízes em
//! `DiffDocument.shadow_roots`; nós internos são identificados pela proximidade
//! com essa raiz (ou pelo seletor/caminho que contém `#shadow-root`).
//!
//! `mode: 'closed'` não é observável e, portanto, **não** tem check: nenhuma
//! linha desta família afirma detectar uma shadow root fechada.

use crate::modern::{CheckSpec, Facet, ModernCtx};
use serde_json::Value;
use sniff_core::modern::Evidence;
use sniff_css_diff::DiffNode;
use std::collections::{BTreeMap, BTreeSet};

/// Marcador usado pelo walker para a raiz de uma shadow root aberta.
const SHADOW_MARKER: &str = "#shadow-root";

/// Pares de chaves ARIA (`aria` facet, `attrs` facet) para referências por id.
const REF_SOURCES: [(&str, &str); 6] = [
    ("aria_labelledby", "aria-labelledby"),
    ("aria_describedby", "aria-describedby"),
    ("labelledby", "aria-labelledby"),
    ("describedby", "aria-describedby"),
    ("aria_controls", "aria-controls"),
    ("controls", "aria-controls"),
];

/// Papéis que, pela WCAG 4.1.2, precisam de nome acessível.
const NAMED_ROLES: [&str; 14] = [
    "button",
    "checkbox",
    "combobox",
    "link",
    "listbox",
    "menuitem",
    "meter",
    "progressbar",
    "radio",
    "searchbox",
    "slider",
    "spinbutton",
    "switch",
    "tab",
];

/// Especificações dos oito checks da família G.
static SPECS: &[CheckSpec] = &[
    CheckSpec::new(
        "shadow-root-open-mode",
        &[
            "shadow-dom",
            "web-components",
            "prerendering-custom-elements",
        ],
        &[Facet::Shadow],
        "Nenhuma shadow root aberta alcançável (ou host não personalizado): componentes light DOM e raízes fechadas ficam de fora; não afirmar detecção de mode:'closed'.",
    ),
    CheckSpec::new(
        "elementinternals-ax-state",
        &[
            "accessible-web-components",
            "custom-elements",
            "form-associated-custom-elements",
        ],
        &[Facet::Shadow, Facet::Ax],
        "ElementInternals sem atributos ARIA é o padrão correto: só emitir quando o estado AX expõe um controle interativo sem nome acessível ou ignorado pela AX; não exigir tabindex em componente estático.",
    ),
    CheckSpec::new(
        "shadow-internals-semantics",
        &[
            "accessible-web-components",
            "shadow-dom",
            "styling-web-components",
            "web-components",
        ],
        &[Facet::Shadow],
        "Não emitir fora de shadow roots, em SVG decorativo com aria-hidden, nem em referência ARIA resolvida na mesma árvore.",
    ),
    CheckSpec::new(
        "custom-element-tag-name",
        &["custom-elements", "web-components"],
        &[],
        "Não emitir em tag com hífen em kebab-case minúsculo, nem em elemento comum sem sinal de personalização; customized built-in via is= é sinal legítimo de legado.",
    ),
    CheckSpec::new(
        "face-form-integration",
        &["form-associated-custom-elements"],
        &[Facet::Shadow, Facet::Attrs],
        "Não emitir em hidden input fora de formulário, sem name, ou fora de host personalizado; não exigir :user-invalid em FACE.",
    ),
    CheckSpec::new(
        "aria-cross-shadow-refs",
        &["accessible-web-components"],
        &[Facet::Shadow, Facet::Attrs],
        "Não emitir quando o id existir na mesma árvore, quando a relação usar aria-label textual, nem quando a API ariaLabelledByElements estiver disponível (não observável no snapshot).",
    ),
    CheckSpec::new(
        "declarative-shadow-prerender",
        &["prerendering-custom-elements"],
        &[Facet::Shadow],
        "Não emitir quando a shadow root contiver <style>, estiver vazia, ou quando o componente usar apenas light DOM; não exigir guarda :not(:defined) em estilo estrutural do template.",
    ),
    CheckSpec::new(
        "component-theming-boundary",
        &["styling-web-components"],
        &[Facet::Shadow],
        "Não emitir em nó light DOM, em propriedade privada --_, em propriedade pública com valor herdado igual ao global, nem em controle interno com font:inherit.",
    ),
];

/// Registro dos checks da família G (fonte única de ids).
#[must_use]
pub fn specs() -> &'static [CheckSpec] {
    SPECS
}

/// O extrator prefixa `shadow-root > ` no `path` de nós dentro de shadow roots
/// abertas; `mode:'closed'` não deixa rastro e permanece fora de alcance.
fn is_shadow_node(node: &DiffNode) -> bool {
    node.selector.contains(SHADOW_MARKER)
        || node
            .path
            .as_deref()
            .is_some_and(|path| path.contains(SHADOW_MARKER))
}

/// Contexto de árvore usado por todos os checks desta família.
struct ShadowView<'a> {
    by_id: BTreeMap<u64, &'a DiffNode>,
    roots: Vec<&'a DiffNode>,
}

impl<'a> ShadowView<'a> {
    /// Indexa os nós uma única vez por execução da família.
    fn new(ctx: &ModernCtx<'a>) -> Self {
        let mut by_id = BTreeMap::new();
        let mut roots = Vec::new();
        for node in &ctx.flat {
            if is_shadow_node(node) {
                roots.push(*node);
            }
            by_id.insert(node.id, *node);
        }
        tracing::trace!(
            shadow_roots = ctx.doc.shadow_roots,
            roots = roots.len(),
            "family G shadow index built"
        );
        Self { by_id, roots }
    }

    /// Pai direto de um nó, quando presente no snapshot.
    fn parent<'n>(&'n self, node: &'n DiffNode) -> Option<&'n DiffNode> {
        node.parent_id.and_then(|id| self.by_id.get(&id)).copied()
    }

    /// Shadow root mais próxima (o próprio nó, se ele já for a raiz).
    fn root_for<'n>(&'n self, node: &'n DiffNode) -> Option<&'n DiffNode> {
        let mut current = node;
        let mut seen = BTreeSet::new();
        loop {
            if is_shadow_node(current) {
                return Some(current);
            }
            let parent = self.parent(current)?;
            if !seen.insert(parent.id) {
                return None;
            }
            current = parent;
        }
    }

    /// Host de uma shadow root (o pai da raiz marcada).
    fn host_for<'n>(&'n self, root: &'n DiffNode) -> Option<&'n DiffNode> {
        self.parent(root)
    }

    /// Identificador determinístico da árvore: `0` é o light DOM.
    fn tree_key(&self, node: &DiffNode) -> u64 {
        self.root_for(node).map_or(0, |root| root.id)
    }

    /// O nó pertence a uma shadow root observável?
    fn in_shadow(&self, node: &DiffNode) -> bool {
        self.root_for(node).is_some()
            || node.selector.contains(SHADOW_MARKER)
            || node
                .path
                .as_deref()
                .is_some_and(|path| path.contains(SHADOW_MARKER))
    }

    /// O nó (ou um ancestral) está dentro de um `<form>`?
    fn in_form<'n>(&'n self, node: &'n DiffNode) -> bool {
        let mut current = node;
        let mut seen = BTreeSet::new();
        while let Some(parent) = self.parent(current) {
            if !seen.insert(parent.id) {
                return false;
            }
            if parent.tag.as_deref() == Some("FORM") {
                return true;
            }
            current = parent;
        }
        false
    }

    /// Coleta os descendentes de uma raiz, em ordem de documento.
    fn collect<'n>(&self, node: &'n DiffNode, out: &mut Vec<&'n DiffNode>) {
        for child in &node.children {
            out.push(child);
            self.collect(child, out);
        }
    }
}

/// Verifica se a tag parece um nome de elemento personalizado.
#[must_use]
fn looks_custom(tag: &str) -> bool {
    !tag.starts_with('#') && (tag.contains('-') || tag.contains('_'))
}

/// Nome kebab-case: minúsculas, segmentos não vazios e sem `_`/caracteres especiais.
#[must_use]
fn is_kebab_case(tag: &str) -> bool {
    let lowered = tag.to_ascii_lowercase();
    let mut segments = lowered.split('-');
    match segments.next() {
        Some(first) if !first.is_empty() && first.bytes().all(|byte| byte.is_ascii_lowercase()) => {
        }
        _ => return false,
    }
    segments.all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    })
}

/// Campo textual da facet `ax`.
fn ax_str<'a>(node: &'a DiffNode, key: &str) -> Option<&'a str> {
    node.ax
        .as_ref()?
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

/// Campo booleano da facet `ax`.
fn ax_bool(node: &DiffNode, key: &str) -> bool {
    node.ax
        .as_ref()
        .and_then(|value| value.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Papel AX que exige nome acessível (WCAG 4.1.2).
#[must_use]
fn needs_name(role: &str) -> bool {
    NAMED_ROLES.contains(&role)
}

/// Referências por id presentes no nó, deduplicadas e ordenadas.
fn aria_refs(ctx: &ModernCtx<'_>, node: &DiffNode) -> BTreeSet<(String, &'static str)> {
    let mut refs = BTreeSet::new();
    for (aria_key, attr_key) in REF_SOURCES {
        for value in [ctx.aria(node, aria_key), ctx.attr(node, attr_key)]
            .into_iter()
            .flatten()
        {
            for token in value.split_whitespace() {
                refs.insert((token.to_string(), attr_key));
            }
        }
    }
    refs
}

/// Mapa `id -> árvores onde o id existe` (0 = light DOM).
fn collect_ids(ctx: &ModernCtx<'_>, view: &ShadowView<'_>) -> BTreeMap<String, BTreeSet<u64>> {
    let mut ids: BTreeMap<String, BTreeSet<u64>> = BTreeMap::new();
    for node in &ctx.flat {
        if let Some(id) = ctx.attr(node, "id") {
            ids.entry(id.to_string())
                .or_default()
                .insert(view.tree_key(node));
        }
    }
    ids
}

/// Rótulo determinístico da árvore de um id para a evidência.
fn tree_label(view: &ShadowView<'_>, tree: u64) -> String {
    if tree == 0 {
        return "light DOM".to_string();
    }
    view.by_id
        .get(&tree)
        .and_then(|root| view.host_for(root))
        .map_or_else(
            || SHADOW_MARKER.to_string(),
            |host| {
                format!(
                    "{SHADOW_MARKER} de <{}>",
                    ctx_tag(host).to_ascii_lowercase()
                )
            },
        )
}

/// Tag de um nó em minúsculas, usada em mensagens.
fn ctx_tag(node: &DiffNode) -> &str {
    node.tag.as_deref().unwrap_or("")
}

/// Inventário de shadow roots abertas: manter `mode:'open'` é pré-requisito
/// para que ferramentas de AX e testes consigam inspecionar os internos.
fn check_shadow_root_open_mode(
    ctx: &ModernCtx<'_>,
    view: &ShadowView<'_>,
    spec: &CheckSpec,
    out: &mut Vec<crate::rules::CheckLine>,
) {
    let mut hosts = BTreeSet::new();
    for root in &view.roots {
        if let Some(host) = view.host_for(root)
            && looks_custom(ctx.tag(host))
        {
            hosts.insert(host.id);
        }
    }
    if hosts.is_empty() {
        tracing::debug!("no custom-element shadow root in snapshot; check stays silent");
        return;
    }
    let observed = hosts.len();
    let tags = hosts
        .iter()
        .filter_map(|id| view.by_id.get(id))
        .map(|host| ctx.tag(host).to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(", ");
    ctx.emit_global(
        out,
        spec,
        SHADOW_MARKER,
        Evidence::numeric(
            format!(
                "shadow root aberta em {tags}: manter mode:'open' — raízes fechadas não podem ser inspecionadas"
            ),
            "shadow_walker",
            "open_shadow_hosts",
            observed,
            1,
            ">=",
        ),
    );
}

/// Estado de `ElementInternals` sem nome acessível ou ignorado pela AX.
fn check_elementinternals_ax_state(
    ctx: &ModernCtx<'_>,
    spec: &CheckSpec,
    out: &mut Vec<crate::rules::CheckLine>,
) {
    for node in &ctx.flat {
        let tag = ctx.tag(node);
        if !looks_custom(tag) {
            continue;
        }
        let ax_role = ax_str(node, "role");
        let has_internal_state = ax_role.is_some()
            || ax_str(node, "checked").is_some()
            || ax_str(node, "expanded").is_some();
        if !has_internal_state {
            continue;
        }
        let has_dom_aria = ctx.aria(node, "role").is_some()
            || ctx.aria(node, "aria_checked").is_some()
            || ctx.aria(node, "aria_expanded").is_some();
        if has_dom_aria || ctx.attr(node, "aria-label").is_some() {
            continue;
        }
        let role = ax_role.unwrap_or("");
        let has_name = ctx.aria(node, "name").is_some() || ax_str(node, "name").is_some();
        let ignored = ax_bool(node, "ignored") && ctx.aria_bool(node, "focusable");
        if !ignored && (has_name || !needs_name(role)) {
            continue;
        }
        let reason = if ignored {
            "host interativo ignorado pela árvore de acessibilidade"
        } else {
            "sem nome acessível (WCAG 4.1.2)"
        };
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!(
                    "<{}> ax.role={role} sem ARIA no DOM: {reason}",
                    tag.to_ascii_lowercase()
                ),
                "ax+aria",
            ),
        );
    }
}

/// Semântica e referências quebradas dentro de shadow roots.
fn check_shadow_internals_semantics(
    ctx: &ModernCtx<'_>,
    view: &ShadowView<'_>,
    ids: &BTreeMap<String, BTreeSet<u64>>,
    spec: &CheckSpec,
    out: &mut Vec<crate::rules::CheckLine>,
) {
    for node in &ctx.flat {
        if !view.in_shadow(node) {
            continue;
        }
        let tag = ctx.tag(node);
        let role = ctx.aria(node, "role").or_else(|| ctx.attr(node, "role"));
        if matches!(tag, "DIV" | "SPAN")
            && role == Some("button")
            && (ctx.attr(node, "tabindex") == Some("0") || ctx.aria_bool(node, "focusable"))
        {
            let named = ctx.aria(node, "name").is_some()
                || ax_str(node, "name").is_some()
                || ctx.attr(node, "aria-label").is_some();
            let evidence = if named {
                format!("{SHADOW_MARKER} > {tag}[role=button][tabindex=0] reimplementa <button>")
            } else {
                format!("{SHADOW_MARKER} > {tag}[role=button][tabindex=0] sem nome (WCAG 4.1.2)")
            };
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(evidence, "shadow_walker+aria"),
            );
            continue;
        }
        let dangling = aria_refs(ctx, node)
            .into_iter()
            .filter(|(id, _)| ids.get(id).is_none())
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        if !dangling.is_empty() {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!(
                        "{SHADOW_MARKER} > {} referencia id(s) inexistente(s): {}",
                        tag.to_ascii_lowercase(),
                        dangling.join(", ")
                    ),
                    "shadow_walker+aria",
                ),
            );
        }
    }
}

/// Nomenclatura de custom elements e legado `is=`.
fn check_custom_element_tag_name(
    ctx: &ModernCtx<'_>,
    view: &ShadowView<'_>,
    spec: &CheckSpec,
    out: &mut Vec<crate::rules::CheckLine>,
) {
    for node in &ctx.flat {
        let tag = ctx.tag(node);
        if tag.is_empty() {
            continue;
        }
        let lowered = tag.to_ascii_lowercase();
        let customized = ctx.attr(node, "is").is_some();
        if customized {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!("<{lowered}> usa is= (customized built-in): legado sem Safari"),
                    "attrs+tag",
                ),
            );
            continue;
        }
        if !looks_custom(&lowered) || is_kebab_case(&lowered) {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!("<{lowered}> não é kebab-case minúsculo com hífen"),
                "tag",
            ),
        );
    }
    for root in &view.roots {
        let Some(host) = view.host_for(root) else {
            continue;
        };
        let host_tag = ctx.tag(host);
        if host_tag.is_empty() || looks_custom(host_tag) {
            continue;
        }
        ctx.emit(
            out,
            spec,
            host,
            Evidence::new(
                format!(
                    "<{}> hospeda shadow root sem hífen no nome: custom elements exigem kebab-case",
                    host_tag.to_ascii_lowercase()
                ),
                "shadow_walker+tag",
            ),
        );
    }
}

/// Integração de form-associated custom elements (FACE).
fn check_face_form_integration(
    ctx: &ModernCtx<'_>,
    view: &ShadowView<'_>,
    spec: &CheckSpec,
    out: &mut Vec<crate::rules::CheckLine>,
) {
    for node in &ctx.flat {
        if ctx.tag(node) != "INPUT" || ctx.attr(node, "type") != Some("hidden") {
            continue;
        }
        let Some(name) = ctx.attr(node, "name") else {
            continue;
        };
        if !view.in_form(node) {
            continue;
        }
        let Some(host) = view.root_for(node).and_then(|root| view.host_for(root)) else {
            continue;
        };
        if !looks_custom(ctx.tag(host)) {
            continue;
        }
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!(
                    "<{}> usa <input type=hidden name={name}>: usar setFormValue() do ElementInternals",
                    ctx.tag(host).to_ascii_lowercase()
                ),
                "shadow_walker+attrs",
            ),
        );
    }
}

/// Referências ARIA que cruzam a fronteira light DOM ↔ shadow root.
fn check_aria_cross_shadow_refs(
    ctx: &ModernCtx<'_>,
    view: &ShadowView<'_>,
    ids: &BTreeMap<String, BTreeSet<u64>>,
    spec: &CheckSpec,
    out: &mut Vec<crate::rules::CheckLine>,
) {
    for node in &ctx.flat {
        let tree = view.tree_key(node);
        let mut bad: Vec<(String, u64)> = Vec::new();
        for (id, _) in aria_refs(ctx, node) {
            let Some(trees) = ids.get(&id) else {
                continue;
            };
            if trees.contains(&tree) {
                continue;
            }
            let other = trees.iter().copied().find(|candidate| *candidate != tree);
            if let Some(other) = other {
                bad.push((id, other));
            }
        }
        bad.sort();
        bad.dedup();
        if bad.is_empty() {
            continue;
        }
        let detail = bad
            .iter()
            .map(|(id, other)| format!("{id} → {}", tree_label(view, *other)))
            .collect::<Vec<_>>()
            .join("; ");
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!(
                    "<{}> referencia ARIA fora da própria árvore: {detail} (WCAG 4.1.2)",
                    ctx.tag(node).to_ascii_lowercase()
                ),
                "shadow_walker+attrs",
            ),
        );
    }
}

/// CSS da shadow root precisa viver dentro do template serializável.
fn check_declarative_shadow_prerender(
    ctx: &ModernCtx<'_>,
    view: &ShadowView<'_>,
    spec: &CheckSpec,
    out: &mut Vec<crate::rules::CheckLine>,
) {
    for root in &view.roots {
        let mut descendants = Vec::new();
        view.collect(root, &mut descendants);
        if descendants.is_empty() || descendants.iter().any(|node| ctx.tag(node) == "STYLE") {
            continue;
        }
        let host_tag = view.host_for(root).map_or_else(
            || "unknown".to_string(),
            |host| ctx.tag(host).to_ascii_lowercase(),
        );
        ctx.emit(
            out,
            spec,
            root,
            Evidence::new(
                format!(
                    "<{host_tag}> {SHADOW_MARKER} renderiza {} nó(s) sem <style> interno: mover o CSS para dentro do template shadowrootmode evita FOUC",
                    descendants.len()
                ),
                "shadow_walker+tag",
            ),
        );
    }
}

/// Fronteira de theming: tokens públicos e fonte herdada nos internos.
fn check_component_theming_boundary(
    ctx: &ModernCtx<'_>,
    view: &ShadowView<'_>,
    spec: &CheckSpec,
    out: &mut Vec<crate::rules::CheckLine>,
) {
    let variables = ctx.doc.css_variables.as_ref();
    for node in &ctx.flat {
        if !view.in_shadow(node) {
            continue;
        }
        let tag = ctx.tag(node);
        if matches!(tag, "BUTTON" | "INPUT" | "SELECT")
            && let Some(font) = ctx.style_any(
                node,
                &[("typography", "font-family"), ("typography", "font")],
            )
            && !font.contains("inherit")
        {
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!(
                        "{SHADOW_MARKER} > {tag} usa fonte própria ({font}): adicionar font:inherit"
                    ),
                    "shadow_walker+styles",
                ),
            );
        }
        let Some(variables) = variables else {
            continue;
        };
        let Some(custom) = node
            .styles
            .as_ref()
            .and_then(|styles| styles.get("custom"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        for (key, value) in custom {
            if !key.starts_with("--") || key.starts_with("--_") {
                continue;
            }
            let Some(global) = variables.get(key) else {
                continue;
            };
            if global == value {
                continue;
            }
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(
                    format!(
                        "{SHADOW_MARKER} redefine a propriedade pública {key}: deixá-la não definida em :host e usar token privado --_"
                    ),
                    "shadow_walker+css_variables",
                ),
            );
        }
    }
}

/// Executa a família G com as facetas disponíveis no snapshot.
pub fn run(ctx: &ModernCtx<'_>, out: &mut Vec<crate::rules::CheckLine>) {
    let view = ShadowView::new(ctx);
    let ids = collect_ids(ctx, &view);
    tracing::debug!(
        shadow_roots = ctx.doc.shadow_roots,
        css_variables = ctx.doc.css_variables.is_some(),
        shadow_index = view.roots.len(),
        aria_ids = ids.len(),
        "family G: shadow component checks"
    );
    for spec in specs() {
        if !ctx.enabled(spec) {
            tracing::trace!(
                id = spec.id,
                "family G check suppressed (facet missing or filtered)"
            );
            continue;
        }
        let before = out.len();
        match spec.id {
            "shadow-root-open-mode" => check_shadow_root_open_mode(ctx, &view, spec, out),
            "elementinternals-ax-state" => check_elementinternals_ax_state(ctx, spec, out),
            "shadow-internals-semantics" => {
                check_shadow_internals_semantics(ctx, &view, &ids, spec, out);
            }
            "custom-element-tag-name" => check_custom_element_tag_name(ctx, &view, spec, out),
            "face-form-integration" => check_face_form_integration(ctx, &view, spec, out),
            "aria-cross-shadow-refs" => {
                check_aria_cross_shadow_refs(ctx, &view, &ids, spec, out);
            }
            "declarative-shadow-prerender" => {
                check_declarative_shadow_prerender(ctx, &view, spec, out);
            }
            "component-theming-boundary" => {
                check_component_theming_boundary(ctx, &view, spec, out);
            }
            _ => {}
        }
        if out.len() > before {
            tracing::debug!(
                id = spec.id,
                emitted = out.len() - before,
                "family G check emitted"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modern::ModernOptions;
    use crate::rules::CheckLine;
    use serde_json::Map;
    use sniff_css_diff::DiffDocument;

    fn node(id: u64, tag: &str, selector: &str) -> DiffNode {
        DiffNode {
            id,
            parent_id: None,
            selector: selector.to_string(),
            tag: Some(tag.to_string()),
            path: Some(selector.to_string()),
            depth: Some(0),
            rect: Some(serde_json::json!({"x": 0, "y": 0, "width": 0, "height": 0})),
            metrics: None,
            noticeable: None,
            hash: None,
            styles: Some(Map::new()),
            pseudo: Some(Map::new()),
            aria: Some(Value::Object(Map::new())),
            contrast: None,
            ax: Some(Value::Object(Map::new())),
            attributes: Some(Value::Object(Map::new())),
            children: Vec::new(),
            effective_background: None,
        }
    }

    fn child(parent: &mut DiffNode, mut nested: DiffNode) {
        nested.parent_id = Some(parent.id);
        nested.depth = parent.depth.map_or(Some(1), |depth| Some(depth + 1));
        parent.children.push(nested);
    }

    fn object(pairs: &[(&str, &str)]) -> Value {
        let mut map = Map::new();
        for (key, value) in pairs {
            map.insert((*key).to_string(), Value::String((*value).to_string()));
        }
        Value::Object(map)
    }

    fn attrs(mut node: DiffNode, pairs: &[(&str, &str)]) -> DiffNode {
        node.attributes = Some(object(pairs));
        node
    }

    fn aria(mut node: DiffNode, pairs: &[(&str, &str)]) -> DiffNode {
        node.aria = Some(object(pairs));
        node
    }

    fn ax(mut node: DiffNode, pairs: &[(&str, Value)]) -> DiffNode {
        let mut map = Map::new();
        for (key, value) in pairs {
            map.insert((*key).to_string(), (*value).clone());
        }
        node.ax = Some(Value::Object(map));
        node
    }

    fn style(mut node: DiffNode, category: &str, pairs: &[(&str, &str)]) -> DiffNode {
        let mut styles = node.styles.unwrap_or_default();
        let mut map = styles
            .get(category)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (key, value) in pairs {
            map.insert((*key).to_string(), Value::String((*value).to_string()));
        }
        styles.insert(category.to_string(), Value::Object(map));
        node.styles = Some(styles);
        node
    }

    fn doc(nodes: Vec<DiffNode>, shadow_roots: usize) -> DiffDocument {
        let mut document = DiffDocument::default();
        document.nodes = nodes;
        document.shadow_roots = shadow_roots;
        document
    }

    fn run_doc(document: &DiffDocument) -> Vec<CheckLine> {
        let options = ModernOptions::default();
        let ctx = ModernCtx::new(document, &options);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        out
    }

    fn has(out: &[CheckLine], id: &str) -> bool {
        out.iter().any(|line| line.check == id)
    }

    #[test]
    fn registry_has_eight_unique_ids() {
        let all = specs();
        assert_eq!(all.len(), 8);
        let mut ids = BTreeSet::new();
        for spec in all {
            assert!(ids.insert(spec.id), "duplicated id: {}", spec.id);
            assert!(!spec.guides.is_empty(), "missing guide: {}", spec.id);
            assert!(!spec.keep_when.is_empty(), "missing keepWhen: {}", spec.id);
        }
        let tag_name = all.iter().find(|spec| spec.id == "custom-element-tag-name");
        assert_eq!(tag_name.map_or(1, |spec| spec.required.len()), 0);
    }

    #[test]
    fn shadow_root_open_mode_positive_negative_and_keep_when() {
        let mut host = node(1, "MY-WIDGET", "my-widget");
        let mut root = node(2, "#shadow-root", "my-widget > #shadow-root");
        root.parent_id = Some(1);
        let content = node(3, "DIV", "my-widget > #shadow-root > div");
        child(&mut root, content);
        child(&mut host, root);
        let positive = run_doc(&doc(vec![host], 1));
        assert!(has(&positive, "shadow-root-open-mode"));

        let mut builtin = node(1, "DIV", "div");
        let root = node(2, "#shadow-root", "div > #shadow-root");
        child(&mut builtin, root);
        let negative = run_doc(&doc(vec![builtin], 1));
        assert!(!has(&negative, "shadow-root-open-mode"));

        let light = node(1, "MY-WIDGET", "my-widget");
        let keep = run_doc(&doc(vec![light], 0));
        assert!(!has(&keep, "shadow-root-open-mode"));
    }

    #[test]
    fn elementinternals_ax_state_positive_negative_and_keep_when() {
        let host = ax(
            aria(
                attrs(
                    node(1, "TOGGLE-SWITCH", "toggle-switch"),
                    &[("tabindex", "0")],
                ),
                &[("focusable", "true")],
            ),
            &[
                ("role", Value::from("switch")),
                ("checked", Value::from("false")),
            ],
        );
        let positive = run_doc(&doc(vec![host], 1));
        assert!(has(&positive, "elementinternals-ax-state"));
        assert!(positive[0].evidence.text.contains("WCAG 4.1.2"));

        let explicit = ax(
            aria(
                node(1, "TOGGLE-SWITCH", "toggle-switch"),
                &[("role", "switch")],
            ),
            &[("role", Value::from("switch"))],
        );
        let negative = run_doc(&doc(vec![explicit], 1));
        assert!(!has(&negative, "elementinternals-ax-state"));

        let named = ax(
            aria(
                node(1, "TOGGLE-SWITCH", "toggle-switch"),
                &[("name", "Alerts")],
            ),
            &[("role", Value::from("switch"))],
        );
        let keep = run_doc(&doc(vec![named], 1));
        assert!(!has(&keep, "elementinternals-ax-state"));
    }

    #[test]
    fn shadow_internals_semantics_positive_negative_and_keep_when() {
        let mut host = node(1, "MY-CARD", "my-card");
        let mut root = node(2, "#shadow-root", "my-card > #shadow-root");
        let fake = attrs(
            node(3, "DIV", "my-card > #shadow-root > div"),
            &[("role", "button"), ("tabindex", "0")],
        );
        child(&mut root, fake);
        child(&mut host, root);
        let positive = run_doc(&doc(vec![host], 1));
        assert!(has(&positive, "shadow-internals-semantics"));

        let mut host = node(1, "MY-CARD", "my-card");
        let mut root = node(2, "#shadow-root", "my-card > #shadow-root");
        child(
            &mut root,
            node(3, "BUTTON", "my-card > #shadow-root > button"),
        );
        child(&mut host, root);
        let negative = run_doc(&doc(vec![host], 1));
        assert!(!has(&negative, "shadow-internals-semantics"));

        let light = attrs(
            node(1, "DIV", "div"),
            &[("role", "button"), ("tabindex", "0")],
        );
        let keep = run_doc(&doc(vec![light], 1));
        assert!(!has(&keep, "shadow-internals-semantics"));
    }

    #[test]
    fn custom_element_tag_name_positive_negative_and_keep_when() {
        let invalid = node(1, "MY_WIDGET", "my_widget");
        let positive = run_doc(&doc(vec![invalid], 0));
        assert!(has(&positive, "custom-element-tag-name"));

        let builtin = attrs(node(1, "BUTTON", "button"), &[("is", "my-button")]);
        let legacy = run_doc(&doc(vec![builtin], 0));
        assert!(has(&legacy, "custom-element-tag-name"));

        let valid = node(1, "MY-WIDGET", "my-widget");
        let negative = run_doc(&doc(vec![valid], 0));
        assert!(!has(&negative, "custom-element-tag-name"));

        let plain = node(1, "DIV", "div");
        let keep = run_doc(&doc(vec![plain], 0));
        assert!(!has(&keep, "custom-element-tag-name"));
    }

    #[test]
    fn face_form_integration_positive_negative_and_keep_when() {
        let mut form = node(1, "FORM", "form");
        let mut host = node(2, "MY-RATING", "form > my-rating");
        let mut root = node(3, "#shadow-root", "form > my-rating > #shadow-root");
        child(
            &mut root,
            attrs(
                node(4, "INPUT", "input"),
                &[("type", "hidden"), ("name", "rating")],
            ),
        );
        child(&mut host, root);
        child(&mut form, host);
        let positive = run_doc(&doc(vec![form], 1));
        assert!(has(&positive, "face-form-integration"));

        let mut host = node(1, "MY-RATING", "my-rating");
        let mut root = node(2, "#shadow-root", "my-rating > #shadow-root");
        child(
            &mut root,
            attrs(
                node(3, "INPUT", "input"),
                &[("type", "hidden"), ("name", "rating")],
            ),
        );
        child(&mut host, root);
        let negative = run_doc(&doc(vec![host], 1));
        assert!(!has(&negative, "face-form-integration"));

        let mut form = node(1, "FORM", "form");
        let mut host = node(2, "MY-RATING", "form > my-rating");
        let mut root = node(3, "#shadow-root", "form > my-rating > #shadow-root");
        child(
            &mut root,
            attrs(
                node(4, "INPUT", "input"),
                &[("type", "text"), ("name", "rating")],
            ),
        );
        child(&mut host, root);
        child(&mut form, host);
        let keep = run_doc(&doc(vec![form], 1));
        assert!(!has(&keep, "face-form-integration"));
    }

    #[test]
    fn aria_cross_shadow_refs_positive_negative_and_keep_when() {
        let mut host = attrs(
            node(1, "MY-INPUT", "my-input"),
            &[("aria-labelledby", "inner-label")],
        );
        let mut root = node(2, "#shadow-root", "my-input > #shadow-root");
        child(
            &mut root,
            attrs(node(3, "SPAN", "span"), &[("id", "inner-label")]),
        );
        child(&mut host, root);
        let positive = run_doc(&doc(vec![host], 1));
        assert!(has(&positive, "aria-cross-shadow-refs"));
        let cross_shadow = positive
            .iter()
            .find(|line| line.check == "aria-cross-shadow-refs")
            .unwrap();
        assert!(
            cross_shadow
                .evidence
                .text
                .contains("#shadow-root de <my-input>")
        );

        let mut host = attrs(
            node(1, "MY-INPUT", "my-input"),
            &[("aria-labelledby", "outer-label")],
        );
        let root = node(2, "#shadow-root", "my-input > #shadow-root");
        child(&mut host, root);
        let label = attrs(node(3, "SPAN", "span"), &[("id", "outer-label")]);
        let negative = run_doc(&doc(vec![host, label], 1));
        assert!(!has(&negative, "aria-cross-shadow-refs"));

        let keep = attrs(node(1, "MY-INPUT", "my-input"), &[("aria-label", "Search")]);
        let keep_out = run_doc(&doc(vec![keep], 1));
        assert!(!has(&keep_out, "aria-cross-shadow-refs"));
    }

    #[test]
    fn declarative_shadow_prerender_positive_negative_and_keep_when() {
        let mut host = node(1, "MY-CARD", "my-card");
        let mut root = node(2, "#shadow-root", "my-card > #shadow-root");
        child(&mut root, node(3, "DIV", "div.card"));
        child(&mut host, root);
        let positive = run_doc(&doc(vec![host], 1));
        assert!(has(&positive, "declarative-shadow-prerender"));

        let mut host = node(1, "MY-CARD", "my-card");
        let mut root = node(2, "#shadow-root", "my-card > #shadow-root");
        child(&mut root, node(3, "STYLE", "style"));
        child(&mut root, node(4, "DIV", "div.card"));
        child(&mut host, root);
        let negative = run_doc(&doc(vec![host], 1));
        assert!(!has(&negative, "declarative-shadow-prerender"));

        let mut host = node(1, "MY-CARD", "my-card");
        let root = node(2, "#shadow-root", "my-card > #shadow-root");
        child(&mut host, root);
        let keep = run_doc(&doc(vec![host], 1));
        assert!(!has(&keep, "declarative-shadow-prerender"));
    }

    #[test]
    fn component_theming_boundary_positive_negative_and_keep_when() {
        let mut host = node(1, "MY-BUTTON", "my-button");
        let mut root = node(2, "#shadow-root", "my-button > #shadow-root");
        child(
            &mut root,
            style(
                node(3, "BUTTON", "button"),
                "typography",
                &[("font-family", "Arial")],
            ),
        );
        child(&mut host, root);
        let positive = run_doc(&doc(vec![host], 1));
        assert!(has(&positive, "component-theming-boundary"));

        let mut host = node(1, "MY-BUTTON", "my-button");
        let mut root = node(2, "#shadow-root", "my-button > #shadow-root");
        child(
            &mut root,
            style(
                node(3, "BUTTON", "button"),
                "typography",
                &[("font-family", "inherit")],
            ),
        );
        child(&mut host, root);
        let negative = run_doc(&doc(vec![host], 1));
        assert!(!has(&negative, "component-theming-boundary"));

        let light = style(
            node(1, "BUTTON", "button"),
            "typography",
            &[("font-family", "Arial")],
        );
        let keep = run_doc(&doc(vec![light], 1));
        assert!(!has(&keep, "component-theming-boundary"));
    }

    #[test]
    fn component_theming_public_property_override_is_detected() {
        let mut document = doc(Vec::new(), 1);
        let mut variables = Map::new();
        variables.insert("--card-bg".to_string(), Value::from("#ffffff"));
        document.css_variables = Some(variables);
        let mut host = node(1, "MY-CARD", "my-card");
        let mut root = node(2, "#shadow-root", "my-card > #shadow-root");
        child(
            &mut root,
            style(node(3, "DIV", "div"), "custom", &[("--card-bg", "#000000")]),
        );
        child(&mut host, root);
        document.nodes = vec![host];
        let positive = run_doc(&document);
        assert!(has(&positive, "component-theming-boundary"));

        let mut document = doc(Vec::new(), 1);
        let mut variables = Map::new();
        variables.insert("--card-bg".to_string(), Value::from("#ffffff"));
        document.css_variables = Some(variables);
        let mut host = node(1, "MY-CARD", "my-card");
        let mut root = node(2, "#shadow-root", "my-card > #shadow-root");
        child(
            &mut root,
            style(
                node(3, "DIV", "div"),
                "custom",
                &[("--card-bg", "#ffffff"), ("--_card-bg", "#000000")],
            ),
        );
        child(&mut host, root);
        document.nodes = vec![host];
        let keep = run_doc(&document);
        assert!(!has(&keep, "component-theming-boundary"));
    }

    #[test]
    fn output_is_byte_for_byte_deterministic() {
        let mut host = node(1, "MY-CARD", "my-card");
        let mut root = node(2, "#shadow-root", "my-card > #shadow-root");
        child(
            &mut root,
            attrs(
                node(3, "DIV", "div"),
                &[("role", "button"), ("tabindex", "0")],
            ),
        );
        child(
            &mut root,
            style(
                node(4, "BUTTON", "button"),
                "typography",
                &[("font-family", "Arial")],
            ),
        );
        child(&mut host, root);
        let document = doc(vec![host], 1);
        let first = serde_json::to_string(&run_doc(&document)).expect("serialize first run");
        let second = serde_json::to_string(&run_doc(&document)).expect("serialize second run");
        assert_eq!(first, second);
        assert!(first.contains("shadow-root-open-mode"));
    }

    #[test]
    fn missing_facets_silence_shadow_checks_but_not_tag_check() {
        let mut host = node(1, "MY_WIDGET", "my_widget");
        let mut root = node(2, "#shadow-root", "my_widget > #shadow-root");
        child(&mut root, node(3, "DIV", "div"));
        child(&mut host, root);
        let document = doc(vec![host], 0);
        let out = run_doc(&document);
        assert!(!has(&out, "shadow-root-open-mode"));
        assert!(!has(&out, "declarative-shadow-prerender"));
        assert!(has(&out, "custom-element-tag-name"));
    }
}
