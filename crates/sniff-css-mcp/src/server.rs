//! MCP service exposing sniffing and diffing as tools for AI agents.
//!
//! Tools:
//! - `sniffCSS_page` — capture computed styles from a live page (JSONL),
//!   streaming phase progress via `notifications/progress`.
//! - `sniffCSS_diff` — deterministic diff of two inline JSONL snapshots,
//!   returning only what changed.
//! - `sniffCSS_categories` — available CSS property categories.
//!
//! Resources:
//! - `sniffCSS://prompts/eval` — the AI evaluation prompt template.
//! - `sniffCSS://schemas/eval` — the rigid JSON Schema for the AI answer.

use std::path::Path;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    ErrorData, ListResourcesResult, PaginatedRequestParams, ReadResourceRequestParams,
    ReadResourceResponse, ReadResourceResult, RequestMetaObject, Resource, ResourceContents,
};
use rmcp::service::{Peer, RequestContext, RoleServer};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use sniff_core::config::{Action, OutputFormat, parse_categories, parse_wait_strategy};
use sniff_core::{ElementFilter, OutputConfig, SniffConfig, SniffError, Viewport, WaitStrategy};
use sniff_engine::write_output;

use crate::browser::ChromePool;
use crate::progress::ProgressReporter;
use crate::store::SnapshotStore;

const EVAL_PROMPT: &str = include_str!("../../../docs/eval-prompt.md");
const EVAL_SCHEMA: &str = include_str!("../../../docs/sniffCSS-eval.schema.json");
const GOLDEN_RUN: &str = include_str!("../../../docs/golden-run.md");

/// The MCP service: holds the shared browser pool and the tool router.
#[derive(Debug, Clone)]
pub struct SniffMcpServer {
    pool: ChromePool,
    /// Persisted-snapshot store; lets diff/check tools operate on paths so
    /// full JSONL snapshots never round-trip through the LLM context.
    store: Arc<SnapshotStore>,
    #[expect(dead_code, reason = "tool_handler macro accesses this router field")]
    tool_router: ToolRouter<Self>,
}

impl SniffMcpServer {
    /// Build a server backed by the given browser pool, with the snapshot
    /// store rooted at `SNIFF_SNAPSHOT_DIR` or `sniffCSS` under the CWD.
    pub fn new(pool: ChromePool) -> Self {
        Self::new_with_store(pool, SnapshotStore::from_env())
    }

