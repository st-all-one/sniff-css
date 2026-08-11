//! End-to-end tests against a real Chromium instance.
//!
//! These tests launch a headless browser and sniff real pages. They are
//! skipped (pass with a message) when no Chrome/Chromium binary is
//! available on the machine, so `cargo test` stays green anywhere.

use sniff_cdp::protocol::LaunchOptions;
use sniff_core::config::{OutputFormat, parse_categories};
use sniff_core::{
    ElementFilter, OutputConfig, ReadyCondition, SniffConfig, SniffResult, WaitStrategy,
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
        },
        viewport: Some(sniff_core::Viewport {
            width: 1366,
            height: 768,
        }),
        include_custom_properties: false,
        stable_key: None,
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
    assert_eq!(card.is_visible, Some(true));
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
    assert_eq!(hidden.is_visible, Some(false));
    let visible = snaps[0]
        .children
        .iter()
        .find(|c| c.selector.contains(".label"))
        .unwrap();
    assert_eq!(visible.is_visible, Some(true));
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

    let config = base_config(&fixture_path("fixture.html"), ".does-not-exist");
    let result = sniffer.sniff(&config).await;
    assert!(result.is_err());
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
