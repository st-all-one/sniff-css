//! End-to-end MCP tests: run the server over an in-memory transport and
//! drive it with a real rmcp client.
//!
//! Chrome-backed tests are skipped when no Chromium binary is available.

use std::sync::{Arc, Mutex, OnceLock};

use rmcp::model::{
    CallToolRequestParams, NumberOrString, ProgressNotificationParam, ProgressToken,
    ReadResourceRequestParams, RequestMetaObject,
};
use rmcp::service::{NotificationContext, RunningService};
use rmcp::{ClientHandler, RoleClient, ServiceExt};
use sniff_cdp::protocol::LaunchOptions;
use sniff_mcp::browser::ChromePool;
use sniff_mcp::server::SniffMcpServer;

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

/// Serve the MCP server on one end of a duplex pipe and connect a client.
async fn setup(pool: ChromePool) -> (TestClient, Running) {
    let server = SniffMcpServer::new(pool);
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
    (client, running)
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
async fn list_tools_exposes_sniff_page_and_diff() {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let pool = launch_pool_with_retry(&opts).await;
    let (_client, running) = setup(pool).await;

    let tools = running.list_all_tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(names.contains(&"sniff_page"));
    assert!(names.contains(&"diff_snapshots"));
    assert!(names.contains(&"list_categories"));

    let sniff = tools.iter().find(|t| t.name == "sniff_page").unwrap();
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
    let (_client, running) = setup(pool).await;

    let resources = running.list_all_resources().await.unwrap();
    let uris: Vec<&str> = resources.iter().map(|r| r.uri.as_str()).collect();
    assert!(uris.contains(&"sniff://prompts/eval"));
    assert!(uris.contains(&"sniff://schemas/eval"));

    let prompt = running
        .read_resource(ReadResourceRequestParams::new("sniff://prompts/eval"))
        .await
        .unwrap();
    assert!(!prompt.contents.is_empty());

    let missing = running
        .read_resource(ReadResourceRequestParams::new("sniff://missing"))
        .await;
    assert!(missing.is_err(), "unknown resource must error");
    running.cancel().await.unwrap();
}

#[tokio::test]
async fn sniff_page_streams_progress_and_returns_jsonl() {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let pool = launch_pool_with_retry(&opts).await;
    let (client, running) = setup(pool).await;

    let params = tool_call_with_progress(
        "sniff_page",
        serde_json::json!({
            "url": fixture_url("page.html"),
            "selector": "[data-testid=\"widget\"]",
            "depth": 1,
            "categories": "box-model,typography,visual",
            "compact": true,
            "stable_key": "data-testid",
            "wait": ["network-idle:200:30000"],
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

    // JSONL root line(s) parse and carry expected fields.
    let first_line: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert_eq!(first_line["tag"], "DIV");
    assert_eq!(first_line["selector"], "div[data-testid=\"widget\"]");
    assert_eq!(first_line["is_visible"], true);
    assert!(first_line["computed_style_hash"].is_string());
    assert!(first_line["styles"]["box_model"]["width"].is_string());

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
async fn sniff_page_with_no_match_returns_tool_error() {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let pool = launch_pool_with_retry(&opts).await;
    let (_client, running) = setup(pool).await;

    let params = tool_call(
        "sniff_page",
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
async fn diff_snapshots_returns_delta_and_summary() {
    let Some(opts) = require_chrome() else {
        eprintln!("skipping: no Chrome binary found");
        return;
    };
    let pool = launch_pool_with_retry(&opts).await;
    let (_client, running) = setup(pool).await;

    // Same page, same snapshot -> zero changes.
    let params = tool_call_with_progress(
        "sniff_page",
        serde_json::json!({
            "url": fixture_url("page.html"),
            "selector": "[data-testid=\"widget\"]",
            "depth": 1,
            "categories": "box-model,typography,visual",
            "compact": true,
            "stable_key": "data-testid",
            "wait": ["network-idle:200:30000"],
        }),
    );
    let snapshot = running
        .call_tool(params)
        .await
        .unwrap()
        .content
        .first()
        .and_then(|c| c.as_text())
        .expect("text")
        .text
        .clone();

    let diff = running
        .call_tool(tool_call(
            "diff_snapshots",
            serde_json::json!({
                "base_jsonl": snapshot,
                "head_jsonl": snapshot,
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
