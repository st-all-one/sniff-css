//! End-to-end tests against a real Chromium instance.
//!
//! These tests launch a headless browser and sniff real pages. They are
//! skipped (pass with a message) when no Chrome/Chromium binary is
//! available on the machine, so `cargo test` stays green anywhere.

use sniff_cdp::protocol::LaunchOptions;
use sniff_core::config::{OutputFormat, parse_categories};
use sniff_core::{
    AccessibilityGrade, Action, ElementFilter, OutputConfig, ReadyCondition, SniffConfig,
    SniffError, SniffResult, WaitStrategy,
};
use sniff_engine::{Sniffer, write_output};
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
            viewport: Some(sniff_core::Viewport {
                width: 1366,
                height: 768,
            }),
        },
        viewport: Some(sniff_core::Viewport {
            width: 1366,
            height: 768,
        }),
        include_custom_properties: false,
        stable_key: None,
        attributes: vec![],
        stabilize: false,
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

#[tokio::test]
async fn click_action_reveals_modal_before_capture() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("interaction.html"), "#modal");
    config.actions = vec![Action::Click {
        selector: "#open".into(),
        timeout_ms: 10_000,
        settle_ms: 150,
    }];
    let outcome = sniffer.sniff(&config).await?;
    assert_eq!(outcome.snapshots.len(), 1);
    let modal = &outcome.snapshots[0];
    assert_eq!(modal.tag, "DIV");
    let noticeable = modal.noticeable.expect("noticeability requested");
    assert!(
        noticeable.display_visible,
        "modal must be visible after the click action"
    );
    Ok(())
}

#[tokio::test]
async fn hidden_modal_without_click_action_times_out() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let config = base_config(&fixture_path("interaction.html"), "#modal");
    let err = sniffer.sniff(&config).await.unwrap_err();
    assert!(
        matches!(err, SniffError::Timeout(_) | SniffError::NoMatch { .. }),
        "expected a timeout/no-match for a display:none target without an action, got {err}"
    );
    Ok(())
}

#[tokio::test]
async fn hover_action_reveals_menu_before_capture() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("interaction.html"), "#menu");
    config.actions = vec![Action::Hover {
        selector: "#user".into(),
        timeout_ms: 10_000,
        settle_ms: 150,
    }];
    let outcome = sniffer.sniff(&config).await?;
    let menu = &outcome.snapshots[0];
    let noticeable = menu.noticeable.expect("noticeability requested");
    assert!(
        noticeable.display_visible,
        "hover menu must be visible after the hover action"
    );
    Ok(())
}

#[tokio::test]
async fn type_action_reveals_results_before_capture() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("interaction.html"), "#results");
    config.actions = vec![Action::Type {
        selector: "#q".into(),
        text: "shoes".into(),
        timeout_ms: 10_000,
        settle_ms: 150,
    }];
    let outcome = sniffer.sniff(&config).await?;
    let results = &outcome.snapshots[0];
    let noticeable = results.noticeable.expect("noticeability requested");
    assert!(
        noticeable.display_visible,
        "type-ahead results must be visible after the type action"
    );
    Ok(())
}

fn click(selector: &str) -> Action {
    Action::Click {
        selector: selector.into(),
        timeout_ms: 10_000,
        settle_ms: 200,
    }
}

fn effect_entry(
    outcome: &sniff_engine::extractor::SniffOutcome,
    index: usize,
) -> &serde_json::Value {
    &outcome.actions.as_ref().unwrap().as_array().unwrap()[index]
}

#[tokio::test]
async fn effects_map_reveals_offscreen_table_and_where() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("interaction.html"), "#table");
    config.actions = vec![click("#open-table")];
    let outcome = sniffer.sniff(&config).await?;

    let report = effect_entry(&outcome, 0);
    assert_eq!(report["effect"], "revealed", "table click must reveal UI");
    let appeared = report["appeared"].as_array().unwrap();
    let table = appeared
        .iter()
        .find(|a| a["tag"] == "TABLE")
        .expect("TABLE in appeared");
    assert_eq!(table["onscreen"], false, "table is below the fold");
    let below = table["out_of_view"]["below"].as_f64().unwrap();
    assert!(
        below > 100.0,
        "table must be well below the viewport, got {below}"
    );
    assert!(
        table["distance_from_action"].as_f64().unwrap() > 100.0,
        "table is far from the click point"
    );
    assert!(report["summary"].as_str().unwrap().contains("TABLE"));
    Ok(())
}

#[tokio::test]
async fn effects_map_reports_far_calendar_with_direction() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("interaction.html"), "#calendar");
    config.actions = vec![click("#open-calendar")];
    let outcome = sniffer.sniff(&config).await?;

    let report = effect_entry(&outcome, 0);
    assert_eq!(report["effect"], "revealed");
    let appeared = report["appeared"].as_array().unwrap();
    let cal = appeared
        .iter()
        .find(|a| a["path"] == "div#calendar")
        .expect("calendar appeared");
    assert_eq!(
        cal["onscreen"], false,
        "calendar is off-screen at the page bottom"
    );
    let dist = cal["distance_from_action"].as_f64().unwrap();
    assert!(
        dist > 500.0,
        "calendar must be far from the open button, got {dist}"
    );
    assert!(
        cal["direction"].as_str().unwrap().contains("below"),
        "direction should point below, got {}",
        cal["direction"]
    );
    Ok(())
}

