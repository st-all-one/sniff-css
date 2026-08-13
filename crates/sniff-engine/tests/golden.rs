//! Golden regression tests: lock the current web pipeline output byte-for-byte.
//!
//! These tests sniff the on-disk HTML fixtures with a fixed config (the
//! golden-run contract: compact ON, 1366x768, stabilize, contrast) and compare
//! the serialized JSONL against committed golden files under `tests/golden/`.
//!
//! They exist so that refactors of the transport/engine (e.g. the
//! `CdpClient -> JsonRpcClient` generalization) can never silently change the
//! web output. Skipped (pass with a message) when no Chrome binary is present.
//!
//! Regenerate deliberately (Chrome bump, intentional output change) with:
//!   UPDATE_GOLDEN=1 cargo test -p sniff-engine --test golden

use sniff_cdp::protocol::LaunchOptions;
use sniff_core::config::{OutputFormat, parse_categories};
use sniff_core::{ElementFilter, OutputConfig, SniffConfig, SniffResult, Viewport, WaitStrategy};
use sniff_engine::{Sniffer, write_output};
use std::sync::OnceLock;
use tokio::sync::{Semaphore, SemaphorePermit};

fn browser_semaphore() -> &'static Semaphore {
    static SEM: OnceLock<Semaphore> = OnceLock::new();
    SEM.get_or_init(|| Semaphore::new(1))
}

async fn acquire_browser_slot() -> SemaphorePermit<'static> {
    browser_semaphore()
        .acquire()
        .await
        .expect("semaphore closed")
}

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

fn golden_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn require_chrome() -> Option<LaunchOptions> {
    sniff_cdp::BrowserProcess::available().map(|_| LaunchOptions {
        headless: true,
        launch_timeout_ms: 90_000,
        ..Default::default()
    })
}

/// Fixed, deterministic config matching the golden-run contract.
fn golden_config(url: &str, selector: &str, depth: usize) -> SniffConfig {
    SniffConfig {
        url: url.to_string(),
        selector: selector.to_string(),
        depth,
        categories: parse_categories("box-model,layout,typography,visual").unwrap(),
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
            compact: true,
            include_visibility: true,
            include_style_hash: true,
            include_aria: true,
            include_contrast: true,
            include_ax: false,
        },
        viewport: Some(Viewport {
            width: 1366,
            height: 768,
        }),
        include_custom_properties: false,
        stable_key: None,
        attributes: vec![],
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

/// Sniff once, serialize, and compare against (or write) the golden file.
async fn check_golden(
    fixture: &str,
    selector: &str,
    depth: usize,
    golden: &str,
) -> SniffResult<()> {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return Ok(());
    };
    let sniffer = launch_with_retry(&opts).await?;
    let _slot = acquire_browser_slot().await;

    let config = golden_config(&fixture_path(fixture), selector, depth);
    let outcome = sniffer.sniff(&config).await?;

    let mut buf = Vec::new();
    write_output(&mut buf, &outcome, &config.output)?;
    let text = String::from_utf8(buf).expect("output is utf8");
    let path = golden_path(golden);

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("golden parent")).expect("create golden dir");
        std::fs::write(&path, &text).expect("write golden");
        eprintln!("golden written: {}", path.display());
        return Ok(());
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {} (run UPDATE_GOLDEN=1 to write it): {e}",
            path.display()
        )
    });
    if text != expected {
        let out = golden_path(&format!("{golden}.actual"));
        std::fs::write(&out, &text).expect("write actual");
        panic!(
            "golden mismatch for {golden}: {} bytes expected, {} got. Actual saved to {} \
             (diff it against the golden; regenerate intentionally with UPDATE_GOLDEN=1)",
            expected.len(),
            text.len(),
            out.display()
        );
    }
    Ok(())
}

#[tokio::test]
async fn golden_card_fixture() -> SniffResult<()> {
    check_golden("fixture.html", ".card", 2, "fixture.card.jsonl").await
}

#[tokio::test]
async fn golden_forms_fixture() -> SniffResult<()> {
    check_golden("forms.html", "form", 2, "forms.form.jsonl").await
}

#[tokio::test]
async fn golden_roles_fixture() -> SniffResult<()> {
    check_golden("roles.html", "main", 2, "roles.main.jsonl").await
}
