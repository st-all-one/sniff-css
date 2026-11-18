//! Tipos compartilhados do contrato v2 (modernização).
//!
//! Aqui vivem apenas **dados**: facetas globais capturadas uma vez, tipos de
//! evidência e classificação de checks. Nenhuma lógica de checagem mora neste
//! módulo — ela fica em `sniff-css-check::modern`.
//!
//! Determinismo: todo mapa é `BTreeMap`, toda sequência é ordenada por
//! construção (DOM, ordem da configuração ou `(href, index, selector)`).

use crate::config::Viewport;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Id de um guia do `modern-web-guidance` (ex.: `native-dialog`).
pub type GuideRef = &'static str;

/// Severidade declarada de um check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Oportunidade: recomendação sem critério normativo obrigatório.
    Warn,
    /// Violação com critério WCAG/WAI-ARIA citado no guia.
    Fail,
}

/// Classe de observabilidade de um guia (ver `05-inventario-guias.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ObservabilityClass {
    /// Detectável com o snapshot atual.
    D,
    /// Detectável com extensão factível.
    F,
    /// Não observável na página renderizada.
    N,
    /// Já coberto por check existente.
    E,
}

/// Família de saída de um check: quebra existente ou oportunidade nova.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckClass {
    /// Quebra detectada pelas regras históricas (`rules`/`uniformity`).
    Bug,
    /// Dívida de plataforma detectada pelos checks `modern`.
    Opportunity,
}

/// Evidência estruturada de um check (contrato v2, quebra B3).
///
/// `text` é sempre determinístico e legível; os demais campos permitem
/// filtros sem regex (`jq 'select(.evidence.observed < .evidence.threshold)'`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// Frase determinística que descreve a evidência medida.
    pub text: String,
    /// Nome do indicador quando há medição numérica (ex.: `contrast_ratio`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<String>,
    /// Valor observado (número, string ou booleano).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed: Option<Value>,
    /// Limiar contra o qual o valor foi comparado.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<Value>,
    /// Operador da comparação (`>=`, `<=`, `==`, `!=`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparator: Option<String>,
    /// Facet que sustenta a afirmação (ex.: `contrast_facet`, `__document`).
    pub source: String,
}

impl Evidence {
    /// Evidência textual, sem limiar numérico.
    pub fn new(text: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            metric: None,
            observed: None,
            threshold: None,
            comparator: None,
            source: source.into(),
        }
    }

    /// Evidência numérica com indicador, valor observado, limiar e operador.
    #[expect(
        clippy::too_many_arguments,
        reason = "construtor único de evidência numérica: text/source/metric/observed/threshold/comparator"
    )]
    pub fn numeric(
        text: impl Into<String>,
        source: impl Into<String>,
        metric: impl Into<String>,
        observed: impl Into<Value>,
        threshold: impl Into<Value>,
        comparator: impl Into<String>,
    ) -> Self {
        Self {
            text: text.into(),
            metric: Some(metric.into()),
            observed: Some(observed.into()),
            threshold: Some(threshold.into()),
            comparator: Some(comparator.into()),
            source: source.into(),
        }
    }

    /// Constrói um `Evidence` a partir de valores já seriáveis (útil em
    /// parsers e checks derivados de JSON).
    #[expect(
        clippy::too_many_arguments,
        reason = "construtor de evidência a partir de partes já tipadas, usado por parsers"
    )]
    pub fn from_parts(
        text: impl Into<String>,
        source: impl Into<String>,
        metric: Option<String>,
        observed: Option<Value>,
        threshold: Option<Value>,
        comparator: Option<String>,
    ) -> Self {
        Self {
            text: text.into(),
            metric,
            observed,
            threshold,
            comparator,
            source: source.into(),
        }
    }
}

/// `<link>` do documento, capturado uma vez.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkInfo {
    /// Valor do atributo `rel`.
    pub rel: String,
    /// URL alvo, quando presente.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    /// Valor do atributo `as` (em `rel=preload`).
    #[serde(rename = "as", skip_serializing_if = "Option::is_none")]
    pub as_: Option<String>,
    /// Dica de prioridade declarada.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetchpriority: Option<String>,
    /// SRI, quando presente.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
    /// Valor de `crossorigin` (ex.: `anonymous` em preloads).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crossorigin: Option<String>,
}

/// `<script>` do documento, capturado uma vez.
#[expect(
    clippy::struct_excessive_bools,
    reason = "defer/async são atributos HTML independentes, não estados mutuamente exclusivos"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptInfo {
    /// URL do script, ausente em scripts inline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    /// `type` declarado (`module`, `importmap`, …).
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    /// Atributo `defer` presente.
    pub defer: bool,
    /// Atributo `async` presente.
    #[serde(rename = "async")]
    pub async_: bool,
    /// Dica de prioridade declarada.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetchpriority: Option<String>,
    /// SRI, quando presente.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
    /// Atributo `blocking` (ex.: `render`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<String>,
    /// O script está no `<head>` (relevante para bloqueio de render).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_head: Option<bool>,
    /// Conteúdo de scripts inline (limitado pelo extrator).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Atributo `importance` legado, quando presente.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub importance: Option<String>,
}

/// Folha de estilo do documento, capturada uma vez.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleSheetInfo {
    /// URL da folha, ausente em `<style>` inline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    /// Media query declarada (padrão `all`).
    pub media: String,
    /// `true` quando o acesso ao CSSOM lançou `SecurityError`.
    pub blocked: bool,
}

