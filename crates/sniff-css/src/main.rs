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
        outcome,
        persist,
        screenshot_path.as_deref(),
        &config.url,
        &config.selector,
    )
}

/// Flutter backend: a debug-mode Flutter app over the Dart VM Service.
async fn run_flutter(cli: &Cli) -> anyhow::Result<()> {
    use sniff_flutter::extractor::extract;
    use sniff_flutter::{
        EmulatorProcess, FlutterDriver, FlutterInspector, FlutterMachine, ViewportGuard,
    };

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
    // A previous capture may have left the app frozen (`timeDilation` 1e6),
    // which blocks driver actions (their `endOfFrame` pump never completes);
    // restore real time before doing anything.
    inspector
        .set_time_dilation(1.0)
        .await
        .context("restoring app time dilation")?;
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
        driver
            .keep_frames_alive()
            .await
            .context("keeping app frames alive for actions")?;
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
    // Leave the app running at real time: the freeze above is only for a
    // deterministic capture, and a permanently-frozen app breaks later driver
    // actions and the developer's own hot-reload.
    inspector
        .set_time_dilation(1.0)
        .await
        .context("restoring app time dilation after capture")?;
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
    let outcome = sniff_engine::extractor::SniffOutcome {
        snapshots: roots,
        global_css_variables: None,
        ax_tree: None,
        actions: None,
        screenshot,
    };
    emit(
        &config,
        outcome,
        cli.persist,
        screenshot_path.as_deref(),
        &url,
        &config.selector,
    )?;

    Ok(())
}

/// Serialize `snapshots` to stdout (and persist when requested).
#[allow(clippy::too_many_arguments)]
fn emit(
    config: &sniff_core::SniffConfig,
    outcome: sniff_engine::extractor::SniffOutcome,
    persist: bool,
    screenshot_path: Option<&str>,
    url: &str,
    selector: &str,
) -> anyhow::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal but structurally valid PNG (signature + IHDR for 1x1).
    fn png_bytes() -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes()); // IHDR chunk length
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&1u32.to_be_bytes()); // width
        bytes.extend_from_slice(&1u32.to_be_bytes()); // height
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]); // bit depth, color type, compression, filter, interlace
        bytes
    }

    fn config(screenshot_full_page: bool) -> sniff_core::SniffConfig {
        sniff_core::SniffConfig {
            url: "https://example.com".into(),
            selector: "body".into(),
            screenshot: true,
            screenshot_full_page,
            ..Default::default()
        }
    }

    fn outcome_with(screenshot: Option<Vec<u8>>) -> sniff_engine::extractor::SniffOutcome {
        sniff_engine::extractor::SniffOutcome {
            snapshots: Vec::new(),
            global_css_variables: None,
            ax_tree: None,
            actions: None,
            screenshot,
        }
    }

    #[test]
    fn emit_writes_screenshot_bytes_when_present() {
        // Regression: `emit` used to rebuild the outcome with `screenshot:
        // None`, silently dropping the PNG the engine captured (web backend).
        let bytes = png_bytes();
        let tmp = std::env::temp_dir().join(format!("sniffcss-emit-{}.png", std::process::id()));
        emit(
            &config(false),
            outcome_with(Some(bytes.clone())),
            false,
            Some(tmp.to_str().unwrap()),
            "https://example.com",
            "body",
        )
        .unwrap();
        assert_eq!(
            std::fs::read(&tmp).unwrap(),
            bytes,
            "screenshot bytes must reach the file"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn emit_writes_screenshot_with_full_page_flag() {
        // Flutter/`--fullpage-screenshot` path: the full-page flag must not
        // drop the screenshot bytes on the way to disk.
        let bytes = png_bytes();
        let tmp =
            std::env::temp_dir().join(format!("sniffcss-emit-full-{}.png", std::process::id()));
        emit(
            &config(true),
            outcome_with(Some(bytes.clone())),
            false,
            Some(tmp.to_str().unwrap()),
            "flutter://emulator-5554",
            "root",
        )
        .unwrap();
        assert_eq!(
            std::fs::read(&tmp).unwrap(),
            bytes,
            "full-page screenshot bytes must reach the file"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn emit_without_screenshot_path_skips_write() {
        // No `--screenshot` path → nothing is written even when bytes exist.
        let tmp =
            std::env::temp_dir().join(format!("sniffcss-emit-none-{}.png", std::process::id()));
        emit(
            &config(false),
            outcome_with(Some(png_bytes())),
            false,
            None,
            "https://example.com",
            "body",
        )
        .unwrap();
        assert!(!tmp.exists());
    }

    #[test]
    fn persist_writes_to_sniff_dir_with_gitignore() {
        // `--persist` must mirror the MCP store layout: `sniffCSS/[domain]/`
        // under `SNIFF_SNAPSHOT_DIR` (or CWD), with a `*.gitignore` inside the
        // root and the right extension for the output format.
        let root = std::env::temp_dir().join(format!("sniffcss-persist-{}", std::process::id()));
        let root_str = root.to_str().unwrap().to_string();
        // Restore afterwards so other tests keep the env pristine.
        let prev = std::env::var("SNIFF_SNAPSHOT_DIR").ok();
        unsafe {
            std::env::set_var("SNIFF_SNAPSHOT_DIR", &root_str);
        }
        let result = (|| {
            let bytes = b"{\"tag\":\"DIV\"}\n";
            let rel = persist_snapshot(
                "http://localhost:3000/products/42?id=1",
                "main",
                &sniff_core::OutputFormat::JsonLines,
                bytes,
            )?;
            let abs = root.join(&rel);
            let written = std::fs::read_to_string(&abs)?;
            assert_eq!(written, String::from_utf8_lossy(bytes));
            assert_eq!(abs.extension().unwrap(), "jsonl");
            let rel_parts: Vec<_> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            assert_eq!(rel_parts.len(), 2, "domain/name layout: {rel:?}");
            assert_eq!(rel_parts[0], "localhost_3000");
            assert!(rel_parts[1].contains("products_42"));
            assert!(rel_parts[1].contains("main"));
            let gitignore = std::fs::read_to_string(root.join(".gitignore"))?;
            assert_eq!(gitignore, "*\n");
            Ok::<(), anyhow::Error>(())
        })();
        match prev {
            Some(v) => unsafe {
                std::env::set_var("SNIFF_SNAPSHOT_DIR", v);
            },
            None => unsafe {
                std::env::remove_var("SNIFF_SNAPSHOT_DIR");
            },
        }
        let _ = std::fs::remove_dir_all(&root);
        result.unwrap();
    }

    #[test]
    fn persist_uses_json_extension_for_json_output() {
        let root = std::env::temp_dir().join(format!("sniffcss-persist-{}", std::process::id()));
        let root_str = root.to_str().unwrap().to_string();
        let prev = std::env::var("SNIFF_SNAPSHOT_DIR").ok();
        unsafe {
            std::env::set_var("SNIFF_SNAPSHOT_DIR", &root_str);
        }
        let result = persist_snapshot(
            "https://example.com/",
            "body",
            &sniff_core::OutputFormat::Json,
            b"[]",
        );
        match prev {
            Some(v) => unsafe {
                std::env::set_var("SNIFF_SNAPSHOT_DIR", v);
            },
            None => unsafe {
                std::env::remove_var("SNIFF_SNAPSHOT_DIR");
            },
        }
        let rel = result.unwrap();
        let abs = root.join(rel);
        assert_eq!(abs.extension().unwrap(), "json");
        let _ = std::fs::remove_dir_all(&root);
    }
}