    /// Build a server with an explicit snapshot store (tests inject a temp dir).
    pub fn new_with_store(pool: ChromePool, store: SnapshotStore) -> Self {
        Self {
            pool,
            store: Arc::new(store),
            tool_router: Self::tool_router(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tool parameters
// ---------------------------------------------------------------------------

/// A single user interaction (`click` | `hover` | `type`) run on the page
/// before capture, to reveal elements that only exist after an action
/// (modals, dropdowns, hover menus, type-ahead suggestions). Actions run
/// in array order; each one waits for its target to appear, then the wait
/// pipeline runs afterwards against the post-interaction DOM.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActionInput {
    /// Action kind: `click`, `hover` or `type`.
    pub r#type: String,
    /// CSS selector of the element to interact with.
    pub selector: String,
    /// Text to insert for `type` actions (required when `type`).
    #[serde(default)]
    pub text: Option<String>,
    /// Milliseconds to wait for the target to appear (default 10000).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Milliseconds to settle after the action (default 150).
    #[serde(default)]
    pub settle_ms: Option<u64>,
}

/// Parameters for the `sniffCSS_page` tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SniffPageRequest {
    /// Page URL to navigate to (http://, https:// or file://).
    pub url: String,
    /// CSS selector of the element(s) to sniff.
    pub selector: String,
    /// How many levels of children to capture (0 = element only).
    #[serde(default)]
    pub depth: usize,
    /// Comma-separated categories: box-model, layout, typography, visual,
    /// transform, animation, interaction, accessibility, all.
    #[serde(default = "default_categories")]
    pub categories: String,
    /// Compact mode: drop redundant/default properties and scope CSS
    /// variables (~55% fewer tokens). Defaults to `true`.
    #[serde(default = "default_true")]
    pub compact: bool,
    /// Capture all CSS custom properties (`--*`). Defaults to `true`.
    #[serde(default = "default_true")]
    pub custom_props: bool,
    /// Attribute used as the stable selector anchor (e.g. `data-testid`),
    /// preferred over generated ids for diffing across deploys.
    #[serde(default)]
    pub stable_key: Option<String>,
    /// Extra DOM attributes to capture verbatim per node (`getAttribute`),
    /// e.g. `["name"]` to validate form field reindexing
    /// (`name="parameters[items][0][title]"`). Emitted under each node's
    /// `attrs` map.
    #[serde(default)]
    pub attributes: Vec<String>,
    /// Pseudo-elements to capture alongside the element, e.g. `::before`.
    #[serde(default)]
    pub pseudo: Vec<String>,
    /// Wait strategies, e.g. `["network-idle:1200:60000"]`.
    #[serde(default)]
    pub wait: Vec<String>,
    /// Ordered user interactions run before capture to reveal elements that
    /// only exist after an action (modals, dropdowns, hover menus, type-ahead
    /// suggestions). Each entry is `{type: "click"|"hover"|"type", selector,
    /// text?, timeout_ms?, settle_ms?}`; e.g. `[{"type":"click",
    /// "selector":"#open-modal"}]` opens a modal before capturing its `.modal`.
    /// Actions run in array order; chained flows (modal → mini-modal → input)
    /// work by giving each step's selector.
    #[serde(default)]
    pub actions: Vec<ActionInput>,
    /// Map the UI effects of each action into a reserved `__actions` area in
    /// the snapshot: what appeared/disappeared/changed and where (rect,
    /// on-screen, out-of-view offset, distance from the action point), plus a
    /// `no_effect` marker when an interaction changed nothing. Defaults to
    /// `true` when `actions` is set.
    #[serde(default = "default_true")]
    pub effects: bool,
    /// Cap on how many appeared/removed/changed elements each `__actions`
    /// entry reports (largest areas first). Defaults to 10.
    #[serde(default)]
    pub effects_limit: Option<usize>,
    /// Emulated viewport as `WxH`.
    #[serde(default = "default_viewport")]
    pub viewport: String,
    /// Output format: `jsonl` (tree), `jsonl-flat`, or `json`.
    #[serde(default = "default_format")]
    pub format: String,
    /// Freeze animations/transitions before capture for deterministic
    /// snapshots of dynamic pages (prefers-reduced-motion + cancel
    /// animations + `animation/transition: none !important`). Defaults to
    /// `true`.
    #[serde(default = "default_true")]
    pub stabilize: bool,
    /// Keep elements the page considers hidden (`display:none`,
    /// `visibility:hidden`, zero-size). CLI `--no-visible`; useful for
    /// scroll-reveal animations (WOW.js) that leave content
    /// `visibility:hidden` until animated. Defaults to `false`.
    #[serde(default)]
    pub include_invisible: bool,
    /// Skip elements matching this selector (repeatable).
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Keep only elements at least this wide (px).
    #[serde(default)]
    pub min_width: Option<f64>,
    /// Keep only elements at least this tall (px).
    #[serde(default)]
    pub min_height: Option<f64>,
    /// Capture a PNG of the page (post-stabilize, post-interaction) and
    /// persist it beside the snapshot as `[UTC]-[path]-[selector].png`,
    /// returning its `screenshot_path`. Defaults to `false`.
    #[serde(default)]
    pub screenshot: bool,
    /// When `screenshot` is set, capture the full scrollable document
    /// instead of only the visible viewport.
    #[serde(default)]
    pub screenshot_full_page: bool,
    /// Derive and emit a measured WCAG `contrast` facet per node.
    /// Defaults to `true`.
    #[serde(default = "default_true")]
    pub contrast: bool,
    /// Capture the browser-computed accessibility-tree node (`ax`) per
    /// element via the CDP `Accessibility` domain. Defaults to `true`.
    #[serde(default = "default_true")]
    pub include_ax: bool,
    /// Capture the full accessibility subtree for the matched elements and
    /// emit it as a `__ax_tree` document (implies `include_ax`).
    #[serde(default)]
    pub ax_tree: bool,
    /// Full-fidelity mode: disables every AI optimization at once
    /// (`compact`, `custom_props`, `stabilize`, `contrast`, `include_ax`).
    /// Defaults to `false`.
    #[serde(default)]
    pub full: bool,
    /// Persist the snapshot to disk as `sniffCSS/[domain]/[UTC]-[path]-
    /// [selector].jsonl` (root overridable via `SNIFF_SNAPSHOT_DIR`). Defaults
    /// to `true` so diff/check can run on paths instead of inline JSONL.
    #[serde(default = "default_true")]
    pub persist: bool,
    /// What to return: `summary` (default) returns the token-lean per-node
    /// digest (structural skeleton + curated css subset + contrast + aria)
    /// while still persisting the full snapshot for later diff/check by
    /// path; `reference` returns only a tiny `{"__sniff": {path, url,
    /// selector, nodes}}` line — the most token-efficient capture handle;
    /// `jsonl` returns the full snapshot inline.
    #[serde(default = "default_return_mode", rename = "return")]
    pub return_mode: String,
}

/// Parameters for the `sniffCSS_diff` tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiffRequest {
    /// Base snapshot JSONL (output of `sniffCSS_page`); ignored when `base_path`
    /// is set.
    #[serde(default)]
    pub base_jsonl: String,
    /// Head snapshot JSONL (output of `sniffCSS_page`); ignored when `head_path`
    /// is set.
    #[serde(default)]
    pub head_jsonl: String,
    /// Ignore value changes smaller than this in the same unit
    /// (e.g. 0.5 absorbs 16px -> 16.2px). 0 disables tolerance.
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
    /// Property names whose changes never mark a node as changed
    /// (volatile/animated props), e.g. `["transform", "opacity"]`.
    #[serde(default)]
    pub ignore_props: Vec<String>,
    /// Suppress ADDED/REMOVED lines (report only CHANGED) — for lists
    /// whose item count varies by design.
    #[serde(default)]
    pub ignore_structural: bool,
    /// Base snapshot on disk (root-relative or absolute), produced by a
    /// persisted `sniffCSS_page`. Precedence over `base_jsonl`; keeps the full
    /// snapshot out of the tool call.
    #[serde(default)]
    pub base_path: Option<String>,
    /// Head snapshot on disk (root-relative or absolute). Precedence over
    /// `head_jsonl`.
    #[serde(default)]
    pub head_path: Option<String>,
}

/// Parameters for the `sniffCSS_check` tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CheckRequest {
    /// Snapshot JSONL (output of `sniffCSS_page`) to evaluate offline; ignored
    /// when `path` is set.
    #[serde(default)]
    pub jsonl: String,
    /// Run the uniformity check (odd card out among sibling instances).
    #[serde(default = "default_true")]
    pub uniform: bool,
    /// Run the derived rule checks (contrast, target size, focus, alt).
    #[serde(default = "default_true")]
    pub rules: bool,
    /// Tolerance for numeric uniformity deviations (same unit).
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
    /// Snapshot on disk (root-relative or absolute), produced by a persisted
    /// `sniffCSS_page`. Precedence over `jsonl`.
    #[serde(default)]
    pub path: Option<String>,
}

