//! Family H — security and manifest checks (catalog §4.10).
//!
//! Only signals actually carried by the snapshot are asserted: `__document`
//! scripts/links/meta, `attrs.allow` and `attrs.autocomplete`, plus the global
//! `isSecureContext` flag captured alongside the document/environment facets.
//! Header-only policy (HSTS, COOP/CORP, CORS, CSP delivered as a header), the
//! server-side `.well-known/web-app-origin-association` and the *contents* of
//! the manifest JSON are deliberately never reported: their absence from the
//! snapshot is not evidence of their absence on the server.
//!
//! `__manifest` (a same-origin fetch/parse pass) is out of v0.6 scope, so
//! `manifest-migration-metadata` asserts only what `__document.links` can
//! prove: a manifest link whose `href` is missing or unfetchable cannot carry
//! migration metadata at all. A valid, even cross-origin, link is left alone.

use crate::modern::{CheckSpec, Facet, ModernCtx};
use crate::rules::CheckLine;
use sniff_core::modern::{DocumentFacet, Evidence, LinkInfo};
use sniff_css_diff::DiffNode;
use std::collections::{BTreeMap, BTreeSet};

/// `rel` tokens whose external targets support subresource integrity.
const SRI_LINK_RELS: &[&str] = &["stylesheet", "preload", "modulepreload"];

/// Directives required by the `security`/`trusted-types` guides when the
/// policy is expressed in a `<meta http-equiv="Content-Security-Policy">`.
///
/// A value after the name is a required token (matched case-insensitively).
/// `frame-ancestors` is intentionally absent: it is ignored in `<meta>` and
/// would be a false positive here.
const MANDATORY_CSP_DIRECTIVES: &[&str] = &[
    "script-src",
    "object-src 'none'",
    "base-uri 'none'",
    "require-trusted-types-for 'script'",
];

/// All catalog ids of this family, with their guides and keep-when rules.
///
/// The returned slice is the single source of ids for the dispatcher, docs and
/// `--ignore`. Every entry has a `warn` severity: none of these checks rests
/// on a WCAG/WAI-ARIA success criterion.
static SPECS: &[CheckSpec] = &[
    CheckSpec::new(
        "external-subresource-integrity",
        &["secure-external-subresources"],
        &[Facet::Document],
        "keepWhen: cross-origin script/link has integrity, URL is relative/same-origin, \
             rel is dns-prefetch/preconnect, or __document is absent. The v0.6 facet has no \
             origin field, so an absolute URL is compared with a canonical href when present.",
    ),
    CheckSpec::new(
        "csp-meta-mandatory-directives",
        &["security", "trusted-types"],
        &[Facet::Document],
        "keepWhen: no meta CSP exists (the policy may be an HTTP header), or all mandatory \
             directives and their tokens are present. frame-ancestors is header-only and is not \
             required; existing 'unsafe-inline'/'strict-dynamic' tokens are never discouraged.",
    ),
    CheckSpec::new(
        "lna-insecure-context-delegation",
        &["local-network-access"],
        &[Facet::Attrs, Facet::Document],
        "keepWhen: isSecureContext is true/unknown, a facet is absent, or attrs.allow has no \
             local-network/loopback-network token. http://localhost is a secure context and is \
             resolved by isSecureContext, not by URL guessing.",
    ),
    CheckSpec::new(
        "email-verification-token-field",
        &["verify-email-ownership"],
        &[Facet::Attrs],
        "keepWhen: no input[autocomplete=email-verification-token] exists, or it has a nonce \
             and shares an ancestor form with input[autocomplete=email]. A missing origin-trial \
             meta is supporting evidence only: the trial may be delivered as a header.",
    ),
    CheckSpec::new(
        "manifest-migration-metadata",
        &["migrate-web-app-origin"],
        &[Facet::Document],
        "keepWhen: no rel=manifest link exists, or its href is non-empty and fetchable \
             (relative, http(s), protocol-relative). Membership in migrate_from/migrate_to needs \
             the __manifest pass, out of v0.6 scope, and is never inferred from the DOM.",
    ),
];

/// Registro dos checks da família H (fonte única de ids).
#[must_use]
pub fn specs() -> &'static [CheckSpec] {
    SPECS
}

