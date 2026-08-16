//! Configuration types that drive a sniffing run.

use crate::error::SniffError;
use crate::properties::{StyleCategory, parse_category};
use serde::{Deserialize, Serialize};

/// Full specification of a sniffing run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SniffConfig {
    /// URL to navigate to.
    pub url: String,
    /// CSS selector for the element(s) to sniff.
    pub selector: String,
    /// How many levels of children to include. `0` means the matched
    /// elements only (no recursion).
    pub depth: usize,
    /// Semantic categories to capture.
    pub categories: Vec<StyleCategory>,
    /// Extra property names requested on top of the categories.
    pub custom_properties: Vec<String>,
    /// Pseudo-elements to capture alongside the element, e.g. `::before`.
    pub pseudo_elements: Vec<String>,
    /// Ordered list of wait strategies executed before extraction.
    pub wait: Vec<WaitStrategy>,
    /// Element filter applied to matched candidates.
    pub filter: ElementFilter,
    /// Output shaping options.
    pub output: OutputConfig,
    /// Emulated viewport size; affects `%`, `vh`, `rem`-derived values
    /// and media queries.
    pub viewport: Option<Viewport>,
    /// Capture all CSS custom properties (`--*`) per element, mirroring
    /// the DevTools Computed panel.
    pub include_custom_properties: bool,
    /// When set, `selector`/`path` are built preferring this attribute
    /// (e.g. `data-testid`) over the DOM `id` as the stable anchor, so
    /// the output can be matched across deploys that change generated ids.
    pub stable_key: Option<String>,
    /// Extra DOM attributes to capture verbatim per node (`getAttribute`),
    /// e.g. `["name"]` to validate form field reindexing
    /// (`name="parameters[items][0][title]"`). Emitted under each node's
    /// `attrs` map. Opt-in; empty by default.
    pub attributes: Vec<String>,
    /// Freeze animations and transitions before capture: pauses running
    /// animations, injects `animation/transition: none !important` and
    /// emulates `prefers-reduced-motion: reduce`, making the captured
    /// state deterministic across runs (no mid-animation jitter).
    pub stabilize: bool,
    /// Capture the full browser-computed accessibility subtree (CDP
    /// `Accessibility` domain) for the matched elements and emit it as
    /// the `__ax_tree` document. Implies `output.include_ax`.
    pub ax_tree: bool,
    /// Ordered user interactions performed on the page before extraction,
    /// to reveal elements that only exist after an action (modals,
    /// dropdowns, hover menus, type-ahead suggestions). When non-empty,
    /// the wait pipeline runs *after* the actions, targeting the
    /// post-interaction DOM.
    pub actions: Vec<Action>,
    /// Map the UI effects of each action (before/after snapshots of the
    /// whole page) into a reserved `__actions` output area: what appeared/
    /// disappeared/changed and where (rect, on-screen, out-of-view offset,
    /// distance from the action point). Default ON when `actions` is set;
    /// disable with `--no-effects`.
    pub effects: bool,
    /// Cap on how many appeared/removed/changed elements each action entry
    /// in `__actions` reports (largest areas first).
    pub effects_limit: usize,
    /// Capture a PNG of the page (`Page.captureScreenshot`) at the end of
    /// the pipeline, after stabilization, waits and interactions. The
    /// decoded bytes land in `SniffOutcome::screenshot`; the caller decides
    /// where to persist them. Deterministic snapshots are unaffected.
    pub screenshot: bool,
    /// When `screenshot` is set, capture the full scrollable document
    /// instead of only the visible viewport.
    pub screenshot_full_page: bool,
    /// Extra HTTP headers applied to every request of this session via
    /// `Network.setExtraHTTPHeaders` before navigation (e.g. a
    /// `X-CMS-AI-Token` used by a stateless CMS AI middleware). Set once per
    /// run; the MCP server can fill them from `SNIFF_DEFAULT_HEADERS`.
    pub headers: Vec<(String, String)>,
    /// Path to a persisted session state (cookies + localStorage, Playwright
    /// `storageState`-style JSON). When set, the state is restored into the
    /// browser **before** navigation: cookies via `Network.setCookies` and
    /// `localStorage` via an init script that runs before page scripts, so a
    /// login performed earlier (see `save_storage_state`) survives into this
    /// capture.
    pub storage_state_path: Option<String>,
    /// When set, the session state (all cookies + the page's `localStorage`)
    /// is written to this path at the end of the pipeline — after any
    /// login/interaction actions ran — so a later capture can pass it back via
    /// `storage_state_path`. Survives browser restarts and the MCP pool
    /// relaunch.
    pub save_storage_state: Option<String>,
}

