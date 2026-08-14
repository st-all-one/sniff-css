//! CLI argument parsing and mapping to [`SniffConfig`].

use clap::Parser;
use sniff_core::config::{OutputFormat, parse_action, parse_categories, parse_wait_strategy};
use sniff_core::{
    ElementFilter, OutputConfig, SniffConfig, SniffError, SniffResult, Viewport, WaitStrategy,
};

/// Which capture backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Backend {
    /// Chromium over CDP (the classic computed-style capture).
    Web,
    /// A debug-mode Flutter app over the Dart VM Service.
    Flutter,
    /// Infer from `--url`: `flutter://<device>` → Flutter, otherwise web.
    Auto,
}

/// High-performance computed-style sniffer for AI-driven development.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "sniffCSS",
    version,
    about = "Capture real computed styles from a page for AI-driven debugging",
    long_about = None
)]
pub struct Cli {
    /// Page URL to navigate to (e.g. http://localhost:3000/foo).
    #[arg(long, short = 'u')]
    pub url: String,

    /// CSS selector of the element(s) to sniff. Required for the web
    /// backend; the Flutter backend defaults to `root`.
    #[arg(long, short = 's')]
    pub selector: Option<String>,

    /// How many levels of children to capture (0 = element only).
    #[arg(long, default_value_t = 0)]
    pub depth: usize,

    /// Comma-separated categories (box-model, layout, typography,
    /// visual, transform, animation, interaction, accessibility, all).
    #[arg(long, short = 'c', default_value = "all")]
    pub categories: String,

    /// Extra CSS properties to capture beyond the categories.
    #[arg(long, value_delimiter = ',')]
    pub props: Vec<String>,

    /// Pseudo-elements to capture (::before, ::after).
    #[arg(long, value_delimiter = ',')]
    pub pseudo: Vec<String>,

    /// Wait strategy, repeatable. Format:
    ///   delay:ms | network-idle:idle_ms[:timeout_ms] |
    ///   fonts-loaded[:timeout_ms] | selector:sel[:timeout_ms] |
    ///   element-ready:sel:cond1,cond2[:timeout_ms] |
    ///   app-flag:flag[:timeout_ms]
    /// Defaults to selector + network-idle + element-ready.
    #[arg(long, action = clap::ArgAction::Append)]
    pub wait: Vec<String>,

    /// Interaction action, repeatable and ORDERED. Format:
    ///   click:<selector>[:<timeout_ms>[:<settle_ms>]] |
    ///   hover:<selector>[:<timeout_ms>[:<settle_ms>]] |
    ///   type:<selector>:<text>
    /// Reveals elements that only exist after an action (modals, dropdowns,
    /// hover menus, type-ahead suggestions). Each action waits for its own
    /// target to appear; the wait pipeline then runs after the actions,
    /// targeting the post-interaction DOM. Prefer this over --click/--hover/
    /// --type when mixing action kinds in a specific order.
    #[arg(long = "action", action = clap::ArgAction::Append)]
    pub actions: Vec<String>,

    /// Click a selector before capture (repeatable). Shorthand for
    /// `--action click:<selector>[:<timeout_ms>[:<settle_ms>]]`.
    #[arg(long, action = clap::ArgAction::Append)]
    pub click: Vec<String>,

    /// Hover a selector before capture (repeatable). Shorthand for
    /// `--action hover:<selector>[:<timeout_ms>[:<settle_ms>]]`.
    #[arg(long, action = clap::ArgAction::Append)]
    pub hover: Vec<String>,

    /// Type text into a selector before capture (repeatable). Shorthand for
    /// `--action type:<selector>:<text>`.
    #[arg(long, action = clap::ArgAction::Append)]
    pub r#type: Vec<String>,

    /// Attach local files to an `<input type=file>` before capture
    /// (repeatable). Shorthand for
    /// `--action upload:<selector>:<file1,file2>`. The browser resolves the
    /// paths — in a container they must exist inside it. Works even when the
    /// input is visually hidden (common for upload buttons).
    #[arg(long, action = clap::ArgAction::Append)]
    pub upload: Vec<String>,

    /// Extra HTTP header `Name: Value`, repeatable, applied to every request
    /// of the session via `Network.setExtraHTTPHeaders` — e.g.
    /// `--header "X-CMS-AI-Token: <token>"` for a stateless CMS auth.
    /// Headers from the `SNIFF_DEFAULT_HEADERS` env var (a JSON object) are
    /// merged first; explicit `--header` flags win on name collision.
    #[arg(long, action = clap::ArgAction::Append)]
    pub header: Vec<String>,

    /// Restore a persisted session state (cookies + localStorage, Playwright
    /// `storageState` JSON) into the browser before navigation, so a login
    /// performed earlier (via `--save-storage-state`) survives this capture.
    #[arg(long = "storage-state")]
    pub storage_state: Option<String>,

    /// Write the session state (all cookies + the page's localStorage) to
    /// this path after the pipeline — pass it back via `--storage-state` in
    /// later captures to keep the login alive across browser restarts.
    #[arg(long = "save-storage-state")]
    pub save_storage_state: Option<String>,