/// Runs every enabled check of this family, appending deterministic lines.
pub fn run(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>) {
    tracing::debug!(
        family = "security",
        checks = specs().len(),
        "running modern check family"
    );
    for spec in specs() {
        if !ctx.enabled(spec) {
            tracing::debug!(
                check = spec.id,
                "security check suppressed by options or missing facet"
            );
            continue;
        }
        let before = out.len();
        match spec.id {
            "external-subresource-integrity" => check_sri(ctx, out, spec),
            "csp-meta-mandatory-directives" => check_csp(ctx, out, spec),
            "lna-insecure-context-delegation" => check_lna(ctx, out, spec),
            "email-verification-token-field" => check_email(ctx, out, spec),
            "manifest-migration-metadata" => check_manifest(ctx, out, spec),
            _ => {}
        }
        tracing::trace!(
            check = spec.id,
            findings = out.len() - before,
            "security check finished"
        );
    }
}

/// Flags cross-origin scripts and integrity-eligible links without SRI.
fn check_sri(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(doc) = ctx.doc.document.as_ref() else {
        return;
    };
    for script in &doc.scripts {
        let Some(src) = script.src.as_deref().filter(|src| !src.trim().is_empty()) else {
            continue;
        };
        if !is_external_subresource(src, doc)
            || script
                .integrity
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        {
            continue;
        }
        tracing::trace!(kind = "script", "external subresource without integrity");
        ctx.emit_global(
            out,
            spec,
            "script[src]",
            Evidence::new(
                format!("cross-origin script without integrity: {src}"),
                "__document.scripts",
            ),
        );
    }
    for link in &doc.links {
        if !SRI_LINK_RELS.iter().any(|rel| rel_has_token(link, rel)) {
            continue;
        }
        let Some(href) = link.href.as_deref().filter(|href| !href.trim().is_empty()) else {
            continue;
        };
        if !is_external_subresource(href, doc)
            || link
                .integrity
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        {
            continue;
        }
        tracing::trace!(kind = "link", rel = %link.rel, "external subresource without integrity");
        ctx.emit_global(
            out,
            spec,
            "link[rel]",
            Evidence::new(
                format!("cross-origin {} without integrity: {href}", link.rel),
                "__document.links",
            ),
        );
    }
}

/// Returns the `authority` (host, optional port) of an HTTP(S) URL.
///
/// Protocol-relative URLs inherit `fallback_scheme` (the document scheme),
/// so default ports are normalized against the scheme actually in use.
/// Other schemes and relative URLs return `None`.
fn url_authority<'a>(url: &'a str, fallback_scheme: Option<&str>) -> Option<&'a str> {
    let (scheme, after_scheme) = if let Some(rest) = url.strip_prefix("//") {
        (fallback_scheme?, rest)
    } else {
        let (scheme, rest) = url.split_once("://")?;
        if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
            return None;
        }
        (scheme, rest)
    };
    let authority = after_scheme.split(['/', '?', '#']).next()?;
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    // `https://host` and `https://host:443` are the same origin.
    let host = if scheme.eq_ignore_ascii_case("https") {
        host.strip_suffix(":443").unwrap_or(host)
    } else {
        host.strip_suffix(":80").unwrap_or(host)
    };
    (!host.is_empty()).then_some(host)
}

