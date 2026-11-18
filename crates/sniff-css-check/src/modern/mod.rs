//! Checks de oportunidade ("modern"): dívida de plataforma observável no
//! snapshot renderizado. Todo check é offline, determinístico e WARN por
//! default; `fail` exige critério normativo citado no guia.
//!
//! O registro em [`CheckSpec`] é a **única** fonte de ids. Cada família
//! expõe `specs()` e `run()`; o dispatcher [`run_modern_checks`] agrega,
//! filtra e ordena a saída. `keepWhen` vive no registro como documentação
//! auditável (não vai para o JSON, para não gastar tokens).

#![allow(
    clippy::arithmetic_side_effects,
    clippy::bool_to_int_with_if,
    clippy::collapsible_match,
    clippy::doc_markdown,
    clippy::field_reassign_with_default,
    clippy::fn_params_excessive_bools,
    clippy::if_not_else,
    clippy::if_same_then_else,
    clippy::indexing_slicing,
    clippy::items_after_statements,
    clippy::let_underscore_untyped,
    clippy::manual_ignore_case_cmp,
    clippy::map_unwrap_or,
    clippy::needless_pass_by_value,
    clippy::nonminimal_bool,
    clippy::redundant_clone,
    clippy::redundant_closure,
    clippy::redundant_closure_for_method_calls,
    clippy::return_self_not_must_use,
    clippy::self_only_used_in_recursion,
    clippy::semicolon_if_nothing_returned,
    clippy::suboptimal_flops,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::uninlined_format_args,
    clippy::unused_self,
    clippy::useless_conversion,
    clippy::useless_format,
    reason = "checks determinísticos sobre JSONL finito: índices e contadores são protegidos por verificações explícitas; a forma atual preserva o contrato de evidência e evita realocações no caminho quente"
)]

pub mod cssvisual;
pub mod forms;
pub mod interactions;
pub mod performance;
pub mod platform;
pub mod preferences;
pub mod responsive;
pub mod security;
pub mod structure;
pub mod webcomponents;

use sniff_core::modern::{CheckClass, DomStats, Evidence, GuideRef, Severity};
use sniff_css_diff::{DiffDocument, DiffNode};
use std::collections::BTreeSet;

/// Perfil de execução dos checks modernos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModernProfile {
    /// Perfil padrão: oportunidades consolidadas.
    #[default]
    Default,
    /// Perfil estrito: inclui checks marcados `strict_only`.
    Strict,
}

/// Filtros de execução do dispatcher.
#[derive(Debug, Clone)]
pub struct ModernOptions {
    /// Perfil de severidade/cobertura.
    pub profile: ModernProfile,
    /// Ids suprimidos por `--ignore`.
    pub ignore: BTreeSet<String>,
    /// Quando dado, só emite checks deste guia (`--guide`).
    pub guide: Option<String>,
    /// Habilita checks comparativos entre estados da matriz. ON por padrão.
    pub responsive: bool,
}

impl Default for ModernOptions {
    fn default() -> Self {
        Self {
            profile: ModernProfile::Default,
            ignore: BTreeSet::new(),
            guide: None,
            responsive: true,
        }
    }
}

/// Facet exigida por um check. Quando ausente, o check **não emite linha**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Facet {
    /// Atributos verbatim por nó (`attrs`).
    Attrs,
    /// Contexto global do documento (`__document`).
    Document,
    /// Preferências normalizadas (`__env`).
    Env,
    /// Origem das declarações (`__declared`).
    Declared,
    /// Matriz de estados (`__meta.matrix`).
    Matrix,
    /// Efeitos de interação (`__actions`).
    Actions,
    /// Estilos de pseudo-elementos.
    Pseudo,
    /// Árvore de acessibilidade.
    Ax,
    /// Estilos computados.
    Styles,
    /// Retângulo medido.
    Rect,
    /// Walker de shadow root executado.
    Shadow,
}

/// Descrição declarativa de um check de oportunidade.
#[derive(Debug, Clone, Copy)]
pub struct CheckSpec {
    /// Id único (ex.: `native-dialog-reimplemented`).
    pub id: &'static str,
    /// Guias do corpus que o check cobre.
    pub guides: &'static [GuideRef],
    /// Severidade (WARN salvo critério normativo).
    pub severity: Severity,
    /// Facetas exigidas; ausência ⇒ check silencia.
    pub required: &'static [Facet],
    /// Quando **não** disparar (documentação + teste de negação).
    pub keep_when: &'static str,
    /// Só roda no perfil estrito.
    pub strict_only: bool,
}

