//! `sniffCSS` binary.

mod args;

use anyhow::Context;
use args::{Backend, Cli};
use clap::Parser;
use sniff_cdp::protocol::LaunchOptions;
use sniff_core::snapshot::{ensure_gitignored, snapshot_file_name, snapshot_root};
use sniff_engine::{Sniffer, write_output};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = Cli::parse();
    match cli.effective_backend() {
        Backend::Flutter => run_flutter(&cli).await,
        Backend::Web => run_web(&cli).await,
        Backend::Auto => unreachable!("effective_backend resolves Auto"),
    }
}

/// Web backend: Chromium over CDP.
async fn run_web(cli: &Cli) -> anyhow::Result<()> {
    let chrome_path = cli.chrome.clone();
    let connect = cli.connect.clone();
    let screenshot_path = cli.screenshot.clone();
    let persist = cli.persist;
    let config = cli.clone().into_config().context("invalid configuration")?;

    let outcome = if let Some(endpoint) = &connect {
        let sniffer = Sniffer::connect(endpoint)
            .await
            .context("connecting to browser")?;
        sniffer
            .sniff(&config)
            .await
            .context("sniffing via existing browser")?
    } else {
        let opts = LaunchOptions {
            executable: chrome_path,
            headless: true,
            ..Default::default()
        };
        let sniffer = Sniffer::launch(&opts).await.context("launching browser")?;
        sniffer.sniff(&config).await.context("sniffing page")?
    };

    emit(
        &config,
        outcome.snapshots,
        persist,
        screenshot_path.as_deref(),
        &config.url,
        &config.selector,
    )
}

/// Flutter backend: a debug-mode Flutter app over the Dart VM Service.
async fn run_flutter(cli: &Cli) -> anyhow::Result<()> {
    use sniff_flutter::extractor::extract;
    use sniff_flutter::{EmulatorProcess, FlutterDriver, FlutterInspector, FlutterMachine, ViewportGuard};

    let config = cli.clone().into_config().context("invalid configuration")?;

    // Resolve the target device: an AVD to launch, an attached serial, or the
    // one implied by the `flutter://<device>` URL.
    let device = if let Some(avd) = &cli.avd {
        let emulator = EmulatorProcess::launch(avd)
            .await
            .context("launching emulator")?;
        let serial = emulator
            .serial()
            .ok_or_else(|| anyhow::anyhow!("emulator did not report a serial"))?
            .to_string();
        // Keep the emulator alive for the whole capture; dropped at exit.
        std::mem::forget(emulator);
        serial
    } else {
        cli.device.clone().or_else(|| cli.flutter_device()).ok_or_else(|| {
            anyhow::anyhow!(
                "flutter backend needs --device <serial>, --avd <name> or a flutter://<device> URL"
            )
        })?
    };

    // Apply --viewport on the device (`adb shell wm size`) before launching the
    // app so the Flutter MediaQuery/layout reflect the requested size. The
    // guard restores the previous size afterwards (and on any error path).
    let viewport_guard = match &cli.viewport {
        Some(spec) => {
            let vp = sniff_core::Viewport::parse_cli(spec)
                .map_err(|e| anyhow::anyhow!("invalid --viewport: {e}"))?;
            Some(
                ViewportGuard::apply(&device, vp.width, vp.height)
                    .await
                    .context("applying --viewport on device (adb wm size)")?,
            )
        }
        None => None,
    };

    // Run (or attach to) the app and discover the VM Service URI.
    let ws_uri = if cli.attach {
        let project = cli.project.clone().unwrap_or_else(|| {
            sniff_flutter::device::find_project_root(&cli.target)
                .unwrap_or_else(|| PathBuf::from("."))
                .to_string_lossy()
                .into_owned()
        });
        let mut machine = FlutterMachine::attach(std::path::Path::new(&project), &device)
            .await
            .context("flutter attach")?;
        machine
            .wait_for_vm_service(std::time::Duration::from_secs(90))
            .await
            .context("waiting for VM Service on attach")?
    } else {
        let project = cli.project.clone().unwrap_or_else(|| {
            sniff_flutter::device::find_project_root(&cli.target)
                .unwrap_or_else(|| PathBuf::from("."))
                .to_string_lossy()
                .into_owned()
        });
        let mut machine = FlutterMachine::run(std::path::Path::new(&project), &cli.target, &device)
            .await
            .context("flutter run")?;
        machine
            .wait_for_vm_service(std::time::Duration::from_secs(180))
            .await
            .context("waiting for VM Service (is the app built in debug mode?)")?
    };

    // Connect the widget inspector (and the Flutter Driver extension when
    // actions are configured). Animations are frozen AFTER the actions so a
    // reveal transition (modal, dropdown) completes at normal speed first —
    // freezing first would capture the interaction mid-animation.
    let inspector = FlutterInspector::connect(&ws_uri)
        .await
        .context("connecting to VM Service")?;
    let roots = if config.actions.is_empty() {
        inspector
            .freeze_animations()
            .await
            .context("freezing animations")?;
        extract(&inspector, config.depth)
            .await
            .context("extracting Flutter widget tree")?
    } else {
        let driver = FlutterDriver::connect(&ws_uri)
            .await
            .context("connecting to Flutter Driver extension")?;
        if !driver.is_available().await {
            anyhow::bail!(
                "flutter --action needs the app to call `enableFlutterDriverExtension()` in \
                 main() (add `flutter_driver` to dev_dependencies; see docs/flutter.md)"
            );
        }
        for act in &config.actions {
            sniff_flutter::perform_action(&driver, act)
                .await
                .with_context(|| format!("performing {} `{}`", act.kind(), act.selector()))?;
            tokio::time::sleep(std::time::Duration::from_millis(act.settle_ms())).await;
        }
        inspector
            .freeze_animations()
            .await
            .context("freezing animations after actions")?;
        extract(&inspector, config.depth)
            .await
            .context("extracting Flutter widget tree after actions")?
    };
    let screenshot_path = cli.screenshot.clone();
    let mut screenshot = None;
    if screenshot_path.is_some() {
        let serial = device.clone();
        let emulator =
            EmulatorProcess::attach(&serial).context("attaching device for screenshot")?;
        screenshot = Some(emulator.screenshot().await.context("adb screencap")?);
    }
    inspector.close().await;

    // Restore the device size captured before `--viewport` was applied. The
    // Drop backstop on `ViewportGuard` covers the error paths above.
    if let Some(guard) = &viewport_guard
        && let Err(e) = guard.restore().await
    {
        eprintln!("warning: restoring device viewport after capture: {e}");
    }

    let url = format!("flutter://{device}");
    emit(
        &config,
        roots,
        cli.persist,
        screenshot_path.as_deref(),
        &url,
        &config.selector,
    )?;

    if let Some(bytes) = screenshot {
        let path = cli.screenshot.as_ref().context("screenshot path")?;
        std::fs::write(path, &bytes).context("writing flutter screenshot")?;
        eprintln!("screenshot saved to {path}");
    }
    Ok(())
}