/// Parameters for the `sniffCSS_snapshots` tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListSnapshotsRequest {
    /// Filter by sanitized host directory (e.g. `localhost_3000`).
    #[serde(default)]
    pub domain: Option<String>,
    /// Filter by target identity (`[path]-[selector]`, e.g. `products_42-card`).
    #[serde(default)]
    pub target: Option<String>,
    /// Return only the N most recent snapshots.
    #[serde(default)]
    pub limit: Option<usize>,
}

fn default_categories() -> String {
    "all".into()
}
fn default_true() -> bool {
    true
}
fn default_viewport() -> String {
    "1366x768".into()
}
fn default_format() -> String {
    "jsonl".into()
}
fn default_tolerance() -> f64 {
    0.5
}
fn default_return_mode() -> String {
    "summary".into()
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

#[tool_router]
impl SniffMcpServer {
    /// Capture computed styles from a live page as JSONL.
    #[tool(
        name = "sniffCSS_page",
        description = "Capture the real computed CSS styles of elements on a page. By default the snapshot \
                        is persisted to disk as sniffCSS/[domain]/[UTC]-[path]-[selector].jsonl and the \
                        tool returns the token-lean per-node summary digest (structural skeleton + \
                        curated css subset + contrast + aria) — pass return=\"reference\" for only a tiny \
                        __sniff handle (path, url, selector, node count), or return=\"jsonl\" for the full \
                        snapshot inline. Feed two persisted captures to sniffCSS_diff via \
                        base_path/head_path (or sniffCSS_check via path) so the full snapshot never \
                        enters the LLM context. Each node carries readable fields (tag, \
                        selector, path, rect, metrics, styles grouped by category) plus \
                        is_user_noticeable and computed_style_hash. The AI-optimized defaults are \
                         already ON: compact (dedup, ~55% fewer tokens), custom_props (CSS variables), \
                         stabilize (freeze animations for deterministic snapshots), contrast (measured \
                         WCAG ratio) and include_ax (browser accessibility node). Pass full=true to \
                         capture full-fidelity output (disables all five at once), or set any flag to \
                         false to opt out individually. Add stable_key (e.g. data-testid) for selectors \
                         that stay matchable across deploys. For elements that only exist after an \
                         interaction, pass actions=[{type:\"click\", selector:\"#open-modal\"}, ...] \
                         (click/hover/type, ordered — chains like modal → mini-modal → input just list \
                         each step's selector) to reveal modals, dropdowns, hover menus and \
                         type-ahead panels before the wait pipeline runs and capture happens. When \
                         actions are set, the snapshot also carries a __actions UI-effect map (default \
                         ON; effects:false to omit): per action, what appeared/disappeared/changed and \
                         where — rect, on-screen flag, out-of-view offset, distance from the action \
                         point — plus a no_effect marker when the interaction changed nothing. Extra \
                         evidence: attributes=[\"name\"] captures verbatim DOM attributes per node under \
                         attrs (e.g. form field reindexing); include_invisible=true (+ wait:[\"delay:...\"]) \
                         captures content hidden by scroll-reveal animations (WOW.js); screenshot=true \
                         (+ screenshot_full_page) persists a PNG beside the snapshot and adds \
                         screenshot_path to the __sniff reference."
    )]
    pub async fn sniff_css_page(
        &self,
        params: Parameters<SniffPageRequest>,
        meta: RequestMetaObject,
        peer: Peer<RoleServer>,
    ) -> Result<String, ErrorData> {
        let request = params.0;
        let reporter = ProgressReporter::new(&meta, &peer);
        let config =
            build_sniff_config(&request).map_err(|e| ErrorData::invalid_params(e, None))?;

        reporter.report(0.05, "acquiring browser slot").await?;
        let outcome = self
            .pool
            .sniff_with(&config, {
                let reporter = reporter.clone();
                move |phase| {
                    let reporter = reporter.clone();
                    async move {
                        let _ = reporter
                            .report(phase.progress(), &phase_message(&phase))
                            .await;
                    }
                }
            })
            .await
            .map_err(to_mcp_error)?;

        let mut buf = Vec::new();
        write_output(&mut buf, &outcome, &config.output)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let jsonl =
            String::from_utf8(buf).map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let rel_path = if request.persist {
            Some(
                self.store
                    .save(&config, &jsonl)
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?,
            )
        } else {
            None
        };

        let screenshot_path = if request.screenshot {
            match &outcome.screenshot {
                Some(bytes) => Some(
                    self.store
                        .save_bytes(&config, "png", bytes)
                        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?,
                ),
                None => {
                    return Err(ErrorData::internal_error(
                        "screenshot requested but the engine returned no image".to_string(),
                        None,
                    ));
                }
            }
        } else {
            None
        };

        match request.return_mode.as_str() {
            "reference" => {
                let path = rel_path.ok_or_else(|| {
                    ErrorData::invalid_params("return=reference requires persist:true", None)
                })?;
                let mut reference = serde_json::json!({
                    "__sniff": {
                        "path": path.display().to_string(),
                        "url": request.url,
                        "selector": request.selector,
                        "nodes": outcome.snapshots.len(),
                        "actions": outcome.actions.as_ref().and_then(|a| a.as_array()).map(|a| a.len()),
                    }
                });
                if let Some(shot) = &screenshot_path {
                    reference["__sniff"]["screenshot_path"] =
                        serde_json::Value::String(shot.display().to_string());
                }
                serde_json::to_string(&reference)
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))
            }
            "summary" => {
                // Full JSONL is still persisted (when persist:true) for
                // later diff/check by path; the response carries only the
                // token-lean structural digest.
                let summary_config = OutputConfig {
                    format: OutputFormat::Summary,
                    pretty: false,
                    ..config.output.clone()
                };
                let mut buf = Vec::new();
                write_output(&mut buf, &outcome, &summary_config)
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                String::from_utf8(buf).map_err(|e| ErrorData::internal_error(e.to_string(), None))
            }
            _ => Ok(jsonl),
        }
    }

    /// Deterministically diff two JSONL snapshots.
    #[tool(
        name = "sniffCSS_diff",
        description = "Deterministically diff two sniffCSS_page snapshots and return only what changed: \
                       CHANGED nodes with before/after values per property (styles, pseudo, aria, \
                       contrast, ax, rect, metrics, is_user_noticeable), ADDED/REMOVED nodes with \
                       their full snapshot, plus a final __diff_summary line. When both snapshots carry \
                       a __actions UI-effect map (captures with actions), it is also compared: \
                       ACTION_CHANGED/ACTION_ADDED/ACTION_REMOVED lines surface interaction regressions \
                       (effect, appeared rect/onscreen/out-of-view, distance from the action point) and \
                       actions_changed counts them in the summary. Snapshots are best \
                       passed as persisted base_path/head_path (from a sniffCSS_page __sniff reference) \
                       so the full JSONL stays out of the tool call; base_jsonl/head_jsonl inline \
                       strings still work. tolerance ignores subpixel jitter in the same unit; \
                       ignore_props skips volatile props (e.g. transform); ignore_structural \
                       suppresses ADDED/REMOVED for variable-count lists. Use this delta as the \
                       input to your evaluation prompt instead of the full snapshots."
    )]
    pub async fn sniff_css_diff(
        &self,
        params: Parameters<DiffRequest>,
    ) -> Result<String, ErrorData> {
        let request = params.0;
        let base = load_snapshot(
            &self.store,
            request.base_path.as_deref(),
            &request.base_jsonl,
            "base",
        )
        .map_err(|e| ErrorData::invalid_params(e, None))?;
        let head = load_snapshot(
            &self.store,
            request.head_path.as_deref(),
            &request.head_jsonl,
            "head",
        )
        .map_err(|e| ErrorData::invalid_params(e, None))?;

        let opts = sniff_css_diff::DiffOptions {
            tolerance: request.tolerance,
            ignore_props: request.ignore_props,
            ignore_structural: request.ignore_structural,
        };
        let (mut deltas, mut stats) = sniff_css_diff::diff_trees(&base.nodes, &head.nodes, &opts);
        sniff_css_diff::diff_actions(&base.actions, &head.actions, &opts, &mut deltas, &mut stats);

        let mut buf = Vec::new();
        sniff_css_diff::write_delta(&mut buf, &deltas)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let summary = serde_json::json!({
            "__diff_summary": {
                "base_nodes": stats.base_nodes,
                "head_nodes": stats.head_nodes,
                "changed": stats.changed,
                "added": stats.added,
                "removed": stats.removed,
                "actions_changed": stats.actions_changed,
            }
        });
        serde_json::to_writer(&mut buf, &summary)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        buf.push(b'\n');

        String::from_utf8(buf).map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }

    /// Run deterministic offline checks over a snapshot.
    #[tool(
        name = "sniffCSS_check",
        description = "Deterministic offline UI checks over a sniffCSS_page snapshot: uniformity \
                       (finds the odd sibling instance against the group norm — the 'odd card \
                       out') and derived rules (measured WCAG contrast, 24x24 target size, visible \
                       focus indicator, hidden focusables, empty alt on large images). Prefer a \
                       persisted snapshot path (from a sniffCSS_page __sniff reference); inline jsonl \
                       still works. No LLM involved: use the results as evidence in the eval prompt."
    )]
    pub async fn sniff_css_check(
        &self,
        params: Parameters<CheckRequest>,
    ) -> Result<String, ErrorData> {
        let request = params.0;
        let nodes = load_snapshot(
            &self.store,
            request.path.as_deref(),
            &request.jsonl,
            "snapshot",
        )
        .map_err(|e| ErrorData::invalid_params(e, None))?;

        let mut buf = Vec::new();
        let mut rule_count = 0usize;
        let mut uniformity_instances = 0usize;
        let mut uniformity_outliers = 0usize;

        if request.rules {
            let lines = sniff_css_check::rules::run_rules(&nodes.nodes);
            rule_count = lines.len();
            let (pass, warn, fail) = sniff_css_check::rules::summarize(&lines);
            sniff_css_check::rules::write_checks(&mut buf, &lines)
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
            tracing::info!("sniffCSS_check: {pass} pass | {warn} warn | {fail} fail");
        }

        if request.uniform {
            let report =
                sniff_css_check::uniformity::check_uniformity(&nodes.nodes, request.tolerance);
            uniformity_instances = report.instances;
            uniformity_outliers = report.outliers.len();
            for outlier in &report.outliers {
                let evidence = outlier
                    .deviations
                    .iter()
                    .map(|d| match (d.norm.as_deref(), d.magnitude) {
                        (Some(norm), Some(mag)) => {
                            format!("{}: {} (norm {norm} ±{mag:0.2})", d.property, d.value)
                        }
                        (Some(norm), None) => format!("{}: {} (norm {norm})", d.property, d.value),
                        (None, _) => format!("{}: {}", d.property, d.value),
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                let line = serde_json::json!({
                    "check": "uniformity",
                    "selector": outlier.selector,
                    "status": "fail",
                    "evidence": format!(
                        "deviates from the {}/{} group norm: {evidence}",
                        report.instances, report.instances
                    ),
                });
                serde_json::to_writer(&mut buf, &line)
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
                buf.push(b'\n');
            }
        }

        let summary = serde_json::json!({
            "__check_summary": {
                "uniformity_instances": uniformity_instances,
                "uniformity_outliers": uniformity_outliers,
                "rules": rule_count,
            }
        });
        serde_json::to_writer(&mut buf, &summary)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        buf.push(b'\n');

        String::from_utf8(buf).map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }

    /// List persisted snapshots to pick base/head pairs for `sniffCSS_diff`.
    #[tool(
        name = "sniffCSS_snapshots",
        description = "List snapshots persisted by sniffCSS_page under sniffCSS/ (or \
                       SNIFF_SNAPSHOT_DIR), newest first. Each line has domain (host directory), \
                       target ([path]-[selector]), path (usable as base_path/head_path or the \
                       sniffCSS_check path), created_at (UTC) and size. Filter by domain/target and \
                       cap with limit to find the base/head pair to diff."
    )]
    pub async fn sniff_css_snapshots(
        &self,
        params: Parameters<ListSnapshotsRequest>,
    ) -> Result<String, ErrorData> {
        let request = params.0;
        let mut entries = self
            .store
            .list()
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        if let Some(domain) = &request.domain {
            entries.retain(|e| &e.domain == domain);
        }
        if let Some(target) = &request.target {
            entries.retain(|e| &e.target == target);
        }
        if let Some(limit) = request.limit {
            entries.truncate(limit);
        }
        let mut buf = Vec::new();
        for entry in &entries {
            serde_json::to_writer(&mut buf, entry)
                .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
            buf.push(b'\n');
        }
        String::from_utf8(buf).map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }

    /// List available CSS property categories.
    #[tool(
        name = "sniffCSS_categories",
        description = "List the CSS property categories accepted by sniffCSS_page."
    )]
    pub async fn sniff_css_categories(&self) -> Result<String, ErrorData> {
        Ok(
            "box-model,layout,typography,visual,transform,animation,interaction,accessibility,all"
                .to_string(),
        )
    }
}