/// Contexto do documento — uma linha `__document` por captura.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentFacet {
    /// `<title>` do documento.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Idioma declarado em `<html lang>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// `location.origin` — âncora determinística para julgar URLs absolutas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// Nome do doctype (`html`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doctype: Option<String>,
    /// Metatags indexadas por `name` (ou `charset`/`http-equiv`).
    pub meta: BTreeMap<String, String>,
    /// `<link>`s na ordem do DOM.
    pub links: Vec<LinkInfo>,
    /// `<script>`s na ordem do DOM.
    pub scripts: Vec<ScriptInfo>,
    /// Folhas na ordem de `document.styleSheets`.
    pub style_sheets: Vec<StyleSheetInfo>,
    /// `window.isSecureContext`, quando capturado.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_secure_context: Option<bool>,
}

/// Dispositivo observado (normalizado, sem user-agent bruto).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DeviceFacet {
    /// Device pixel ratio arredondado a 2 casas.
    pub dpr: f64,
    /// Largura da viewport em CSS px.
    pub width: u32,
    /// Altura da viewport em CSS px.
    pub height: u32,
}

/// Preferências normalizadas e emulação pedida — uma linha `__env`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvFacet {
    /// Preferências lidas via `matchMedia`, em chaves canônicas.
    pub media: BTreeMap<String, String>,
    /// Emulação efetivamente aplicada (o que a CLI pediu).
    pub emulated: BTreeMap<String, String>,
    /// Dados de dispositivo normalizados.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceFacet>,
}

/// Regra CSSOM casada com um seletor capturado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredRule {
    /// Texto do seletor (`rule.selectorText`).
    pub selector: String,
    /// Folha de origem, `null` para inline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    /// Índice da regra dentro da folha.
    pub index: usize,
    /// Origem: `author`, `inline` ou `ua`.
    pub origin: String,
    /// Declarações casadas, em ordem alfabética.
    pub declarations: BTreeMap<String, String>,
}

/// Prelúdio de at-rule necessário para guias de `@supports`/`@container`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtRuleInfo {
    /// Texto da condição (ex.: `@supports (anchor-name: --x)`).
    pub condition_text: String,
    /// Índice da at-rule na folha.
    pub index: usize,
    /// Regras internas capturadas.
    pub rules: Vec<DeclaredRule>,
}

/// Origem das declarações — uma linha `__declared` (opt-in).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeclaredFacet {
    /// Regras casadas, ordenadas por `(href, index, selector)`.
    pub rules: Vec<DeclaredRule>,
    /// Folhas cujo CSSOM foi bloqueado por CORS.
    pub blocked_sheets: Vec<String>,
    /// Prelúdios de at-rules.
    pub at_rules: Vec<AtRuleInfo>,
    /// `true` quando `--declared-limit` foi atingido.
    pub truncated: bool,
}

/// Identificador de um estado da matriz de viewport/emulação.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixTag {
    /// Índice do estado (0-based, ordem da configuração).
    pub index: usize,
    /// Número total de estados esperados.
    pub total: usize,
    /// Viewport aplicada neste estado.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
    /// Emulação aplicada neste estado.
    pub emulation: BTreeMap<String, String>,
    /// Primeiro id de nó deste estado (inclusive), quando conhecido.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id_start: Option<u64>,
    /// Último id de nó deste estado (inclusive), quando conhecido.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id_end: Option<u64>,
}

/// Estatísticas derivadas do DOM em `sniff check` (sem facet de captura).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomStats {
    /// Total de nós na floresta.
    pub nodes: usize,
    /// Profundidade máxima.
    pub max_depth: usize,
    /// Contagem de `<img>`.
    pub images: usize,
    /// Imagens sem `width`/`height` declarados.
    pub images_without_dims: usize,
    /// Elementos interativos (nativo ou ARIA).
    pub interactive: usize,
    /// Interativos sem nome acessível.
    pub interactive_without_name: usize,
    /// Contagem de `<iframe>`.
    pub iframes: usize,
    /// Contagem de `<script>` no documento capturado.
    pub scripts: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_serialization_omits_absent_numeric_fields() {
        let value = serde_json::to_value(Evidence::new("ok", "attrs")).expect("serialize");
        let obj = value.as_object().expect("object");
        assert_eq!(obj.get("text").and_then(Value::as_str), Some("ok"));
        assert_eq!(obj.get("source").and_then(Value::as_str), Some("attrs"));
        assert!(!obj.contains_key("observed"));
        assert!(!obj.contains_key("threshold"));
        assert!(!obj.contains_key("metric"));
    }

    #[test]
    fn evidence_numeric_keeps_typed_fields() {
        let ev = Evidence::numeric(
            "ratio 2.1",
            "contrast_facet",
            "contrast_ratio",
            2.1,
            4.5,
            ">=",
        );
        assert_eq!(ev.observed, Some(serde_json::json!(2.1)));
        assert_eq!(ev.threshold, Some(serde_json::json!(4.5)));
        assert_eq!(ev.comparator.as_deref(), Some(">="));
    }

    #[test]
    fn facet_defaults_are_empty_and_deterministic() {
        let doc = DocumentFacet::default();
        assert!(doc.meta.is_empty());
        assert!(doc.links.is_empty());
        assert!(doc.scripts.is_empty());
        let env = EnvFacet::default();
        assert!(env.media.is_empty());
        assert!(env.emulated.is_empty());
    }
}
