//! End-to-end MCP tests: run the server over an in-memory transport and
//! drive it with a real rmcp client.
//!
//! Chrome-backed tests are skipped when no Chromium binary is available.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use rmcp::model::{
    CallToolRequestParams, NumberOrString, ProgressNotificationParam, ProgressToken,
    ReadResourceRequestParams, RequestMetaObject,
};
use rmcp::service::{NotificationContext, RunningService};
use rmcp::{ClientHandler, RoleClient, ServiceExt};
use sniff_cdp::protocol::LaunchOptions;
use sniff_css_mcp::browser::ChromePool;
use sniff_css_mcp::server::SniffMcpServer;
use sniff_css_mcp::store::SnapshotStore;

/// Cap concurrent Chrome launches to keep CI/containers happy.
fn browser_semaphore() -> &'static tokio::sync::Semaphore {
    static SEM: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    SEM.get_or_init(|| tokio::sync::Semaphore::new(1))
}

async fn acquire_browser_slot() -> tokio::sync::SemaphorePermit<'static> {
    browser_semaphore()
        .acquire()
        .await
        .expect("semaphore closed")
}

async fn launch_pool_with_retry(opts: &LaunchOptions) -> ChromePool {
    let _slot = acquire_browser_slot().await;
    let mut last = None;
    for attempt in 0..3 {
        match ChromePool::launch(opts).await {
            Ok(pool) => return pool,
            Err(e) => {
                eprintln!("browser launch attempt {attempt} failed: {e}");
                last = Some(e);
                tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            }
        }
    }
    panic!("failed to launch browser: {}", last.unwrap())
}

fn require_chrome() -> Option<LaunchOptions> {
    sniff_cdp::BrowserProcess::available().map(|_| LaunchOptions {
        headless: true,
        launch_timeout_ms: 90_000,
        ..Default::default()
    })
}

fn fixture_url(name: &str) -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    format!("file://{}", dir.join(name).display())
}

/// Minimal client that records progress notifications.
#[derive(Clone, Default)]
struct TestClient {
    progress: Arc<Mutex<Vec<(f64, String)>>>,
}

impl ClientHandler for TestClient {
    async fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        if let Ok(mut progress) = self.progress.lock() {
            progress.push((params.progress, params.message.unwrap_or_default()));
        }
    }
}

type Running = RunningService<RoleClient, TestClient>;

/// Unique temp dir that cleans itself up on drop — the snapshot store lives
/// here so tests never write into the repo's CWD.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "sniffCSS-mcp-it-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Serve the MCP server on one end of a duplex pipe and connect a client.
/// The snapshot store is rooted in a fresh temp dir.
async fn setup(pool: ChromePool) -> (TestClient, Running, TempDir) {
    let temp = TempDir::new();
    let store = SnapshotStore::new(temp.path.clone());
    let server = SniffMcpServer::new_with_store(pool, store);
    let (server_side, client_side) = tokio::io::duplex(1 << 20);
    tokio::spawn(async move {
        server
            .serve(server_side)
            .await
            .expect("server serve")
            .waiting()
            .await
            .expect("server waiting");
    });
    let client = TestClient::default();
    let running = client
        .clone()
        .serve(client_side)
        .await
        .expect("client serve");
    (client, running, temp)
}

fn tool_call(name: &str, args: serde_json::Value) -> CallToolRequestParams {
    let mut params = CallToolRequestParams::new(name.to_owned());
    params.arguments = Some(args.as_object().expect("args object").clone());
    params
}

fn tool_call_with_progress(name: &str, args: serde_json::Value) -> CallToolRequestParams {
    let mut params = tool_call(name, args);
    params.meta = Some(RequestMetaObject::with_progress_token(ProgressToken(
        NumberOrString::String("sniff-test".into()),
    )));
    params
}