impl Default for SniffConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            selector: String::new(),
            depth: 0,
            categories: vec![StyleCategory::All],
            custom_properties: Vec::new(),
            pseudo_elements: Vec::new(),
            wait: WaitStrategy::default_pipeline(""),
            filter: ElementFilter::default(),
            output: OutputConfig::default(),
            // Laptop-sized viewport is the assumed development default.
            viewport: Some(Viewport {
                width: 1366,
                height: 768,
            }),
            // The AI-optimized default: capture design tokens too.
            include_custom_properties: true,
            stable_key: None,
            attributes: Vec::new(),
            // Deterministic snapshots are the tool's contract: freeze
            // animations/transitions unless explicitly disabled.
            stabilize: true,
            ax_tree: false,
            actions: Vec::new(),
            effects: true,
            effects_limit: 10,
            screenshot: false,
            screenshot_full_page: false,
            headers: Vec::new(),
            storage_state_path: None,
            save_storage_state: None,
        }
    }
}

/// Emulated viewport dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

impl Viewport {
    /// Parse a `WxH` string (e.g. `1280x720`).
    pub fn parse_cli(input: &str) -> Result<Self, SniffError> {
        let (w, h) = input.trim().split_once(['x', 'X']).ok_or_else(|| {
            SniffError::InvalidOutputFormat(format!("invalid viewport `{input}`"))
        })?;
        let width = w.trim().parse().map_err(|_| {
            SniffError::InvalidOutputFormat(format!("invalid viewport width `{input}`"))
        })?;
        let height = h.trim().parse().map_err(|_| {
            SniffError::InvalidOutputFormat(format!("invalid viewport height `{input}`"))
        })?;
        if width == 0 || height == 0 {
            return Err(SniffError::InvalidOutputFormat(format!(
                "viewport must be non-zero `{input}`"
            )));
        }
        Ok(Self { width, height })
    }
}

/// A single wait/readiness strategy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WaitStrategy {
    /// Wait until `selector` exists in the DOM.
    Selector { selector: String, timeout_ms: u64 },
    /// Wait until the network is idle (no in-flight requests) for
    /// `idle_ms`, up to `timeout_ms`.
    NetworkIdle { idle_ms: u64, timeout_ms: u64 },
    /// Poll `selector` until its computed style satisfies every
    /// condition, up to `timeout_ms`.
    ElementReady {
        selector: String,
        conditions: Vec<ReadyCondition>,
        timeout_ms: u64,
    },
    /// Wait until `document.fonts.ready` resolves.
    FontsLoaded { timeout_ms: u64 },
    /// Wait until `window[flag] === true`.
    AppFlag { flag: String, timeout_ms: u64 },
    /// Fixed sleep.
    Delay { ms: u64 },
}

impl WaitStrategy {
    /// Sensible default pipeline for a selector.
    pub fn default_pipeline(selector: &str) -> Vec<Self> {
        let selector = selector.to_string();
        vec![
            Self::Selector {
                selector: selector.clone(),
                // Keep default waits short so failures are fast and
                // actionable; long waits are passed explicitly.
                timeout_ms: 10_000,
            },
            Self::NetworkIdle {
                idle_ms: 400,
                timeout_ms: 30_000,
            },
            Self::ElementReady {
                selector,
                conditions: vec![ReadyCondition::Visible, ReadyCondition::HasSize],
                timeout_ms: 10_000,
            },
        ]
    }
}

/// Granular readiness condition for [`WaitStrategy::ElementReady`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ReadyCondition {
    /// `display != none` and `visibility != hidden`.
    Visible,
    /// `width > 0 && height > 0`.
    HasSize,
    /// `opacity >= threshold`.
    Opacity(f64),
}

/// A single user interaction performed on the page before extraction,
/// used to reveal elements that only exist after an action (modals,
/// dropdowns, hover menus, type-ahead suggestions).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// Wait for `selector`, scroll it into view and click its center with
    /// a real trusted mouse event (`Input.dispatchMouseEvent`), triggering
    /// the full `pointer`/`mouse`/`click` chain and `:active` state.
    Click {
        selector: String,
        timeout_ms: u64,
        settle_ms: u64,
    },
    /// Wait for `selector`, scroll it into view and move the pointer to its
    /// center (`Input.dispatchMouseEvent` `mouseMoved`), revealing CSS
    /// `:hover` menus and tooltips.
    Hover {
        selector: String,
        timeout_ms: u64,
        settle_ms: u64,
    },
    /// Wait for `selector`, focus it and insert `text`
    /// (`Input.insertText`), revealing search/type-ahead dropdowns.
    Type {
        selector: String,
        text: String,
        timeout_ms: u64,
        settle_ms: u64,
    },
    /// Wait for `selector` to exist, then attach local `files` to an
    /// `<input type=file>` via `DOM.setFileInputFiles`. The browser fires
    /// the `change` event itself, so upload handlers (e.g. an image cropper
    /// inside a CMS modal) run for real. Works even when the input is
    /// visually hidden (`display:none`), which is common for upload buttons.
    /// `files` are resolved by the browser process — in a container the paths
    /// must exist inside it.
    Upload {
        selector: String,
        files: Vec<String>,
        timeout_ms: u64,
        settle_ms: u64,
    },
}

