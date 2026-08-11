//! End-to-end tests against a real Chromium instance.
//!
//! These tests launch a headless browser and sniff real pages. They are
//! skipped (pass with a message) when no Chrome/Chromium binary is
//! available on the machine, so `cargo test` stays green anywhere.

use sniff_cdp::protocol::LaunchOptions;
use sniff_core::config::{OutputFormat, parse_categories};
use sniff_core::{
    AccessibilityGrade, ElementFilter, OutputConfig, ReadyCondition, SniffConfig, SniffResult,
    WaitStrategy,
};
use sniff_engine::Sniffer;
use std::sync::OnceLock;
use tokio::sync::{Semaphore, SemaphorePermit};

/// Chrome instances are heavy; cap concurrent launches to keep CI/containers happy.
fn browser_semaphore() -> &'static Semaphore {
    static SEM: OnceLock<Semaphore> = OnceLock::new();
    SEM.get_or_init(|| Semaphore::new(1))
}

/// Acquire a permit so only a few browsers run at once.
async fn acquire_browser_slot() -> SemaphorePermit<'static> {
    browser_semaphore()
        .acquire()
        .await
        .expect("semaphore closed")
}

/// Launch a browser, retrying on transient failures (Chrome starts can be
/// flaky under load in CI/containers).
async fn launch_with_retry(opts: &LaunchOptions) -> SniffResult<Sniffer> {
    let mut last = None;
    for attempt in 0..3 {
        match Sniffer::launch(opts).await {
            Ok(s) => return Ok(s),
            Err(e) => {
                eprintln!("browser launch attempt {attempt} failed: {e}");
                last = Some(e);
                tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            }
        }
    }
    Err(last.unwrap())
}

fn fixture_path(name: &str) -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    format!("file://{}", dir.join(name).display())
}

/// Skip the test body unless a Chrome binary is available.
fn require_chrome() -> Option<LaunchOptions> {
    sniff_cdp::BrowserProcess::available().map(|_| LaunchOptions {
        headless: true,
        launch_timeout_ms: 90_000,
        ..Default::default()
    })
}

fn base_config(url: &str, selector: &str) -> SniffConfig {
    SniffConfig {
        url: url.to_string(),
        selector: selector.to_string(),
        depth: 0,
        categories: parse_categories("box-model,typography,visual,layout").unwrap(),
        custom_properties: Vec::new(),
        pseudo_elements: Vec::new(),
        wait: WaitStrategy::default_pipeline(selector),
        filter: ElementFilter::default(),
        output: OutputConfig {
            format: OutputFormat::JsonLines,
            include_rect: true,
            include_path: true,
            include_metrics: true,
            normalize_colors: true,
            group_by_category: true,
            pretty: false,
            compact: false,
            include_visibility: true,
            include_style_hash: true,
            include_aria: true,
            include_contrast: false,
            include_ax: false,
        },
        viewport: Some(sniff_core::Viewport {
            width: 1366,
            height: 768,
        }),
        include_custom_properties: false,
        stable_key: None,
        stabilize: false,
        ax_tree: false,
    }
}

#[tokio::test]
async fn sniffs_computed_styles_of_element() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;
    let config = base_config(&fixture_path("fixture.html"), ".card");
    let outcome = sniffer.sniff(&config).await?;
    let snaps = &outcome.snapshots;

    assert_eq!(snaps.len(), 1);
    let card = &snaps[0];
    assert_eq!(card.tag, "DIV");
    assert_eq!(card.depth, 0);

    let styles = &card.styles;
    assert_eq!(styles.get("width"), Some("300px"));
    assert_eq!(styles.get("font-size"), Some("16px"));
    assert_eq!(styles.get("background-color"), Some("#2563eb"));

    let rect = card.rect.expect("rect requested");
    assert!(rect.width > 0.0 && rect.height > 0.0);
    let noticeable = card.noticeable.expect("noticeability requested");
    assert!(noticeable.display_visible);
    assert_eq!(noticeable.accessibility_grade, AccessibilityGrade::Aaa);
    Ok(())
}

#[tokio::test]
async fn recurses_into_children_up_to_depth() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("fixture.html"), ".card");
    config.depth = 2;
    let outcome = sniffer.sniff(&config).await?;
    let snaps = &outcome.snapshots;
    let card = &snaps[0];
    // 2 visible children (.icon, .label); .hidden is filtered out.
    assert_eq!(card.children.len(), 2);
    assert_eq!(card.children[0].depth, 1);
    Ok(())
}