/// Serialize `snapshots` to stdout (and persist when requested).
#[allow(clippy::too_many_arguments)]
fn emit(
    config: &sniff_core::SniffConfig,
    snapshots: Vec<sniff_core::ElementSnapshot>,
    persist: bool,
    screenshot_path: Option<&str>,
    url: &str,
    selector: &str,
) -> anyhow::Result<()> {
    let outcome = sniff_engine::extractor::SniffOutcome {
        snapshots,
        global_css_variables: None,
        ax_tree: None,
        actions: None,
        screenshot: None,
    };

    let mut buf = Vec::new();
    {
        let mut out = BufWriter::new(&mut buf);
        write_output(&mut out, &outcome, &config.output)?;
        out.flush()?;
    }

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    out.write_all(&buf)?;
    out.flush()?;

    if persist {
        let path = persist_snapshot(url, selector, &config.output.format, &buf)?;
        eprintln!("snapshot saved to {}", path.display());
    }

    if let Some(path) = screenshot_path
        && let Some(bytes) = &outcome.screenshot
    {
        std::fs::write(path, bytes).with_context(|| format!("writing screenshot to {path}"))?;
        eprintln!("screenshot saved to {path}");
    }
    Ok(())
}

/// Persist the serialized output to the same sortable tree the MCP store
/// uses: `sniffCSS/[domain]/[UTC]-[path]-[selector].<ext>` (root from
/// `SNIFF_SNAPSHOT_DIR` or the CWD), with the tree auto-ignored by git.
/// Returns the root-relative path (matching MCP `__sniff` semantics).
fn persist_snapshot(
    url: &str,
    selector: &str,
    format: &sniff_core::OutputFormat,
    bytes: &[u8],
) -> anyhow::Result<PathBuf> {
    let root = snapshot_root();
    ensure_gitignored(&root)
        .with_context(|| format!("preparing snapshot root {}", root.display()))?;

    let ext = match format {
        sniff_core::OutputFormat::Json => "json",
        _ => "jsonl",
    };
    let domain = sniff_core::snapshot::domain_of(url);
    let name = snapshot_file_name(url, selector, ext);
    let rel = PathBuf::from(&domain).join(&name);
    let abs = root.join(&rel);

    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(&abs, bytes)
        .with_context(|| format!("writing snapshot to {}", abs.display()))?;
    Ok(rel)
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