impl Action {
    /// Default timeout for an action target to become ready (ms).
    pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;
    /// Default settle time after an action (ms), letting the interaction
    /// and any resulting layout settle before the next step.
    pub const DEFAULT_SETTLE_MS: u64 = 150;

    /// The kind of this action (`click`, `hover`, `type` or `upload`).
    pub fn kind(&self) -> &'static str {
        match self {
            Action::Click { .. } => "click",
            Action::Hover { .. } => "hover",
            Action::Type { .. } => "type",
            Action::Upload { .. } => "upload",
        }
    }

    /// The target selector of this action.
    pub fn selector(&self) -> &str {
        match self {
            Action::Click { selector, .. }
            | Action::Hover { selector, .. }
            | Action::Type { selector, .. }
            | Action::Upload { selector, .. } => selector,
        }
    }

    /// Milliseconds to wait for the target to become ready.
    pub fn timeout_ms(&self) -> u64 {
        match self {
            Action::Click { timeout_ms, .. }
            | Action::Hover { timeout_ms, .. }
            | Action::Type { timeout_ms, .. }
            | Action::Upload { timeout_ms, .. } => *timeout_ms,
        }
    }

    /// Milliseconds to settle after the interaction.
    pub fn settle_ms(&self) -> u64 {
        match self {
            Action::Click { settle_ms, .. }
            | Action::Hover { settle_ms, .. }
            | Action::Type { settle_ms, .. }
            | Action::Upload { settle_ms, .. } => *settle_ms,
        }
    }
}

/// Element filter applied after matching the selector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementFilter {
    /// Keep only elements that are visible (display != none, visibility != hidden).
    pub visible: bool,
    /// Keep only elements with width >= min_width.
    pub min_width: Option<f64>,
    /// Keep only elements with height >= min_height.
    pub min_height: Option<f64>,
    /// Skip elements matching any of these selectors.
    pub exclude_selectors: Vec<String>,
}

impl Default for ElementFilter {
    fn default() -> Self {
        Self {
            visible: true,
            min_width: None,
            min_height: None,
            exclude_selectors: Vec::new(),
        }
    }
}

/// Output shaping options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Serialization format.
    pub format: OutputFormat,
    /// Include the bounding rect for each element.
    pub include_rect: bool,
    /// Include the DOM path for each element.
    pub include_path: bool,
    /// Include derived metrics (z-index, stacking context).
    pub include_metrics: bool,
    /// Normalize `rgb(...)` colors to hexadecimal.
    pub normalize_colors: bool,
    /// Group styles by category in the output object.
    pub group_by_category: bool,
    /// Pretty-print JSON (single JSON output only).
    pub pretty: bool,
    /// Compact mode: drop redundant logical/default properties and scope
    /// `css_variables` to a single global map plus per-node overrides.
    pub compact: bool,
    /// Emit `is_user_noticeable` per node (display_visible +
    /// accessibility_grade, derived in-page from display/visibility/opacity/rect/aria).
    pub include_visibility: bool,
    /// Emit `computed_style_hash` per node: a fast 64-bit checksum of the
    /// effective styles, for change detection between runs.
    pub include_style_hash: bool,
    /// Emit a resolved `aria` facet per node (role, accessible name,
    /// focusable, aria-* attributes), computed in-page.
    pub include_aria: bool,
    /// Derive and emit a measured WCAG `contrast` facet per node.
    pub include_contrast: bool,
    /// Capture the browser-computed accessibility-tree node (`ax`) per
    /// element via the CDP `Accessibility` domain.
    pub include_ax: bool,
    /// Emulated viewport (width/height) used for the capture, emitted in the
    /// `__meta` line so offline diff/check tools can reason about
    /// viewport-relative geometry (e.g. horizontal overflow). `None` omits it
    /// from the output.
    pub viewport: Option<Viewport>,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: OutputFormat::JsonLines,
            include_rect: true,
            include_path: true,
            include_metrics: true,
            normalize_colors: true,
            group_by_category: true,
            pretty: false,
            // AI-first default: compact drops redundant/default properties
            // (~55% fewer tokens) while keeping every meaningful value.
            compact: true,
            include_visibility: true,
            include_style_hash: true,
            include_aria: true,
            // Measured accessibility facets are cheap (~10-20 tokens per
            // node) and high-value; they make sniffCSS-check work out of
            // the box. Use `--no-contrast`/`--no-ax`/`--full` to disable.
            include_contrast: true,
            include_ax: true,
            viewport: None,
        }
    }
}