fn url_scheme(url: &str) -> Option<&str> {
    if url.starts_with("//") {
        return None;
    }
    let (scheme, _) = url.split_once("://")?;
    (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")).then_some(scheme)
}

fn url_origin(url: &str, fallback_scheme: Option<&str>) -> Option<String> {
    let scheme = url_scheme(url).or(fallback_scheme)?;
    let authority = url_authority(url, fallback_scheme)?;
    Some(format!(
        "{}://{}",
        scheme.to_ascii_lowercase(),
        authority.to_ascii_lowercase()
    ))
}

/// Best-effort own-origin hint (scheme + normalized authority).
fn document_origin(doc: &DocumentFacet) -> Option<String> {
    let scheme = doc.origin.as_deref().and_then(url_scheme);
    doc.origin
        .as_deref()
        .and_then(|origin| url_origin(origin, None))
        .or_else(|| {
            doc.links
                .iter()
                .find(|link| rel_has_token(link, "canonical"))
                .and_then(|link| link.href.as_deref())
                .and_then(|href| url_origin(href, scheme))
        })
        .or_else(|| {
            doc.meta
                .get("origin")
                .and_then(|value| url_origin(value, scheme))
        })
        .or_else(|| {
            doc.meta
                .get("document-url")
                .and_then(|value| url_origin(value, scheme))
        })
}

/// Whether a URL is an absolute foreign subresource candidate.
fn is_external_subresource(url: &str, doc: &DocumentFacet) -> bool {
    let Some(origin) = url_origin(url, doc.origin.as_deref().and_then(url_scheme)) else {
        return false;
    };
    document_origin(doc).is_none_or(|own| own != origin)
}

/// Case-insensitive `rel` token test (handles `rel="stylesheet alternate"`).
fn rel_has_token(link: &LinkInfo, token: &str) -> bool {
    link.rel
        .split_whitespace()
        .any(|value| value.eq_ignore_ascii_case(token))
}

/// Normalizes a `meta` key so `http-equiv:Content-Security-Policy`,
/// `http_equiv_content_security_policy` and `content-security-policy` agree.
fn meta_key_is(key: &str, expected: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase().replace(['_', ':'], "-");
    let canonical = normalized
        .strip_prefix("http-equiv-")
        .unwrap_or(&normalized);
    canonical == expected
}

/// Finds the first meta CSP value in deterministic `BTreeMap` key order.
fn csp_meta(doc: &DocumentFacet) -> Option<(&str, &str)> {
    doc.meta.iter().find_map(|(key, content)| {
        (meta_key_is(key, "content-security-policy") || meta_key_is(key, "csp"))
            .then_some((key.as_str(), content.as_str()))
    })
}

/// Lowercased directives keyed by name; the first occurrence wins (CSP rules).
fn csp_directives(content: &str) -> BTreeMap<String, String> {
    let mut directives = BTreeMap::new();
    for directive in content.split(';') {
        let directive = directive.trim();
        let Some(name) = directive.split_whitespace().next() else {
            continue;
        };
        directives
            .entry(name.to_ascii_lowercase())
            .or_insert_with(|| directive.to_ascii_lowercase());
    }
    directives
}

/// Checks a mandatory directive and, when present, its required token.
fn csp_has(directives: &BTreeMap<String, String>, required: &str) -> bool {
    let (name, token) = match required.split_once(' ') {
        Some((name, token)) => (name, token),
        None => (required, ""),
    };
    let Some(actual) = directives.get(&name.to_ascii_lowercase()) else {
        return false;
    };
    if token.is_empty() {
        return true;
    }
    let sources: Vec<&str> = actual.split_whitespace().skip(1).collect();
    if token.eq_ignore_ascii_case("'none'") {
        // Browsers ignore 'none' when other source expressions are present.
        return sources.len() == 1 && sources[0].eq_ignore_ascii_case("'none'");
    }
    sources
        .iter()
        .any(|value| value.eq_ignore_ascii_case(token))
}

/// Reports each mandatory directive missing from an existing meta CSP.
fn check_csp(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(doc) = ctx.doc.document.as_ref() else {
        return;
    };
    let Some((_key, content)) = csp_meta(doc) else {
        tracing::trace!("no meta CSP: policy may be delivered as an HTTP header");
        return;
    };
    let directives = csp_directives(content);
    for required in MANDATORY_CSP_DIRECTIVES {
        if csp_has(&directives, required) {
            continue;
        }
        tracing::trace!(
            directive = required,
            "mandatory directive missing from meta CSP"
        );
        ctx.emit_global(
            out,
            spec,
            "meta[http-equiv=Content-Security-Policy]",
            Evidence::new(
                format!("meta CSP missing {required} (content=\"{content}\")"),
                "__document.meta",
            ),
        );
    }
}

/// Whether a Permissions-Policy-style `allow` delegates local network access.
fn has_lna_token(allow: &str) -> bool {
    allow
        .split([';', ','])
        .flat_map(str::split_whitespace)
        .any(|token| {
            let token = token.trim_matches(['\'', '"']);
            token.eq_ignore_ascii_case("local-network")
                || token.eq_ignore_ascii_case("loopback-network")
        })
}

/// Flags LNA delegation in an explicitly insecure context.
fn check_lna(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(doc) = ctx.doc.document.as_ref() else {
        return;
    };
    if doc.is_secure_context != Some(false) {
        tracing::trace!("isSecureContext is true or unknown: LNA check silenced");
        return;
    }
    for node in &ctx.flat {
        let Some(allow) = ctx.attr(node, "allow") else {
            continue;
        };
        if !has_lna_token(allow) {
            continue;
        }
        tracing::warn!("local network access delegated from an insecure context");
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!(
                    "{}[allow~=local-network] in insecure context — LNA permissions are denied automatically",
                    ctx.tag(node).to_ascii_lowercase()
                ),
                "__document.isSecureContext + attrs.allow",
            ),
        );
    }
}