#[tokio::test]
async fn visible_filter_excludes_hidden_elements() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("fixture.html"), ".card");
    config.depth = 1;
    config.filter.visible = false;
    let outcome = sniffer.sniff(&config).await?;
    let snaps = &outcome.snapshots;
    let children: Vec<&str> = snaps[0]
        .children
        .iter()
        .map(|c| c.selector.as_str())
        .collect();
    assert!(
        children.iter().any(|s| s.contains(".hidden")),
        "expected .hidden child with visible filter disabled, got {children:?}"
    );
    let hidden = snaps[0]
        .children
        .iter()
        .find(|c| c.selector.contains(".hidden"))
        .unwrap();
    let hidden_n = hidden.noticeable.expect("noticeability requested");
    assert!(!hidden_n.display_visible);
    assert_eq!(hidden_n.accessibility_grade, AccessibilityGrade::None);
    let visible = snaps[0]
        .children
        .iter()
        .find(|c| c.selector.contains(".label"))
        .unwrap();
    let visible_n = visible.noticeable.expect("noticeability requested");
    assert!(visible_n.display_visible);
    assert_eq!(visible_n.accessibility_grade, AccessibilityGrade::Aaa);
    Ok(())
}

#[tokio::test]
async fn off_viewport_sr_only_element_is_displayed_but_grade_aa() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let config = base_config(&fixture_path("fixture.html"), ".sr-focusable");
    let outcome = sniffer.sniff(&config).await?;
    let link = &outcome.snapshots[0];
    // position: absolute at (-1px,-1px) with 1x1px: rendered (display_visible)
    // but NOT on screen, so the accessibility grade drops to AA instead of
    // reporting the whole element as invisible.
    assert_eq!(link.tag, "A");
    let noticeable = link.noticeable.expect("noticeability requested");
    assert!(noticeable.display_visible);
    assert_eq!(noticeable.accessibility_grade, AccessibilityGrade::Aa);
    let rect = link.rect.expect("rect requested");
    assert_eq!(rect.width, 1.0);
    assert_eq!(rect.height, 1.0);
    Ok(())
}

#[tokio::test]
async fn waits_for_application_flag() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("delayed.html"), "#btn");
    config.wait = vec![WaitStrategy::AppFlag {
        flag: "__APP_READY__".into(),
        timeout_ms: 10_000,
    }];
    let outcome = sniffer.sniff(&config).await?;
    let snaps = &outcome.snapshots;
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].tag, "BUTTON");
    Ok(())
}

#[tokio::test]
async fn element_ready_waits_for_visibility_and_size() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("delayed.html"), "#btn");
    config.wait = vec![WaitStrategy::ElementReady {
        selector: "#btn".into(),
        conditions: vec![ReadyCondition::Visible, ReadyCondition::HasSize],
        timeout_ms: 10_000,
    }];
    let outcome = sniffer.sniff(&config).await?;
    let snaps = &outcome.snapshots;
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0].tag, "BUTTON");
    Ok(())
}

#[tokio::test]
async fn no_match_returns_error() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    // A selector that never appears: the Selector wait fails fast and
    // reports NoMatch (not a generic timeout).
    let mut config = base_config(&fixture_path("fixture.html"), ".does-not-exist");
    config.wait = vec![WaitStrategy::Selector {
        selector: ".does-not-exist".into(),
        timeout_ms: 1_000,
    }];
    let result = sniffer.sniff(&config).await;
    assert!(
        matches!(result, Err(sniff_core::SniffError::NoMatch { .. })),
        "expected NoMatch, got {result:?}"
    );
    Ok(())
}

#[tokio::test]
async fn element_ready_missing_element_reports_no_match() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("fixture.html"), ".does-not-exist");
    config.wait = vec![WaitStrategy::ElementReady {
        selector: ".does-not-exist".into(),
        conditions: vec![ReadyCondition::Visible],
        timeout_ms: 1_000,
    }];
    let result = sniffer.sniff(&config).await;
    assert!(
        matches!(result, Err(sniff_core::SniffError::NoMatch { .. })),
        "expected NoMatch for never-matching element-ready, got {result:?}"
    );
    Ok(())
}