/// Supported output serialization formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputFormat {
    /// One JSON object per line (streaming-friendly). Tree mode: each
    /// line is a matched root element with nested `children`.
    JsonLines,
    /// One JSON object per line, fully flat: every node is its own line
    /// with `id`/`parent_id` references.
    JsonLinesFlat,
    /// A single JSON array.
    Json,
    /// Token-lean per-node digest: one line per node with the structural
    /// skeleton `{tag, selector, path, depth, rect, visible}` plus the
    /// diagnostic facets an AI actually needs to answer "what does this
    /// look like / is it accessible": a curated `css` subset (display,
    /// position, colors, font-size/weight, overflow, ...), a compact
    /// `contrast` (`{ratio, aa, aaa}`) and a compact `aria`
    /// (`{role, name, focusable}`) — the intermediate fidelity between the
    /// bare DOM skeleton and the full computed-style payload. Still ~10x
    /// smaller than the full snapshot.
    Summary,
}

impl OutputFormat {
    /// Parse a CLI value (`jsonl`, `jsonl-flat`, `json`, `summary`/`slim`).
    pub fn parse_cli(input: &str) -> Result<Self, SniffError> {
        match input.trim().to_ascii_lowercase().as_str() {
            "jsonl" | "ndjson" => Ok(Self::JsonLines),
            "jsonl-flat" | "ndjson-flat" => Ok(Self::JsonLinesFlat),
            "json" => Ok(Self::Json),
            "summary" | "slim" => Ok(Self::Summary),
            other => Err(SniffError::InvalidOutputFormat(other.to_string())),
        }
    }
}

/// Parse a `name:arg:arg` wait strategy spec.
pub fn parse_wait_strategy(spec: &str) -> Result<WaitStrategy, SniffError> {
    let mut parts = spec.split(':');
    let name = parts.next().unwrap_or("").trim();
    let mut args: Vec<&str> = parts.map(str::trim).collect();

    let err = |e: String| {
        SniffError::InvalidWaitStrategy(format!(
            "{e} — wait format: delay:<ms> | network-idle:<idle_ms>[:<timeout_ms>] | \
             fonts-loaded[:<timeout_ms>] | selector:<sel>[:<timeout_ms>] | \
             element-ready:<sel>:<visible,has-size[,opacity=N]>[:<timeout_ms>] | \
             app-flag:<flag>[:<timeout_ms>]"
        ))
    };
    let take_arg = |args: &mut Vec<&str>, what: &str| -> Result<String, SniffError> {
        if args.is_empty() {
            Err(err(format!("missing {what} for `{name}` strategy")))
        } else {
            Ok(args.remove(0).to_string())
        }
    };
    let take_ms = |args: &mut Vec<&str>, what: &str| -> Result<u64, SniffError> {
        take_arg(args, what)?.parse().map_err(|_| {
            err(format!(
                "invalid {what} (expected milliseconds) for `{name}`"
            ))
        })
    };

    match name {
        "delay" => {
            let ms = take_ms(&mut args, "delay")?;
            Ok(WaitStrategy::Delay { ms })
        }
        "network-idle" => {
            let idle_ms = take_ms(&mut args, "idle_ms")?;
            let timeout_ms = if args.is_empty() {
                30_000
            } else {
                take_ms(&mut args, "timeout_ms")?
            };
            Ok(WaitStrategy::NetworkIdle {
                idle_ms,
                timeout_ms,
            })
        }
        "fonts-loaded" => {
            let timeout_ms = if args.is_empty() {
                15_000
            } else {
                take_ms(&mut args, "timeout_ms")?
            };
            Ok(WaitStrategy::FontsLoaded { timeout_ms })
        }
        "selector" => {
            let selector = take_arg(&mut args, "selector")?;
            let timeout_ms = if args.is_empty() {
                30_000
            } else {
                take_ms(&mut args, "timeout_ms")?
            };
            Ok(WaitStrategy::Selector {
                selector,
                timeout_ms,
            })
        }
        "element-ready" => {
            let selector = take_arg(&mut args, "selector")?;
            let conditions_spec = take_arg(&mut args, "conditions")?;
            let conditions = conditions_spec
                .split(',')
                .filter(|c| !c.is_empty())
                .map(parse_ready_condition)
                .collect::<Result<Vec<_>, _>>()?;
            if conditions.is_empty() {
                return Err(err("element-ready requires at least one condition".into()));
            }
            let timeout_ms = if args.is_empty() {
                30_000
            } else {
                take_ms(&mut args, "timeout_ms")?
            };
            Ok(WaitStrategy::ElementReady {
                selector,
                conditions,
                timeout_ms,
            })
        }
        "app-flag" => {
            let flag = take_arg(&mut args, "flag")?;
            let timeout_ms = if args.is_empty() {
                15_000
            } else {
                take_ms(&mut args, "timeout_ms")?
            };
            Ok(WaitStrategy::AppFlag { flag, timeout_ms })
        }
        other => Err(err(format!("unknown strategy `{other}`"))),
    }
}