/// Identity of the nearest ancestor form, from `parent_id` or DOM `path`.
fn form_key(
    ctx: &ModernCtx<'_>,
    by_id: &BTreeMap<u64, &DiffNode>,
    node: &DiffNode,
) -> Option<String> {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if ctx.tag(candidate).eq_ignore_ascii_case("form") {
            return Some(format!("id:{}", candidate.id));
        }
        current = candidate.parent_id.and_then(|id| by_id.get(&id).copied());
    }
    let path = node.path.as_deref()?;
    let segments: Vec<&str> = path
        .split(['/', '>'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();
    for (index, segment) in segments.iter().enumerate() {
        let tag = segment.split(['[', ':']).next().unwrap_or_default().trim();
        if tag.eq_ignore_ascii_case("form") {
            return Some(format!("path:{}", segments[..=index].join("/")));
        }
    }
    None
}

/// Whether the node is an `input` carrying a given `autocomplete` token.
fn is_input_with_autocomplete(ctx: &ModernCtx<'_>, node: &DiffNode, value: &str) -> bool {
    ctx.tag(node).eq_ignore_ascii_case("input")
        && ctx.attr(node, "autocomplete").is_some_and(|actual| {
            actual
                .split_whitespace()
                .any(|token| token.eq_ignore_ascii_case(value))
        })
}

/// Flags email-verification-token inputs without a nonce or without an email
/// field in the same form.
fn check_email(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let by_id: BTreeMap<u64, &DiffNode> = ctx.flat.iter().map(|node| (node.id, *node)).collect();
    let email_forms: BTreeSet<String> = ctx
        .flat
        .iter()
        .filter(|node| is_input_with_autocomplete(ctx, node, "email"))
        .filter_map(|node| form_key(ctx, &by_id, node))
        .collect();
    let has_origin_trial = ctx
        .doc
        .document
        .as_ref()
        .is_some_and(|doc| doc.meta.keys().any(|key| meta_key_is(key, "origin-trial")));
    for node in &ctx.flat {
        if !is_input_with_autocomplete(ctx, node, "email-verification-token") {
            continue;
        }
        let missing_nonce = ctx.attr(node, "nonce").is_none();
        let outside_form =
            form_key(ctx, &by_id, node).is_none_or(|key| !email_forms.contains(&key));
        if !missing_nonce && !outside_form {
            continue;
        }
        let mut reasons = Vec::new();
        if missing_nonce {
            reasons.push("missing nonce");
        }
        if outside_form {
            reasons.push("no email field in the same form");
        }
        if !has_origin_trial {
            reasons.push("origin-trial not declared in __document.meta (may be an HTTP header)");
        }
        tracing::trace!(
            missing_nonce,
            outside_form,
            "incomplete email verification token field"
        );
        ctx.emit(
            out,
            spec,
            node,
            Evidence::new(
                format!(
                    "input[autocomplete=email-verification-token] {} ({})",
                    if missing_nonce && outside_form {
                        "has no nonce and is outside the email form"
                    } else if missing_nonce {
                        "has no nonce"
                    } else {
                        "is outside the email form"
                    },
                    reasons.join("; ")
                ),
                "attrs.autocomplete",
            ),
        );
    }
}

/// Whether a manifest `href` can be fetched (relative, HTTP(S), or absolute
/// path). Non-fetchable pseudo-schemes cannot carry migration metadata.
fn is_fetchable_manifest_href(href: &str) -> bool {
    let trimmed = href.trim();
    let Some((scheme, _)) = trimmed.split_once(':') else {
        return !trimmed.is_empty();
    };
    scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
}

/// Flags manifest links that provably cannot expose migration metadata.
fn check_manifest(ctx: &ModernCtx<'_>, out: &mut Vec<CheckLine>, spec: &CheckSpec) {
    let Some(doc) = ctx.doc.document.as_ref() else {
        return;
    };
    for link in &doc.links {
        if !rel_has_token(link, "manifest") {
            continue;
        }
        let Some(href) = link.href.as_deref() else {
            tracing::trace!("manifest link without href");
            ctx.emit_global(
                out,
                spec,
                "link[rel=manifest]",
                Evidence::new(
                    "link[rel=manifest] has no href — manifest migration metadata cannot be read",
                    "__document.links",
                ),
            );
            continue;
        };
        if href.trim().is_empty() || !is_fetchable_manifest_href(href) {
            tracing::trace!("manifest link with unfetchable href");
            ctx.emit_global(
                out,
                spec,
                "link[rel=manifest]",
                Evidence::new(
                    format!(
                        "link[rel=manifest] has unfetchable href \"{href}\" — migration metadata cannot be read"
                    ),
                    "__document.links",
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modern::{ModernOptions, ModernProfile};
    use crate::rules::RuleStatus;
    use sniff_core::modern::Severity;
    use sniff_core::modern::{EnvFacet, ScriptInfo};
    use sniff_css_diff::DiffDocument;
    use std::collections::BTreeMap;

    fn link(rel: &str, href: Option<&str>, integrity: Option<&str>) -> LinkInfo {
        LinkInfo {
            rel: rel.to_string(),
            href: href.map(String::from),
            as_: None,
            fetchpriority: None,
            integrity: integrity.map(String::from),
            crossorigin: None,
        }
    }

    fn script(src: Option<&str>, integrity: Option<&str>) -> ScriptInfo {
        ScriptInfo {
            src: src.map(String::from),
            type_: None,
            defer: false,
            async_: false,
            fetchpriority: None,
            integrity: integrity.map(String::from),
            blocking: None,
            in_head: Some(true),
            text: None,
            importance: None,
        }
    }

    fn node(id: u64, parent: Option<u64>, tag: &str, attrs: &[(&str, &str)]) -> DiffNode {
        let mut attributes = serde_json::Map::new();
        for (key, value) in attrs {
            attributes.insert(
                (*key).to_string(),
                serde_json::Value::String((*value).to_string()),
            );
        }
        DiffNode {
            id,
            parent_id: parent,
            selector: format!("{tag}#{id}"),
            tag: Some(tag.to_string()),
            path: None,
            depth: None,
            rect: Some(serde_json::json!({"x": 0, "y": 0, "width": 0, "height": 0})),
            metrics: None,
            noticeable: None,
            hash: None,
            styles: Some(serde_json::Map::new()),
            pseudo: Some(serde_json::Map::new()),
            aria: Some(serde_json::Value::Object(serde_json::Map::new())),
            contrast: None,
            ax: Some(serde_json::Value::Object(serde_json::Map::new())),
            attributes: Some(serde_json::Value::Object(attributes)),
            effective_background: None,
            children: Vec::new(),
        }
    }

    fn snapshot(document: DocumentFacet, nodes: Vec<DiffNode>) -> DiffDocument {
        let mut doc = DiffDocument::default();
        doc.document = Some(document);
        doc.env = Some(EnvFacet::default());
        doc.nodes = nodes;
        doc
    }

    fn run_checks(doc: &DiffDocument) -> Vec<CheckLine> {
        let opts = ModernOptions::default();
        let ctx = ModernCtx::new(doc, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        out
    }

    #[test]
    fn strict_profile_escalates_opportunities_to_fail() {
        let document = DocumentFacet {
            scripts: vec![script(Some("https://cdn.example/a.js"), None)],
            ..DocumentFacet::default()
        };
        let doc = snapshot(document, Vec::new());
        let opts = ModernOptions {
            profile: ModernProfile::Strict,
            ..ModernOptions::default()
        };
        let ctx = ModernCtx::new(&doc, &opts);
        let mut out = Vec::new();
        run(&ctx, &mut out);
        let line = out
            .iter()
            .find(|line| line.check == "external-subresource-integrity")
            .expect("SRI opportunity must fire");
        assert_eq!(line.status, RuleStatus::Fail);
        assert_eq!(line.severity, Some(Severity::Fail));
    }

    #[test]
    fn specs_cover_the_catalog_ids_with_guides_and_keep_when() {
        let ids: Vec<&str> = specs().iter().map(|spec| spec.id).collect();
        assert_eq!(
            ids,
            vec![
                "external-subresource-integrity",
                "csp-meta-mandatory-directives",
                "lna-insecure-context-delegation",
                "email-verification-token-field",
                "manifest-migration-metadata",
            ]
        );
        for spec in specs() {
            assert!(!spec.guides.is_empty(), "{} has no guide", spec.id);
            assert!(!spec.keep_when.is_empty(), "{} has no keepWhen", spec.id);
            assert_eq!(spec.severity, Severity::Warn);
        }
        assert_eq!(specs()[2].required, &[Facet::Attrs, Facet::Document]);
    }

    #[test]
    fn sri_flags_external_script_and_link_but_not_local_or_hashed() {
        let document = DocumentFacet {
            links: vec![
                link("canonical", Some("https://site.example/"), None),
                link("stylesheet", Some("https://cdn.example/a.css"), None),
                link(
                    "preload",
                    Some("https://cdn.example/font.woff2"),
                    Some("sha384-abc"),
                ),
                link("preconnect", Some("https://cdn.example"), None),
                link("manifest", Some("/manifest.webmanifest"), None),
            ],
            scripts: vec![
                script(Some("https://cdn.example/a.js"), None),
                script(Some("https://cdn.example/hashed.js"), Some("sha384-abc")),
                script(Some("/local.js"), None),
                script(Some("https://site.example/own.js"), None),
                script(Some("data:text/javascript,1"), None),
            ],
            ..DocumentFacet::default()
        };
        let lines = run_checks(&snapshot(document, Vec::new()));
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(
            lines
                .iter()
                .all(|line| line.check == "external-subresource-integrity")
        );
        assert!(
            lines
                .iter()
                .any(|line| line.evidence.text.contains("https://cdn.example/a.js"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.evidence.text.contains("https://cdn.example/a.css"))
        );
    }

    #[test]
    fn sri_uses_captured_origin_to_silence_same_origin_subresources() {
        let document = DocumentFacet {
            origin: Some("https://site.example".to_string()),
            links: vec![
                link("stylesheet", Some("https://site.example/app.css"), None),
                link(
                    "stylesheet",
                    Some("https://site.example:443/explicit-port.css"),
                    None,
                ),
            ],
            scripts: vec![script(Some("https://site.example/app.js"), None)],
            ..DocumentFacet::default()
        };
        let lines = run_checks(&snapshot(document, Vec::new()));
        assert!(
            !lines
                .iter()
                .any(|line| line.check == "external-subresource-integrity"),
            "same-origin resources must not be flagged: {lines:?}"
        );
    }

    #[test]
    fn sri_flags_different_scheme_on_same_host() {
        let document = DocumentFacet {
            origin: Some("https://site.example".to_string()),
            scripts: vec![script(Some("http://site.example/app.js"), None)],
            ..DocumentFacet::default()
        };
        let lines = run_checks(&snapshot(document, Vec::new()));
        assert!(
            lines
                .iter()
                .any(|line| line.check == "external-subresource-integrity"),
            "HTTP script on HTTPS page is mixed content: {lines:?}"
        );
    }

    #[test]
    fn sri_silences_without_document_and_without_origin_hint_canonical() {
        let mut doc = snapshot(
            DocumentFacet {
                scripts: vec![script(Some("https://cdn.example/a.js"), None)],
                ..DocumentFacet::default()
            },
            Vec::new(),
        );
        doc.document = None;
        assert!(run_checks(&doc).is_empty());

        // Without a canonical hint an absolute URL is the only available
        // external signal; with one it is recognized as same-origin.
        let mut document = DocumentFacet {
            scripts: vec![script(Some("https://site.example/a.js"), None)],
            ..DocumentFacet::default()
        };
        assert_eq!(run_checks(&snapshot(document.clone(), Vec::new())).len(), 1);
        document
            .links
            .push(link("canonical", Some("https://site.example/"), None));
        assert!(run_checks(&snapshot(document, Vec::new())).is_empty());
    }

    #[test]
    fn csp_meta_reports_each_missing_mandatory_directive() {
        let document = DocumentFacet {
            meta: BTreeMap::from([(
                "http-equiv:content-security-policy".to_string(),
                "script-src 'nonce-x' 'strict-dynamic'".to_string(),
            )]),
            ..DocumentFacet::default()
        };
        let lines = run_checks(&snapshot(document, Vec::new()));
        assert_eq!(lines.len(), 3, "{lines:?}");
        let evidence: Vec<&str> = lines
            .iter()
            .map(|line| line.evidence.text.as_str())
            .collect();
        assert!(
            evidence
                .iter()
                .any(|text| text.contains("object-src 'none'"))
        );
        assert!(evidence.iter().any(|text| text.contains("base-uri 'none'")));
        assert!(
            evidence
                .iter()
                .any(|text| text.contains("require-trusted-types-for 'script'"))
        );
    }

    #[test]
    fn csp_meta_accepts_complete_policy_and_silences_without_meta() {
        let complete = "script-src 'nonce-x' 'strict-dynamic'; object-src 'none'; base-uri 'none'; require-trusted-types-for 'script'; frame-ancestors 'self'";
        let document = DocumentFacet {
            meta: BTreeMap::from([("content-security-policy".to_string(), complete.to_string())]),
            ..DocumentFacet::default()
        };
        assert!(run_checks(&snapshot(document, Vec::new())).is_empty());

        let empty = snapshot(DocumentFacet::default(), Vec::new());
        assert!(run_checks(&empty).is_empty());

        // Case-insensitive names and tokens, with whitespace around separators.
        let mixed = DocumentFacet {
            meta: BTreeMap::from([(
                "CONTENT_SECURITY_POLICY".to_string(),
                " SCRIPT-SRC 'self' ; OBJECT-SRC 'NONE' ; BASE-URI 'none' ; REQUIRE-TRUSTED-TYPES-FOR 'SCRIPT' ".to_string(),
            )]),
            ..DocumentFacet::default()
        };
        assert!(run_checks(&snapshot(mixed, Vec::new())).is_empty());
    }

    #[test]
    fn lna_flags_insecure_context_with_local_network_token_only() {
        let nodes = vec![
            node(
                1,
                None,
                "IFRAME",
                &[("allow", "local-network; loopback-network")],
            ),
            node(2, None, "IFRAME", &[("allow", "camera; microphone")]),
            node(3, None, "IFRAME", &[]),
        ];
        let document = |secure: bool| DocumentFacet {
            is_secure_context: Some(secure),
            ..DocumentFacet::default()
        };

        let lines = run_checks(&snapshot(document(false), nodes.clone()));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0].check, "lna-insecure-context-delegation");
        assert_eq!(lines[0].selector, "IFRAME#1");

        assert!(run_checks(&snapshot(document(true), nodes.clone())).is_empty());

        let mut unknown_document = document(false);
        unknown_document.is_secure_context = None;
        assert!(run_checks(&snapshot(unknown_document, nodes.clone())).is_empty());

        let mut no_env = snapshot(document(false), nodes);
        no_env.env = None;
        assert_eq!(run_checks(&no_env).len(), 1);
    }

    #[test]
    fn email_token_needs_nonce_and_shared_form() {
        let nodes = vec![
            node(1, None, "FORM", &[]),
            node(2, Some(1), "INPUT", &[("autocomplete", "email")]),
            // No nonce: emit even though it shares the form with the email.
            node(
                3,
                Some(1),
                "INPUT",
                &[("autocomplete", "email-verification-token")],
            ),
            node(4, None, "FORM", &[]),
            // Nonce but no email field in the form: emit.
            node(
                5,
                Some(4),
                "INPUT",
                &[
                    ("autocomplete", "email-verification-token"),
                    ("nonce", "abc123"),
                ],
            ),
            node(6, None, "FORM", &[]),
            node(7, Some(6), "INPUT", &[("autocomplete", "email")]),
            node(
                8,
                Some(6),
                "INPUT",
                &[
                    ("autocomplete", "email-verification-token"),
                    ("nonce", "abc123"),
                ],
            ),
        ];
        let lines = run_checks(&snapshot(DocumentFacet::default(), nodes));
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(
            lines
                .iter()
                .all(|line| line.check == "email-verification-token-field")
        );
        assert_eq!(lines[0].selector, "INPUT#3");
        assert_eq!(lines[1].selector, "INPUT#5");
        assert!(lines[0].evidence.text.contains("no nonce"));

        // keepWhen: no token field at all.
        let no_token = vec![
            node(1, None, "FORM", &[]),
            node(2, Some(1), "INPUT", &[("autocomplete", "email")]),
        ];
        assert!(run_checks(&snapshot(DocumentFacet::default(), no_token)).is_empty());
    }

    #[test]
    fn email_token_uses_path_when_parent_ids_are_absent() {
        let mut with_form = node(
            1,
            None,
            "INPUT",
            &[
                ("autocomplete", "email-verification-token"),
                ("nonce", "abc"),
            ],
        );
        with_form.path = Some("html/body/form[1]/input[2]".to_string());
        let mut email = node(2, None, "INPUT", &[("autocomplete", "email")]);
        email.path = Some("html/body/form[1]/input[1]".to_string());
        assert!(
            run_checks(&snapshot(
                DocumentFacet::default(),
                vec![with_form.clone(), email]
            ))
            .is_empty()
        );

        let lone = node(
            3,
            None,
            "INPUT",
            &[
                ("autocomplete", "email-verification-token"),
                ("nonce", "abc"),
            ],
        );
        // O e-mail do mesmo formulário precisa estar no snapshot: o check
        // decide por evidência capturada, não por histórico de testes.
        let mut same_form_email = node(4, None, "INPUT", &[("autocomplete", "email")]);
        same_form_email.path = Some("html/body/form[1]/input[1]".to_string());
        let lines = run_checks(&snapshot(
            DocumentFacet::default(),
            vec![with_form, lone, same_form_email],
        ));
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.check == "email-verification-token-field")
                .count(),
            1
        );
    }

    #[test]
    fn manifest_flags_missing_or_unfetchable_href_only() {
        let document = DocumentFacet {
            links: vec![
                link("manifest", None, None),
                link("manifest", Some("   "), None),
                link("manifest", Some("data:application/manifest+json,{}"), None),
                link("manifest", Some("/manifest.webmanifest"), None),
                link(
                    "manifest",
                    Some("https://cdn.example/manifest.webmanifest"),
                    None,
                ),
                link("stylesheet", None, None),
            ],
            ..DocumentFacet::default()
        };
        let lines = run_checks(&snapshot(document, Vec::new()));
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(
            lines
                .iter()
                .all(|line| line.check == "manifest-migration-metadata")
        );

        let valid_only = DocumentFacet {
            links: vec![link("manifest", Some("/manifest.webmanifest"), None)],
            ..DocumentFacet::default()
        };
        assert!(run_checks(&snapshot(valid_only.clone(), Vec::new())).is_empty());
        assert!(run_checks(&snapshot(DocumentFacet::default(), Vec::new())).is_empty());
    }

    #[test]
    fn helper_parsers_cover_canonical_and_malformed_inputs() {
        assert_eq!(
            url_authority("https://cdn.example:8443/a.js", None),
            Some("cdn.example:8443")
        );
        assert_eq!(
            url_authority("HTTP://CDN.EXAMPLE/a.js", None),
            Some("CDN.EXAMPLE")
        );
        assert_eq!(
            url_authority("//cdn.example/a.js", Some("https")),
            Some("cdn.example")
        );
        assert_eq!(
            url_authority("//cdn.example:443/a.js", Some("https")),
            Some("cdn.example")
        );
        assert_eq!(
            url_authority("//cdn.example:80/a.js", Some("https")),
            Some("cdn.example:80")
        );
        assert_eq!(url_authority("cdn.example/a.js", None), None);
        assert_eq!(url_authority("data:text/javascript,1", None), None);
        assert_eq!(url_authority("https:///empty", None), None);
        assert_eq!(
            url_origin("http://site.example/a.js", None),
            Some("http://site.example".to_string())
        );

        assert!(has_lna_token("local-network"));
        assert!(has_lna_token("loopback-network 'self', camera"));
        assert!(!has_lna_token("local-networkish"));
        assert!(!has_lna_token(""));

        assert!(is_fetchable_manifest_href("/manifest.webmanifest"));
        assert!(is_fetchable_manifest_href("//cdn.example/m.json"));
        assert!(is_fetchable_manifest_href("HTTPS://site.example/m.json"));
        assert!(!is_fetchable_manifest_href("data:application/json,{}"));
        assert!(!is_fetchable_manifest_href(
            "blob:https://site.example/uuid"
        ));
        assert!(!is_fetchable_manifest_href("   "));

        assert!(meta_key_is("http_equiv_ORIGIN-TRIAL", "origin-trial"));
        assert!(!meta_key_is("viewport", "content-security-policy"));
    }

    #[test]
    fn output_is_byte_for_byte_deterministic() {
        let document = DocumentFacet {
            links: vec![
                link("canonical", Some("https://site.example/"), None),
                link("stylesheet", Some("https://cdn.example/a.css"), None),
                link("manifest", None, None),
            ],
            scripts: vec![script(Some("https://cdn.example/a.js"), None)],
            meta: BTreeMap::from([(
                "content-security-policy".to_string(),
                "default-src 'self'".to_string(),
            )]),
            is_secure_context: Some(false),
            ..DocumentFacet::default()
        };
        let nodes = vec![node(1, None, "IFRAME", &[("allow", "local-network")])];
        let doc = snapshot(document, nodes);
        let first = serde_json::to_string(&run_checks(&doc)).expect("serializable");
        for _ in 0..4 {
            assert_eq!(
                serde_json::to_string(&run_checks(&doc)).expect("serializable"),
                first
            );
        }
    }
}
