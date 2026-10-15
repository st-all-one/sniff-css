//! Handler de `sniff capture`.

use anyhow::Context;
use sniff_cdp::protocol::LaunchOptions;
use sniff_core::snapshot::{ensure_gitignored, snapshot_file_name, snapshot_root};
use sniff_engine::{Sniffer, write_output};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::cli::Cli;
use crate::cli::capture::{CaptureArgs, EffectiveBackend};
use crate::output::AppError;

/// Captura estilos computados (web via CDP ou Flutter via VM Service).
pub(crate) async fn run(global: &Cli, args: &CaptureArgs) -> Result<(), AppError> {
    let mut args = args.clone();
    if global.json {
        // `--json` é a saída de máquina: força um único array JSON.
        args.output = "json".to_string();
        args.summary = false;
        args.no_summary = false;
    }
    match args.effective_backend() {
        EffectiveBackend::Flutter => run_flutter(&args).await.map_err(AppError::from),
        EffectiveBackend::Web => run_web(&args).await.map_err(AppError::from),
    }
}

/// Web backend: Chromium over CDP.
async fn run_web(args: &CaptureArgs) -> anyhow::Result<()> {
    let chrome_path = args.chrome.clone();
    let connect = args.connect.clone();
    let screenshot_path = args.screenshot.clone();
    let persist = args.persist;
    let ignore_certificate_errors = args.ignore_certificate_errors;
    let config = args
        .clone()
        .into_config()
        .context("invalid configuration")?;

    let outcome = if let Some(endpoint) = &connect {
        let sniffer = Sniffer::connect(endpoint)
            .await
            .context("connecting to browser")?;
        sniffer.set_ignore_certificate_errors(ignore_certificate_errors);
        sniffer
            .sniff(&config)
            .await
            .context("sniffing via existing browser")?
    } else {
        let opts = LaunchOptions {
            executable: chrome_path,
            headless: true,
            ignore_certificate_errors,
            ..Default::default()
        };
        let sniffer = Sniffer::launch(&opts).await.context("launching browser")?;
        sniffer.sniff(&config).await.context("sniffing page")?
    };

    emit(
        &config,
        &outcome,
        persist,
        screenshot_path.as_deref(),
        &config.url,
        &config.selector,
    )
}

/// Flutter backend: a debug-mode Flutter app over the Dart VM Service.
async fn run_flutter(args: &CaptureArgs) -> anyhow::Result<()> {
    use sniff_flutter::extractor::extract;
    use sniff_flutter::{
        EmulatorProcess, FlutterDriver, FlutterInspector, FlutterMachine, ViewportGuard,
    };

    let config = args
        .clone()
        .into_config()
        .context("invalid configuration")?;

    // Resolve the target device: an AVD to launch, an attached serial, or the
    // one implied by the `flutter://<device>` URL.
    let device = if let Some(avd) = &args.avd {
        let emulator = EmulatorProcess::launch(avd)
            .await
            .context("launching emulator")?;
        let serial = emulator
            .serial()
            .ok_or_else(|| anyhow::anyhow!("emulator did not report a serial"))?
            .to_string();
        // Keep the emulator alive for the whole capture; dropped at exit.
        #[expect(
            clippy::mem_forget,
            clippy::disallowed_methods,
            reason = "o emulador deve viver até o fim do processo"
        )]
        std::mem::forget(emulator);
        serial
    } else {
        args.device.clone().or_else(|| args.flutter_device()).ok_or_else(|| {
            anyhow::anyhow!(
                "flutter backend needs --device <serial>, --avd <name> or a flutter://<device> URL"
            )
        })?
    };

    // Apply --viewport on the device (`adb shell wm size`) before launching the
    // app so the Flutter MediaQuery/layout reflect the requested size. The
    // guard restores the previous size afterwards (and on any error path).
    let viewport_guard = match &args.viewport {
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
    let ws_uri = if args.attach {
        let project = args.project.clone().unwrap_or_else(|| {
            sniff_flutter::device::find_project_root(&args.target)
                .unwrap_or_else(|| PathBuf::from("."))
                .to_string_lossy()
                .into_owned()
        });
        let mut machine =
            FlutterMachine::attach(Path::new(&project), &device).context("flutter attach")?;
        machine
            .wait_for_vm_service(std::time::Duration::from_secs(90))
            .await
            .context("waiting for VM Service on attach")?
    } else {
        let project = args.project.clone().unwrap_or_else(|| {
            sniff_flutter::device::find_project_root(&args.target)
                .unwrap_or_else(|| PathBuf::from("."))
                .to_string_lossy()
                .into_owned()
        });
        let mut machine = FlutterMachine::run(Path::new(&project), &args.target, &device)
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
    let screenshot_path = args.screenshot.clone();
    let screenshot = if screenshot_path.is_some() {
        let serial = device.clone();
        let emulator =
            EmulatorProcess::attach(&serial).context("attaching device for screenshot")?;
        Some(emulator.screenshot().await.context("adb screencap")?)
    } else {
        None
    };
    inspector.close().await;

    // Restore the device size captured before `--viewport` was applied. The
    // Drop backstop on `ViewportGuard` covers the error paths above.
    if let Some(guard) = &viewport_guard
        && let Err(e) = guard.restore().await
    {
        sniff_core::diagnostic::write(format_args!(
            "warning: restoring device viewport after capture: {e}"
        ));
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
        &outcome,
        args.persist,
        screenshot_path.as_deref(),
        &url,
        &config.selector,
    )?;

    Ok(())
}

/// Serialize `snapshots` to stdout (and persist when requested).
#[expect(
    clippy::too_many_arguments,
    reason = "agrega config, outcome e flags de saída"
)]
fn emit(
    config: &sniff_core::SniffConfig,
    outcome: &sniff_engine::extractor::SniffOutcome,
    persist: bool,
    screenshot_path: Option<&str>,
    url: &str,
    selector: &str,
) -> anyhow::Result<()> {
    let mut buf = Vec::new();
    {
        let mut out = BufWriter::new(&mut buf);
        write_output(&mut out, outcome, &config.output)?;
        out.flush()?;
    }

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    out.write_all(&buf)?;
    out.flush()?;

    if persist {
        let root = snapshot_root();
        let path = persist_snapshot(&root, url, selector, config.output.format, &buf)?;
        sniff_core::diagnostic::write(format_args!("snapshot saved to {}", path.display()));
    }

    if let Some(path) = screenshot_path
        && let Some(bytes) = &outcome.screenshot
    {
        std::fs::write(path, bytes).with_context(|| format!("writing screenshot to {path}"))?;
        sniff_core::diagnostic::write(format_args!("screenshot saved to {path}"));
    }
    Ok(())
}