#[tokio::test]
async fn element_ready_present_but_not_ready_reports_timeout_with_hint() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    // `.hidden` exists in the DOM but is display:none, so the Visible
    // condition never holds: the error must say the element exists but is
    // not ready, and hint at delay/longer timeout.
    let mut config = base_config(&fixture_path("fixture.html"), ".hidden");
    config.wait = vec![WaitStrategy::ElementReady {
        selector: ".hidden".into(),
        conditions: vec![ReadyCondition::Visible],
        timeout_ms: 1_000,
    }];
    let result = sniffer.sniff(&config).await;
    match result {
        Err(sniff_core::SniffError::Timeout(msg)) => {
            assert!(msg.contains("element exists but conditions"), "got: {msg}");
            assert!(msg.contains("delay:N"), "got: {msg}");
        }
        other => panic!("expected Timeout with hint, got {other:?}"),
    }
    Ok(())
}

#[tokio::test]
async fn captures_pseudo_elements() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("fixture.html"), ".card");
    config.pseudo_elements = vec!["::before".into()];
    let outcome = sniffer.sniff(&config).await?;
    let snaps = &outcome.snapshots;
    assert_eq!(snaps[0].pseudo.len(), 1);
    assert_eq!(snaps[0].pseudo[0].name, "::before");
    Ok(())
}

#[tokio::test]
async fn reuses_browser_across_multiple_sniffs() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;
    let url = fixture_path("fixture.html");
    for _ in 0..3 {
        let config = base_config(&url, ".card");
        let outcome = sniffer.sniff(&config).await?;
        let snaps = &outcome.snapshots;
        assert_eq!(snaps.len(), 1);
    }
    Ok(())
}

#[tokio::test]
async fn captures_aria_facet_with_roles_and_names() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("a11y.html"), "main");
    config.depth = 1;
    let outcome = sniffer.sniff(&config).await?;
    let main = &outcome.snapshots[0];
    let button = main
        .children
        .iter()
        .find(|c| c.tag == "BUTTON")
        .expect("button child");
    let aria = button.aria.as_ref().expect("aria facet");
    assert_eq!(aria.role.as_deref(), Some("button"));
    assert_eq!(aria.name.as_deref(), Some("Salvar alterações"));
    assert!(aria.focusable);
    assert_eq!(aria.aria_expanded.as_deref(), Some("false"));

    let h1 = main
        .children
        .iter()
        .find(|c| c.tag == "H1")
        .expect("h1 child");
    assert_eq!(
        h1.aria.as_ref().and_then(|a| a.role.as_deref()),
        Some("heading")
    );
    assert!(h1.aria.as_ref().is_some_and(|a| a.has_text));
    Ok(())
}

#[tokio::test]
async fn implicit_roles_cover_arria_spec_tags() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("roles.html"), "body");
    config.depth = 6;
    let outcome = sniffer.sniff(&config).await?;
    let body = &outcome.snapshots[0];

    let role_of = |node: &sniff_core::types::ElementSnapshot| -> Option<String> {
        node.aria.as_ref().and_then(|a| a.role.clone())
    };
    let find = |tag: &str, id: &str| {
        let mut stack: Vec<&sniff_core::types::ElementSnapshot> = body.children.iter().collect();
        while let Some(n) = stack.pop() {
            if n.tag == tag && n.selector.contains(&format!("#{id}")) {
                return Some(n);
            }
            stack.extend(n.children.iter());
        }
        None
    };

    // Landmarks: site-level header/footer are banner/contentinfo; a header
    // nested in <main> is not.
    assert_eq!(
        find("HEADER", "site-header").and_then(role_of).as_deref(),
        Some("banner")
    );
    assert_eq!(
        find("HEADER", "article-header")
            .and_then(role_of)
            .as_deref(),
        None,
        "header inside main must not be a banner"
    );
    assert_eq!(
        find("FOOTER", "site-footer").and_then(role_of).as_deref(),
        Some("contentinfo")
    );

    // Forms & structure.
    assert_eq!(
        find("FIELDSET", "prefs").and_then(role_of).as_deref(),
        Some("group")
    );
    assert_eq!(find("LEGEND", "legend").and_then(role_of).as_deref(), None);
    assert_eq!(find("DT", "dt").and_then(role_of).as_deref(), Some("term"));
    assert_eq!(
        find("DD", "dd").and_then(role_of).as_deref(),
        Some("definition")
    );
    assert_eq!(
        find("PROGRESS", "prog").and_then(role_of).as_deref(),
        Some("progressbar")
    );
    assert_eq!(
        find("METER", "met").and_then(role_of).as_deref(),
        Some("meter")
    );
    assert_eq!(
        find("OUTPUT", "out").and_then(role_of).as_deref(),
        Some("status")
    );

    // Tables.
    assert_eq!(find("CAPTION", "cap").and_then(role_of).as_deref(), None);
    assert_eq!(
        find("THEAD", "thead").and_then(role_of).as_deref(),
        Some("rowgroup")
    );
    assert_eq!(
        find("TBODY", "tbody").and_then(role_of).as_deref(),
        Some("rowgroup")
    );
    assert_eq!(
        find("TH", "th-row").and_then(role_of).as_deref(),
        Some("rowheader")
    );
    assert_eq!(
        find("TH", "th-col").and_then(role_of).as_deref(),
        Some("columnheader")
    );
    assert_eq!(find("TD", "td").and_then(role_of).as_deref(), Some("cell"));

    // Select: multiple -> listbox, single -> combobox.
    assert_eq!(
        find("SELECT", "multi").and_then(role_of).as_deref(),
        Some("listbox")
    );
    assert_eq!(
        find("SELECT", "single").and_then(role_of).as_deref(),
        Some("combobox")
    );

    // Text roles.
    assert_eq!(
        find("STRONG", "s").and_then(role_of).as_deref(),
        Some("strong")
    );
    assert_eq!(
        find("EM", "e").and_then(role_of).as_deref(),
        Some("emphasis")
    );
    assert_eq!(find("CODE", "c").and_then(role_of).as_deref(), Some("code"));
    assert_eq!(find("MARK", "m").and_then(role_of).as_deref(), Some("mark"));
    assert_eq!(find("TIME", "t").and_then(role_of).as_deref(), Some("time"));
    assert_eq!(
        find("P", "para").and_then(role_of).as_deref(),
        Some("paragraph")
    );

    // canvas with fallback -> img; audio with controls -> group.
    assert_eq!(
        find("CANVAS", "cv").and_then(role_of).as_deref(),
        Some("img")
    );
    assert_eq!(
        find("AUDIO", "au").and_then(role_of).as_deref(),
        Some("group")
    );
    Ok(())
}