async fn wait_for_progress(client: &TestClient, count: usize) {
    for _ in 0..100 {
        if client.progress.lock().unwrap().len() >= count {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn list_tools_exposes_sniff_css_page_and_diff() {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let pool = launch_pool_with_retry(&opts).await;
    let (_client, running, _temp) = setup(pool).await;

    let tools = running.list_all_tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(names.contains(&"sniffCSS_page"));
    assert!(names.contains(&"sniffCSS_diff"));
    assert!(names.contains(&"sniffCSS_check"));
    assert!(names.contains(&"sniffCSS_categories"));

    let sniff = tools.iter().find(|t| t.name == "sniffCSS_page").unwrap();
    assert!(sniff.description.is_some());
    running.cancel().await.unwrap();
}

#[tokio::test]
async fn resources_expose_eval_prompt_and_schema() {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let pool = launch_pool_with_retry(&opts).await;
    let (_client, running, _temp) = setup(pool).await;

    let resources = running.list_all_resources().await.unwrap();
    let uris: Vec<&str> = resources.iter().map(|r| r.uri.as_str()).collect();
    assert!(uris.contains(&"sniffCSS://prompts/eval"));
    assert!(uris.contains(&"sniffCSS://schemas/eval"));
    assert!(uris.contains(&"sniffCSS://guides/golden"));

    let prompt = running
        .read_resource(ReadResourceRequestParams::new("sniffCSS://prompts/eval"))
        .await
        .unwrap();
    assert!(!prompt.contents.is_empty());

    let missing = running
        .read_resource(ReadResourceRequestParams::new("sniffCSS://missing"))
        .await;
    assert!(missing.is_err(), "unknown resource must error");
    running.cancel().await.unwrap();
}

#[tokio::test]
async fn sniff_css_page_streams_progress_and_returns_jsonl() {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let pool = launch_pool_with_retry(&opts).await;
    let (client, running, _temp) = setup(pool).await;

    let params = tool_call_with_progress(
        "sniffCSS_page",
        serde_json::json!({
            "url": fixture_url("page.html"),
            "selector": "[data-testid=\"widget\"]",
            "depth": 1,
            "categories": "box-model,typography,visual",
            "compact": true,
            "stable_key": "data-testid",
            "wait": ["network-idle:200:30000"],
            "return": "jsonl",
        }),
    );
    let result = running.call_tool(params).await.unwrap();
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .expect("text content")
        .text
        .clone();

    // JSONL: a leading __meta line (compact style_defaults hoist) precedes
    // the node line(s); the node line(s) parse and carry expected fields.
    let first_line: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    if first_line.get("__meta").is_some() {
        let node_line: serde_json::Value =
            serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
        assert_eq!(node_line["tag"], "DIV");
        assert_eq!(node_line["selector"], "div[data-testid=\"widget\"]");
        assert_eq!(node_line["is_user_noticeable"]["display_visible"], true);
        assert!(node_line["computed_style_hash"].is_string());
        assert!(node_line["styles"]["box_model"]["width"].is_string());
    } else {
        assert_eq!(first_line["tag"], "DIV");
        assert_eq!(first_line["selector"], "div[data-testid=\"widget\"]");
        assert_eq!(first_line["is_user_noticeable"]["display_visible"], true);
        assert!(first_line["computed_style_hash"].is_string());
        assert!(first_line["styles"]["box_model"]["width"].is_string());
    }

    // Progress notifications were streamed asynchronously.
    wait_for_progress(&client, 2).await;
    {
        let progress = client.progress.lock().unwrap();
        assert!(
            !progress.is_empty(),
            "expected at least one progress notification"
        );
        assert!(
            progress.iter().any(|(_, m)| m.contains("navigating")),
            "expected navigating phase, got {progress:?}"
        );
        assert!(
            progress.iter().any(|(_, m)| m.contains("formatting")),
            "expected formatting phase with node count, got {progress:?}"
        );
    }
    running.cancel().await.unwrap();
}

#[tokio::test]
async fn sniff_css_page_with_no_match_returns_tool_error() {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let pool = launch_pool_with_retry(&opts).await;
    let (_client, running, _temp) = setup(pool).await;

    let params = tool_call(
        "sniffCSS_page",
        serde_json::json!({
            "url": fixture_url("page.html"),
            "selector": ".does-not-exist",
            "wait": ["network-idle:200:30000"],
        }),
    );
    let result = running.call_tool(params).await;
    let err = result.expect_err("no-match must surface as an MCP error");
    let text = err.to_string();
    assert!(
        text.contains("no element matched"),
        "unexpected error text: {text}"
    );
}

#[tokio::test]
async fn sniff_css_diff_via_persisted_paths_returns_delta_and_summary() {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let pool = launch_pool_with_retry(&opts).await;
    let (_client, running, temp) = setup(pool).await;

    // A __sniff reference return lets us diff by persisted path.
    let params = tool_call_with_progress(
        "sniffCSS_page",
        serde_json::json!({
            "url": fixture_url("page.html"),
            "selector": "[data-testid=\"widget\"]",
            "depth": 1,
            "categories": "box-model,typography,visual",
            "compact": true,
            "stable_key": "data-testid",
            "wait": ["network-idle:200:30000"],
            "return": "reference",
        }),
    );
    let text = running
        .call_tool(params)
        .await
        .unwrap()
        .content
        .first()
        .and_then(|c| c.as_text())
        .expect("text")
        .text
        .clone();
    let reference: serde_json::Value = serde_json::from_str(&text).unwrap();
    let path = reference["__sniff"]["path"].as_str().expect("path");
    assert!(reference["__sniff"]["nodes"].as_u64().unwrap() >= 1);

    // The snapshot actually landed under the store root.
    let abs = temp.path.join(path);
    assert!(abs.exists(), "snapshot must be persisted at {abs:?}");

    // Diff the snapshot against itself via paths -> zero changes, no inline JSONL.
    let diff = running
        .call_tool(tool_call(
            "sniffCSS_diff",
            serde_json::json!({
                "base_path": path,
                "head_path": path,
                "tolerance": 0.5,
            }),
        ))
        .await
        .unwrap();
    let text = diff
        .content
        .first()
        .and_then(|c| c.as_text())
        .expect("text")
        .text
        .clone();
    let summary: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(summary["__diff_summary"]["changed"], 0);
    assert_eq!(summary["__diff_summary"]["added"], 0);
    assert_eq!(summary["__diff_summary"]["removed"], 0);
    running.cancel().await.unwrap();
}

#[tokio::test]
async fn sniff_css_diff_rejects_traversal_and_bad_files() {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let pool = launch_pool_with_retry(&opts).await;
    let (_client, running, temp) = setup(pool).await;

    // A traversal path must be rejected as invalid params.
    let err = running
        .call_tool(tool_call(
            "sniffCSS_diff",
            serde_json::json!({
                "base_path": "../escape.jsonl",
                "head_path": "missing.jsonl",
            }),
        ))
        .await
        .expect_err("traversal must fail");
    assert!(err.to_string().contains("rejected"));

    // Two known snapshots on disk that differ in one property.
    std::fs::create_dir_all(&temp.path).unwrap();
    let base = "{\"id\":1,\"tag\":\"DIV\",\"selector\":\"div.card\",\"depth\":0,\"styles\":{\"box_model\":{\"width\":\"300px\"}},\"children\":[]}\n";
    let head = "{\"id\":1,\"tag\":\"DIV\",\"selector\":\"div.card\",\"depth\":0,\"styles\":{\"box_model\":{\"width\":\"310px\"}},\"children\":[]}\n";
    std::fs::write(temp.path.join("base.jsonl"), base).unwrap();
    std::fs::write(temp.path.join("head.jsonl"), head).unwrap();

    let diff = running
        .call_tool(tool_call(
            "sniffCSS_diff",
            serde_json::json!({
                "base_path": "base.jsonl",
                "head_path": "head.jsonl",
                "tolerance": 0.5,
            }),
        ))
        .await
        .unwrap();
    let text = diff
        .content
        .first()
        .and_then(|c| c.as_text())
        .expect("text")
        .text
        .clone();
    let summary: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(summary["__diff_summary"]["changed"], 1);
    assert_eq!(summary["__diff_summary"]["base_nodes"], 1);
    assert_eq!(summary["__diff_summary"]["head_nodes"], 1);
    running.cancel().await.unwrap();
}

#[tokio::test]
async fn sniff_css_snapshots_enumerates_persisted_targets() {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let pool = launch_pool_with_retry(&opts).await;
    let (_client, running, _temp) = setup(pool).await;

    let params = tool_call(
        "sniffCSS_page",
        serde_json::json!({
            "url": fixture_url("page.html"),
            "selector": "[data-testid=\"widget\"]",
            "depth": 1,
            "categories": "box-model",
            "compact": true,
            "wait": ["network-idle:200:30000"],
        }),
    );
    running.call_tool(params).await.unwrap();

    let result = running
        .call_tool(tool_call(
            "sniffCSS_snapshots",
            serde_json::json!({ "limit": 10 }),
        ))
        .await
        .unwrap();
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .expect("text")
        .text
        .clone();
    let entry: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert_eq!(entry["domain"], "local");
    assert!(entry["path"].as_str().unwrap().ends_with(".jsonl"));
    assert!(entry["created_at"].as_str().unwrap().ends_with('Z'));
    running.cancel().await.unwrap();
}

#[tokio::test]
async fn sniff_css_check_finds_odd_card_and_contrast_failure() {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let pool = launch_pool_with_retry(&opts).await;
    let (_client, running, _temp) = setup(pool).await;

    // Three sibling cards; the third is short (uniformity outlier) and the
    // first carries a low-contrast paragraph (rules failure) in its subtree.
    let jsonl = r##"{"id":1,"tag":"DIV","selector":"div.card:nth-child(1)","depth":0,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"aria":{"focusable":false,"has_text":true},"styles":{"box_model":{"width":"300px","height":"120px"},"visual":{"color":"#212529","background-color":"#ffffff","background-image":"none"},"typography":{"font-size":"16px","font-weight":"400"}},"children":[{"id":4,"tag":"P","selector":"div.card:nth-child(1) > p","depth":1,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"aria":{"focusable":false,"has_text":true},"styles":{"visual":{"color":"#212529","background-color":"#020842","background-image":"none"},"typography":{"font-size":"16px","font-weight":"400"}},"children":[]}]}
{"id":2,"tag":"DIV","selector":"div.card:nth-child(2)","depth":0,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"aria":{"focusable":false,"has_text":true},"styles":{"box_model":{"width":"300px","height":"120px"},"visual":{"color":"#212529","background-color":"#ffffff","background-image":"none"},"typography":{"font-size":"16px","font-weight":"400"}},"children":[]}
{"id":3,"tag":"DIV","selector":"div.card:nth-child(3)","depth":0,"is_user_noticeable":{"display_visible":true,"accessibility_grade":"AAA"},"aria":{"focusable":false,"has_text":true},"styles":{"box_model":{"width":"300px","height":"80px"},"visual":{"color":"#212529","background-color":"#ffffff","background-image":"none"},"typography":{"font-size":"16px","font-weight":"400"}},"children":[]}
"##;

    let result = running
        .call_tool(tool_call(
            "sniffCSS_check",
            serde_json::json!({
                "jsonl": jsonl,
                "uniform": true,
                "rules": true,
                "tolerance": 0.5,
            }),
        ))
        .await
        .unwrap();
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .expect("text")
        .text
        .clone();

    assert!(
        text.contains("contrast-aa"),
        "rules must emit contrast-aa: {text}"
    );
    assert!(
        text.contains("\"uniformity\""),
        "uniformity outlier: {text}"
    );
    let summary: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(summary["__check_summary"]["uniformity_outliers"], 1);
    assert!(summary["__check_summary"]["rules"].as_u64().unwrap() > 0);
    running.cancel().await.unwrap();
}

#[tokio::test]
async fn page_supports_include_invisible_screenshot_attrs_and_summary_return() {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let pool = launch_pool_with_retry(&opts).await;
    let (client, running, _temp) = setup(pool).await;

    let params = tool_call_with_progress(
        "sniffCSS_page",
        serde_json::json!({
            "url": fixture_url("page.html"),
            "selector": "[data-testid=\"widget\"]",
            "depth": 1,
            "include_invisible": true,
            "screenshot": true,
            "attributes": ["data-testid", "class"],
            "return": "summary",
        }),
    );
    let result = running.call_tool(params).await.unwrap();
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .expect("text content")
        .text
        .clone();

    // Summary return: token-lean digest lines without styles. A leading
    // __meta line (compact style_defaults hoist) may precede the nodes.
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.len() >= 4, "root + 3 children, got {lines:?}");
    let root: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    if root.get("__meta").is_some() {
        assert!(lines.len() >= 5, "meta + root + 3 children, got {lines:?}");
    }
    let root: serde_json::Value =
        serde_json::from_str(lines.iter().find(|l| !l.contains("\"__meta\"")).unwrap()).unwrap();
    assert_eq!(root["tag"], "DIV");
    assert!(root.get("styles").is_none(), "summary omits styles");
    assert_eq!(root["visible"], true);

    // include_invisible: the display:none ghost child is in the digest
    // (visible=false), instead of being filtered out before capture.
    let ghost = lines
        .iter()
        .find(|l| l.contains("\"visible\":false"))
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .expect("hidden node present in summary with visible=false");
    assert_eq!(ghost["visible"], false);
    assert!(ghost["tag"].as_str().is_some());

    // Progress notifications were streamed for the multi-phase pipeline.
    assert!(!client.progress.lock().unwrap().is_empty());
    running.cancel().await.unwrap();
}