    /// Backend to sniff against. `auto` (default) infers it from `--url`:
    /// a `flutter://<device>` URL means the Flutter/Dart VM Service backend
    /// (and that device), anything else is the web (Chromium/CDP) backend.
    #[arg(long, value_enum, default_value_t = Backend::Auto)]
    pub backend: Backend,

    /// Flutter backend: `adb` serial of the emulator/device (e.g.
    /// `emulator-5554`). Defaults to the device in the `flutter://<device>`
    /// URL when `--backend auto`. Ignored for `--backend web`.
    #[arg(long)]
    pub device: Option<String>,

    /// Flutter backend: launch this AVD instead of attaching to a running
    /// device. Ignored for `--backend web`.
    #[arg(long)]
    pub avd: Option<String>,

    /// Flutter backend: directory containing the app's `pubspec.yaml`.
    /// Defaults to the directory of `--target`.
    #[arg(long)]
    pub project: Option<String>,

    /// Flutter backend: app entry (default `lib/main.dart`).
    #[arg(long, default_value = "lib/main.dart")]
    pub target: String,

    /// Flutter backend: attach to an already-running debug app instead of
    /// `flutter run`.
    #[arg(long, default_value_t = false)]
    pub attach: bool,

    /// Keep only visible elements (default true; use --no-visible).
    #[arg(long = "no-visible", default_value_t = false)]
    pub no_visible: bool,

    /// Keep only elements at least this wide (px).
    #[arg(long)]
    pub min_width: Option<f64>,

    /// Keep only elements at least this tall (px).
    #[arg(long)]
    pub min_height: Option<f64>,

    /// Skip elements matching this selector (repeatable).
    #[arg(long, action = clap::ArgAction::Append)]
    pub exclude: Vec<String>,

    /// Output format: summary (default), jsonl, jsonl-flat or json.
    #[arg(long, default_value = "summary")]
    pub output: String,

    /// Shorthand for `--output summary`: emit the intermediate per-node
    /// digest (structural skeleton + curated css subset + contrast + aria)
    /// instead of the full snapshot. This is the default.
    #[arg(long, default_value_t = false)]
    pub summary: bool,

    /// Emit the full, non-summarized snapshot (`--output jsonl`) instead of
    /// the default summary digest.
    #[arg(long = "no-summary", default_value_t = false)]
    pub no_summary: bool,

    /// Save a PNG of the page (post-stabilize, post-interaction) to this
    /// path, alongside the snapshot output. Complements the computed
    /// snapshot with the "how it actually looks" view.
    #[arg(long)]
    pub screenshot: Option<String>,

    /// When `--screenshot` is set, capture the full scrollable document
    /// instead of only the visible viewport.
    #[arg(long = "fullpage-screenshot", default_value_t = false)]
    pub fullpage_screenshot: bool,

    /// Persist the snapshot to disk, mirroring the MCP store: creates
    /// `sniffCSS/[domain]/[UTC]-[path]-[selector].<ext>` under the CWD (or
    /// `SNIFF_SNAPSHOT_DIR` when set), using the selected `--output` format,
    /// and auto-ignores the tree with a `.gitignore`. The output is still
    /// written to stdout as usual.
    #[arg(long, default_value_t = false)]
    pub persist: bool,

    /// Pretty-print JSON (single-JSON output only).
    #[arg(long, default_value_t = false)]
    pub pretty: bool,

    /// Omit the bounding rect.
    #[arg(long = "no-rect", default_value_t = false)]
    pub no_rect: bool,

    /// Omit the DOM path.
    #[arg(long = "no-path", default_value_t = false)]
    pub no_path: bool,

    /// Omit derived metrics (z-index, stacking context).
    #[arg(long = "no-metrics", default_value_t = false)]
    pub no_metrics: bool,

    /// Keep colors as the browser reports them (no hex normalization).
    #[arg(long = "no-normalize-colors", default_value_t = false)]
    pub no_normalize_colors: bool,

    /// Flatten styles into a single map instead of category groups.
    #[arg(long = "no-group", default_value_t = false)]
    pub no_group: bool,

    /// Compact mode: drop redundant logical/default CSS properties and
    /// scope `css_variables` to a single global map + per-node overrides.
    /// ON by default (AI-optimized, ~55% fewer tokens); use `--no-compact`
    /// or `--full` to keep the full raw property set.
    #[arg(long, default_value_t = true)]
    pub compact: bool,

    /// Disable compact mode (keep every captured property, no dedup).
    #[arg(long = "no-compact", default_value_t = false)]
    pub no_compact: bool,

    /// Omit the per-node `is_user_noticeable` field (default: computed).
    #[arg(long = "no-visibility", default_value_t = false)]
    pub no_visibility: bool,

    /// Omit the per-node `computed_style_hash` checksum (default: computed).
    #[arg(long = "no-style-hash", default_value_t = false)]
    pub no_style_hash: bool,

