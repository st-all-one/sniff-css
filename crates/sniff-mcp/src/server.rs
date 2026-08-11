//! MCP service exposing sniffing and diffing as tools for AI agents.
//!
//! Tools:
//! - `sniff_page` — capture computed styles from a live page (JSONL),
//!   streaming phase progress via `notifications/progress`.
//! - `diff_snapshots` — deterministic diff of two inline JSONL snapshots,
//!   returning only what changed.
//! - `list_categories` — available CSS property categories.
//!
//! Resources:
//! - `sniff://prompts/eval` — the AI evaluation prompt template.
//! - `sniff://schemas/eval` — the rigid JSON Schema for the AI answer.

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

use sniff_core::config::{OutputFormat, parse_categories, parse_wait_strategy};
use sniff_core::{ElementFilter, OutputConfig, SniffConfig, SniffError, Viewport, WaitStrategy};
use sniff_engine::write_output;

use crate::browser::ChromePool;
use crate::progress::ProgressReporter;

const EVAL_PROMPT: &str = include_str!("../../../docs/eval-prompt.md");
const EVAL_SCHEMA: &str = include_str!("../../../docs/sniff-eval.schema.json");

/// The MCP service: holds the shared browser pool and the tool router.
#[derive(Debug, Clone)]
pub struct SniffMcpServer {
    pool: ChromePool,
    #[expect(dead_code, reason = "tool_handler macro accesses this router field")]
    tool_router: ToolRouter<Self>,
}