#[tokio::test]
async fn effects_map_marks_no_effect_interaction() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("interaction.html"), "#noop");
    config.actions = vec![click("#noop")];
    let outcome = sniffer.sniff(&config).await?;

    let report = effect_entry(&outcome, 0);
    assert_eq!(
        report["effect"], "no_effect",
        "a no-op button must be flagged"
    );
    assert!(report["appeared"].as_array().unwrap().is_empty());
    assert!(report["changed"].as_array().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn effects_map_tracks_chained_modal_minimodal_input() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("interaction.html"), "#suggestions");
    config.actions = vec![
        click("#open"),
        click("#open-mini"),
        Action::Type {
            selector: "#mini-input".into(),
            text: "2024-08-12".into(),
            timeout_ms: 10_000,
            settle_ms: 200,
        },
    ];
    let outcome = sniffer.sniff(&config).await?;
    let suggestions = &outcome.snapshots[0];
    assert!(
        suggestions
            .noticeable
            .as_ref()
            .expect("noticeability requested")
            .display_visible,
        "chained suggestions must be visible at the end"
    );

    let actions = outcome.actions.as_ref().unwrap().as_array().unwrap();
    assert_eq!(actions.len(), 3, "one __actions entry per chain step");
    assert_eq!(actions[0]["effect"], "revealed", "step 0 opens the modal");
    assert_eq!(
        actions[1]["effect"], "revealed",
        "step 1 opens the mini-modal"
    );
    assert_eq!(
        actions[2]["effect"], "revealed",
        "step 2 reveals suggestions"
    );
    assert_eq!(actions[0]["selector"], "#open");
    assert_eq!(actions[2]["action"], "type");
    Ok(())
}

#[tokio::test]
async fn effects_map_disabled_when_effects_false() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("interaction.html"), "#modal");
    config.actions = vec![click("#open")];
    config.effects = false;
    let outcome = sniffer.sniff(&config).await?;
    assert!(
        outcome.actions.is_none(),
        "no __actions map when effects is disabled"
    );
    assert_eq!(outcome.snapshots.len(), 1);
    Ok(())
}

#[tokio::test]
async fn broken_chain_error_names_the_step() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("interaction.html"), "#modal");
    config.actions = vec![click("#open"), click("#does-not-exist")];
    let err = sniffer.sniff(&config).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("action #1") && msg.contains("click:#does-not-exist"),
        "error must name the failing step: {msg}"
    );
    assert!(
        msg.contains("Prior steps") && msg.contains("click:#open"),
        "error must list prior steps: {msg}"
    );
    Ok(())
}

#[tokio::test]
async fn attributes_capture_verifies_form_field_names() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("forms.html"), "form#sucesu-form");
    config.depth = 3;
    config.attributes = vec!["name".to_string(), "value".to_string()];
    let outcome = sniffer.sniff(&config).await?;
    let root = &outcome.snapshots[0];

    // Walk the tree collecting every input's `name` attribute.
    let mut names: Vec<(String, String)> = Vec::new();
    fn collect(snap: &sniff_core::types::ElementSnapshot, out: &mut Vec<(String, String)>) {
        if snap.tag == "INPUT"
            && let Some(attrs) = &snap.attributes
        {
            for (k, v) in attrs {
                out.push((k.clone(), v.clone()));
            }
        }
        for child in &snap.children {
            collect(child, out);
        }
    }
    collect(root, &mut names);

    let attrs: Vec<(String, String)> = names.iter().filter(|(k, _)| k == "name").cloned().collect();
    assert!(
        attrs
            .iter()
            .any(|(_, v)| v == "parameters[items][0][title]"),
        "expected indexed name in attrs, got {attrs:?}"
    );
    assert!(
        attrs
            .iter()
            .any(|(_, v)| v == "parameters[items][1][title]"),
        "expected second indexed name in attrs, got {attrs:?}"
    );
    Ok(())
}

#[tokio::test]
async fn summary_and_screenshot_are_produced_on_demand() -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let mut config = base_config(&fixture_path("fixture.html"), ".card");
    config.depth = 1;
    config.screenshot = true;
    config.screenshot_full_page = true;
    let outcome = sniffer.sniff(&config).await?;

    // Screenshot: non-empty PNG magic bytes.
    let png = outcome
        .screenshot
        .as_ref()
        .expect("screenshot bytes requested and captured");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "must be a PNG");

    // Summary: token-lean digest, no styles.
    let mut buf = Vec::new();
    let summary_cfg = OutputConfig {
        format: OutputFormat::Summary,
        ..config.output.clone()
    };
    write_output(&mut buf, &outcome, &summary_cfg)?;
    let text = String::from_utf8(buf).expect("utf8");
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.len() >= 2, "root + children, got {lines:?}");
    // A leading `__meta` line may carry style_defaults + viewport; the first
    // node line is the root element.
    let root_line = lines
        .iter()
        .find(|l| !l.contains("\"__meta\""))
        .expect("a node line after the optional __meta");
    let root: serde_json::Value = serde_json::from_str(root_line).unwrap();
    assert_eq!(root["tag"], "DIV");
    assert!(root.get("styles").is_none(), "summary omits styles");
    assert_eq!(root["rect"]["width"].as_f64(), Some(332.0));
    Ok(())
}
