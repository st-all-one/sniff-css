//! Checks da Família C — formulários, autofill e seletores customizáveis.
//!
//! Todos os sinais vêm de campos do snapshot: atributos verbatim (`attrs`),
//! ARIA resolvido, estilos computados, CSSOM declarado, retângulos e ações.
//! Nenhum check recomenda remover progressive enhancement: `autocomplete`,
//! `:user-invalid`, `appearance: base-select`, `accent-color`, `field-sizing`
//! e `:autofill` são melhorias que degradam para texto/constraints nativas.

use crate::modern::{CheckSpec, Facet, ModernCtx};
use crate::rules::CheckLine;
use serde_json::{Value, json};
use sniff_core::contrast::{contrast_ratio, parse_color};
use sniff_core::modern::{DeclaredFacet, Evidence, Severity};
use sniff_css_diff::DiffNode;
use std::collections::{BTreeMap, BTreeSet};

/// Campos que coletam dados pessoais e exigem token de propósito (WCAG 1.3.5).
const AUTOFILL_PURPOSE: CheckSpec = CheckSpec::new(
    "autofill-purpose-tokens",
    &[
        "autofill-address-form",
        "autofill-sign-in-form",
        "autofill-sign-up-form",
        "forms",
    ],
    &[Facet::Attrs],
    "Silenciar em honeypot (aria-hidden/tabindex=-1), autocomplete=off deliberado, campo sem propósito pessoal inferível e credenciais/pagamento/OTP (cobertos por checks próprios).",
)
.failing();

/// Tokens de credencial em formulários de entrada e cadastro.
const CREDENTIAL_TOKENS: CheckSpec = CheckSpec::new(
    "credential-autocomplete-tokens",
    &["autofill-sign-in-form", "autofill-sign-up-form"],
    &[Facet::Attrs],
    "Silenciar em fluxo passkey-only, login federado sem senha e recuperação/reset (onde new-password é correto); qualquer token válido é aceito, inclusive a heurística do gestor de senhas.",
);

/// Cartão/telefone de pagamento devem usar um único input de texto.
const PAYMENT_SINGLE_INPUT: CheckSpec = CheckSpec::new(
    "payment-field-single-input",
    &["autofill-payment-form"],
    &[Facet::Attrs],
    "Silenciar em campos hospedados por iframe de PSP (fora da árvore), em cc-name com type=text e quando não há token de pagamento; não exigir pattern em todos os campos.",
);

/// Hint de formato deve estar acima do campo e ligado por `aria-describedby`.
const HINT_ABOVE_INPUT: CheckSpec = CheckSpec::new(
    "hint-above-input",
    &[
        "autofill-payment-form",
        "required-field-feedback",
        "sms-otp-form",
        "validate-input-after-interaction",
    ],
    &[Facet::Attrs, Facet::Rect],
    "Silenciar em hint visualmente oculto (0x0, aceito para leitores de tela), em campo sem formato restrito e quando placeholder apenas complementa label + hint persistente.",
);

/// `aria-invalid` deve acompanhar o erro visível/referenceado após interação.
const USER_INVALID_SYNC: CheckSpec = CheckSpec::new(
    "user-invalid-aria-sync",
    &[
        "required-field-feedback",
        "select-menu-interaction",
        "validate-input-after-interaction",
        "accessible-error-announcement",
    ],
    &[Facet::Attrs],
    "Silenciar em campo não interagido sem erro visível, em novalidate, quando o valor veio do servidor e quando o fallback .user-invalid-fallback está presente; não exigir erro no load.",
);

/// OTP de SMS deve ser um único input textual com inputmode numérico.
const OTP_SINGLE_INPUT: CheckSpec = CheckSpec::new(
    "otp-single-input",
    &["sms-otp-form"],
    &[Facet::Attrs],
    "Silenciar em PIN local sem SMS e em fluxo passkey/email (não observável); não exigir WebOTP/OTPCredential nem confundir letter-spacing visual com múltiplos inputs.",
);

/// `<select>` estilizado precisa de name, label e opções com valor utilizável.
const SELECT_BASE_SEMANTICS: CheckSpec = CheckSpec::new(
    "select-base-semantics",
    &["branded-select-styling", "rich-media-picker"],
    &[Facet::Attrs, Facet::Styles],
    "Silenciar quando appearance não é base-select (dropdown nativo), quando o texto da opção é o valor enviado por omissão e quando aria-hidden está em SVG decorativo; não exigir estilização do picker onde não há suporte.",
);

/// Placeholder de `<select required>` não pode ser selecionável.
const SELECT_PLACEHOLDER_OPTION: CheckSpec = CheckSpec::new(
    "select-placeholder-option",
    &["select-menu-interaction"],
    &[Facet::Attrs],
    "Silenciar em select multiple, quando a opção vazia usa hidden em vez de disabled e quando há valor padrão válido (sem placeholder).",
);

/// Estado selecionado de opção rica não pode depender só de cor.
const SELECT_OPTION_NONCOLOR: CheckSpec = CheckSpec::new(
    "select-option-noncolor-state",
    &["rich-media-picker"],
    &[Facet::Attrs, Facet::Pseudo, Facet::Styles],
    "Silenciar quando appearance não é base-select, quando ::checkmark/selectedcontent fornece indicador não-colorido e quando aria-checked ou texto visually-hidden já tornam o estado inteligível.",
);

/// Animação do picker precisa de `@starting-style` e guarda reduced-motion.
const SELECT_PICKER_ANIMATION: CheckSpec = CheckSpec::new(
    "select-picker-animation-guard",
    &["animated-select-picker"],
    &[Facet::Styles, Facet::Declared, Facet::Pseudo],
    "Silenciar quando appearance não é base-select, quando a transição não envolve display/overlay (ex.: só opacity) e quando @media (prefers-reduced-motion: reduce) já zera a transição; preservar o degrade para dropdown nativo.",
);

/// Layout do picker aberto precisa da pseudoclasse `:open`.
const SELECT_PICKER_OPEN_LAYOUT: CheckSpec = CheckSpec::new(
    "select-picker-open-layout",
    &["custom-select-picker-layouts"],
    &[Facet::Styles, Facet::Declared],
    "Silenciar no picker nativo do SO (appearance != base-select) e quando appearance:none apenas troca a seta com role nativo preservado.",
);

/// Destaque de autofill precisa de indicador não-colorido na regra CSSOM.
const AUTOFILL_NONCOLOR: CheckSpec = CheckSpec::new(
    "autofill-noncolor-indicator",
    &["autofill-highlight-inputs"],
    &[Facet::Declared, Facet::Pseudo, Facet::Attrs],
    "Silenciar quando não há regra :autofill (highlight padrão do browser), em checkbox/radio/file/button/contenteditable/FACE; julgar a regra, não o estado efêmero, e preservar :is(:autofill, :-webkit-autofill) sem fallback JS.",
);

/// Cor de acento precisa de contraste 3:1 contra o fundo efetivo.
const ACCENT_COLOR_CONTRAST: CheckSpec = CheckSpec::new(
    "accent-color-contrast",
    &["brand-consistent-forms"],
    &[Facet::Styles],
    "Silenciar em controle disabled, quando accent-color é auto/ausente, quando o fundo efetivo é imagem/indisponível e quando @supports not traz fallback .visually-hidden válido; manter o fallback e o aviso de bugs de implementação.",
)
.failing();

/// Variante de aviso para razão fronteiriça (2,9–3,1) — bugs de implementação.
const ACCENT_BORDERLINE: CheckSpec = CheckSpec {
    severity: Severity::Warn,
    ..ACCENT_COLOR_CONTRAST
};

/// `field-sizing: content` exige intervalos de tamanho, não width fixo.
const FIELD_SIZING_CONSTRAINTS: CheckSpec = CheckSpec::new(
    "field-sizing-constraints",
    &["form-fields-automatically-fit-contents"],
    &[Facet::Styles, Facet::Attrs],
    "Silenciar em browser sem field-sizing (fallback de tamanho fixo é legítimo), quando há min/max-inline-size e em textarea com width:50ch intencional; não exigir o workaround grid + data-replicated-value.",
);

/// Cada estado de interação deve ser visualmente distinto.
const INTERACTION_STATE_FEEDBACK: CheckSpec = CheckSpec::new(
    "interaction-state-feedback",
    &["natural-interactions"],
    &[Facet::Styles, Facet::Actions, Facet::Declared],
    "Silenciar em controles nativos com feedback do browser, não exigir filter:blur, não duplicar focus-indicator e respeitar prefers-reduced-motion; não exigir position-area onde não há suporte.",
);

/// Barras persistentes e avisos não-modais têm orçamento de viewport.
const PERSISTENT_BAR_BUDGET: CheckSpec = CheckSpec::new(
    "persistent-bar-budget",
    &["content-first-noise-reduction"],
    &[Facet::Rect, Facet::Styles],
    "Silenciar em barra legalmente obrigatória com ≤15% e dismiss visível, em z-index legítimo e quando :focus-within mantém a barra visível; o fallback sem scroll-state queries é aceito.",
);

/// Registro completo da Família C (ids do catálogo §4.5).
const SPECS: &[CheckSpec] = &[
    AUTOFILL_PURPOSE,
    CREDENTIAL_TOKENS,
    PAYMENT_SINGLE_INPUT,
    HINT_ABOVE_INPUT,
    USER_INVALID_SYNC,
    OTP_SINGLE_INPUT,
    SELECT_BASE_SEMANTICS,
    SELECT_PLACEHOLDER_OPTION,
    SELECT_OPTION_NONCOLOR,
    SELECT_PICKER_ANIMATION,
    SELECT_PICKER_OPEN_LAYOUT,
    AUTOFILL_NONCOLOR,
    ACCENT_COLOR_CONTRAST,
    FIELD_SIZING_CONSTRAINTS,
    INTERACTION_STATE_FEEDBACK,
    PERSISTENT_BAR_BUDGET,
];

/// Especificações de todos os checks de formulários do catálogo.
pub fn specs() -> &'static [CheckSpec] {
    SPECS
}