    /// Omit the per-node resolved `aria` facet (role, accessible name,
    /// focusable; default: computed in-page).
    #[arg(long = "no-aria", default_value_t = false)]
    pub no_aria: bool,

    /// Derive and emit a measured WCAG `contrast` facet per node (AA/AAA
    /// vs. normal/large text). ON by default; use `--no-contrast` or
    /// `--full` to omit.
    #[arg(long, default_value_t = true)]
    pub contrast: bool,

    /// Disable the measured WCAG `contrast` facet.
    #[arg(long = "no-contrast", default_value_t = false)]
    pub no_contrast: bool,

    /// Capture the browser-computed accessibility-tree node (`ax`) per
    /// element via the CDP `Accessibility` domain. ON by default.
    #[arg(long, default_value_t = true)]
    pub ax: bool,

    /// Disable the per-element `ax` accessibility-tree node capture.
    #[arg(long = "no-ax", default_value_t = false)]
    pub no_ax: bool,

    /// Capture the full accessibility subtree for the matched elements and
    /// emit it as a `__ax_tree` document (implies --ax).
    #[arg(long = "ax-tree", default_value_t = false)]
    pub ax_tree: bool,

    /// Freeze animations/transitions before capture for deterministic
    /// snapshots of dynamic pages (emulates prefers-reduced-motion,
    /// cancels running animations and injects
    /// `animation/transition: none !important`). ON by default; use
    /// `--no-stabilize` or `--full` to capture the live animated state.
    #[arg(long, default_value_t = true)]
    pub stabilize: bool,

    /// Disable animation/transition freezing.
    #[arg(long = "no-stabilize", default_value_t = false)]
    pub no_stabilize: bool,

    /// Map the UI effects of each interaction into a reserved `__actions`
    /// output area (what appeared/disappeared/changed and where: rect,
    /// on-screen, out-of-view offset, distance from the action point).
    /// ON by default when actions are configured; use `--no-effects` to
    /// omit the map.
    #[arg(long, default_value_t = true)]
    pub effects: bool,

    /// Disable the `__actions` UI-effect map.
    #[arg(long = "no-effects", default_value_t = false)]
    pub no_effects: bool,

    /// Cap on how many appeared/removed/changed elements each `__actions`
    /// entry reports (largest areas first).
    #[arg(long = "effects-limit", default_value_t = 10)]
    pub effects_limit: usize,

    /// Path to the Chrome/Chromium binary.
    #[arg(long)]
    pub chrome: Option<String>,

    /// Connect to an already-running browser instead of launching one.
    /// Accepts a `ws://`/`wss://` endpoint directly, or an HTTP origin
    /// (`http://127.0.0.1:9222` / `127.0.0.1:9222`) which is resolved via
    /// `/json/version`. Defaults to the `SNIFF_CONNECT` environment variable.
    #[arg(long, env = "SNIFF_CONNECT")]
    pub connect: Option<String>,

    /// Emulated viewport as WxH (default: 1366x768 laptop).
    #[arg(long)]
    pub viewport: Option<String>,

    /// Capture all CSS custom properties (`--*`), like the DevTools
    /// Computed panel. ON by default (scoped to a single global `__meta`
    /// map in compact mode); use `--no-custom-props` or `--full` to omit.
    #[arg(long = "custom-props", default_value_t = true)]
    pub custom_props: bool,

    /// Disable CSS custom property capture.
    #[arg(long = "no-custom-props", default_value_t = false)]
    pub no_custom_props: bool,

    /// Attribute to use as the stable anchor in `selector`/`path`,
    /// preferred over the DOM `id` (e.g. `data-testid`). Keeps output
    /// matchable across deploys that regenerate ids.
    #[arg(long = "stable-key")]
    pub stable_key: Option<String>,

    /// Extra DOM attributes to capture verbatim per node (repeatable or
    /// comma-separated), e.g. `--attrs name,data-id`. Emitted under each
    /// node's `attrs` map; useful to validate form field reindexing
    /// (`name="parameters[items][0][title]"`) without scraping the DOM.
    #[arg(long, value_delimiter = ',', action = clap::ArgAction::Append)]
    pub attrs: Vec<String>,

    /// Full-fidelity mode: disables every AI optimization at once
    /// (`--no-compact --no-custom-props --no-stabilize --no-contrast
    /// --no-ax`). Equivalent to the pre-AI-default behavior.
    #[arg(long, default_value_t = false)]
    pub full: bool,
}

impl Cli {
    /// The effective backend: `--backend auto` (default) infers Flutter from
    /// a `flutter://` URL scheme, otherwise web. Explicit `--backend` wins.
    pub fn effective_backend(&self) -> Backend {
        match self.backend {
            Backend::Auto => {
                if self.url.starts_with("flutter://") {
                    Backend::Flutter
                } else {
                    Backend::Web
                }
            }
            b => b,
        }
    }

    /// The `adb` serial implied by a `flutter://<device>` URL (the URL host).
    pub fn flutter_device(&self) -> Option<String> {
        let rest = self.url.strip_prefix("flutter://")?;
        let host = rest.split('/').next()?;
        if host.is_empty() {
            None
        } else {
            Some(host.to_string())
        }
    }