#[tokio::test]
async fn derives_measured_contrast_pass_and_fail() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("a11y.html"), "main");
    config.depth = 1;
    config.output.include_contrast = true;
    let outcome = sniffer.sniff(&config).await?;
    let main = &outcome.snapshots[0];

    let good = main
        .children
        .iter()
        .find(|c| c.selector.contains(".good"))
        .expect("good paragraph");
    let good_c = good.contrast.as_ref().expect("contrast facet");
    assert_eq!(good_c.aa, sniff_core::TriState::Pass);
    assert!(good_c.ratio > 4.5);

    let dim = main
        .children
        .iter()
        .find(|c| c.selector.contains(".dim"))
        .expect("dim paragraph");
    let dim_c = dim.contrast.as_ref().expect("contrast facet");
    assert_eq!(dim_c.aa, sniff_core::TriState::Fail);
    assert!(dim_c.ratio < 4.5, "got {}", dim_c.ratio);
    assert!(dim_c.ratio > 0.0);
    Ok(())
}

#[tokio::test]
async fn stabilize_freezes_animations_deterministically() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("animated.html"), ".mover");
    config.stabilize = true;
    let first = sniffer.sniff(&config).await?;
    let mover = &first.snapshots[0];
    assert_eq!(
        mover.styles.get("transform").unwrap_or("none"),
        "none",
        "animation must be cancelled by --stabilize"
    );
    let baseline_rect = mover.rect;

    let second = sniffer.sniff(&config).await?;
    assert_eq!(
        second.snapshots[0].rect, baseline_rect,
        "stabilized rect must be deterministic across runs"
    );
    Ok(())
}

#[tokio::test]
async fn captures_ax_facet_and_ax_tree() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("a11y.html"), "main");
    config.depth = 1;
    config.output.include_ax = true;
    config.ax_tree = true;
    let outcome = sniffer.sniff(&config).await?;

    let main = &outcome.snapshots[0];
    let button = main
        .children
        .iter()
        .find(|c| c.tag == "BUTTON")
        .expect("button child");
    let ax = button.ax.as_ref().expect("ax facet");
    assert_eq!(ax.role.as_deref(), Some("button"));
    assert_eq!(ax.name.as_deref(), Some("Salvar alterações"));

    let tree = outcome.ax_tree.as_ref().expect("__ax_tree present");
    let roots = tree.as_array().expect("tree is an array");
    assert!(!roots.is_empty(), "at least one root subtree");
    let tree_text = serde_json::to_string(tree).unwrap();
    assert!(
        tree_text.contains("heading"),
        "subtree contains roles: {tree_text}"
    );
    Ok(())
}
