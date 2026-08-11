//! CLI argument parsing and mapping to [`SniffConfig`].

use clap::Parser;
use sniff_core::config::{OutputFormat, parse_categories, parse_wait_strategy};
use sniff_core::{ElementFilter, OutputConfig, SniffConfig, SniffResult, Viewport, WaitStrategy};

/// High-performance computed-style sniffer for AI-driven development.
#[derive(Debug, Parser)]
#[command(
    name = "sniff-computed-style",
    version,
    about = "Capture real computed styles from a page for AI-driven debugging",
    long_about = None
)]
pub struct Cli {
    /// Page URL to navigate to (e.g. http://localhost:3000/foo).
    #[arg(long, short = 'u')]
    pub url: String,

    /// CSS selector of the element(s) to sniff.
    #[arg(long, short = 's')]
    pub selector: String,

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

    /// Output format: jsonl (default) or json.
    #[arg(long, default_value = "jsonl")]
    pub output: String,

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
    #[arg(long, default_value_t = false)]
    pub compact: bool,

    /// Omit the per-node `is_visible` field (default: computed).
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
    /// vs. normal/large text). Off by default to keep the capture lean.
    #[arg(long, default_value_t = false)]
    pub contrast: bool,

    /// Capture the browser-computed accessibility-tree node (`ax`) per
    /// element via the CDP `Accessibility` domain.
    #[arg(long, default_value_t = false)]
    pub ax: bool,

    /// Capture the full accessibility subtree for the matched elements and
    /// emit it as a `__ax_tree` document (implies --ax).
    #[arg(long = "ax-tree", default_value_t = false)]
    pub ax_tree: bool,

    /// Freeze animations/transitions before capture for deterministic
    /// snapshots of dynamic pages (emulates prefers-reduced-motion,
    /// cancels running animations and injects
    /// `animation/transition: none !important`).
    #[arg(long, default_value_t = false)]
    pub stabilize: bool,

    /// Path to the Chrome/Chromium binary.
    #[arg(long)]
    pub chrome: Option<String>,

    /// Connect to an already-running browser (ws:// endpoint) instead
    /// of launching one.
    #[arg(long)]
    pub connect: Option<String>,

    /// Emulated viewport as WxH (default: 1366x768 laptop).
    #[arg(long)]
    pub viewport: Option<String>,

    /// Capture all CSS custom properties (`--*`), like the DevTools
    /// Computed panel.
    #[arg(long = "custom-props", default_value_t = false)]
    pub custom_props: bool,

    /// Attribute to use as the stable anchor in `selector`/`path`,
    /// preferred over the DOM `id` (e.g. `data-testid`). Keeps output
    /// matchable across deploys that regenerate ids.
    #[arg(long = "stable-key")]
    pub stable_key: Option<String>,
}

impl Cli {
    /// Convert parsed CLI arguments into a full sniffing config.
    pub fn into_config(self) -> SniffResult<SniffConfig> {
        let wait = if self.wait.is_empty() {
            WaitStrategy::default_pipeline(&self.selector)
        } else {
            self.wait
                .iter()
                .map(|spec| parse_wait_strategy(spec))
                .collect::<SniffResult<Vec<_>>>()?
        };

        let output = OutputConfig {
            format: OutputFormat::parse_cli(&self.output)?,
            include_rect: !self.no_rect,
            include_path: !self.no_path,
            include_metrics: !self.no_metrics,
            normalize_colors: !self.no_normalize_colors,
            group_by_category: !self.no_group,
            pretty: self.pretty,
            compact: self.compact,
            include_visibility: !self.no_visibility,
            include_style_hash: !self.no_style_hash,
            include_aria: !self.no_aria,
            include_contrast: self.contrast,
            include_ax: self.ax || self.ax_tree,
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
            selector: self.selector,
            depth: self.depth,
            categories: parse_categories(&self.categories)?,
            custom_properties: self.props,
            pseudo_elements: self.pseudo,
            wait,
            filter,
            output,
            viewport,
            include_custom_properties: self.custom_props,
            stable_key: self.stable_key,
            stabilize: self.stabilize,
            ax_tree: self.ax_tree,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            selector: ".card".into(),
            depth: 0,
            categories: "all".into(),
            props: vec![],
            pseudo: vec![],
            wait: vec![],
            no_visible: false,
            min_width: None,
            min_height: None,
            exclude: vec![],
            output: "jsonl".into(),
            pretty: false,
            no_rect: false,
            no_path: false,
            no_metrics: false,
            no_normalize_colors: false,
            no_group: false,
            compact: false,
            no_visibility: false,
            no_style_hash: false,
            no_aria: false,
            contrast: false,
            ax: false,
            ax_tree: false,
            stabilize: false,
            chrome: None,
            connect: None,
            viewport: None,
            custom_props: false,
            stable_key: None,
        };
        let cfg = cli.into_config().unwrap();
        assert_eq!(cfg.categories, vec![StyleCategory::All]);
        assert_eq!(cfg.output.format, OutputFormat::JsonLines);
        assert!(cfg.output.include_rect);
        assert_eq!(cfg.wait.len(), 3);
        assert_eq!(cfg.stable_key, None);
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
            selector: ".card".into(),
            depth: 0,
            categories: "all".into(),
            props: vec![],
            pseudo: vec![],
            wait: vec![],
            no_visible: false,
            min_width: None,
            min_height: None,
            exclude: vec![],
            output: "jsonl".into(),
            pretty: false,
            no_rect: false,
            no_path: false,
            no_metrics: false,
            no_normalize_colors: false,
            no_group: false,
            compact: false,
            no_visibility: false,
            no_style_hash: false,
            no_aria: false,
            contrast: false,
            ax: false,
            ax_tree: false,
            stabilize: false,
            chrome: None,
            connect: None,
            viewport: None,
            custom_props: false,
            stable_key: None,
        };
        cli.stable_key = Some("data-testid".into());
        let cfg = cli.into_config().unwrap();
        assert_eq!(cfg.stable_key.as_deref(), Some("data-testid"));
    }
}