    /// Convert parsed CLI arguments into a full sniffing config.
    pub fn into_config(self) -> SniffResult<SniffConfig> {
        let is_flutter = self.effective_backend() == Backend::Flutter;
        let selector = match self.selector {
            Some(s) => s,
            None if is_flutter => "root".to_string(),
            None => {
                return Err(SniffError::Other(
                    "web backend needs --selector <css> (or use a `flutter://<device>` URL to sniff Flutter)"
                        .into(),
                ))
            }
        };

        let wait = if self.wait.is_empty() {
            WaitStrategy::default_pipeline(&selector)
        } else {
            self.wait
                .iter()
                .map(|spec| parse_wait_strategy(spec))
                .collect::<SniffResult<Vec<_>>>()?
        };

        // Ordered interactions. `--action` is the ordered, full-control form
        // (for mixed click/hover/type flows); the convenience flags map to
        // click -> hover -> type group order when --action is absent.
        let actions = if !self.actions.is_empty() {
            self.actions
                .iter()
                .map(|spec| parse_action(spec))
                .collect::<SniffResult<Vec<_>>>()?
        } else {
            let mut out = Vec::new();
            for spec in &self.click {
                out.push(parse_action(&format!("click:{spec}"))?);
            }
            for spec in &self.hover {
                out.push(parse_action(&format!("hover:{spec}"))?);
            }
            for spec in &self.r#type {
                out.push(parse_action(&format!("type:{spec}"))?);
            }
            for spec in &self.upload {
                out.push(parse_action(&format!("upload:{spec}"))?);
            }
            out
        };

        // Headers: `SNIFF_DEFAULT_HEADERS` (JSON object) first, explicit
        // `--header` flags win on name collision.
        let mut headers: Vec<(String, String)> = Vec::new();
        if let Ok(raw) = std::env::var("SNIFF_DEFAULT_HEADERS")
            && !raw.trim().is_empty()
        {
            let map = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&raw)
                .map_err(|e| {
                    SniffError::Other(format!(
                        "invalid SNIFF_DEFAULT_HEADERS (expected a JSON object): {e}"
                    ))
                })?;
            for (k, v) in map {
                if let Some(v) = v.as_str() {
                    headers.push((k, v.to_string()));
                }
            }
        }
        for spec in &self.header {
            let (k, v) = parse_header(spec)?;
            match headers.iter_mut().find(|(existing, _)| existing == &k) {
                Some(existing) => existing.1 = v,
                None => headers.push((k, v)),
            }
        }

        let compact = self.compact && !self.no_compact && !self.full;
        let include_contrast = self.contrast && !self.no_contrast && !self.full;
        let include_ax = (self.ax && !self.no_ax && !self.full) || self.ax_tree;
        let include_custom_properties = self.custom_props && !self.no_custom_props && !self.full;
        let stabilize = self.stabilize && !self.no_stabilize && !self.full;

        let output = OutputConfig {
            format: if self.no_summary {
                OutputFormat::JsonLines
            } else if self.summary {
                OutputFormat::Summary
            } else {
                OutputFormat::parse_cli(&self.output)?
            },
            include_rect: !self.no_rect,
            include_path: !self.no_path,
            include_metrics: !self.no_metrics,
            normalize_colors: !self.no_normalize_colors,
            group_by_category: !self.no_group,
            pretty: self.pretty,
            compact,
            include_visibility: !self.no_visibility,
            include_style_hash: !self.no_style_hash,
            include_aria: !self.no_aria,
            include_contrast,
            include_ax,
        };

        let filter = ElementFilter {
            visible: !self.no_visible,
            min_width: self.min_width,
            min_height: self.min_height,
            exclude_selectors: self.exclude,
        };

        let viewport = match self.viewport {
            Some(v) => Some(Viewport::parse_cli(&v)?),
            None => Some(Viewport {
                width: 1366,
                height: 768,
            }),
        };

        Ok(SniffConfig {
            url: self.url,
            selector,
            depth: self.depth,
            categories: parse_categories(&self.categories)?,
            custom_properties: self.props,
            pseudo_elements: self.pseudo,
            wait,
            filter,
            output,
            viewport,
            include_custom_properties,
            stable_key: self.stable_key,
            attributes: self.attrs,
            stabilize,
            ax_tree: self.ax_tree,
            actions,
            effects: self.effects && !self.no_effects,
            effects_limit: self.effects_limit,
            screenshot: self.screenshot.is_some(),
            screenshot_full_page: self.fullpage_screenshot,
            headers,
            storage_state_path: self.storage_state,
            save_storage_state: self.save_storage_state,
        })
    }
}