/// Parse an `name:arg:arg` interaction action spec.
///
/// Formats: `click:<selector>[:<timeout_ms>[:<settle_ms>]]` |
/// `hover:<selector>[:<timeout_ms>[:<settle_ms>]]` |
/// `type:<selector>:<text>` (text may contain colons) |
/// `upload:<selector>:<file1,file2>` (files may contain colons).
///
/// For `click`/`hover` the **selector may itself contain `:`** (CSS
/// pseudo-classes such as `:nth-child(2)`, `:hover`, `:not(...)`): only
/// *trailing* `:N` / `:N:M` fields that are all digits are interpreted as
/// `timeout_ms` / `settle_ms`. Example:
/// `click:.btn-group:nth-child(2) .dropdown-toggle:3000` → selector
/// `.btn-group:nth-child(2) .dropdown-toggle`, timeout 3000 ms. For
/// `type`/`upload` the selector is the first token after the action name
/// (so it must not contain `:`; prefer an attribute or `aria-label` anchor
/// when it would).
pub fn parse_action(spec: &str) -> Result<Action, SniffError> {
    let mut parts = spec.splitn(2, ':');
    let name = parts.next().unwrap_or("").trim();
    let body = parts.next().unwrap_or("");

    let err = |e: String| {
        SniffError::InvalidAction(format!(
            "{e} — action format: click:<selector>[:<timeout_ms>[:<settle_ms>]] | \
             hover:<selector>[:<timeout_ms>[:<settle_ms>]] | \
             type:<selector>:<text> | upload:<selector>:<file1,file2>"
        ))
    };

    match name {
        "click" | "hover" => {
            // `body` may contain `:` in the selector (pseudo-classes such as
            // `:nth-child(2)`, `:hover`, `:not(...)`). Only trailing
            // all-digit tokens are options (`[:<timeout_ms>[:<settle_ms>]]`).
            let toks: Vec<&str> = body.split(':').collect();
            let mut trailing: Vec<&str> = Vec::new();
            for t in toks.iter().rev() {
                let t = t.trim();
                if !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()) {
                    trailing.push(t);
                } else {
                    break;
                }
            }
            if trailing.len() > 2 {
                return Err(err(format!(
                    "too many numeric options in `{name}` action (a selector ending in `:digits` is ambiguous)"
                )));
            }
            let selector = toks[..toks.len() - trailing.len()].join(":");
            let selector = selector.trim().to_string();
            if selector.is_empty() {
                return Err(err("missing selector".into()));
            }
            // trailing is built from the end: [settle, timeout].
            let timeout_ms = match trailing.as_slice() {
                [] => Action::DEFAULT_TIMEOUT_MS,
                [to] | [_, to] => to.parse().map_err(|_| {
                    err(format!(
                        "invalid timeout_ms (expected milliseconds) for `{name}` action"
                    ))
                })?,
                _ => unreachable!(),
            };
            let settle_ms = match trailing.as_slice() {
                [se, _] => se.parse().map_err(|_| {
                    err(format!(
                        "invalid settle_ms (expected milliseconds) for `{name}` action"
                    ))
                })?,
                _ => Action::DEFAULT_SETTLE_MS,
            };
            Ok(if name == "click" {
                Action::Click {
                    selector,
                    timeout_ms,
                    settle_ms,
                }
            } else {
                Action::Hover {
                    selector,
                    timeout_ms,
                    settle_ms,
                }
            })
        }
        "type" | "upload" => {
            // splitn(2) keeps `type` text and `upload` file paths (which may
            // themselves contain `:`) intact after the selector.
            let mut sp = body.splitn(2, ':');
            let selector = sp.next().unwrap_or("").trim().to_string();
            if selector.is_empty() {
                return Err(err("missing selector".into()));
            }
            let rest = sp.next().unwrap_or("");
            if name == "type" {
                let text = rest.trim().to_string();
                if text.is_empty() {
                    return Err(err("missing text for `type` action".into()));
                }
                Ok(Action::Type {
                    selector,
                    text,
                    timeout_ms: Action::DEFAULT_TIMEOUT_MS,
                    settle_ms: Action::DEFAULT_SETTLE_MS,
                })
            } else {
                if rest.trim().is_empty() {
                    return Err(err("missing files for `upload` action".into()));
                }
                let files = rest
                    .split(',')
                    .map(|f| f.trim().to_string())
                    .filter(|f| !f.is_empty())
                    .collect::<Vec<_>>();
                Ok(Action::Upload {
                    selector,
                    files,
                    timeout_ms: Action::DEFAULT_TIMEOUT_MS,
                    settle_ms: Action::DEFAULT_SETTLE_MS,
                })
            }
        }
        other => Err(err(format!("unknown action `{other}`"))),
    }
}