impl SniffMcpServer {
    /// Build a server backed by the given browser pool.
    pub fn new(pool: ChromePool) -> Self {
        Self {
            pool,
            tool_router: Self::tool_router(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tool parameters
// ---------------------------------------------------------------------------

/// Parameters for the `sniff_page` tool.
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
    /// variables (~55% fewer tokens).
    #[serde(default = "default_true")]
    pub compact: bool,
    /// Capture all CSS custom properties (`--*`).
    #[serde(default)]
    pub custom_props: bool,
    /// Attribute used as the stable selector anchor (e.g. `data-testid`),
    /// preferred over generated ids for diffing across deploys.
    #[serde(default)]
    pub stable_key: Option<String>,
    /// Pseudo-elements to capture alongside the element, e.g. `::before`.
    #[serde(default)]
    pub pseudo: Vec<String>,
    /// Wait strategies, e.g. `["network-idle:1200:60000"]`.
    #[serde(default)]
    pub wait: Vec<String>,
    /// Emulated viewport as `WxH`.
    #[serde(default = "default_viewport")]
    pub viewport: String,
    /// Output format: `jsonl` (tree), `jsonl-flat`, or `json`.
    #[serde(default = "default_format")]
    pub format: String,
}

/// Parameters for the `diff_snapshots` tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiffRequest {
    /// Base snapshot JSONL (output of `sniff_page`).
    pub base_jsonl: String,
    /// Head snapshot JSONL (output of `sniff_page`).
    pub head_jsonl: String,
    /// Ignore value changes smaller than this in the same unit
    /// (e.g. 0.5 absorbs 16px -> 16.2px). 0 disables tolerance.
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
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

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

#[tool_router]
impl SniffMcpServer {
    /// Capture computed styles from a live page as JSONL.
    #[tool(
        name = "sniff_page",
        description = "Capture the real computed CSS styles of elements on a page and return them as \
                       JSONL. Each node carries readable fields (tag, selector, path, rect, metrics, \
                       styles grouped by category) plus is_visible and computed_style_hash. \
                       Use compact=true for ~55% fewer tokens and stable_key (e.g. data-testid) for \
                       selectors that stay matchable across deploys. Feed two runs to diff_snapshots \
                       to detect what changed."
    )]
    pub async fn sniff_page(
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
        String::from_utf8(buf).map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }

    /// Deterministically diff two inline JSONL snapshots.
    #[tool(
        name = "diff_snapshots",
        description = "Deterministically diff two sniff_page JSONL snapshots (passed inline as \
                       strings) and return only what changed: CHANGED nodes with before/after values \
                       per property (styles, pseudo, rect, metrics, is_visible), ADDED/REMOVED nodes \
                       with their full snapshot, plus a final __diff_summary line. tolerance ignores \
                       subpixel jitter in the same unit. Use this delta as the input to your \
                       evaluation prompt instead of the full snapshots."
    )]
    pub async fn diff_snapshots(
        &self,
        params: Parameters<DiffRequest>,
    ) -> Result<String, ErrorData> {
        let request = params.0;
        let base = sniff_diff::load_str(&request.base_jsonl)
            .map_err(|e| ErrorData::invalid_params(format!("invalid base JSONL: {e}"), None))?;
        let head = sniff_diff::load_str(&request.head_jsonl)
            .map_err(|e| ErrorData::invalid_params(format!("invalid head JSONL: {e}"), None))?;

        let opts = sniff_diff::DiffOptions {
            tolerance: request.tolerance,
        };
        let (deltas, stats) = sniff_diff::diff_trees(&base, &head, &opts);

        let mut buf = Vec::new();
        sniff_diff::write_delta(&mut buf, &deltas)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let summary = serde_json::json!({
            "__diff_summary": {
                "base_nodes": stats.base_nodes,
                "head_nodes": stats.head_nodes,
                "changed": stats.changed,
                "added": stats.added,
                "removed": stats.removed,
            }
        });
        serde_json::to_writer(&mut buf, &summary)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        buf.push(b'\n');

        String::from_utf8(buf).map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }

    /// List available CSS property categories.
    #[tool(
        name = "list_categories",
        description = "List the CSS property categories accepted by sniff_page."
    )]
    pub async fn list_categories(&self) -> Result<String, ErrorData> {
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
            Resource::new("sniff://prompts/eval", "Eval prompt")
                .with_title("AI evaluation prompt")
                .with_description("Prompt template to evaluate a sniff-diff delta")
                .with_mime_type("text/markdown"),
            Resource::new("sniff://schemas/eval", "Eval response schema")
                .with_title("AI evaluation JSON schema")
                .with_description("Rigid JSON Schema the AI answer must validate against")
                .with_mime_type("application/json"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let contents = match request.uri.as_str() {
            "sniff://prompts/eval" => ResourceContents::text(EVAL_PROMPT, "sniff://prompts/eval"),
            "sniff://schemas/eval" => ResourceContents::text(EVAL_SCHEMA, "sniff://schemas/eval"),
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

/// Map `sniff_page` arguments to a full `SniffConfig`.
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

    Ok(SniffConfig {
        url: request.url.clone(),
        selector: request.selector.clone(),
        depth: request.depth,
        categories,
        custom_properties: Vec::new(),
        pseudo_elements: request.pseudo.clone(),
        wait,
        filter: ElementFilter::default(),
        output: OutputConfig {
            format,
            include_rect: true,
            include_path: true,
            include_metrics: true,
            normalize_colors: true,
            group_by_category: true,
            pretty: false,
            compact: request.compact,
            include_visibility: true,
            include_style_hash: true,
        },
        viewport: Some(viewport),
        include_custom_properties: request.custom_props,
        stable_key: request.stable_key.clone(),
    })
}

/// Human-readable message for a pipeline phase.
fn phase_message(phase: &sniff_engine::Phase) -> String {
    match phase {
        sniff_engine::Phase::Navigating => "navigating to page".to_string(),
        sniff_engine::Phase::Waiting => "waiting for page readiness".to_string(),
        sniff_engine::Phase::Extracting => "extracting computed styles".to_string(),
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
            custom_props: false,
            stable_key: Some("data-testid".into()),
            pseudo: vec![],
            wait: vec![],
            viewport: "1366x768".into(),
            format: "jsonl".into(),
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
        assert_eq!(cfg.output.format, OutputFormat::JsonLines);
        assert_eq!(cfg.wait.len(), 3); // default pipeline
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
    fn phase_messages_are_ordered_and_readable() {
        assert_eq!(
            phase_message(&sniff_engine::Phase::Formatting { nodes: 7 }),
            "formatting 7 nodes"
        );
        assert_eq!(sniff_engine::Phase::Extracting.progress(), 0.7);
    }

    #[test]
    fn diff_request_defaults_via_serde() {
        let v: DiffRequest = serde_json::from_value(serde_json::json!({
            "base_jsonl": "a\n",
            "head_jsonl": "b\n",
        }))
        .unwrap();
        assert_eq!(v.tolerance, 0.5);
    }

    #[test]
    fn resources_embedded_from_docs() {
        assert!(EVAL_PROMPT.contains("sniff-diff"));
        assert!(EVAL_SCHEMA.contains("SniffEvalResponse"));
    }
}