/// Persist the serialized output to the same sortable tree the MCP store
/// uses: `sniff/[domain]/[UTC]-[path]-[selector].<ext>` (root from
/// `SNIFF_SNAPSHOT_DIR` or the CWD), with the tree auto-ignored by git.
/// Returns the root-relative path (matching MCP `__sniff` semantics).
fn persist_snapshot(
    root: &Path,
    url: &str,
    selector: &str,
    format: sniff_core::OutputFormat,
    bytes: &[u8],
) -> anyhow::Result<PathBuf> {
    ensure_gitignored(root)
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
            &outcome_with(Some(bytes.clone())),
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
        drop(std::fs::remove_file(&tmp));
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
            &outcome_with(Some(bytes.clone())),
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
        drop(std::fs::remove_file(&tmp));
    }

    #[test]
    fn emit_without_screenshot_path_skips_write() {
        // No `--screenshot` path → nothing is written even when bytes exist.
        let tmp =
            std::env::temp_dir().join(format!("sniffcss-emit-none-{}.png", std::process::id()));
        emit(
            &config(false),
            &outcome_with(Some(png_bytes())),
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
        // `--persist` must mirror the MCP store layout: `sniff/[domain]/`
        // under the snapshot root, with a `*.gitignore` inside the root and the
        // right extension for the output format. The root is injected so the
        // test never mutates the process-global `SNIFF_SNAPSHOT_DIR`.
        let root =
            std::env::temp_dir().join(format!("sniffcss-persist-gitignore-{}", std::process::id()));
        let result = (|| {
            let bytes = b"{\"tag\":\"DIV\"}\n";
            let rel = persist_snapshot(
                &root,
                "http://localhost:3000/products/42?id=1",
                "main",
                sniff_core::OutputFormat::JsonLines,
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
        drop(std::fs::remove_dir_all(&root));
        result.unwrap();
    }

    #[test]
    fn persist_uses_json_extension_for_json_output() {
        let root =
            std::env::temp_dir().join(format!("sniffcss-persist-json-{}", std::process::id()));
        let rel = persist_snapshot(
            &root,
            "https://example.com/",
            "body",
            sniff_core::OutputFormat::Json,
            b"[]",
        )
        .unwrap();
        let abs = root.join(rel);
        assert_eq!(abs.extension().unwrap(), "json");
        drop(std::fs::remove_dir_all(&root));
    }
}