/// Parse a `Name: Value` header spec into `(name, value)`.
fn parse_header(spec: &str) -> SniffResult<(String, String)> {
    let (name, value) = spec.split_once(':').ok_or_else(|| {
        SniffError::Other(format!(
            "invalid header `{spec}` — expected `Name: Value` (e.g. `X-CMS-AI-Token: <token>`)"
        ))
    })?;
    let name = name.trim();
    let value = value.trim();
    if name.is_empty() || value.is_empty() {
        return Err(SniffError::Other(format!(
            "invalid header `{spec}` — both name and value must be non-empty"
        )));
    }
    Ok((name.to_string(), value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sniff_core::Action;
    use sniff_core::ReadyCondition;
    use sniff_core::properties::StyleCategory;

    #[test]
    fn parse_delay() {
        assert_eq!(
            parse_wait_strategy("delay:2000").unwrap(),
            WaitStrategy::Delay { ms: 2000 }
        );
    }

    #[test]
    fn parse_network_idle_defaults_timeout() {
        match parse_wait_strategy("network-idle:500").unwrap() {
            WaitStrategy::NetworkIdle {
                idle_ms,
                timeout_ms,
            } => {
                assert_eq!(idle_ms, 500);
                assert_eq!(timeout_ms, 30_000);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_element_ready_with_conditions() {
        match parse_wait_strategy("element-ready:.card:visible,has-size,opacity=0.9:5000").unwrap()
        {
            WaitStrategy::ElementReady {
                selector,
                conditions,
                timeout_ms,
            } => {
                assert_eq!(selector, ".card");
                assert_eq!(conditions.len(), 3);
                assert!(
                    matches!(conditions[2], ReadyCondition::Opacity(v) if (v - 0.9).abs() < 1e-9)
                );
                assert_eq!(timeout_ms, 5000);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_app_flag() {
        assert!(matches!(
            parse_wait_strategy("app-flag:APP_READY").unwrap(),
            WaitStrategy::AppFlag { flag, .. } if flag == "APP_READY"
        ));
    }

    #[test]
    fn parse_unknown_strategy_fails() {
        assert!(parse_wait_strategy("bogus:1").is_err());
    }

    #[test]
    fn parse_missing_arg_fails() {
        assert!(parse_wait_strategy("delay").is_err());
    }

    #[test]
    fn cli_to_config_defaults() {
        let cli = Cli {
            url: "http://localhost:3000".into(),
            selector: Some(".card".into()),
            depth: 0,
            categories: "all".into(),
            props: vec![],
            pseudo: vec![],
            wait: vec![],
            actions: vec![],
            click: vec![],
            hover: vec![],
            r#type: vec![],
            no_visible: false,
            min_width: None,
            min_height: None,
            exclude: vec![],
            output: "jsonl".into(),
            summary: false,
            no_summary: false,
            screenshot: None,
            fullpage_screenshot: false,
            persist: false,
            pretty: false,
            no_rect: false,
            no_path: false,
            no_metrics: false,
            no_normalize_colors: false,
            no_group: false,
            compact: true,
            no_compact: false,
            no_visibility: false,
            no_style_hash: false,
            no_aria: false,
            contrast: true,
            no_contrast: false,
            ax: true,
            no_ax: false,
            ax_tree: false,
            stabilize: true,
            no_stabilize: false,
            effects: true,
            no_effects: false,
            effects_limit: 10,
            chrome: None,
            connect: None,
            viewport: None,
            custom_props: true,
            no_custom_props: false,
            stable_key: None,
            attrs: vec![],
            full: false,
            upload: vec![],
            header: vec![],
            storage_state: None,
            save_storage_state: None,
            backend: Backend::Web,
            device: None,
            avd: None,
            project: None,
            target: "lib/main.dart".into(),
            attach: false,
        };
        let cfg = cli.into_config().unwrap();
        assert_eq!(cfg.categories, vec![StyleCategory::All]);
        assert_eq!(cfg.output.format, OutputFormat::JsonLines);
        assert!(cfg.output.include_rect);
        assert_eq!(cfg.wait.len(), 3);
        assert_eq!(cfg.stable_key, None);
        // AI-optimized defaults: all optimizations ON.
        assert!(cfg.output.compact);
        assert!(cfg.output.include_contrast);
        assert!(cfg.output.include_ax);
        assert!(cfg.include_custom_properties);
        assert!(cfg.stabilize);
        // Laptop viewport default.
        assert_eq!(
            cfg.viewport,
            Some(Viewport {
                width: 1366,
                height: 768
            })
        );
    }

    #[test]
    fn cli_to_config_wires_stable_key() {
        let mut cli = Cli {
            url: "http://localhost:3000".into(),
            selector: Some(".card".into()),
            depth: 0,
            categories: "all".into(),
            props: vec![],
            pseudo: vec![],
            wait: vec![],
            actions: vec![],
            click: vec![],
            hover: vec![],
            r#type: vec![],
            no_visible: false,
            min_width: None,
            min_height: None,
            exclude: vec![],
            output: "jsonl".into(),
            summary: false,
            no_summary: false,
            screenshot: None,
            fullpage_screenshot: false,
            persist: false,
            pretty: false,
            no_rect: false,
            no_path: false,
            no_metrics: false,
            no_normalize_colors: false,
            no_group: false,
            compact: true,
            no_compact: false,
            no_visibility: false,
            no_style_hash: false,
            no_aria: false,
            contrast: true,
            no_contrast: false,
            ax: true,
            no_ax: false,
            ax_tree: false,
            stabilize: true,
            no_stabilize: false,
            effects: true,
            no_effects: false,
            effects_limit: 10,
            chrome: None,
            connect: None,
            viewport: None,
            custom_props: true,
            no_custom_props: false,
            stable_key: None,
            attrs: vec![],
            full: false,
            upload: vec![],
            header: vec![],
            storage_state: None,
            save_storage_state: None,
            backend: Backend::Web,
            device: None,
            avd: None,
            project: None,
            target: "lib/main.dart".into(),
            attach: false,
        };
        cli.stable_key = Some("data-testid".into());
        let cfg = cli.into_config().unwrap();
        assert_eq!(cfg.stable_key.as_deref(), Some("data-testid"));
    }

    #[test]
    fn cli_parses_ordered_actions() {
        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "-s",
            ".modal",
            "--action",
            "click:#open:5000",
            "--action",
            "type:#q:hello world",
            "--action",
            "hover:.submenu:2000:100",
        ])
        .unwrap();
        let cfg = cli.into_config().unwrap();
        assert_eq!(cfg.actions.len(), 3);
        assert!(
            matches!(&cfg.actions[0], Action::Click { selector, timeout_ms, .. }
            if selector == "#open" && *timeout_ms == 5000)
        );
        assert!(matches!(&cfg.actions[1], Action::Type { text, .. } if text == "hello world"));
        assert!(
            matches!(&cfg.actions[2], Action::Hover { timeout_ms, settle_ms, .. }
            if *timeout_ms == 2000 && *settle_ms == 100)
        );
    }

    #[test]
    fn cli_convenience_click_flag_maps_to_action() {
        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "-s",
            ".modal",
            "--click",
            "#open",
        ])
        .unwrap();
        let cfg = cli.into_config().unwrap();
        assert_eq!(cfg.actions.len(), 1);
        assert!(matches!(&cfg.actions[0], Action::Click { selector, .. } if selector == "#open"));
    }

    #[test]
    fn cli_actions_absent_by_default() {
        let cli = Cli::try_parse_from(["sniffCSS", "-u", "http://localhost:3000", "-s", ".card"])
            .unwrap();
        let cfg = cli.into_config().unwrap();
        assert!(cfg.actions.is_empty());
    }

    #[test]
    fn cli_effects_default_on_and_tunable() {
        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "-s",
            ".modal",
            "--click",
            "#open",
        ])
        .unwrap();
        let cfg = cli.into_config().unwrap();
        assert!(cfg.effects, "effects must be ON by default");
        assert_eq!(cfg.effects_limit, 10);

        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "-s",
            ".modal",
            "--click",
            "#open",
            "--no-effects",
            "--effects-limit",
            "3",
        ])
        .unwrap();
        let cfg = cli.into_config().unwrap();
        assert!(!cfg.effects);
        assert_eq!(cfg.effects_limit, 3);
    }

    #[test]
    fn full_mode_disables_all_optimizations() {
        let mut cli = Cli {
            url: "http://localhost:3000".into(),
            selector: Some(".card".into()),
            depth: 0,
            categories: "all".into(),
            props: vec![],
            pseudo: vec![],
            wait: vec![],
            actions: vec![],
            click: vec![],
            hover: vec![],
            r#type: vec![],
            no_visible: false,
            min_width: None,
            min_height: None,
            exclude: vec![],
            output: "jsonl".into(),
            summary: false,
            no_summary: false,
            screenshot: None,
            fullpage_screenshot: false,
            persist: false,
            pretty: false,
            no_rect: false,
            no_path: false,
            no_metrics: false,
            no_normalize_colors: false,
            no_group: false,
            compact: true,
            no_compact: false,
            no_visibility: false,
            no_style_hash: false,
            no_aria: false,
            contrast: true,
            no_contrast: false,
            ax: true,
            no_ax: false,
            ax_tree: false,
            stabilize: true,
            no_stabilize: false,
            effects: true,
            no_effects: false,
            effects_limit: 10,
            chrome: None,
            connect: None,
            viewport: None,
            custom_props: true,
            no_custom_props: false,
            stable_key: None,
            attrs: vec![],
            full: false,
            upload: vec![],
            header: vec![],
            storage_state: None,
            save_storage_state: None,
            backend: Backend::Web,
            device: None,
            avd: None,
            project: None,
            target: "lib/main.dart".into(),
            attach: false,
        };
        cli.full = true;
        let cfg = cli.into_config().unwrap();
        assert!(!cfg.output.compact);
        assert!(!cfg.output.include_contrast);
        assert!(!cfg.output.include_ax);
        assert!(!cfg.include_custom_properties);
        assert!(!cfg.stabilize);
        // Per-node facets stay on in full mode too.
        assert!(cfg.output.include_visibility);
        assert!(cfg.output.include_style_hash);
        assert!(cfg.output.include_aria);
    }

    #[test]
    fn individual_no_flags_override_defaults() {
        let mut cli = Cli {
            url: "http://localhost:3000".into(),
            selector: Some(".card".into()),
            depth: 0,
            categories: "all".into(),
            props: vec![],
            pseudo: vec![],
            wait: vec![],
            actions: vec![],
            click: vec![],
            hover: vec![],
            r#type: vec![],
            no_visible: false,
            min_width: None,
            min_height: None,
            exclude: vec![],
            output: "jsonl".into(),
            summary: false,
            no_summary: false,
            screenshot: None,
            fullpage_screenshot: false,
            persist: false,
            pretty: false,
            no_rect: false,
            no_path: false,
            no_metrics: false,
            no_normalize_colors: false,
            no_group: false,
            compact: true,
            no_compact: false,
            no_visibility: false,
            no_style_hash: false,
            no_aria: false,
            contrast: true,
            no_contrast: false,
            ax: true,
            no_ax: false,
            ax_tree: false,
            stabilize: true,
            no_stabilize: false,
            effects: true,
            no_effects: false,
            effects_limit: 10,
            chrome: None,
            connect: None,
            viewport: None,
            custom_props: true,
            no_custom_props: false,
            stable_key: None,
            attrs: vec![],
            full: false,
            upload: vec![],
            header: vec![],
            storage_state: None,
            save_storage_state: None,
            backend: Backend::Web,
            device: None,
            avd: None,
            project: None,
            target: "lib/main.dart".into(),
            attach: false,
        };
        cli.no_compact = true;
        cli.no_contrast = true;
        cli.no_ax = true;
        cli.no_custom_props = true;
        cli.no_stabilize = true;
        let cfg = cli.into_config().unwrap();
        assert!(!cfg.output.compact);
        assert!(!cfg.output.include_contrast);
        assert!(!cfg.output.include_ax);
        assert!(!cfg.include_custom_properties);
        assert!(!cfg.stabilize);
    }

    #[test]
    fn ax_tree_implies_ax_even_in_full_mode() {
        let mut cli = Cli {
            url: "http://localhost:3000".into(),
            selector: Some(".card".into()),
            depth: 0,
            categories: "all".into(),
            props: vec![],
            pseudo: vec![],
            wait: vec![],
            actions: vec![],
            click: vec![],
            hover: vec![],
            r#type: vec![],
            no_visible: false,
            min_width: None,
            min_height: None,
            exclude: vec![],
            output: "jsonl".into(),
            summary: false,
            no_summary: false,
            screenshot: None,
            fullpage_screenshot: false,
            persist: false,
            pretty: false,
            no_rect: false,
            no_path: false,
            no_metrics: false,
            no_normalize_colors: false,
            no_group: false,
            compact: true,
            no_compact: false,
            no_visibility: false,
            no_style_hash: false,
            no_aria: false,
            contrast: true,
            no_contrast: false,
            ax: true,
            no_ax: false,
            ax_tree: false,
            stabilize: true,
            no_stabilize: false,
            effects: true,
            no_effects: false,
            effects_limit: 10,
            chrome: None,
            connect: None,
            viewport: None,
            custom_props: true,
            no_custom_props: false,
            stable_key: None,
            attrs: vec![],
            full: false,
            upload: vec![],
            header: vec![],
            storage_state: None,
            save_storage_state: None,
            backend: Backend::Web,
            device: None,
            avd: None,
            project: None,
            target: "lib/main.dart".into(),
            attach: false,
        };
        cli.full = true;
        cli.ax_tree = true;
        let cfg = cli.into_config().unwrap();
        assert!(cfg.output.include_ax, "ax_tree must imply include_ax");
        assert!(cfg.ax_tree);
    }

    #[test]
    fn summary_is_the_default_output() {
        let cli = Cli::try_parse_from(["sniffCSS", "-u", "http://localhost:3000", "-s", ".card"])
            .unwrap();
        let cfg = cli.into_config().unwrap();
        assert_eq!(
            cfg.output.format,
            OutputFormat::Summary,
            "summary digest must be the default output"
        );
    }

    #[test]
    fn summary_flag_and_output_summary_agree() {
        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "-s",
            ".card",
            "--summary",
        ])
        .unwrap();
        let cfg = cli.into_config().unwrap();
        assert_eq!(cfg.output.format, OutputFormat::Summary);

        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "-s",
            ".card",
            "--output",
            "summary",
        ])
        .unwrap();
        let cfg = cli.into_config().unwrap();
        assert_eq!(cfg.output.format, OutputFormat::Summary);
    }

    #[test]
    fn no_summary_forces_full_jsonl() {
        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "-s",
            ".card",
            "--no-summary",
        ])
        .unwrap();
        let cfg = cli.into_config().unwrap();
        assert_eq!(
            cfg.output.format,
            OutputFormat::JsonLines,
            "--no-summary must emit the full non-summarized snapshot"
        );
    }

    #[test]
    fn output_json_still_available_without_summary_flag() {
        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "-s",
            ".card",
            "--output",
            "json",
        ])
        .unwrap();
        let cfg = cli.into_config().unwrap();
        assert_eq!(cfg.output.format, OutputFormat::Json);
    }

    #[test]
    fn header_flag_maps_to_config_headers() {
        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "-s",
            ".card",
            "--header",
            "X-CMS-AI-Token: abc123",
        ])
        .unwrap();
        let cfg = cli.into_config().unwrap();
        assert_eq!(
            cfg.headers,
            vec![("X-CMS-AI-Token".to_string(), "abc123".to_string())]
        );
    }

    #[test]
    fn header_parse_rejects_missing_colon_or_value() {
        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "-s",
            ".card",
            "--header",
            "not-a-header",
        ])
        .unwrap();
        assert!(cli.into_config().is_err());

        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "-s",
            ".card",
            "--header",
            "X-Foo: ",
        ])
        .unwrap();
        assert!(cli.into_config().is_err());
    }

    #[test]
    fn upload_flag_maps_to_upload_action() {
        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "-s",
            ".modal",
            "--upload",
            "#file:img/a.jpg,img/b.png",
        ])
        .unwrap();
        let cfg = cli.into_config().unwrap();
        assert!(matches!(
            &cfg.actions[0],
            Action::Upload { selector, files, .. }
                if selector == "#file" && files == &vec!["img/a.jpg".to_string(), "img/b.png".to_string()]
        ));
    }

    #[test]
    fn storage_state_flags_map_to_config() {
        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "-s",
            ".card",
            "--storage-state",
            "state.json",
            "--save-storage-state",
            "out.json",
        ])
        .unwrap();
        let cfg = cli.into_config().unwrap();
        assert_eq!(cfg.storage_state_path.as_deref(), Some("state.json"));
        assert_eq!(cfg.save_storage_state.as_deref(), Some("out.json"));
    }

    #[test]
    fn flutter_backend_parses_device_and_attach() {
        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "flutter://emulator-5554",
            "-s",
            "*",
            "--backend",
            "flutter",
            "--device",
            "emulator-5554",
            "--target",
            "lib/main.dart",
        ])
        .unwrap();
        assert_eq!(cli.backend, Backend::Flutter);
        assert_eq!(cli.device.as_deref(), Some("emulator-5554"));
        assert!(!cli.attach);

        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "flutter://emulator-5554",
            "-s",
            "*",
            "--backend",
            "flutter",
            "--avd",
            "pixel",
            "--attach",
        ])
        .unwrap();
        assert_eq!(cli.avd.as_deref(), Some("pixel"));
        assert!(cli.attach);
    }

    #[test]
    fn web_is_the_default_backend() {
        let cli = Cli::try_parse_from(["sniffCSS", "-u", "http://localhost:3000", "-s", ".card"])
            .unwrap();
        assert_eq!(cli.backend, Backend::Auto);
        assert_eq!(cli.effective_backend(), Backend::Web);
    }

    #[test]
    fn flutter_url_infers_backend_device_and_selector() {
        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "flutter://emulator-5554",
            "--project",
            "/tmp/app",
            "--depth",
            "10",
        ])
        .unwrap();
        assert_eq!(cli.backend, Backend::Auto);
        assert_eq!(cli.effective_backend(), Backend::Flutter);
        assert_eq!(cli.flutter_device().as_deref(), Some("emulator-5554"));
        assert_eq!(cli.device, None);

        let cfg = cli.into_config().unwrap();
        assert_eq!(cfg.selector, "root");
    }

    #[test]
    fn flutter_url_with_path_still_yields_device() {
        let cli =
            Cli::try_parse_from(["sniffCSS", "-u", "flutter://emulator-5554/home", "-s", "*"])
                .unwrap();
        assert_eq!(cli.flutter_device().as_deref(), Some("emulator-5554"));
    }

    #[test]
    fn explicit_backend_and_device_override_inference() {
        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "flutter://emulator-5554",
            "--backend",
            "web",
            "-s",
            ".card",
        ])
        .unwrap();
        assert_eq!(cli.effective_backend(), Backend::Web);

        let cli = Cli::try_parse_from([
            "sniffCSS",
            "-u",
            "http://localhost:3000",
            "--backend",
            "flutter",
            "--device",
            "emulator-5554",
            "-s",
            "root",
        ])
        .unwrap();
        assert_eq!(cli.effective_backend(), Backend::Flutter);
        assert_eq!(cli.device.as_deref(), Some("emulator-5554"));
    }

    #[test]
    fn web_backend_requires_selector() {
        let cli = Cli::try_parse_from(["sniffCSS", "-u", "http://localhost:3000"]).unwrap();
        assert!(cli.into_config().is_err());
    }

    #[test]
    fn storage_state_absent_by_default() {
        let cli = Cli::try_parse_from(["sniffCSS", "-u", "http://localhost:3000", "-s", ".card"])
            .unwrap();
        let cfg = cli.into_config().unwrap();
        assert!(cfg.storage_state_path.is_none());
        assert!(cfg.save_storage_state.is_none());
        assert!(cfg.headers.is_empty());
    }
}