/// Executa os checks habilitados da Família C sobre o contexto compartilhado.
pub fn run(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    let index = DomIndex::new(&ctx.flat);
    for spec in specs() {
        if !ctx.enabled(spec) {
            tracing::trace!(check = spec.id, "forms check silenced by options or facets");
            continue;
        }
        let before = out.len();
        match spec.id {
            "autofill-purpose-tokens" => autofill_purpose(ctx, &index, out, spec),
            "credential-autocomplete-tokens" => credential_tokens(ctx, &index, out, spec),
            "payment-field-single-input" => payment_single_input(ctx, &index, out, spec),
            "hint-above-input" => hint_above_input(ctx, &index, out, spec),
            "user-invalid-aria-sync" => user_invalid_sync(ctx, &index, out, spec),
            "otp-single-input" => otp_single_input(ctx, &index, out, spec),
            "select-base-semantics" => select_base_semantics(ctx, &index, out, spec),
            "select-placeholder-option" => select_placeholder_option(ctx, &index, out, spec),
            "select-option-noncolor-state" => select_option_noncolor(ctx, &index, out, spec),
            "select-picker-animation-guard" => select_picker_animation(ctx, &index, out, spec),
            "select-picker-open-layout" => select_picker_open_layout(ctx, &index, out, spec),
            "autofill-noncolor-indicator" => autofill_noncolor(ctx, &index, out, spec),
            "accent-color-contrast" => accent_color_contrast(ctx, out, spec),
            "field-sizing-constraints" => field_sizing_constraints(ctx, &index, out, spec),
            "interaction-state-feedback" => interaction_state_feedback(ctx, &index, out, spec),
            "persistent-bar-budget" => persistent_bar_budget(ctx, &index, out, spec),
            _ => tracing::warn!(check = spec.id, "unrouted forms check id"),
        }
        if out.len() > before {
            tracing::debug!(
                check = spec.id,
                emitted = out.len() - before,
                "forms check emitted lines"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Índice derivado do DOM (uma passagem; sem HashMap, ordem determinística)
// ---------------------------------------------------------------------------

/// Relações de parentesco e ids do DOM derivadas do flatten pré-ordem.
struct DomIndex<'a> {
    by_id: BTreeMap<u64, &'a DiffNode>,
    children: BTreeMap<u64, Vec<&'a DiffNode>>,
    by_dom_id: BTreeMap<&'a str, &'a DiffNode>,
}

impl<'a> DomIndex<'a> {
    fn new(flat: &[&'a DiffNode]) -> Self {
        let mut by_id = BTreeMap::new();
        let mut children: BTreeMap<u64, Vec<&'a DiffNode>> = BTreeMap::new();
        let mut by_dom_id = BTreeMap::new();
        for &node in flat {
            by_id.insert(node.id, node);
            if let Some(parent) = node.parent_id {
                children.entry(parent).or_default().push(node);
            }
            if let Some(id) = raw_attr(node, "id") {
                by_dom_id.insert(id, node);
            }
        }
        Self {
            by_id,
            children,
            by_dom_id,
        }
    }

    fn kids(&self, node: &DiffNode) -> &[&'a DiffNode] {
        self.children.get(&node.id).map_or(&[], Vec::as_slice)
    }

    fn ancestor(&self, node: &DiffNode, tag: &str) -> Option<&'a DiffNode> {
        let mut id = node.id;
        loop {
            let current = *self.by_id.get(&id)?;
            if current.tag.as_deref() == Some(tag) {
                return Some(current);
            }
            id = current.parent_id?;
        }
    }

    fn dom_id(&self, id: &str) -> Option<&'a DiffNode> {
        self.by_dom_id.get(id).copied()
    }

    fn described(&self, node: &DiffNode) -> Vec<&'a DiffNode> {
        node.aria
            .as_ref()
            .and_then(|v| v.get("aria_describedby"))
            .and_then(Value::as_str)
            .map(|ids| {
                ids.split_whitespace()
                    .filter_map(|id| self.dom_id(id))
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Helpers de leitura (sem unwrap/expect em caminho de biblioteca)
// ---------------------------------------------------------------------------

/// Atributo verbatim, inclusive valor vazio (necessário para `value=""`).
fn raw_attr<'a>(node: &'a DiffNode, name: &str) -> Option<&'a str> {
    node.attributes.as_ref()?.get(name).and_then(Value::as_str)
}

fn has_attr(node: &DiffNode, name: &str) -> bool {
    node.attributes
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|m| m.contains_key(name))
}

fn attr_is(node: &DiffNode, name: &str, expected: &str) -> bool {
    raw_attr(node, name).is_some_and(|v| v.eq_ignore_ascii_case(expected))
}

fn class_has(node: &DiffNode, token: &str) -> bool {
    raw_attr(node, "class")
        .is_some_and(|v| v.split_whitespace().any(|candidate| candidate == token))
}

fn aria_is(node: &DiffNode, key: &str, expected: &str) -> bool {
    node.aria
        .as_ref()
        .and_then(|v| v.get(key))
        .and_then(Value::as_str)
        .is_some_and(|v| v.eq_ignore_ascii_case(expected))
}

fn aria_has_text(node: &DiffNode) -> bool {
    node.aria
        .as_ref()
        .and_then(|v| v.get("has_text"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn pseudo_present(node: &DiffNode, name: &str) -> bool {
    node.pseudo.as_ref().is_some_and(|m| m.contains_key(name))
}

fn pseudo_style<'a>(
    node: &'a DiffNode,
    pseudo: &str,
    category: &str,
    property: &str,
) -> Option<&'a str> {
    node.pseudo
        .as_ref()?
        .get(pseudo)?
        .get(category)?
        .get(property)?
        .as_str()
}

fn rect(node: &DiffNode) -> Option<(f64, f64, f64, f64)> {
    let value = node.rect.as_ref()?;
    Some((
        value.get("x")?.as_f64()?,
        value.get("y")?.as_f64()?,
        value.get("width")?.as_f64()?,
        value.get("height")?.as_f64()?,
    ))
}

/// Cor de fundo efetiva já composta na captura (`effective_background`).
fn effective_background(node: &DiffNode) -> Option<&str> {
    let value = node.effective_background.as_ref()?;
    let text = match value {
        Value::String(s) => s.as_str(),
        Value::Object(map) => map
            .get("color")
            .and_then(Value::as_str)
            .or_else(|| map.get("value").and_then(Value::as_str))?,
        _ => return None,
    };
    let unknown = text.is_empty()
        || text.eq_ignore_ascii_case("image")
        || text.eq_ignore_ascii_case("unknown")
        || text.eq_ignore_ascii_case("none");
    (!unknown).then_some(text)
}

fn is_form_control(tag: &str) -> bool {
    matches!(tag, "INPUT" | "SELECT" | "TEXTAREA")
}

fn is_honeypot(node: &DiffNode) -> bool {
    aria_is(node, "aria_hidden", "true") || attr_is(node, "tabindex", "-1")
}

fn is_autofillable_input(node: &DiffNode) -> bool {
    if ctx_tag_is(node, "INPUT") {
        let kind = raw_attr(node, "type").unwrap_or_default();
        return !matches!(
            kind.to_ascii_lowercase().as_str(),
            "checkbox"
                | "radio"
                | "file"
                | "button"
                | "submit"
                | "reset"
                | "image"
                | "hidden"
                | "color"
        );
    }
    ctx_tag_is(node, "SELECT") || ctx_tag_is(node, "TEXTAREA")
}

fn ctx_tag_is(node: &DiffNode, tag: &str) -> bool {
    node.tag.as_deref() == Some(tag)
}

/// `true` quando `token` aparece como pseudoclasse/seletor delimitado.
fn contains_token(text: &str, token: &str) -> bool {
    let haystack = text.to_ascii_lowercase();
    let needle = token.to_ascii_lowercase();
    let boundary =
        |ch: Option<char>| ch.is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    let mut from = 0;
    while let Some(offset) = haystack[from..].find(&needle) {
        let start = from + offset;
        let after = haystack[start + needle.len()..].chars().next();
        // `:open` etc. are valid immediately after an identifier in a
        // selector (`select:open`), so only the right edge matters.
        let before_ok = needle.starts_with(':') || boundary(haystack[..start].chars().next_back());
        if before_ok && boundary(after) {
            return true;
        }
        from = start + needle.len();
    }
    false
}

fn token_in(value: Option<&str>, allowed: &[&str]) -> bool {
    value.is_some_and(|v| allowed.iter().any(|token| v.eq_ignore_ascii_case(token)))
}

fn appearance_base(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    ctx.style_any(
        node,
        &[("interaction", "appearance"), ("visual", "appearance")],
    )
    .is_some_and(|v| v.eq_ignore_ascii_case("base-select"))
}

fn declared_at(decl: &DeclaredFacet, condition: &str, selector: Option<&str>) -> bool {
    let needle = condition.to_ascii_lowercase();
    decl.at_rules.iter().any(|at| {
        at.condition_text.to_ascii_lowercase().contains(&needle)
            && selector
                .is_none_or(|wanted| at.rules.iter().any(|rule| rule.selector.contains(wanted)))
    })
}

fn reduced_motion_guard(decl: &DeclaredFacet) -> bool {
    decl.at_rules.iter().any(|at| {
        let condition = at.condition_text.to_ascii_lowercase();
        condition.contains("prefers-reduced-motion")
            && condition.contains("reduce")
            && at.rules.iter().any(|rule| {
                rule.declarations.iter().any(|(property, value)| {
                    let value = value.to_ascii_lowercase();
                    (property == "transition" && (value.contains("none") || value.contains("0s")))
                        || (property == "transition-duration" && value.contains("0s"))
                })
            })
    })
}

fn rule_matches_node(selector: &str, node: &DiffNode) -> bool {
    let lower = selector.to_ascii_lowercase();
    if lower.contains('*') {
        return true;
    }
    if let Some(id) = raw_attr(node, "id")
        && !id.is_empty()
        && lower.contains(&format!("#{id}"))
    {
        return true;
    }
    if let Some(classes) = raw_attr(node, "class")
        && classes
            .split_whitespace()
            .any(|class| lower.contains(&format!(".{class}")))
    {
        return true;
    }
    node.tag.as_deref().is_some_and(|tag| {
        lower
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-'))
            .any(|part| part.eq_ignore_ascii_case(tag))
    })
}

fn has_click_action(ctx: &ModernCtx<'_>) -> bool {
    ctx.doc.actions.iter().any(|action| {
        matches!(
            action.get("action").and_then(Value::as_str),
            Some(kind)
                if kind.eq_ignore_ascii_case("click")
                    || kind.eq_ignore_ascii_case("submit")
        )
    })
}

fn no_effect_action(ctx: &ModernCtx<'_>, node: &DiffNode) -> bool {
    ctx.doc.actions.iter().any(|action| {
        action.get("selector").and_then(Value::as_str) == Some(node.selector.as_str())
            && action
                .get("effect")
                .and_then(Value::as_str)
                .is_some_and(|effect| effect.eq_ignore_ascii_case("no_effect"))
    })
}

fn viewport(ctx: &ModernCtx<'_>) -> Option<(f64, f64)> {
    ctx.doc.viewport.or_else(|| {
        ctx.doc
            .env
            .as_ref()
            .and_then(|env| env.device)
            .map(|device| (f64::from(device.width), f64::from(device.height)))
    })
}

// ---------------------------------------------------------------------------
// 1. autofill-purpose-tokens
// ---------------------------------------------------------------------------

/// Propósito pessoal inferível e tokens aceitos para ele.
fn personal_purpose(node: &DiffNode) -> Option<(&'static str, &'static [&'static str])> {
    let hint = raw_attr(node, "name")
        .or_else(|| raw_attr(node, "id"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let kind = raw_attr(node, "type")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let has = |token: &str| hint.contains(token);

    // Credenciais, pagamento e OTP têm checks próprios: não duplicar sinal.
    if has("pass") || has("user") || has("login") || kind == "password" {
        return None;
    }
    if has("cc-") || has("card") || has("cvv") || has("cvc") {
        return None;
    }
    if has("one-time") || has("otp") || has("totp") {
        return None;
    }
    if kind == "email" || has("email") || has("e-mail") {
        return Some(("email", &["email", "username"]));
    }
    if kind == "tel" || has("tel") || has("phone") || has("mobile") {
        return Some(("tel", &["tel", "tel-national"]));
    }
    if has("postal") || has("zip") {
        return Some(("postal-code", &["postal-code"]));
    }
    if has("street") || has("address") {
        return Some((
            "street-address",
            &["street-address", "address-line1", "address-line2"],
        ));
    }
    if has("city") {
        return Some(("address-level2", &["address-level2"]));
    }
    if has("region") || has("state") || has("province") {
        return Some(("address-level1", &["address-level1"]));
    }
    if has("country") {
        return Some(("country-name", &["country-name"]));
    }
    if has("name") || has("fname") || has("lname") {
        return Some(("name", &["name", "given-name", "family-name"]));
    }
    None
}

fn autofill_purpose(
    ctx: &ModernCtx<'_>,
    _index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        if !is_form_control(ctx.tag(node)) || is_honeypot(node) || has_attr(node, "disabled") {
            continue;
        }
        let Some((purpose, allowed)) = personal_purpose(node) else {
            continue;
        };
        let current = raw_attr(node, "autocomplete");
        if attr_is(node, "autocomplete", "off") {
            continue; // opt-out deliberado para token sensível
        }
        if token_in(current, allowed) {
            continue;
        }
        let kind = raw_attr(node, "type").unwrap_or("text");
        let text = format!(
            "{} type={kind} autocomplete={} purpose={purpose} expected={} — WCAG 1.3.5 (AA)",
            node.selector,
            current.unwrap_or("null"),
            allowed.join("/")
        );
        ctx.emit(out, spec, node, Evidence::new(text, "attrs+aria"));
    }
}

// ---------------------------------------------------------------------------
// 2. credential-autocomplete-tokens
// ---------------------------------------------------------------------------

fn credential_tokens(
    ctx: &ModernCtx<'_>,
    index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    let passkeys: Vec<&DiffNode> = ctx
        .flat
        .iter()
        .copied()
        .filter(|node| ctx_tag_is(node, "INPUT") && attr_is(node, "type", "password"))
        .collect();
    if passkeys.is_empty() {
        return; // passkey-only / login federado sem campo de senha
    }
    let has_new = passkeys
        .iter()
        .any(|node| attr_is(node, "autocomplete", "new-password"));
    let has_current = passkeys
        .iter()
        .any(|node| attr_is(node, "autocomplete", "current-password"));

    for &node in &passkeys {
        let mut issues = Vec::new();
        let token = raw_attr(node, "autocomplete");
        if !token_in(token, &["current-password", "new-password"]) {
            issues.push(format!(
                "autocomplete={} expected=current-password|new-password",
                token.unwrap_or("null")
            ));
        }
        if !has_attr(node, "required") {
            issues.push("required=false".to_string());
        }
        if has_current && has_new {
            let recovery = index.ancestor(node, "FORM").is_some_and(|form| {
                ["forgot", "recover", "reset", "change"].iter().any(|word| {
                    raw_attr(form, "id")
                        .or_else(|| raw_attr(form, "class"))
                        .is_some_and(|v| v.to_ascii_lowercase().contains(word))
                })
            });
            if !recovery {
                issues.push(
                    "current-password and new-password coexist outside a change-password flow"
                        .to_string(),
                );
            }
        }
        if issues.is_empty() {
            continue;
        }
        let text = format!("{} {}", node.selector, issues.join("; "));
        ctx.emit(out, spec, node, Evidence::new(text, "attrs"));
    }

    // Campo de usuário (email) no mesmo formulário de senha deve usar username.
    for &node in &ctx.flat {
        if !ctx_tag_is(node, "INPUT") || !attr_is(node, "type", "email") {
            continue;
        }
        let same_form = passkeys.iter().any(|password| {
            index.ancestor(node, "FORM").map(|form| form.id)
                == index.ancestor(password, "FORM").map(|form| form.id)
        });
        if !same_form || is_honeypot(node) {
            continue;
        }
        let token = raw_attr(node, "autocomplete");
        if token_in(token, &["username", "email"]) {
            continue;
        }
        let mut issues = vec![format!(
            "autocomplete={} expected=username",
            token.unwrap_or("null")
        )];
        if !has_attr(node, "required") {
            issues.push("required=false".to_string());
        }
        let text = format!("{} {}", node.selector, issues.join("; "));
        ctx.emit(out, spec, node, Evidence::new(text, "attrs"));
    }
}

// ---------------------------------------------------------------------------
// 3. payment-field-single-input
// ---------------------------------------------------------------------------

fn payment_role(token: &str) -> Option<&'static str> {
    let lowered = token.to_ascii_lowercase();
    if lowered.contains("cc-number") {
        Some("cc-number")
    } else if lowered.contains("cc-csc") || lowered.contains("cc-cvv") {
        Some("cc-csc")
    } else if lowered.contains("cc-exp") {
        Some("cc-exp")
    } else if lowered.contains("cc-name") {
        Some("cc-name")
    } else {
        None
    }
}

fn payment_single_input(
    ctx: &ModernCtx<'_>,
    index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    let mut grouped: BTreeSet<(u64, &'static str)> = BTreeSet::new();
    let mut counts: BTreeMap<(u64, &'static str), usize> = BTreeMap::new();

    for &node in &ctx.flat {
        if !ctx_tag_is(node, "INPUT") {
            continue;
        }
        let Some(role) = raw_attr(node, "autocomplete").and_then(payment_role) else {
            continue;
        };
        let parent = node.parent_id.unwrap_or(node.id);
        *counts.entry((parent, role)).or_default() += 1;
    }

    for &node in &ctx.flat {
        if !ctx_tag_is(node, "INPUT") {
            continue;
        }
        let Some(token) = raw_attr(node, "autocomplete") else {
            continue;
        };
        let Some(role) = payment_role(token) else {
            continue;
        };
        let parent = node.parent_id.unwrap_or(node.id);
        let mut issues = Vec::new();

        if matches!(role, "cc-number" | "cc-csc") {
            let kind = raw_attr(node, "type")
                .unwrap_or("text")
                .to_ascii_lowercase();
            if kind != "text" {
                issues.push(format!(
                    "type={kind} expected=text (spinner/perda de zeros ou CSC mascarado)"
                ));
            }
            if !attr_is(node, "inputmode", "numeric") {
                issues.push(format!(
                    "inputmode={} expected=numeric",
                    raw_attr(node, "inputmode").unwrap_or("null")
                ));
            }
            if raw_attr(node, "maxlength").is_none() {
                issues.push("maxlength ausente".to_string());
            }
            let group = (parent, role);
            let total = counts.get(&group).copied().unwrap_or(0);
            if total > 1 && grouped.insert(group) {
                issues.push(format!(
                    "campo dividido em {total} inputs irmãos — usar um único input"
                ));
            }
        } else if role == "cc-exp" && !attr_is(node, "inputmode", "numeric") {
            issues.push(format!(
                "inputmode={} expected=numeric",
                raw_attr(node, "inputmode").unwrap_or("null")
            ));
        }
        // cc-name com type=text está correto: nenhum sinal.

        if issues.is_empty() {
            continue;
        }
        let _ = index; // índice usado por outros checks; mantido na assinatura comum.
        let text = format!(
            "{} autocomplete={token} {}",
            node.selector,
            issues.join("; ")
        );
        ctx.emit(out, spec, node, Evidence::new(text, "attrs+tag"));
    }
}

// ---------------------------------------------------------------------------
// 4. hint-above-input
// ---------------------------------------------------------------------------

fn label_for_node(node: &DiffNode) -> bool {
    if node.parent_id.is_none() {
        return false;
    }
    node.attributes
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|attrs| attrs.contains_key("id") || attrs.contains_key("for"))
}

fn hint_above_input(
    ctx: &ModernCtx<'_>,
    index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        if !is_form_control(ctx.tag(node)) {
            continue;
        }
        let Some((_x, input_y, _w, _h)) = rect(node) else {
            continue;
        };
        let described = index.described(node);
        if !described.is_empty() {
            for hint in described {
                let Some((_hx, hy, hw, hh)) = rect(hint) else {
                    continue;
                };
                if hw <= 0.0 || hh <= 0.0 {
                    continue; // visually-hidden legítimo
                }
                let bottom = ((hy + hh) * 100.0).round() / 100.0;
                if bottom <= input_y {
                    continue;
                }
                let text = format!(
                    "{} aria-describedby={} hint bottom={bottom:.2} > input top={input_y:.2} — hint abaixo do campo",
                    node.selector,
                    raw_attr(hint, "id").unwrap_or("?")
                );
                let evidence = Evidence::numeric(
                    text,
                    "attrs+rect",
                    "hint_bottom_minus_input_top",
                    json!(bottom),
                    json!(input_y),
                    "<=",
                );
                ctx.emit(out, spec, node, evidence);
            }
            continue; // hint presente: placeholder é complemento legítimo
        }
        let placeholder = raw_attr(node, "placeholder");
        if placeholder.is_none() || label_for_node(node) {
            continue;
        }
        let format_bearing = has_attr(node, "pattern")
            || raw_attr(node, "autocomplete").is_some_and(|token| {
                payment_role(token).is_some() || token.eq_ignore_ascii_case("one-time-code")
            });
        if !format_bearing {
            continue;
        }
        let text = format!(
            "{} placeholder={} é o único indicador de formato; usar elemento de hint acima com aria-describedby",
            node.selector,
            placeholder.unwrap_or("")
        );
        ctx.emit(out, spec, node, Evidence::new(text, "attrs+aria"));
    }
}

// ---------------------------------------------------------------------------
// 5. user-invalid-aria-sync
// ---------------------------------------------------------------------------

fn fallback_in_use(node: &DiffNode) -> bool {
    class_has(node, "user-invalid-fallback")
        || raw_attr(node, "class").is_some_and(|v| v.contains("user-invalid-fallback"))
}

fn novalidate_form(index: &DomIndex<'_>, node: &DiffNode) -> bool {
    index
        .ancestor(node, "FORM")
        .is_some_and(|form| has_attr(form, "novalidate"))
}

fn user_invalid_sync(
    ctx: &ModernCtx<'_>,
    index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    let post_action = has_click_action(ctx);
    for &node in &ctx.flat {
        if !is_form_control(ctx.tag(node)) || !has_attr(node, "required") {
            continue;
        }
        if fallback_in_use(node) || novalidate_form(index, node) {
            continue;
        }
        let error_node = raw_attr(node, "data-error")
            .and_then(|id| index.dom_id(id))
            .or_else(|| index.described(node).into_iter().next());
        let Some(error) = error_node else {
            continue; // validação do servidor / nenhum erro referenciado
        };
        let error_visible = ctx.visible(error);
        let invalid = raw_attr(node, "aria-invalid");
        let mut issues = Vec::new();
        if post_action && error_visible && !token_in(invalid, &["true"]) {
            issues.push(format!(
                "aria-invalid={} com erro visível após ação",
                invalid.unwrap_or("null")
            ));
        }
        if token_in(invalid, &["true"]) && !error_visible {
            issues.push("aria-invalid=true com erro oculto".to_string());
        }
        if !post_action && !error_visible {
            issues.push(
                "erro oculto referenciado desde o load (aria-describedby/data-error)".to_string(),
            );
        }
        if issues.is_empty() {
            continue;
        }
        let text = format!(
            "{} data-error={} {}",
            node.selector,
            raw_attr(error, "id").unwrap_or("?"),
            issues.join("; ")
        );
        ctx.emit(out, spec, node, Evidence::new(text, "attrs+aria"));
    }
}

// ---------------------------------------------------------------------------
// 6. otp-single-input
// ---------------------------------------------------------------------------

fn otp_single_input(
    ctx: &ModernCtx<'_>,
    index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    let mut groups: BTreeMap<u64, Vec<&DiffNode>> = BTreeMap::new();
    for &node in &ctx.flat {
        if !ctx_tag_is(node, "INPUT") || raw_attr(node, "maxlength") != Some("1") {
            continue;
        }
        let numeric = attr_is(node, "inputmode", "numeric")
            || raw_attr(node, "pattern").is_some_and(|p| p.contains("0-9"));
        if numeric {
            groups
                .entry(node.parent_id.unwrap_or(node.id))
                .or_default()
                .push(node);
        }
    }
    for (parent, mut boxes) in groups {
        if boxes.len() < 2 {
            continue;
        }
        boxes.sort_by_key(|node| node.id);
        let first = boxes[0];
        let total = boxes.len();
        let container = index
            .by_id
            .get(&parent)
            .map_or(first.selector.as_str(), |node| node.selector.as_str());
        let text = format!(
            "{container} {total} × input[maxlength=1][inputmode=numeric] — quebra autocomplete=one-time-code e copiar/colar; usar um único input"
        );
        ctx.emit(out, spec, first, Evidence::new(text, "attrs+tag"));
    }

    for &node in &ctx.flat {
        if !ctx_tag_is(node, "INPUT") {
            continue;
        }
        let token = raw_attr(node, "autocomplete");
        let one_time = token.is_some_and(|v| v.eq_ignore_ascii_case("one-time-code"));
        let name_hint = raw_attr(node, "name")
            .or_else(|| raw_attr(node, "id"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        let looks_like_otp = one_time
            || ["otp", "one-time", "code"]
                .iter()
                .any(|t| name_hint.contains(t));
        let local_pin = !one_time && name_hint.contains("pin");
        if !looks_like_otp || local_pin {
            continue;
        }
        let mut issues = Vec::new();
        let kind = raw_attr(node, "type")
            .unwrap_or("text")
            .to_ascii_lowercase();
        if kind != "text" {
            issues.push(format!("type={kind} expected=text"));
        }
        if !attr_is(node, "inputmode", "numeric") {
            issues.push(format!(
                "inputmode={} expected=numeric",
                raw_attr(node, "inputmode").unwrap_or("null")
            ));
        }
        if issues.is_empty() {
            continue;
        }
        let text = format!(
            "{} autocomplete={} {}",
            node.selector,
            token.unwrap_or("null"),
            issues.join("; ")
        );
        ctx.emit(out, spec, node, Evidence::new(text, "attrs+tag"));
    }
}

// ---------------------------------------------------------------------------
// 7. select-base-semantics
// ---------------------------------------------------------------------------

fn select_base_semantics(
    ctx: &ModernCtx<'_>,
    index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        if !ctx_tag_is(node, "SELECT") || !appearance_base(ctx, node) {
            continue;
        }
        let mut issues = Vec::new();
        if raw_attr(node, "name").is_none() {
            issues.push("attrs.name ausente".to_string());
        }
        if ctx.aria(node, "name").is_none() {
            issues.push("aria.name ausente".to_string());
        }
        let options: Vec<&DiffNode> = index
            .kids(node)
            .iter()
            .copied()
            .filter(|child| ctx_tag_is(child, "OPTION"))
            .collect();
        for (position, option) in options.iter().enumerate() {
            // value ausente com texto visível é o valor submetido por omissão: keep.
            if position > 0 && has_attr(option, "value") && raw_attr(option, "value").is_none() {
                issues.push(format!("option[{position}] com value vazio"));
            }
            if raw_attr(option, "aria-label").is_some() && !aria_has_text(option) {
                issues.push(format!(
                    "option[{position}] aria-label mascara o texto visível"
                ));
            }
        }
        if issues.is_empty() {
            continue;
        }
        let text = format!(
            "{} appearance=base-select {}",
            node.selector,
            issues.join("; ")
        );
        ctx.emit(out, spec, node, Evidence::new(text, "attrs+styles+aria"));
    }
}

// ---------------------------------------------------------------------------
// 8. select-placeholder-option
// ---------------------------------------------------------------------------

fn is_empty_value(option: &DiffNode) -> bool {
    has_attr(option, "value")
        && raw_attr(option, "value").is_none_or(|value| value.trim().is_empty())
}

fn select_placeholder_option(
    ctx: &ModernCtx<'_>,
    index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        if !ctx_tag_is(node, "SELECT") || !has_attr(node, "required") {
            continue;
        }
        if has_attr(node, "multiple") {
            continue;
        }
        let options: Vec<&DiffNode> = index
            .kids(node)
            .iter()
            .copied()
            .filter(|child| ctx_tag_is(child, "OPTION"))
            .collect();
        let empty_positions: Vec<usize> = options
            .iter()
            .enumerate()
            .filter_map(|(i, option)| is_empty_value(option).then_some(i))
            .collect();
        let Some(&first_empty) = empty_positions.first() else {
            continue; // valor padrão válido: sem placeholder
        };
        let placeholder = options[first_empty];
        if has_attr(placeholder, "disabled") || has_attr(placeholder, "hidden") {
            continue;
        }
        let issue = if first_empty > 0 {
            format!("placeholder na posição {first_empty} (deve ser o primeiro option)")
        } else {
            format!("option[0] value=\"\" selected disabled=null — placeholder selecionável")
        };
        let text = format!("{} required; {issue}", node.selector);
        ctx.emit(out, spec, node, Evidence::new(text, "attrs"));
    }
}

// ---------------------------------------------------------------------------
// 9. select-option-noncolor-state
// ---------------------------------------------------------------------------

fn checkmark_visible(node: &DiffNode) -> bool {
    if pseudo_present(node, "selectedcontent") {
        return true;
    }
    // A visible ::checkmark is itself the non-color cue; `content` is
    // generated by the UA and is often absent from computed styles.
    pseudo_style(node, "::checkmark", "visual", "display")
        .is_some_and(|display| !display.eq_ignore_ascii_case("none"))
}

fn noncolor_border(node: &DiffNode) -> bool {
    [
        "border-top-width",
        "border-inline-start-width",
        "outline-width",
    ]
    .iter()
    .any(|property| {
        ctx_style_local(node, "box_model", property)
            .or_else(|| ctx_style_local(node, "visual", property))
            .is_some_and(|value| value != "0px" && !value.is_empty() && value != "medium")
    })
}

fn ctx_style_local<'a>(node: &'a DiffNode, category: &str, property: &str) -> Option<&'a str> {
    node.styles.as_ref()?.get(category)?.get(property)?.as_str()
}

fn select_option_noncolor(
    ctx: &ModernCtx<'_>,
    index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        if !ctx_tag_is(node, "SELECT") || !appearance_base(ctx, node) {
            continue;
        }
        let indicator = checkmark_visible(node);
        for (position, option) in index
            .kids(node)
            .iter()
            .copied()
            .filter(|child| ctx_tag_is(child, "OPTION"))
            .enumerate()
        {
            let selected = has_attr(option, "selected") || aria_is(option, "aria_checked", "true");
            if !selected || indicator || has_attr(option, "aria-checked") || noncolor_border(option)
            {
                continue;
            }
            let hidden_text = option.children.iter().any(|child| {
                raw_attr(child, "class")
                    .is_some_and(|c| c.contains("visually-hidden") || c.contains("sr-only"))
            });
            if hidden_text {
                continue;
            }
            let color = ctx
                .style_any(
                    option,
                    &[("visual", "background-color"), ("visual", "color")],
                )
                .unwrap_or("null");
            let text = format!(
                "{} option[{position}] selected background/color={color} sem ::checkmark, borda, aria-checked ou texto visually-hidden — estado apenas por cor",
                node.selector
            );
            ctx.emit(
                out,
                spec,
                option,
                Evidence::new(text, "pseudo+styles+attrs"),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 10. select-picker-animation-guard
// ---------------------------------------------------------------------------

fn animation_issues(ctx: &ModernCtx<'_>, decl: &DeclaredFacet, node: &DiffNode) -> Vec<String> {
    let mut issues = Vec::new();
    let transition = ctx
        .style(node, "animation", "transition-property")
        .unwrap_or_default();
    let discrete = contains_token(transition, "display") || contains_token(transition, "overlay");
    let behavior = ctx
        .style(node, "animation", "transition-behavior")
        .unwrap_or("absent");
    if discrete && !behavior.eq_ignore_ascii_case("allow-discrete") {
        issues.push(format!(
            "transition-behavior={behavior} com display/overlay"
        ));
    }
    if discrete && !declared_at(decl, "starting-style", Some("::picker(select)")) {
        issues.push("@starting-style para ::picker(select) ausente".to_string());
    }
    if !reduced_motion_guard(decl) {
        issues
            .push("@media (prefers-reduced-motion: reduce) zerando transition ausente".to_string());
    }
    if pseudo_style(node, "::picker(select)", "box_model", "overflow")
        .is_some_and(|v| v.eq_ignore_ascii_case("hidden"))
    {
        issues.push("overflow:hidden em ::picker(select)".to_string());
    }
    issues
}

fn select_picker_animation(
    ctx: &ModernCtx<'_>,
    _index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    let Some(decl) = ctx.doc.declared.as_ref() else {
        return;
    };
    for &node in &ctx.flat {
        if !ctx_tag_is(node, "SELECT") || !appearance_base(ctx, node) {
            continue;
        }
        let issues = animation_issues(ctx, decl, node);
        if issues.is_empty() {
            continue;
        }
        let text = format!(
            "{} appearance=base-select {}",
            node.selector,
            issues.join("; ")
        );
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(text, "styles+declared+pseudo"),
        );
    }
}

// ---------------------------------------------------------------------------
// 11. select-picker-open-layout
// ---------------------------------------------------------------------------

fn select_picker_open_layout(
    ctx: &ModernCtx<'_>,
    _index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    let Some(decl) = ctx.doc.declared.as_ref() else {
        return;
    };
    for &node in &ctx.flat {
        if !ctx_tag_is(node, "SELECT") || !appearance_base(ctx, node) {
            continue;
        }
        let mut issues = Vec::new();
        for rule in &decl.rules {
            if !rule.selector.contains("::picker(select)") {
                continue;
            }
            let display = rule.declarations.get("display").map_or("", String::as_str);
            if !display.eq_ignore_ascii_case("grid") && !display.eq_ignore_ascii_case("flex") {
                continue;
            }
            if !contains_token(&rule.selector, ":open") {
                issues.push(format!(
                    "regra '{}' aplica display={display} sem :open",
                    rule.selector
                ));
            }
        }
        let pseudo_display = pseudo_style(node, "::picker(select)", "layout", "display");
        let pseudo_flex = pseudo_display
            .is_some_and(|v| v.eq_ignore_ascii_case("grid") || v.eq_ignore_ascii_case("flex"));
        let open_rule = decl.rules.iter().any(|rule| {
            rule.selector.contains("::picker(select)") && contains_token(&rule.selector, ":open")
        });
        if pseudo_flex && !open_rule && issues.is_empty() {
            issues.push("::picker(select) com display grid/flex sem regra :open".to_string());
        }
        issues.sort();
        issues.dedup();
        if issues.is_empty() {
            continue;
        }
        let text = format!("{} {}", node.selector, issues.join("; "));
        ctx.emit(out, spec, node, Evidence::new(text, "declared+styles"));
    }
}

// ---------------------------------------------------------------------------
// 12. autofill-noncolor-indicator
// ---------------------------------------------------------------------------

fn declaration_noncolor(rule: &sniff_core::modern::DeclaredRule) -> bool {
    rule.declarations.iter().any(|(property, value)| {
        let lower = value.to_ascii_lowercase();
        (property == "box-shadow" && lower.contains("inset"))
            || (property == "outline" && !lower.contains("none") && !lower.starts_with('0'))
            || (property == "outline-width" && lower != "0px" && !lower.is_empty())
            || (property.starts_with("border-")
                && property.ends_with("-width")
                && lower != "0px"
                && lower != "medium")
            || (property == "background-image" && lower != "none" && !lower.is_empty())
    })
}

fn autofill_noncolor(
    ctx: &ModernCtx<'_>,
    _index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    let Some(decl) = ctx.doc.declared.as_ref() else {
        return;
    };
    let first_eligible = ctx
        .flat
        .iter()
        .copied()
        .find(|node| is_autofillable_input(node) && !is_honeypot(node));
    let Some(eligible) = first_eligible else {
        return;
    };
    for rule in &decl.rules {
        let has_autofill = contains_token(&rule.selector, ":autofill")
            || contains_token(&rule.selector, ":-webkit-autofill");
        if !has_autofill {
            continue;
        }
        if declaration_noncolor(rule) {
            continue;
        }
        let compensated = decl.rules.iter().any(|other| {
            contains_token(&other.selector, ":autofill")
                && contains_token(&other.selector, ":focus-visible")
                && other.declarations.iter().any(|(property, value)| {
                    property == "outline" && !value.to_ascii_lowercase().contains("none")
                })
        });
        if compensated {
            continue;
        }
        let text = format!(
            "{} regra '{}' com indicadores apenas por cor (sem box-shadow inset/outline/border-width/background-image); manter :is(:autofill, :-webkit-autofill) sem fallback JS",
            eligible.selector, rule.selector
        );
        ctx.emit(out, spec, eligible, Evidence::new(text, "declared+pseudo"));
        break; // uma linha por documento: a regra é global
    }
}

// ---------------------------------------------------------------------------
// 13. accent-color-contrast
// ---------------------------------------------------------------------------

fn accent_target(node: &DiffNode) -> bool {
    if ctx_tag_is(node, "PROGRESS") {
        return true;
    }
    ctx_tag_is(node, "INPUT")
        && ["checkbox", "radio", "range"]
            .iter()
            .any(|kind| attr_is(node, "type", kind))
}

fn accent_color_contrast(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let declared = ctx.doc.declared.as_ref();
    let malformed_fallback = declared.is_some_and(|decl| {
        decl.at_rules.iter().any(|at| {
            let condition = at.condition_text.to_ascii_lowercase();
            condition.contains("accent-color")
                && condition.contains("not")
                && !at
                    .rules
                    .iter()
                    .any(|rule| rule.selector.contains("visually-hidden"))
        })
    });
    let mut fallback_reported = false;

    for &node in &ctx.flat {
        if !accent_target(node) || has_attr(node, "disabled") || ctx.aria_bool(node, "disabled") {
            continue;
        }
        let accent = ctx.style_any(
            node,
            &[("visual", "accent-color"), ("interaction", "accent-color")],
        );
        if accent.is_none() {
            if malformed_fallback && !fallback_reported {
                fallback_reported = true;
                let text = format!(
                    "{} @supports not (accent-color) sem fallback .visually-hidden — manter a técnica de fallback progressiva",
                    node.selector
                );
                ctx.emit(
                    out,
                    &ACCENT_BORDERLINE,
                    node,
                    Evidence::new(text, "declared"),
                );
            }
            continue;
        }
        let accent = accent.unwrap_or_default();
        if accent.eq_ignore_ascii_case("auto") {
            continue;
        }
        let Some(background) = effective_background(node) else {
            continue; // fundo imagem/indisponível: sem precisão falsa
        };
        let (Some((foreground_rgb, _)), Some((background_rgb, _))) =
            (parse_color(accent), parse_color(background))
        else {
            continue;
        };
        let ratio = (contrast_ratio(foreground_rgb, background_rgb) * 100.0).round() / 100.0;
        // Ratios just above 3:1 still fail in some implementations, so the
        // 2.9–3.1 window is reported as a warning instead of being silent.
        if ratio >= 3.1 {
            continue;
        }
        let text = format!(
            "{} accent-color={accent} vs fundo efetivo={background} ratio={ratio:.2}:1 (< 3:1, WCAG 1.4.11; manter fallback @supports not)",
            node.selector
        );
        let evidence = Evidence::numeric(
            text,
            "styles+effective_background",
            "accent_color_contrast_ratio",
            json!(ratio),
            json!(3.0),
            ">=",
        );
        // 2,9–3,1 é fronteiriço por bugs de implementação conhecidos: warn.
        if (2.9..3.1).contains(&ratio) {
            ctx.emit(out, &ACCENT_BORDERLINE, node, evidence);
        } else {
            ctx.emit(out, spec, node, evidence);
        }
    }
}

// ---------------------------------------------------------------------------
// 14. field-sizing-constraints
// ---------------------------------------------------------------------------

fn missing_constraint(value: Option<&str>) -> bool {
    value.is_none_or(|v| {
        let lower = v.to_ascii_lowercase();
        matches!(lower.as_str(), "none" | "auto" | "0px" | "")
    })
}

fn field_sizing_constraints(
    ctx: &ModernCtx<'_>,
    _index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    for &node in &ctx.flat {
        if !is_form_control(ctx.tag(node)) {
            continue;
        }
        let sizing = ctx.style_any(
            node,
            &[
                ("box_model", "field-sizing"),
                ("layout", "field-sizing"),
                ("custom", "field-sizing"),
            ],
        );
        if !sizing.is_some_and(|v| v.eq_ignore_ascii_case("content")) {
            continue;
        }
        let width = ctx.style(node, "box_model", "width").unwrap_or("null");
        let height = ctx.style(node, "box_model", "height").unwrap_or("null");
        let inline_min = ctx.style(node, "box_model", "min-inline-size");
        let inline_max = ctx.style(node, "box_model", "max-inline-size");
        let block_min = ctx.style(node, "box_model", "min-block-size");
        let block_max = ctx.style(node, "box_model", "max-block-size");

        let mut issues = Vec::new();
        match ctx.tag(node) {
            "INPUT" => {
                if !width.eq_ignore_ascii_case("auto") {
                    issues.push(format!("width={width} sobrescreve field-sizing"));
                }
                if missing_constraint(inline_min) && missing_constraint(inline_max) {
                    issues.push("min-inline-size/max-inline-size ausentes".to_string());
                }
            }
            "TEXTAREA" => {
                if !height.eq_ignore_ascii_case("auto") {
                    issues.push(format!("height={height} trava o auto-sizing vertical"));
                }
                if missing_constraint(block_min) && missing_constraint(block_max) {
                    issues.push("min-block-size/max-block-size ausentes".to_string());
                }
            }
            "SELECT" => {
                if missing_constraint(inline_max) {
                    issues.push("max-inline-size ausente".to_string());
                }
            }
            _ => {}
        }
        if issues.is_empty() {
            continue;
        }
        let text = format!(
            "{} field-sizing=content width={width} height={height} {}",
            node.selector,
            issues.join("; ")
        );
        ctx.emit(out, spec, node, Evidence::new(text, "styles+attrs"));
    }
}

// ---------------------------------------------------------------------------
// 15. interaction-state-feedback
// ---------------------------------------------------------------------------

fn interaction_state_feedback(
    ctx: &ModernCtx<'_>,
    _index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    let Some(decl) = ctx.doc.declared.as_ref() else {
        return;
    };
    for &node in &ctx.flat {
        let is_button = ctx_tag_is(node, "BUTTON");
        let is_link = ctx_tag_is(node, "A") && has_attr(node, "href");
        if is_button || is_link {
            let hover = decl.rules.iter().any(|rule| {
                contains_token(&rule.selector, ":hover") && rule_matches_node(&rule.selector, node)
            });
            let active = decl.rules.iter().any(|rule| {
                contains_token(&rule.selector, ":active") && rule_matches_node(&rule.selector, node)
            });
            let mut issues = Vec::new();
            if !hover {
                issues.push("sem regra :hover no CSSOM".to_string());
            }
            if !active {
                issues.push("sem regra :active no CSSOM".to_string());
            }
            if has_attr(node, "popovertarget")
                && !decl
                    .rules
                    .iter()
                    .any(|rule| contains_token(&rule.selector, ":popover-open"))
            {
                issues.push("sem regra :popover-open no CSSOM".to_string());
            }
            if no_effect_action(ctx, node) {
                issues.push("ação pós-captura sem efeito observável".to_string());
            }
            if issues.is_empty() {
                continue;
            }
            let text = format!(
                "{} {} (:focus-visible coberto por focus-indicator; não exigir filter:blur)",
                node.selector,
                issues.join("; ")
            );
            ctx.emit(
                out,
                spec,
                node,
                Evidence::new(text, "declared+actions+styles"),
            );
        }
        let overflow = ctx.style(node, "box_model", "overflow-x");
        let scrollable = overflow
            .is_some_and(|v| v.eq_ignore_ascii_case("auto") || v.eq_ignore_ascii_case("scroll"));
        if scrollable && ctx.style(node, "interaction", "scroll-snap-type").is_none() {
            let text = format!(
                "{} overflow-x={} sem scroll-snap-type — feedback de rolagem ausente",
                node.selector,
                overflow.unwrap_or("null")
            );
            ctx.emit(out, spec, node, Evidence::new(text, "styles+actions"));
        }
    }
}

// ---------------------------------------------------------------------------
// 16. persistent-bar-budget
// ---------------------------------------------------------------------------

fn persistent_bar_budget(
    ctx: &ModernCtx<'_>,
    _index: &DomIndex<'_>,
    out: &mut Vec<CheckLine>,
    spec: &CheckSpec,
) {
    let Some((viewport_width, viewport_height)) = viewport(ctx) else {
        return; // __meta/__env ausente: sem denominador confiável
    };
    if viewport_height <= 0.0 || viewport_width <= 0.0 {
        return;
    }
    let area = viewport_width * viewport_height;

    for &node in &ctx.flat {
        let position = ctx.style(node, "layout", "position").unwrap_or_default();
        let fixed =
            position.eq_ignore_ascii_case("fixed") || position.eq_ignore_ascii_case("sticky");
        let Some((_x, _y, width, height)) = rect(node) else {
            if ctx_tag_is(node, "DIALOG") && has_attr(node, "open") {
                let text = format!(
                    "{} dialog[open] presente na captura sem ação do usuário — interrupção no load",
                    node.selector
                );
                ctx.emit(out, spec, node, Evidence::new(text, "attrs+styles"));
            }
            continue;
        };
        if fixed && height > 0.0 {
            let ratio = (height / viewport_height * 1000.0).round() / 1000.0;
            if ratio > 0.12 {
                let text = format!(
                    "{} position={position} rect.height={height:.0}px viewport.height={viewport_height:.0}px → {:.1}% > 12%",
                    node.selector,
                    ratio * 100.0
                );
                let evidence = Evidence::numeric(
                    text,
                    "styles+rect+__meta",
                    "bar_height_viewport_ratio",
                    json!(ratio),
                    json!(0.12),
                    "<=",
                );
                ctx.emit(out, spec, node, evidence);
                continue;
            }
        }
        let modal = ctx.aria(node, "role").is_some_and(|role| {
            role.eq_ignore_ascii_case("dialog") || role.eq_ignore_ascii_case("alertdialog")
        }) || has_attr(node, "open");
        let named = ctx.aria(node, "name").is_some() || aria_has_text(node);
        if !modal && named && width > 0.0 && height > 0.0 {
            let coverage = (width * height / area * 1000.0).round() / 1000.0;
            if coverage > 0.15 {
                let text = format!(
                    "{} aviso não-modal cobre {:.1}% do viewport (> 15%) — usar barra compacta e dismissível",
                    node.selector,
                    coverage * 100.0
                );
                let evidence = Evidence::numeric(
                    text,
                    "rect+__meta+attrs",
                    "notice_viewport_coverage",
                    json!(coverage),
                    json!(0.15),
                    "<=",
                );
                ctx.emit(out, spec, node, evidence);
            }
        }
        if ctx_tag_is(node, "DIALOG") && has_attr(node, "open") {
            let text = format!(
                "{} dialog[open] presente na captura sem ação do usuário — interrupção no load",
                node.selector
            );
            ctx.emit(out, spec, node, Evidence::new(text, "attrs+styles"));
        }
    }
}

// ---------------------------------------------------------------------------
// Testes: sintéticos, determinísticos, positivo / negativo / keepWhen
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;
    use sniff_core::modern::{
        AtRuleInfo, DeclaredFacet, DeclaredRule, DocumentFacet, EnvFacet, MatrixTag,
    };
    use sniff_css_diff::DiffDocument;

    fn node(id: u64, tag: &str, selector: &str) -> DiffNode {
        DiffNode {
            id,
            parent_id: None,
            selector: selector.to_string(),
            tag: Some(tag.to_string()),
            path: None,
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
            effective_background: None,
            children: Vec::new(),
        }
    }

    fn child(mut parent: DiffNode, mut kid: DiffNode) -> DiffNode {
        kid.parent_id = Some(parent.id);
        kid.depth = parent.depth.map(|d| d + 1);
        parent.children.push(kid);
        parent
    }

    fn attrs(mut n: DiffNode, pairs: &[(&str, &str)]) -> DiffNode {
        let map = n
            .attributes
            .get_or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("attributes object");
        for (key, value) in pairs {
            map.insert((*key).to_string(), Value::String((*value).to_string()));
        }
        n
    }

    fn aria(mut n: DiffNode, pairs: &[(&str, &str)]) -> DiffNode {
        let map = n
            .aria
            .get_or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("aria object");
        for (key, value) in pairs {
            map.insert((*key).to_string(), Value::String((*value).to_string()));
        }
        n
    }

    fn style(mut n: DiffNode, category: &str, property: &str, value: &str) -> DiffNode {
        n.styles
            .get_or_insert_with(Map::new)
            .entry(category.to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("styles category object")
            .insert(property.to_string(), Value::String(value.to_string()));
        n
    }

    fn pseudo(
        mut n: DiffNode,
        name: &str,
        category: &str,
        property: &str,
        value: &str,
    ) -> DiffNode {
        n.pseudo
            .get_or_insert_with(Map::new)
            .entry(name.to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("pseudo category object")
            .entry(category.to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("pseudo styles object")
            .insert(property.to_string(), Value::String(value.to_string()));
        n
    }

    fn rect(mut n: DiffNode, x: f64, y: f64, w: f64, h: f64) -> DiffNode {
        n.rect = Some(json!({"x": x, "y": y, "width": w, "height": h}));
        n
    }

    fn background(mut n: DiffNode, color: &str) -> DiffNode {
        n.effective_background = Some(Value::String(color.to_string()));
        n
    }

    fn visible(mut n: DiffNode, value: bool) -> DiffNode {
        n.noticeable = Some(json!({"display_visible": value, "accessibility_grade": "NONE"}));
        n
    }

    fn rule(selector: &str, pairs: &[(&str, &str)]) -> DeclaredRule {
        DeclaredRule {
            selector: selector.to_string(),
            href: None,
            index: 0,
            origin: "author".to_string(),
            declarations: pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        }
    }

    fn declared(rules: Vec<DeclaredRule>, at_rules: Vec<AtRuleInfo>) -> DeclaredFacet {
        DeclaredFacet {
            rules,
            blocked_sheets: Vec::new(),
            at_rules,
            truncated: false,
        }
    }

    fn document(
        mut nodes: Vec<DiffNode>,
        declared: Option<DeclaredFacet>,
        actions: Vec<Value>,
        viewport: Option<(f64, f64)>,
    ) -> DiffDocument {
        // Several family C checks require the pseudo facet; an empty map
        // means "captured, no pseudo styles" rather than "not captured".
        if let Some(first) = nodes.first_mut()
            && first.pseudo.is_none()
        {
            first.pseudo = Some(Map::new());
        }
        DiffDocument {
            nodes,
            actions,
            viewport,
            declared,
            document: Some(DocumentFacet::default()),
            env: Some(EnvFacet::default()),
            matrix: Vec::<MatrixTag>::new(),
            ..DiffDocument::default()
        }
    }

    fn run_doc(doc: &DiffDocument) -> Vec<CheckLine> {
        let opts = crate::modern::ModernOptions::default();
        let ctx = ModernCtx::new(doc, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        out
    }

    fn ids(lines: &[CheckLine]) -> Vec<&str> {
        lines.iter().map(|line| line.check.as_str()).collect()
    }

    #[test]
    fn catalog_has_all_sixteen_family_c_ids() {
        let expected = [
            "autofill-purpose-tokens",
            "credential-autocomplete-tokens",
            "payment-field-single-input",
            "hint-above-input",
            "user-invalid-aria-sync",
            "otp-single-input",
            "select-base-semantics",
            "select-placeholder-option",
            "select-option-noncolor-state",
            "select-picker-animation-guard",
            "select-picker-open-layout",
            "autofill-noncolor-indicator",
            "accent-color-contrast",
            "field-sizing-constraints",
            "interaction-state-feedback",
            "persistent-bar-budget",
        ];
        assert_eq!(specs().len(), expected.len());
        for id in expected {
            let spec = specs().iter().find(|s| s.id == id).expect("id do catálogo");
            assert!(!spec.guides.is_empty(), "sem guia: {id}");
            assert!(!spec.keep_when.is_empty(), "sem keepWhen: {id}");
        }
    }

    // --- autofill-purpose-tokens -------------------------------------------

    #[test]
    fn autofill_purpose_fails_email_without_token() {
        let input = attrs(
            aria(node(1, "INPUT", "input#email"), &[("name", "Email")]),
            &[("type", "email"), ("name", "email")],
        );
        let lines = run_doc(&document(vec![input], None, Vec::new(), None));
        assert!(ids(&lines).contains(&"autofill-purpose-tokens"));
        assert_eq!(lines[0].severity, Some(Severity::Fail));
    }

    #[test]
    fn autofill_purpose_keeps_valid_token_and_honeypot() {
        let valid = attrs(
            aria(node(1, "INPUT", "input#email"), &[("name", "Email")]),
            &[("type", "email"), ("autocomplete", "email")],
        );
        let lines = run_doc(&document(vec![valid], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"autofill-purpose-tokens"));

        let honeypot = attrs(
            aria(node(1, "INPUT", "input#email"), &[("aria_hidden", "true")]),
            &[("type", "email")],
        );
        let lines = run_doc(&document(vec![honeypot], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"autofill-purpose-tokens"));

        let opt_out = attrs(
            node(1, "INPUT", "input#email"),
            &[("type", "email"), ("autocomplete", "off")],
        );
        let lines = run_doc(&document(vec![opt_out], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"autofill-purpose-tokens"));
    }

    // --- credential-autocomplete-tokens -----------------------------------

    #[test]
    fn credential_tokens_warn_on_missing_token_and_keep_valid() {
        let password = attrs(
            node(1, "INPUT", "#password"),
            &[("type", "password"), ("name", "password")],
        );
        let lines = run_doc(&document(vec![password], None, Vec::new(), None));
        assert!(ids(&lines).contains(&"credential-autocomplete-tokens"));

        let good = attrs(
            node(1, "INPUT", "#password"),
            &[
                ("type", "password"),
                ("autocomplete", "current-password"),
                ("required", ""),
            ],
        );
        let lines = run_doc(&document(vec![good], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"credential-autocomplete-tokens"));
    }

    #[test]
    fn credential_tokens_keep_passkey_only_flow() {
        let email = attrs(
            node(1, "INPUT", "#email"),
            &[("type", "email"), ("autocomplete", "username")],
        );
        let lines = run_doc(&document(vec![email], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"credential-autocomplete-tokens"));
    }

    // --- payment-field-single-input ---------------------------------------

    #[test]
    fn payment_single_input_warns_on_masked_csc_and_keeps_text() {
        let bad = attrs(
            node(1, "INPUT", "#cc-csc"),
            &[
                ("autocomplete", "cc-csc"),
                ("type", "password"),
                ("maxlength", "4"),
            ],
        );
        let lines = run_doc(&document(vec![bad], None, Vec::new(), None));
        assert!(ids(&lines).contains(&"payment-field-single-input"));

        let good = attrs(
            node(1, "INPUT", "#cc-csc"),
            &[
                ("autocomplete", "cc-csc"),
                ("type", "text"),
                ("inputmode", "numeric"),
                ("maxlength", "4"),
            ],
        );
        let lines = run_doc(&document(vec![good], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"payment-field-single-input"));
    }

    #[test]
    fn payment_single_input_detects_split_card_number() {
        let mut form = node(1, "FORM", "form#pay");
        form = child(
            form,
            attrs(
                node(2, "INPUT", "#cc1"),
                &[
                    ("autocomplete", "cc-number"),
                    ("type", "text"),
                    ("inputmode", "numeric"),
                    ("maxlength", "8"),
                ],
            ),
        );
        form = child(
            form,
            attrs(
                node(3, "INPUT", "#cc2"),
                &[
                    ("autocomplete", "cc-number"),
                    ("type", "text"),
                    ("inputmode", "numeric"),
                    ("maxlength", "8"),
                ],
            ),
        );
        let lines = run_doc(&document(vec![form], None, Vec::new(), None));
        let payment: Vec<_> = lines
            .iter()
            .filter(|l| l.check == "payment-field-single-input")
            .collect();
        assert_eq!(payment.len(), 1);
        assert!(payment[0].evidence.text.contains("2 inputs"));
    }

    // --- hint-above-input --------------------------------------------------

    #[test]
    fn hint_above_input_warns_when_below_and_keeps_visually_hidden() {
        let hint = rect(
            attrs(node(2, "SPAN", "#h"), &[("id", "h")]),
            0.0,
            310.0,
            100.0,
            20.0,
        );
        let input = rect(
            aria(
                attrs(node(1, "INPUT", "#cc"), &[("autocomplete", "cc-exp")]),
                &[("aria_describedby", "h")],
            ),
            0.0,
            298.0,
            200.0,
            24.0,
        );
        let lines = run_doc(&document(vec![input, hint], None, Vec::new(), None));
        assert!(ids(&lines).contains(&"hint-above-input"));

        let hidden = rect(
            attrs(node(2, "SPAN", "#h"), &[("id", "h")]),
            0.0,
            310.0,
            0.0,
            0.0,
        );
        let input = rect(
            aria(
                attrs(node(1, "INPUT", "#cc"), &[("autocomplete", "cc-exp")]),
                &[("aria_describedby", "h")],
            ),
            0.0,
            298.0,
            200.0,
            24.0,
        );
        let lines = run_doc(&document(vec![input, hidden], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"hint-above-input"));
    }

    // --- user-invalid-aria-sync -------------------------------------------

    #[test]
    fn user_invalid_sync_positive_and_keep_without_action() {
        let error = visible(attrs(node(2, "DIV", "#err"), &[("id", "err")]), true);
        let input = attrs(
            aria(node(1, "INPUT", "#name"), &[("aria_describedby", "err")]),
            &[("required", ""), ("data-error", "err")],
        );
        let action =
            json!({"action": "click", "selector": "button[type=submit]", "effect": "changed"});
        let lines = run_doc(&document(
            vec![input.clone(), error.clone()],
            None,
            vec![action],
            None,
        ));
        assert!(ids(&lines).contains(&"user-invalid-aria-sync"));

        let lines = run_doc(&document(vec![input, error], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"user-invalid-aria-sync")); // erro visível no load é o estado descrito, não pós-ação
    }

    #[test]
    fn user_invalid_sync_keeps_fallback_class_and_novalidate() {
        let error = visible(node(2, "DIV", "#err"), true);
        let input = attrs(
            aria(node(1, "INPUT", "#name"), &[("aria_describedby", "err")]),
            &[
                ("required", ""),
                ("data-error", "err"),
                ("class", "user-invalid-fallback"),
            ],
        );
        let action = json!({"action": "click", "effect": "changed"});
        let lines = run_doc(&document(vec![input, error], None, vec![action], None));
        assert!(!ids(&lines).contains(&"user-invalid-aria-sync"));
    }

    // --- otp-single-input --------------------------------------------------

    #[test]
    fn otp_multi_box_is_detected_once() {
        let mut form = node(1, "FORM", "form#otp");
        for id in 2..8 {
            form = child(
                form,
                attrs(
                    node(id, "INPUT", &format!("#d{id}")),
                    &[("maxlength", "1"), ("inputmode", "numeric")],
                ),
            );
        }
        let lines = run_doc(&document(vec![form], None, Vec::new(), None));
        let otp: Vec<_> = lines
            .iter()
            .filter(|l| l.check == "otp-single-input")
            .collect();
        assert_eq!(otp.len(), 1);
        assert!(otp[0].evidence.text.contains("6 × input"));
    }

    #[test]
    fn otp_keeps_correct_single_input_and_local_pin() {
        let good = attrs(
            node(1, "INPUT", "#code"),
            &[
                ("autocomplete", "one-time-code"),
                ("type", "text"),
                ("inputmode", "numeric"),
            ],
        );
        let lines = run_doc(&document(vec![good], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"otp-single-input"));

        let pin = attrs(
            node(1, "INPUT", "#pin"),
            &[("name", "pin"), ("type", "text"), ("inputmode", "numeric")],
        );
        let lines = run_doc(&document(vec![pin], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"otp-single-input"));
    }

    #[test]
    fn otp_wrong_type_is_detected() {
        let bad = attrs(
            node(1, "INPUT", "#code"),
            &[
                ("autocomplete", "one-time-code"),
                ("type", "number"),
                ("inputmode", "numeric"),
            ],
        );
        let lines = run_doc(&document(vec![bad], None, Vec::new(), None));
        assert!(ids(&lines).contains(&"otp-single-input"));
    }

    // --- select-base-semantics --------------------------------------------

    #[test]
    fn select_base_semantics_warns_on_missing_name_and_aria_label_option() {
        let option = attrs(
            aria(
                node(2, "OPTION", "option[1]"),
                &[("name", "Expresso"), ("has_text", "true")],
            ),
            &[("aria-label", "Espresso")],
        );
        let select = style(
            node(1, "SELECT", "select#p"),
            "interaction",
            "appearance",
            "base-select",
        );
        let select = child(select, option);
        let lines = run_doc(&document(vec![select], None, Vec::new(), None));
        let found: Vec<_> = lines
            .iter()
            .filter(|l| l.check == "select-base-semantics")
            .collect();
        assert_eq!(found.len(), 1);
        assert!(found[0].evidence.text.contains("attrs.name ausente"));
        assert!(found[0].evidence.text.contains("aria.name ausente"));
    }

    #[test]
    fn select_base_semantics_keeps_native_dropdown_implicit_values() {
        let option = node(2, "OPTION", "option[1]");
        let select = style(
            node(1, "SELECT", "select#p"),
            "interaction",
            "appearance",
            "auto",
        );
        let select = attrs(
            aria(child(select, option), &[("name", "Pref")]),
            &[("name", "p")],
        );
        let lines = run_doc(&document(vec![select], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"select-base-semantics"));
    }

    // --- select-placeholder-option ----------------------------------------

    #[test]
    fn select_placeholder_positive_and_keeps_hidden_disabled_multiple() {
        let placeholder = attrs(node(2, "OPTION", "option[0]"), &[("value", "")]);
        let valid = attrs(node(3, "OPTION", "option[1]"), &[("value", "us")]);
        let select = child(
            child(
                attrs(node(1, "SELECT", "select#c"), &[("required", "")]),
                placeholder,
            ),
            valid,
        );
        let lines = run_doc(&document(vec![select], None, Vec::new(), None));
        assert!(ids(&lines).contains(&"select-placeholder-option"));

        let disabled = attrs(
            node(2, "OPTION", "option[0]"),
            &[("value", ""), ("disabled", "")],
        );
        let select = child(
            attrs(node(1, "SELECT", "select#c"), &[("required", "")]),
            disabled,
        );
        let lines = run_doc(&document(vec![select], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"select-placeholder-option"));

        let multiple = attrs(
            node(1, "SELECT", "select#c"),
            &[("required", ""), ("multiple", "")],
        );
        let multiple = child(
            multiple,
            attrs(node(2, "OPTION", "option[0]"), &[("value", "")]),
        );
        let lines = run_doc(&document(vec![multiple], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"select-placeholder-option"));
    }

    // --- select-option-noncolor-state -------------------------------------

    #[test]
    fn select_option_noncolor_positive_and_checkmark_keep() {
        let option = attrs(
            style(
                node(2, "OPTION", "option[0]"),
                "visual",
                "background-color",
                "rgb(0, 0, 255)",
            ),
            &[("selected", "")],
        );
        let select = pseudo(
            style(
                node(1, "SELECT", "select#s"),
                "interaction",
                "appearance",
                "base-select",
            ),
            "::checkmark",
            "visual",
            "display",
            "none",
        );
        let select = child(select, option);
        let lines = run_doc(&document(vec![select], None, Vec::new(), None));
        assert!(ids(&lines).contains(&"select-option-noncolor-state"));

        let select = pseudo(
            node(1, "SELECT", "select#s"),
            "::checkmark",
            "visual",
            "display",
            "block",
        );
        let select = style(select, "interaction", "appearance", "base-select");
        let select = child(
            select,
            attrs(node(2, "OPTION", "option[0]"), &[("selected", "")]),
        );
        let lines = run_doc(&document(vec![select], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"select-option-noncolor-state"));
    }

    // --- select-picker-animation-guard ------------------------------------

    #[test]
    fn select_picker_animation_positive_and_guard_keep() {
        let select = style(
            node(1, "SELECT", "select#a"),
            "interaction",
            "appearance",
            "base-select",
        );
        let select = style(
            select,
            "animation",
            "transition-property",
            "display, overlay, opacity",
        );
        let decl = declared(Vec::new(), Vec::new());
        let lines = run_doc(&document(vec![select], Some(decl), Vec::new(), None));
        let found: Vec<_> = lines
            .iter()
            .filter(|l| l.check == "select-picker-animation-guard")
            .collect();
        assert_eq!(found.len(), 1);
        assert!(found[0].evidence.text.contains("starting-style"));

        let guard = AtRuleInfo {
            condition_text: "@media (prefers-reduced-motion: reduce)".to_string(),
            index: 0,
            rules: vec![rule("select::picker(select)", &[("transition", "none")])],
        };
        let starting = AtRuleInfo {
            condition_text: "@starting-style".to_string(),
            index: 1,
            rules: vec![rule("select::picker(select)", &[("display", "block")])],
        };
        let select = style(
            node(1, "SELECT", "select#a"),
            "interaction",
            "appearance",
            "base-select",
        );
        let select = style(
            select,
            "animation",
            "transition-property",
            "display, overlay",
        );
        let select = style(select, "animation", "transition-behavior", "allow-discrete");
        let decl = declared(Vec::new(), vec![guard, starting]);
        let lines = run_doc(&document(vec![select], Some(decl), Vec::new(), None));
        assert!(!ids(&lines).contains(&"select-picker-animation-guard"));
    }

    // --- select-picker-open-layout ----------------------------------------

    #[test]
    fn select_picker_open_layout_positive_and_keep() {
        let select = style(
            node(1, "SELECT", "select#s"),
            "interaction",
            "appearance",
            "base-select",
        );
        let decl = declared(
            vec![rule("select::picker(select)", &[("display", "grid")])],
            Vec::new(),
        );
        let lines = run_doc(&document(vec![select], Some(decl), Vec::new(), None));
        assert!(ids(&lines).contains(&"select-picker-open-layout"));

        let select = style(
            node(1, "SELECT", "select#s"),
            "interaction",
            "appearance",
            "base-select",
        );
        let decl = declared(
            vec![rule("select:open::picker(select)", &[("display", "grid")])],
            Vec::new(),
        );
        let lines = run_doc(&document(vec![select], Some(decl), Vec::new(), None));
        assert!(!ids(&lines).contains(&"select-picker-open-layout"));
    }

    // --- autofill-noncolor-indicator --------------------------------------

    #[test]
    fn autofill_noncolor_positive_and_keep() {
        let input = attrs(
            node(1, "INPUT", "#email"),
            &[("type", "text"), ("autocomplete", "email")],
        );
        let decl = declared(
            vec![rule("input:autofill", &[("border-color", "red")])],
            Vec::new(),
        );
        let lines = run_doc(&document(vec![input], Some(decl), Vec::new(), None));
        assert!(ids(&lines).contains(&"autofill-noncolor-indicator"));

        let input = attrs(
            node(1, "INPUT", "#email"),
            &[("type", "text"), ("autocomplete", "email")],
        );
        let decl = declared(
            vec![rule(
                "input:is(:autofill, :-webkit-autofill)",
                &[("box-shadow", "inset 0 0 0 2px #000")],
            )],
            Vec::new(),
        );
        let lines = run_doc(&document(vec![input], Some(decl), Vec::new(), None));
        assert!(!ids(&lines).contains(&"autofill-noncolor-indicator"));
    }

    #[test]
    fn autofill_noncolor_keeps_checkbox_and_no_rule() {
        let input = attrs(node(1, "INPUT", "#c"), &[("type", "checkbox")]);
        let decl = declared(
            vec![rule("input:autofill", &[("border-color", "red")])],
            Vec::new(),
        );
        let lines = run_doc(&document(vec![input], Some(decl), Vec::new(), None));
        assert!(!ids(&lines).contains(&"autofill-noncolor-indicator"));

        let input = attrs(node(1, "INPUT", "#email"), &[("type", "text")]);
        let lines = run_doc(&document(
            vec![input],
            Some(declared(Vec::new(), Vec::new())),
            Vec::new(),
            None,
        ));
        assert!(!ids(&lines).contains(&"autofill-noncolor-indicator"));
    }

    // --- accent-color-contrast --------------------------------------------

    #[test]
    fn accent_color_fails_low_contrast_with_numeric_evidence() {
        let input = style(
            attrs(node(1, "INPUT", "#subscribe"), &[("type", "checkbox")]),
            "visual",
            "accent-color",
            "#bb86fc",
        );
        let input = background(input, "#ffffff");
        let lines = run_doc(&document(vec![input], None, Vec::new(), None));
        let found: Vec<_> = lines
            .iter()
            .filter(|l| l.check == "accent-color-contrast")
            .collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Some(Severity::Fail));
        let evidence = &found[0].evidence;
        assert_eq!(
            evidence.metric.as_deref(),
            Some("accent_color_contrast_ratio")
        );
        assert_eq!(evidence.threshold, Some(json!(3.0)));
        assert!(
            evidence
                .observed
                .as_ref()
                .and_then(Value::as_f64)
                .is_some_and(|v| v < 3.0)
        );
    }

    #[test]
    fn accent_color_keeps_passing_contrast_and_disabled_control() {
        let input = style(
            attrs(node(1, "INPUT", "#subscribe"), &[("type", "checkbox")]),
            "visual",
            "accent-color",
            "#6200ee",
        );
        let input = background(input, "#ffffff");
        let lines = run_doc(&document(vec![input], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"accent-color-contrast"));

        let disabled = attrs(
            node(1, "INPUT", "#sub"),
            &[("type", "checkbox"), ("disabled", "")],
        );
        let disabled = background(
            style(disabled, "visual", "accent-color", "#bb86fc"),
            "#ffffff",
        );
        let lines = run_doc(&document(vec![disabled], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"accent-color-contrast"));
    }

    #[test]
    fn accent_color_borderline_is_warn_and_unknown_background_is_silent() {
        // #9a86e8 vs #ffffff fica na faixa 2,9–3,1.
        let input = style(
            attrs(node(1, "INPUT", "#x"), &[("type", "checkbox")]),
            "visual",
            "accent-color",
            "#9a86e8",
        );
        let input = background(input, "#ffffff");
        let lines = run_doc(&document(vec![input], None, Vec::new(), None));
        let found: Vec<_> = lines
            .iter()
            .filter(|l| l.check == "accent-color-contrast")
            .collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Some(Severity::Warn));

        let unknown = style(
            attrs(node(1, "INPUT", "#x"), &[("type", "checkbox")]),
            "visual",
            "accent-color",
            "#bb86fc",
        );
        let lines = run_doc(&document(
            vec![background(unknown, "image")],
            None,
            Vec::new(),
            None,
        ));
        assert!(!ids(&lines).contains(&"accent-color-contrast"));
    }

    // --- field-sizing-constraints -----------------------------------------

    #[test]
    fn field_sizing_positive_and_keep_with_constraints() {
        let bad = style(
            style(
                attrs(node(1, "INPUT", "#q"), &[("name", "q")]),
                "box_model",
                "field-sizing",
                "content",
            ),
            "box_model",
            "width",
            "100%",
        );
        let lines = run_doc(&document(vec![bad], None, Vec::new(), None));
        assert!(ids(&lines).contains(&"field-sizing-constraints"));

        let good = style(
            style(
                style(
                    node(1, "INPUT", "#q"),
                    "box_model",
                    "field-sizing",
                    "content",
                ),
                "box_model",
                "width",
                "auto",
            ),
            "box_model",
            "min-inline-size",
            "8ch",
        );
        let good = style(good, "box_model", "max-inline-size", "40ch");
        let lines = run_doc(&document(vec![good], None, Vec::new(), None));
        assert!(!ids(&lines).contains(&"field-sizing-constraints"));
    }

    // --- interaction-state-feedback ---------------------------------------

    #[test]
    fn interaction_state_positive_and_keep() {
        let button = attrs(node(1, "BUTTON", "button#send"), &[("id", "send")]);
        let decl = declared(Vec::new(), Vec::new());
        let action = json!({"action": "click", "selector": "button#send", "effect": "no_effect"});
        let lines = run_doc(&document(vec![button], Some(decl), vec![action], None));
        assert!(ids(&lines).contains(&"interaction-state-feedback"));

        let button = attrs(node(1, "BUTTON", "button#send"), &[("id", "send")]);
        let decl = declared(
            vec![
                rule("button#send:hover", &[("color", "red")]),
                rule("button#send:active", &[("transform", "scale(.98)")]),
            ],
            Vec::new(),
        );
        let lines = run_doc(&document(vec![button], Some(decl), Vec::new(), None));
        assert!(!ids(&lines).contains(&"interaction-state-feedback"));
    }

    // --- persistent-bar-budget --------------------------------------------

    #[test]
    fn persistent_bar_positive_and_keep_compact() {
        let bar = rect(
            style(
                node(1, "HEADER", "header.top"),
                "layout",
                "position",
                "sticky",
            ),
            0.0,
            0.0,
            800.0,
            120.0,
        );
        let lines = run_doc(&document(vec![bar], None, Vec::new(), Some((800.0, 640.0))));
        let found: Vec<_> = lines
            .iter()
            .filter(|l| l.check == "persistent-bar-budget")
            .collect();
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].evidence.metric.as_deref(),
            Some("bar_height_viewport_ratio")
        );

        let compact = rect(
            style(
                node(1, "HEADER", "header.top"),
                "layout",
                "position",
                "sticky",
            ),
            0.0,
            0.0,
            800.0,
            48.0,
        );
        let lines = run_doc(&document(
            vec![compact],
            None,
            Vec::new(),
            Some((800.0, 640.0)),
        ));
        assert!(!ids(&lines).contains(&"persistent-bar-budget"));
    }

    #[test]
    fn run_is_byte_for_byte_deterministic() {
        let option = attrs(node(2, "OPTION", "option[0]"), &[("value", "")]);
        let select = child(
            attrs(node(1, "SELECT", "select#c"), &[("required", "")]),
            option,
        );
        let input = attrs(node(3, "INPUT", "#email"), &[("type", "email")]);
        let doc = document(vec![select, input], None, Vec::new(), None);
        let first = serde_json::to_string(&run_doc(&doc)).expect("serialização");
        let second = serde_json::to_string(&run_doc(&doc)).expect("serialização");
        assert_eq!(first, second);
    }
}