fn parse_ready_condition(input: &str) -> Result<ReadyCondition, SniffError> {
    let trimmed = input.trim();
    match trimmed {
        "visible" => Ok(ReadyCondition::Visible),
        "has-size" | "size" => Ok(ReadyCondition::HasSize),
        "opacity=1" => Ok(ReadyCondition::Opacity(1.0)),
        s if s.starts_with("opacity=") => s["opacity=".len()..]
            .parse::<f64>()
            .map(ReadyCondition::Opacity)
            .map_err(|_| {
                SniffError::InvalidReadyCondition(format!(
                    "invalid opacity in `{input}` (expected opacity=<0..1>)"
                ))
            }),
        other => Err(SniffError::InvalidReadyCondition(format!(
            "`{other}` — expected one of: visible, has-size, opacity=<0..1> \
             (element-ready format: element-ready:<selector>:<cond1,cond2>[:<timeout_ms>])"
        ))),
    }
}

/// Parse a comma-separated list of categories.
pub fn parse_categories(input: &str) -> Result<Vec<StyleCategory>, SniffError> {
    let mut out = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match parse_category(part) {
            Some(cat) => out.push(cat),
            None => return Err(SniffError::UnknownCategory(part.to_string())),
        }
    }
    if out.is_empty() {
        return Err(SniffError::UnknownCategory(input.to_string()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_format_parse() {
        assert_eq!(
            OutputFormat::parse_cli("jsonl").unwrap(),
            OutputFormat::JsonLines
        );
        assert_eq!(
            OutputFormat::parse_cli("ndjson").unwrap(),
            OutputFormat::JsonLines
        );
        assert_eq!(
            OutputFormat::parse_cli("jsonl-flat").unwrap(),
            OutputFormat::JsonLinesFlat
        );
        assert_eq!(OutputFormat::parse_cli("json").unwrap(), OutputFormat::Json);
        assert_eq!(
            OutputFormat::parse_cli("summary").unwrap(),
            OutputFormat::Summary
        );
        assert!(OutputFormat::parse_cli("yaml").is_err());
    }

    #[test]
    fn parse_categories_multiple() {
        let cats = parse_categories("box-model, layout, visual").unwrap();
        assert_eq!(
            cats,
            vec![
                StyleCategory::BoxModel,
                StyleCategory::Layout,
                StyleCategory::Visual
            ]
        );
    }

    #[test]
    fn parse_categories_unknown_fails() {
        assert!(parse_categories("bogus").is_err());
    }

    #[test]
    fn default_pipeline_is_sane() {
        let pipe = WaitStrategy::default_pipeline(".card");
        assert_eq!(pipe.len(), 3);
        assert!(matches!(pipe[0], WaitStrategy::Selector { .. }));
        assert!(matches!(pipe[1], WaitStrategy::NetworkIdle { .. }));
        assert!(matches!(pipe[2], WaitStrategy::ElementReady { .. }));
        // Default waits are short so failures fail fast; callers pass
        // longer timeouts explicitly when needed.
        match &pipe[0] {
            WaitStrategy::Selector { timeout_ms, .. } => assert_eq!(*timeout_ms, 10_000),
            _ => unreachable!(),
        }
        match &pipe[2] {
            WaitStrategy::ElementReady { timeout_ms, .. } => assert_eq!(*timeout_ms, 10_000),
            _ => unreachable!(),
        }
    }

    #[test]
    fn malformed_ready_condition_error_hints_format() {
        let err = parse_wait_strategy("element-ready:.card:1000")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("expected one of: visible, has-size, opacity=<0..1>"),
            "got: {err}"
        );
        assert!(
            err.contains("element-ready:<selector>:<cond1,cond2>"),
            "got: {err}"
        );
        // The intended-timeout-in-conditions-slot case must be recognizable.
        assert!(err.contains("1000"), "got: {err}");
    }

    #[test]
    fn wait_strategy_errors_include_format_hint() {
        let err = parse_wait_strategy("delay").unwrap_err().to_string();
        assert!(err.contains("wait format: delay:<ms>"), "got: {err}");
        let err = parse_wait_strategy("bogus:1").unwrap_err().to_string();
        assert!(err.contains("wait format:"), "got: {err}");
    }

    #[test]
    fn parse_wait_strategy_success_paths() {
        assert_eq!(
            parse_wait_strategy("delay:2000").unwrap(),
            WaitStrategy::Delay { ms: 2000 }
        );

        match parse_wait_strategy("network-idle:800:15000").unwrap() {
            WaitStrategy::NetworkIdle {
                idle_ms,
                timeout_ms,
            } => {
                assert_eq!(idle_ms, 800);
                assert_eq!(timeout_ms, 15_000);
            }
            other => panic!("unexpected: {other:?}"),
        }

        assert!(matches!(
            parse_wait_strategy("fonts-loaded:10000").unwrap(),
            WaitStrategy::FontsLoaded { timeout_ms: 10_000 }
        ));

        match parse_wait_strategy("selector:.card:5000").unwrap() {
            WaitStrategy::Selector {
                selector,
                timeout_ms,
            } => {
                assert_eq!(selector, ".card");
                assert_eq!(timeout_ms, 5000);
            }
            other => panic!("unexpected: {other:?}"),
        }

        assert!(matches!(
            parse_wait_strategy("app-flag:__APP_READY__:20000").unwrap(),
            WaitStrategy::AppFlag {
                flag,
                timeout_ms: 20_000
            } if flag == "__APP_READY__"
        ));

        // Defaults when trailing timeout is omitted.
        assert!(matches!(
            parse_wait_strategy("fonts-loaded").unwrap(),
            WaitStrategy::FontsLoaded { timeout_ms: 15_000 }
        ));
        assert!(matches!(
            parse_wait_strategy("app-flag:READY").unwrap(),
            WaitStrategy::AppFlag {
                timeout_ms: 15_000,
                ..
            }
        ));
    }

    #[test]
    fn parse_action_click_keeps_colons_in_selector() {
        // Regression: CSS pseudo-classes (`:nth-child(2)`, `:not(...)`)
        // inside the selector must not be mistaken for the trailing
        // timeout/settle options.
        assert_eq!(
            parse_action("click:#toolbar-content .btn-group:nth-child(2) .dropdown-toggle")
                .unwrap(),
            Action::Click {
                selector: "#toolbar-content .btn-group:nth-child(2) .dropdown-toggle".into(),
                timeout_ms: Action::DEFAULT_TIMEOUT_MS,
                settle_ms: Action::DEFAULT_SETTLE_MS,
            }
        );
        // The exact reported regression: `:nth-child(2)` + trailing timeout.
        assert_eq!(
            parse_action("click:#toolbar-content .btn-group:nth-child(2) .dropdown-toggle:3000")
                .unwrap(),
            Action::Click {
                selector: "#toolbar-content .btn-group:nth-child(2) .dropdown-toggle".into(),
                timeout_ms: 3000,
                settle_ms: Action::DEFAULT_SETTLE_MS,
            }
        );
        assert_eq!(
            parse_action("click:.btn-group:nth-child(2) .dropdown-toggle:3000").unwrap(),
            Action::Click {
                selector: ".btn-group:nth-child(2) .dropdown-toggle".into(),
                timeout_ms: 3000,
                settle_ms: Action::DEFAULT_SETTLE_MS,
            }
        );
        assert_eq!(
            parse_action("hover:li:nth-child(3) a:5000:250").unwrap(),
            Action::Hover {
                selector: "li:nth-child(3) a".into(),
                timeout_ms: 5000,
                settle_ms: 250,
            }
        );
        assert_eq!(
            parse_action("click:div:not(.x):2000:100").unwrap(),
            Action::Click {
                selector: "div:not(.x)".into(),
                timeout_ms: 2000,
                settle_ms: 100,
            }
        );
        // A numeric pseudo-looking token is ambiguous → explicit error.
        let err = parse_action("click:.a:1:2:3").unwrap_err().to_string();
        assert!(err.contains("too many numeric options"), "got: {err}");
    }

    #[test]
    fn parse_action_click_with_defaults() {
        assert_eq!(
            parse_action("click:#open-modal").unwrap(),
            Action::Click {
                selector: "#open-modal".into(),
                timeout_ms: Action::DEFAULT_TIMEOUT_MS,
                settle_ms: Action::DEFAULT_SETTLE_MS,
            }
        );
    }

    #[test]
    fn parse_action_hover_with_timeout_and_settle() {
        assert_eq!(
            parse_action("hover:.menu:5000:300").unwrap(),
            Action::Hover {
                selector: ".menu".into(),
                timeout_ms: 5000,
                settle_ms: 300,
            }
        );
        // settle optional after timeout.
        assert_eq!(
            parse_action("hover:.menu:5000").unwrap(),
            Action::Hover {
                selector: ".menu".into(),
                timeout_ms: 5000,
                settle_ms: Action::DEFAULT_SETTLE_MS,
            }
        );
    }

    #[test]
    fn parse_action_type_keeps_colons_in_text() {
        assert_eq!(
            parse_action("type:#q:filter https://x?a=b").unwrap(),
            Action::Type {
                selector: "#q".into(),
                text: "filter https://x?a=b".into(),
                timeout_ms: Action::DEFAULT_TIMEOUT_MS,
                settle_ms: Action::DEFAULT_SETTLE_MS,
            }
        );
    }

    #[test]
    fn parse_action_upload_with_files() {
        assert_eq!(
            parse_action("upload:#file:img/a.jpg,img/b.png").unwrap(),
            Action::Upload {
                selector: "#file".into(),
                files: vec!["img/a.jpg".into(), "img/b.png".into()],
                timeout_ms: Action::DEFAULT_TIMEOUT_MS,
                settle_ms: Action::DEFAULT_SETTLE_MS,
            }
        );
    }

    #[test]
    fn parse_action_upload_keeps_colons_in_paths() {
        assert_eq!(
            parse_action("upload:#file:/tmp/photo:v1.jpg").unwrap(),
            Action::Upload {
                selector: "#file".into(),
                files: vec!["/tmp/photo:v1.jpg".into()],
                timeout_ms: Action::DEFAULT_TIMEOUT_MS,
                settle_ms: Action::DEFAULT_SETTLE_MS,
            }
        );
    }

    #[test]
    fn parse_action_upload_requires_files() {
        let err = parse_action("upload:#file:").unwrap_err().to_string();
        assert!(err.contains("missing files"), "got: {err}");
        let err = parse_action("upload").unwrap_err().to_string();
        assert!(err.contains("missing selector"), "got: {err}");
    }

    #[test]
    fn parse_action_errors_hint_format() {
        let err = parse_action("click").unwrap_err().to_string();
        assert!(
            err.contains("action format: click:<selector>"),
            "got: {err}"
        );
        let err = parse_action("hover::5000").unwrap_err().to_string();
        assert!(err.contains("missing selector"), "got: {err}");
        let err = parse_action("type:#q:").unwrap_err().to_string();
        assert!(err.contains("missing text"), "got: {err}");
        let err = parse_action("dblclick:#x").unwrap_err().to_string();
        assert!(err.contains("unknown action `dblclick`"), "got: {err}");
        // A non-numeric trailing token is now part of the selector (it could
        // be a pseudo-class), so it must not be rejected as a timeout; the
        // ambiguous all-numeric case still errors.
        assert_eq!(parse_action("click:#x:abc").unwrap().selector(), "#x:abc");
        let err = parse_action("click:.a:1:2:3").unwrap_err().to_string();
        assert!(err.contains("too many numeric options"), "got: {err}");
    }

    #[test]
    fn default_has_no_actions() {
        let cfg = SniffConfig::default();
        assert!(cfg.actions.is_empty());
        // Effects are ON by default (mapped when actions are present).
        assert!(cfg.effects);
        assert_eq!(cfg.effects_limit, 10);
    }

    #[test]
    fn default_viewport_is_laptop() {
        let cfg = SniffConfig::default();
        assert_eq!(
            cfg.viewport,
            Some(Viewport {
                width: 1366,
                height: 768
            })
        );
    }

    #[test]
    fn default_is_ai_optimized() {
        let cfg = SniffConfig::default();
        assert!(cfg.output.compact, "compact must be ON by default");
        assert!(
            cfg.output.include_contrast,
            "contrast facet must be ON by default"
        );
        assert!(cfg.output.include_ax, "ax facet must be ON by default");
        assert!(
            cfg.include_custom_properties,
            "custom props must be ON by default"
        );
        assert!(cfg.stabilize, "stabilize must be ON by default");
        // Per-node facets stay on for an AI-ready capture.
        assert!(cfg.output.include_visibility);
        assert!(cfg.output.include_style_hash);
        assert!(cfg.output.include_aria);
        // The full AX subtree document stays opt-in (large).
        assert!(!cfg.ax_tree);
    }

    #[test]
    fn viewport_parse_cli() {
        assert_eq!(
            Viewport::parse_cli("1280x720").unwrap(),
            Viewport {
                width: 1280,
                height: 720
            }
        );
        assert_eq!(Viewport::parse_cli("1920X1080").unwrap().width, 1920);
        assert!(Viewport::parse_cli("800").is_err());
        assert!(Viewport::parse_cli("0x0").is_err());
    }
}