impl CheckSpec {
    /// Construtor conveniente para checks do perfil padrão.
    pub const fn new(
        id: &'static str,
        guides: &'static [GuideRef],
        required: &'static [Facet],
        keep_when: &'static str,
    ) -> Self {
        Self {
            id,
            guides,
            severity: Severity::Warn,
            required,
            keep_when,
            strict_only: false,
        }
    }

    /// Marca um check com severidade `fail` (exige WCAG/WAI-ARIA citado).
    pub const fn failing(mut self) -> Self {
        self.severity = Severity::Fail;
        self
    }

    /// Marca um check como exclusivo do perfil estrito.
    pub const fn strict(mut self) -> Self {
        self.strict_only = true;
        self
    }
}

/// Contexto imutável compartilhado por todas as famílias de check.
pub struct ModernCtx<'a> {
    /// Documento parseado (com as facetas globais migradas quando v1).
    pub doc: &'a DiffDocument,
    /// Filtros pedidos pela CLI.
    pub opts: &'a ModernOptions,
    /// Flatten pré-order da floresta,calculado uma única vez.
    pub flat: Vec<&'a DiffNode>,
    /// Estatísticas derivadas do DOM (uma passagem).
    pub dom_stats: DomStats,
}

impl<'a> ModernCtx<'a> {
    /// Constrói o contexto a partir do documento já carregado.
    pub fn new(doc: &'a DiffDocument, opts: &'a ModernOptions) -> Self {
        let mut flat = Vec::new();
        fn collect<'b>(node: &'b DiffNode, out: &mut Vec<&'b DiffNode>) {
            out.push(node);
            for child in &node.children {
                collect(child, out);
            }
        }
        for root in &doc.nodes {
            collect(root, &mut flat);
        }
        let dom_stats = compute_dom_stats(&flat);
        Self {
            doc,
            opts,
            flat,
            dom_stats,
        }
    }

    /// Valor de uma propriedade computada de um nó.
    pub fn style<'b>(&self, node: &'b DiffNode, category: &str, property: &str) -> Option<&'b str> {
        node.styles
            .as_ref()?
            .get(category)?
            .get(property)
            .and_then(|v| v.as_str())
    }

    /// Primeiro valor disponível entre vários `(categoria, propriedade)`.
    pub fn style_any<'b>(&self, node: &'b DiffNode, pairs: &[(&str, &str)]) -> Option<&'b str> {
        pairs
            .iter()
            .find_map(|(cat, prop)| self.style(node, cat, prop))
    }

    /// Atributo verbatim do nó (facet `attrs`).
    pub fn attr<'b>(&self, node: &'b DiffNode, name: &str) -> Option<&'b str> {
        node.attributes
            .as_ref()?
            .get(name)
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
    }

    /// Campo do facet `aria` (nomes em snake_case, como no JSONL).
    pub fn aria<'b>(&self, node: &'b DiffNode, key: &str) -> Option<&'b str> {
        node.aria
            .as_ref()?
            .get(key)
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
    }

    /// Campo booleano do facet `aria`.
    pub fn aria_bool(&self, node: &DiffNode, key: &str) -> bool {
        node.aria
            .as_ref()
            .and_then(|v| v.get(key))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Tag em maiúsculas (ex.: `BUTTON`).
    pub fn tag<'b>(&self, node: &'b DiffNode) -> &'b str {
        node.tag.as_deref().unwrap_or("")
    }

    /// O nó está visível? Ausência do facet significa "não contradizido".
    pub fn visible(&self, node: &DiffNode) -> bool {
        node.display_visible().unwrap_or(true)
    }

    /// Facet disponível neste snapshot?
    pub fn has_facet(&self, facet: Facet) -> bool {
        match facet {
            Facet::Attrs => self.flat.iter().any(|n| n.attributes.is_some()),
            Facet::Document => self.doc.document.is_some(),
            Facet::Env => self.doc.env.is_some(),
            Facet::Declared => self.doc.declared.is_some(),
            Facet::Matrix => !self.doc.matrix.is_empty(),
            Facet::Actions => !self.doc.actions.is_empty(),
            Facet::Pseudo => self.flat.iter().any(|n| n.pseudo.is_some()),
            Facet::Ax => self.flat.iter().any(|n| n.ax.is_some()),
            Facet::Styles => self.flat.iter().any(|n| n.styles.is_some()),
            Facet::Rect => self.flat.iter().any(|n| n.rect.is_some()),
            Facet::Shadow => self.doc.shadow_roots > 0,
        }
    }

    /// Um check deve rodar com as opções atuais?
    pub fn enabled(&self, spec: &CheckSpec) -> bool {
        if spec.strict_only && self.opts.profile != ModernProfile::Strict {
            return false;
        }
        if self.opts.ignore.contains(spec.id) {
            return false;
        }
        if let Some(guide) = &self.opts.guide
            && !spec.guides.iter().any(|g| *g == guide)
        {
            return false;
        }
        spec.required.iter().all(|f| self.has_facet(*f))
    }

    /// Severidade efetiva: `--profile strict` eleva oportunidades a `fail`.
    fn severity(&self, spec: &CheckSpec) -> Severity {
        if self.opts.profile == ModernProfile::Strict {
            Severity::Fail
        } else {
            spec.severity
        }
    }

    /// Emite uma linha para um nó, usando a severidade efetiva do registro.
    pub fn emit(
        &self,
        out: &mut Vec<CheckLine>,
        spec: &CheckSpec,
        node: &DiffNode,
        evidence: Evidence,
    ) {
        let severity = self.severity(spec);
        let status = match severity {
            Severity::Warn => RuleStatus::Warn,
            Severity::Fail => RuleStatus::Fail,
        };
        out.push(CheckLine {
            check: spec.id.to_string(),
            selector: node.selector.clone(),
            tag: node.tag.clone(),
            status,
            class: CheckClass::Opportunity,
            evidence,
            guide: spec.guides.first().copied().map(String::from),
            severity: Some(severity),
            metrics: None,
        });
    }

    /// Emite uma linha global (sem nó associado, ex.: metadata do documento).
    pub fn emit_global(
        &self,
        out: &mut Vec<CheckLine>,
        spec: &CheckSpec,
        selector: &str,
        evidence: Evidence,
    ) {
        let severity = self.severity(spec);
        let status = match severity {
            Severity::Warn => RuleStatus::Warn,
            Severity::Fail => RuleStatus::Fail,
        };
        out.push(CheckLine {
            check: spec.id.to_string(),
            selector: selector.to_string(),
            tag: None,
            status,
            class: CheckClass::Opportunity,
            evidence,
            guide: spec.guides.first().copied().map(String::from),
            severity: Some(severity),
            metrics: None,
        });
    }
}