#[tool_handler]
impl ServerHandler for SniffMcpServer {
    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new("sniffCSS://prompts/eval", "Eval prompt")
                .with_title("AI evaluation prompt")
                .with_description("Prompt template to evaluate a sniffCSS-diff delta")
                .with_mime_type("text/markdown"),
            Resource::new("sniffCSS://schemas/eval", "Eval response schema")
                .with_title("AI evaluation JSON schema")
                .with_description("Rigid JSON Schema the AI answer must validate against")
                .with_mime_type("application/json"),
            Resource::new("sniffCSS://guides/golden", "Golden runbook")
                .with_title("Padrão ouro de execução")
                .with_description(
                    "The deterministic capture/diff/check/eval recipe the tools are optimized for",
                )
                .with_mime_type("text/markdown"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let contents = match request.uri.as_str() {
            "sniffCSS://prompts/eval" => {
                ResourceContents::text(EVAL_PROMPT, "sniffCSS://prompts/eval")
            }
            "sniffCSS://schemas/eval" => {
                ResourceContents::text(EVAL_SCHEMA, "sniffCSS://schemas/eval")
            }
            "sniffCSS://guides/golden" => {
                ResourceContents::text(GOLDEN_RUN, "sniffCSS://guides/golden")
            }
            other => {
                return Err(ErrorData::resource_not_found(
                    format!("unknown resource `{other}`"),
                    None,
                ));
            }
        };
        Ok(ReadResourceResult::new(vec![contents]).into())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map `sniffCSS_page` arguments to a full `SniffConfig`.
fn build_sniff_config(request: &SniffPageRequest) -> Result<SniffConfig, String> {
    let wait = if request.wait.is_empty() {
        WaitStrategy::default_pipeline(&request.selector)
    } else {
        request
            .wait
            .iter()
            .map(|spec| parse_wait_strategy(spec))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    let categories = parse_categories(&request.categories).map_err(|e| e.to_string())?;
    let format = OutputFormat::parse_cli(&request.format).map_err(|e| e.to_string())?;
    let viewport = Viewport::parse_cli(&request.viewport).map_err(|e| e.to_string())?;
    let actions = request
        .actions
        .iter()
        .map(action_from_input)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SniffConfig {
        url: request.url.clone(),
        selector: request.selector.clone(),
        depth: request.depth,
        categories,
        custom_properties: Vec::new(),
        pseudo_elements: request.pseudo.clone(),
        wait,
        filter: ElementFilter {
            visible: !request.include_invisible,
            min_width: request.min_width,
            min_height: request.min_height,
            exclude_selectors: request.exclude.clone(),
        },
        output: OutputConfig {
            format,
            include_rect: true,
            include_path: true,
            include_metrics: true,
            normalize_colors: true,
            group_by_category: true,
            pretty: false,
            compact: request.compact && !request.full,
            include_visibility: true,
            include_style_hash: true,
            include_aria: true,
            include_contrast: request.contrast && !request.full,
            include_ax: (request.include_ax && !request.full) || request.ax_tree,
        },
        viewport: Some(viewport),
        include_custom_properties: request.custom_props && !request.full,
        stable_key: request.stable_key.clone(),
        attributes: request.attributes.clone(),
        stabilize: request.stabilize && !request.full,
        ax_tree: request.ax_tree,
        actions,
        effects: request.effects,
        effects_limit: request.effects_limit.unwrap_or(10),
        screenshot: request.screenshot,
        screenshot_full_page: request.screenshot_full_page,
    })
}

/// Map an MCP action input to an engine [`Action`].
fn action_from_input(input: &ActionInput) -> Result<Action, String> {
    let timeout_ms = input.timeout_ms.unwrap_or(Action::DEFAULT_TIMEOUT_MS);
    let settle_ms = input.settle_ms.unwrap_or(Action::DEFAULT_SETTLE_MS);
    let selector = input.selector.clone();
    match input.r#type.as_str() {
        "click" => Ok(Action::Click {
            selector,
            timeout_ms,
            settle_ms,
        }),
        "hover" => Ok(Action::Hover {
            selector,
            timeout_ms,
            settle_ms,
        }),
        "type" => {
            let text = input
                .text
                .clone()
                .ok_or_else(|| "type action requires `text`".to_string())?;
            Ok(Action::Type {
                selector,
                text,
                timeout_ms,
                settle_ms,
            })
        }
        other => Err(format!(
            "unknown action type `{other}` (expected click | hover | type)"
        )),
    }
}

/// Load a snapshot for diff/check from an optional persisted path, falling
/// back to inline JSONL when no path is given.
fn load_snapshot(
    store: &SnapshotStore,
    path: Option<&str>,
    inline: &str,
    label: &str,
) -> Result<sniff_css_diff::DiffDocument, String> {
    if let Some(path) = path {
        let abs = store
            .resolve(Path::new(path))
            .map_err(|e| format!("invalid {label} path `{path}`: {e}"))?;
        sniff_css_diff::load_file_doc(&abs)
            .map_err(|e| format!("cannot load {label} snapshot `{path}`: {e}"))
    } else {
        sniff_css_diff::load_str_doc(inline).map_err(|e| format!("invalid {label} JSONL: {e}"))
    }
}

/// Human-readable message for a pipeline phase.
fn phase_message(phase: &sniff_engine::Phase) -> String {
    match phase {
        sniff_engine::Phase::Navigating => "navigating to page".to_string(),
        sniff_engine::Phase::Interacting => {
            "performing interactions (click/hover/type)".to_string()
        }
        sniff_engine::Phase::Waiting => "waiting for page readiness".to_string(),
        sniff_engine::Phase::Extracting => "extracting computed styles".to_string(),
        sniff_engine::Phase::Accessibility => "capturing accessibility tree".to_string(),
        sniff_engine::Phase::Formatting { nodes } => format!("formatting {nodes} nodes"),
    }
}

/// Translate engine errors into MCP error responses with useful messages.
fn to_mcp_error(e: SniffError) -> ErrorData {
    match e {
        SniffError::NoMatch { selector } => {
            ErrorData::invalid_params(format!("no element matched selector `{selector}`"), None)
        }
        SniffError::Timeout(what) => {
            ErrorData::internal_error(format!("timeout while waiting for {what}"), None)
        }
        other => ErrorData::internal_error(other.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> SniffPageRequest {
        SniffPageRequest {
            url: "http://localhost:3000".into(),
            selector: ".card".into(),
            depth: 0,
            categories: "all".into(),
            compact: true,
            custom_props: true,
            stable_key: Some("data-testid".into()),
            attributes: vec![],
            pseudo: vec![],
            wait: vec![],
            viewport: "1366x768".into(),
            format: "jsonl".into(),
            stabilize: true,
            include_invisible: false,
            exclude: vec![],
            min_width: None,
            min_height: None,
            screenshot: false,
            screenshot_full_page: false,
            contrast: true,
            include_ax: true,
            ax_tree: false,
            persist: true,
            return_mode: "reference".into(),
            full: false,
            actions: vec![],
            effects: true,
            effects_limit: None,
        }
    }

    #[test]
    fn build_config_maps_defaults() {
        let cfg = build_sniff_config(&request()).unwrap();
        assert_eq!(cfg.url, "http://localhost:3000");
        assert_eq!(cfg.selector, ".card");
        assert_eq!(cfg.depth, 0);
        assert_eq!(cfg.stable_key.as_deref(), Some("data-testid"));
        assert!(cfg.output.compact);
        assert!(cfg.output.include_visibility);
        assert!(cfg.output.include_style_hash);
        assert!(cfg.output.include_aria);
        // AI-optimized defaults are ON.
        assert!(cfg.output.include_contrast);
        assert!(cfg.output.include_ax);
        assert!(cfg.include_custom_properties);
        assert!(cfg.stabilize);
        assert!(!cfg.ax_tree);
        assert!(cfg.filter.visible, "visible by default");
        assert_eq!(cfg.output.format, OutputFormat::JsonLines);
        assert_eq!(cfg.wait.len(), 3); // default pipeline
    }

    #[test]
    fn include_invisible_and_filters_map_to_element_filter() {
        let mut req = request();
        req.include_invisible = true;
        req.exclude = vec![".skip".into()];
        req.min_width = Some(120.0);
        req.min_height = Some(40.0);
        let cfg = build_sniff_config(&req).unwrap();
        assert!(!cfg.filter.visible, "include_invisible flips visible off");
        assert_eq!(cfg.filter.exclude_selectors, vec![".skip"]);
        assert_eq!(cfg.filter.min_width, Some(120.0));
        assert_eq!(cfg.filter.min_height, Some(40.0));
    }

    #[test]
    fn full_mode_disables_ai_optimizations() {
        let mut req = request();
        req.full = true;
        let cfg = build_sniff_config(&req).unwrap();
        assert!(!cfg.output.compact);
        assert!(!cfg.output.include_contrast);
        assert!(!cfg.output.include_ax);
        assert!(!cfg.include_custom_properties);
        assert!(!cfg.stabilize);
        // Per-node facets stay on.
        assert!(cfg.output.include_visibility);
        assert!(cfg.output.include_style_hash);
        assert!(cfg.output.include_aria);
    }

    #[test]
    fn ax_tree_implies_ax_in_full_mode() {
        let mut req = request();
        req.full = true;
        req.ax_tree = true;
        let cfg = build_sniff_config(&req).unwrap();
        assert!(cfg.output.include_ax, "ax_tree must imply include_ax");
        assert!(cfg.ax_tree);
    }

    #[test]
    fn build_config_wires_new_flags() {
        let mut req = request();
        req.ax_tree = true;
        let cfg = build_sniff_config(&req).unwrap();
        assert!(cfg.stabilize);
        assert!(cfg.ax_tree);
        assert!(cfg.output.include_contrast);
        assert!(cfg.output.include_ax, "ax_tree implies include_ax");
    }

    #[test]
    fn build_config_parses_wait_specs() {
        let mut req = request();
        req.wait = vec![
            "network-idle:1200:60000".into(),
            "element-ready:.card:visible,has-size:5000".into(),
        ];
        let cfg = build_sniff_config(&req).unwrap();
        assert_eq!(cfg.wait.len(), 2);
        assert!(matches!(
            cfg.wait[0],
            WaitStrategy::NetworkIdle { idle_ms: 1200, .. }
        ));
    }

    #[test]
    fn build_config_rejects_bad_input() {
        let mut req = request();
        req.viewport = "800".into();
        assert!(build_sniff_config(&req).is_err());

        let mut req = request();
        req.categories = "bogus".into();
        assert!(build_sniff_config(&req).is_err());
    }

    #[test]
    fn build_config_maps_ordered_actions() {
        let mut req = request();
        req.actions = serde_json::from_value(serde_json::json!([
            { "type": "click", "selector": "#open-modal", "timeout_ms": 5000 },
            { "type": "type", "selector": "#q", "text": "shoes" },
            { "type": "hover", "selector": ".submenu", "settle_ms": 100 }
        ]))
        .unwrap();
        let cfg = build_sniff_config(&req).unwrap();
        assert_eq!(cfg.actions.len(), 3);
        assert!(
            matches!(&cfg.actions[0], Action::Click { selector, timeout_ms, settle_ms }
            if selector == "#open-modal" && *timeout_ms == 5000
               && *settle_ms == Action::DEFAULT_SETTLE_MS)
        );
        assert!(matches!(&cfg.actions[1], Action::Type { text, .. } if text == "shoes"));
        assert!(matches!(&cfg.actions[2], Action::Hover { settle_ms, .. } if *settle_ms == 100));
    }

    #[test]
    fn build_config_defaults_actions_empty() {
        let cfg = build_sniff_config(&request()).unwrap();
        assert!(cfg.actions.is_empty());
        // Effects map is ON by default.
        assert!(cfg.effects);
        assert_eq!(cfg.effects_limit, 10);
    }

    #[test]
    fn build_config_wires_effects_flags() {
        let mut req = request();
        req.effects = false;
        req.effects_limit = Some(3);
        let cfg = build_sniff_config(&req).unwrap();
        assert!(!cfg.effects);
        assert_eq!(cfg.effects_limit, 3);
    }

    #[test]
    fn action_from_input_rejects_unknown_and_missing_text() {
        let err = action_from_input(&ActionInput {
            r#type: "dblclick".into(),
            selector: "#x".into(),
            text: None,
            timeout_ms: None,
            settle_ms: None,
        })
        .unwrap_err();
        assert!(err.contains("unknown action type `dblclick`"), "got: {err}");

        let err = action_from_input(&ActionInput {
            r#type: "type".into(),
            selector: "#q".into(),
            text: None,
            timeout_ms: None,
            settle_ms: None,
        })
        .unwrap_err();
        assert!(err.contains("requires `text`"), "got: {err}");
    }

    #[test]
    fn phase_messages_are_ordered_and_readable() {
        assert_eq!(
            phase_message(&sniff_engine::Phase::Formatting { nodes: 7 }),
            "formatting 7 nodes"
        );
        assert_eq!(
            phase_message(&sniff_engine::Phase::Accessibility),
            "capturing accessibility tree"
        );
        assert_eq!(
            phase_message(&sniff_engine::Phase::Interacting),
            "performing interactions (click/hover/type)"
        );
        assert_eq!(sniff_engine::Phase::Extracting.progress(), 0.7);
        // Interacting sits between navigating (0.2) and the final wait (0.4)
        // so progress stays monotonic in both action and no-action flows.
        assert!(sniff_engine::Phase::Interacting.progress() > 0.2);
        assert!(sniff_engine::Phase::Interacting.progress() < 0.4);
    }

    #[test]
    fn diff_request_defaults_via_serde() {
        let v: DiffRequest = serde_json::from_value(serde_json::json!({
            "base_jsonl": "a\n",
            "head_jsonl": "b\n",
        }))
        .unwrap();
        assert_eq!(v.tolerance, 0.5);
        assert!(v.ignore_props.is_empty());
        assert!(!v.ignore_structural);
        assert!(v.base_path.is_none());
        assert!(v.head_path.is_none());
    }

    #[test]
    fn sniff_css_page_defaults_to_persist_and_summary() {
        let v: SniffPageRequest = serde_json::from_value(serde_json::json!({
            "url": "http://localhost:3000",
            "selector": ".card",
        }))
        .unwrap();
        assert!(v.persist, "persist defaults to true");
        assert_eq!(v.return_mode, "summary", "return defaults to summary");
    }

    #[test]
    fn check_request_accepts_path() {
        let v: CheckRequest = serde_json::from_value(serde_json::json!({
            "path": "localhost/foo.jsonl",
        }))
        .unwrap();
        assert_eq!(v.path.as_deref(), Some("localhost/foo.jsonl"));
        assert!(v.uniform);
        assert!(v.rules);
    }

    #[test]
    fn load_snapshot_prefers_path_over_inline() {
        let dir = std::env::temp_dir().join(format!("sniffCSS-mcp-srv-{}", std::process::id()));
        let store = SnapshotStore::new(dir.clone());
        let base =
            "{\"id\":1,\"tag\":\"DIV\",\"selector\":\"div.card\",\"depth\":0,\"children\":[]}\n";

        // Without a path, inline JSONL is parsed.
        let doc = load_snapshot(&store, None, base, "base").unwrap();
        assert_eq!(doc.nodes.len(), 1);
        assert!(doc.actions.is_empty());

        // A rejected path errors with a clear message.
        let err = load_snapshot(&store, Some("../escape.jsonl"), base, "base").unwrap_err();
        assert!(
            err.contains("rejected"),
            "traversal must be rejected: {err}"
        );

        // A real persisted file loads.
        std::fs::create_dir_all(&dir).ok();
        let file = dir.join("loaded.jsonl");
        std::fs::write(&file, base).ok();
        let doc = load_snapshot(&store, Some("loaded.jsonl"), "", "base").unwrap();
        assert_eq!(doc.nodes.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resources_embedded_from_docs() {
        assert!(EVAL_PROMPT.contains("sniffCSS-diff"));
        assert!(EVAL_SCHEMA.contains("SniffEvalResponse"));
        assert!(GOLDEN_RUN.contains("sniffCSS"));
    }
}