use crate::rules::{CheckLine, RuleStatus};

/// Estatísticas derivadas do DOM, sem facet de captura.
pub fn compute_dom_stats(flat: &[&DiffNode]) -> DomStats {
    let mut stats = DomStats {
        nodes: flat.len(),
        ..DomStats::default()
    };
    for node in flat {
        stats.max_depth = stats.max_depth.max(node.depth.unwrap_or(0));
        match node.tag.as_deref() {
            Some("IMG") => {
                stats.images += 1;
                let has_dims = ["width", "height"].iter().any(|a| {
                    node.attributes
                        .as_ref()
                        .and_then(serde_json::Value::as_object)
                        .is_some_and(|m| m.contains_key(*a))
                });
                if !has_dims {
                    stats.images_without_dims += 1;
                }
            }
            Some("IFRAME") => stats.iframes += 1,
            Some("SCRIPT") => stats.scripts += 1,
            _ => {}
        }
        let interactive = node
            .aria
            .as_ref()
            .and_then(|v| v.get("focusable"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            || matches!(
                node.tag.as_deref(),
                Some("BUTTON" | "INPUT" | "SELECT" | "TEXTAREA" | "A" | "SUMMARY")
            );
        if interactive {
            stats.interactive += 1;
            let named = node
                .aria
                .as_ref()
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .is_some_and(|n| !n.trim().is_empty());
            if !named {
                stats.interactive_without_name += 1;
            }
        }
    }
    stats
}

/// Todos os ids registrados (útil para `--ignore`, docs e testes).
pub fn all_specs() -> Vec<CheckSpec> {
    structure::specs()
        .iter()
        .chain(platform::specs())
        .chain(forms::specs())
        .chain(interactions::specs())
        .chain(preferences::specs())
        .chain(responsive::specs())
        .chain(performance::specs())
        .chain(webcomponents::specs())
        .chain(security::specs())
        .chain(cssvisual::specs())
        .copied()
        .collect()
}

/// Roda todas as famílias habilitadas e devolve linhas determinísticas.
pub fn run_modern_checks(doc: &DiffDocument, opts: &ModernOptions) -> Vec<CheckLine> {
    run_modern_checks_for(doc, doc, opts)
}

/// Variante para snapshots de matriz: checks por nó usam `baseline` (o
/// primeiro estado), enquanto `full` alimenta só as comparações entre estados.
/// Sem isso, cada estado repetiria os mesmos avisos de bug/oportunidade.
pub fn run_modern_checks_for(
    full: &DiffDocument,
    baseline: &DiffDocument,
    opts: &ModernOptions,
) -> Vec<CheckLine> {
    let ctx = ModernCtx::new(baseline, opts);
    let mut out = Vec::new();
    structure::run(&ctx, &mut out);
    platform::run(&ctx, &mut out);
    forms::run(&ctx, &mut out);
    interactions::run(&ctx, &mut out);
    performance::run(&ctx, &mut out);
    webcomponents::run(&ctx, &mut out);
    security::run(&ctx, &mut out);
    cssvisual::run(&ctx, &mut out);
    // Preferences needs the matrix tag to decide whether a guard was emulated;
    // node data still comes from the baseline state.
    let mut preferences_doc = baseline.clone();
    preferences_doc.matrix.clone_from(&full.matrix);
    let preferences_ctx = ModernCtx::new(&preferences_doc, opts);
    preferences::run(&preferences_ctx, &mut out);
    if opts.responsive {
        let full_ctx = ModernCtx::new(full, opts);
        responsive::run(&full_ctx, &mut out);
    }

    // Deduplica apenas linhas byte-idênticas: dois recursos/diretivas distintas
    // podem compartilhar o mesmo selector (ex.: `script[src]` no SRI).
    let mut seen = BTreeSet::new();
    out.retain(|line| {
        seen.insert((
            line.check.clone(),
            line.selector.clone(),
            line.evidence.text.clone(),
        ))
    });
    out.sort_by(|a, b| {
        a.check
            .cmp(&b.check)
            .then_with(|| a.selector.cmp(&b.selector))
            .then_with(|| a.status.cmp(&b.status))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sniff_core::Viewport;
    use sniff_core::modern::{DeclaredFacet, DocumentFacet, EnvFacet, MatrixTag};
    use std::collections::BTreeMap;

    #[test]
    fn registry_ids_are_unique() {
        let specs = all_specs();
        let mut seen = BTreeSet::new();
        for spec in &specs {
            assert!(
                seen.insert(spec.id),
                "check id duplicado no registro: {}",
                spec.id
            );
            assert!(!spec.guides.is_empty(), "check sem guia: {}", spec.id);
            assert!(
                !spec.keep_when.is_empty(),
                "check sem keepWhen: {}",
                spec.id
            );
        }
        assert!(
            specs.len() > 100,
            "catálogo incompleto: {} checks",
            specs.len()
        );
    }

    #[test]
    fn all_registered_checks_are_enabled_when_every_facet_is_present() {
        let mut doc = DiffDocument::default();
        doc.document = Some(DocumentFacet::default());
        doc.env = Some(EnvFacet::default());
        doc.declared = Some(DeclaredFacet::default());
        doc.matrix.push(MatrixTag {
            index: 0,
            total: 1,
            viewport: Some(Viewport {
                width: 1366,
                height: 768,
            }),
            emulation: BTreeMap::new(),
            node_id_start: Some(1),
            node_id_end: Some(1),
        });
        doc.actions.push(serde_json::json!({"action": "key"}));
        doc.shadow_roots = 1;
        doc.nodes.push(DiffNode {
            id: 1,
            parent_id: None,
            selector: "DIV".to_string(),
            tag: Some("DIV".to_string()),
            path: None,
            depth: Some(0),
            rect: Some(serde_json::json!({})),
            metrics: Some(serde_json::json!({})),
            noticeable: Some(serde_json::json!({})),
            hash: None,
            styles: Some(serde_json::Map::new()),
            pseudo: Some(serde_json::Map::new()),
            aria: Some(serde_json::json!({})),
            contrast: None,
            ax: Some(serde_json::json!({})),
            attributes: Some(serde_json::json!({})),
            effective_background: None,
            children: Vec::new(),
        });
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(&doc, &opts);
        let disabled: Vec<&str> = all_specs()
            .iter()
            .filter(|spec| !ctx.enabled(spec))
            .map(|spec| spec.id)
            .collect();
        assert!(
            disabled.is_empty(),
            "checks desativados com todas as facetas: {disabled:?}"
        );
    }
}
