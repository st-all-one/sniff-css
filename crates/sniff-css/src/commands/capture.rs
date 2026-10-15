//! Handler de `sniff capture`.

use anyhow::Context;
use serde_json::json;
use sniff_cdp::protocol::LaunchOptions;
use sniff_core::snapshot::{ensure_gitignored, snapshot_file_name, snapshot_root};
use sniff_engine::{Sniffer, write_output};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::cli::Cli;
use crate::cli::capture::{CaptureArgs, EffectiveBackend};
use crate::output::{self, AppError};

/// Resultado serializado de uma captura (bytes + caminho persistido).
struct Captured {
    bytes: Vec<u8>,
    persisted: Option<PathBuf>,
}

/// Captura estilos computados (web via CDP ou Flutter via VM Service).
///
/// Resolve a fonte (`--params`/`--batch`/operando), executa uma ou N operações
/// e escreve o payload/status em stdout.
pub(crate) async fn run(global: &Cli, args: &CaptureArgs) -> Result<(), AppError> {
    let base = args.clone();
    let has_operand = base.url.is_some() || base.url_legacy.is_some();
    let mut params = args.params.as_deref();
    let batch = args.batch.as_deref();
    if params.is_none() && batch.is_none() && !has_operand {
        if sniff_core::input::stdin_is_piped() {
            params = Some("-");
        } else {
            return Err(AppError::invalid(
                "missing <URL>: pass the page URL, `--params`/`--batch`, or a pipe",
            ));
        }
    }
    let items = crate::input::requests(&base, params, batch)?;

    if args.dry_run {
        return dry_run(&items);
    }
    if batch.is_some() {
        return run_batch(global, &items).await;
    }

    let item = items
        .into_iter()
        .next()
        .ok_or_else(|| AppError::invalid("sem operação"))??;
    let item = apply_global(item, global);
    let captured = run_item(&item).await?;
    output::write_stdout(&captured.bytes).map_err(AppError::from)
}

/// `--batch`: executa N operações, persistindo cada payload e emitindo uma
/// linha JSONL de status por item (best-effort: um item ruim não aborta o lote).
async fn run_batch(global: &Cli, items: &[Result<CaptureArgs, AppError>]) -> Result<(), AppError> {
    let mut lines = String::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut failures = 0_usize;
    for (index, item) in items.iter().enumerate() {
        let mut item = match item {
            Ok(item) => apply_global(item.clone(), global),
            Err(error) => {
                failures = failures.saturating_add(1);
                warnings.push(format!(
                    "item {}: {}",
                    index.saturating_add(1),
                    error.message
                ));
                lines.push_str(
                    &json!({"index": index, "ok": false, "error": error.message}).to_string(),
                );
                lines.push('\n');
                continue;
            }
        };
        // Lote nunca descarta payload: sem destino explícito, persiste no store.
        if item.persist.is_none() {
            item.persist = Some(String::new());
        }
        match run_item(&item).await {
            Ok(captured) => {
                let path = captured
                    .persisted
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned());
                lines.push_str(&json!({"index": index, "ok": true, "path": path}).to_string());
                lines.push('\n');
            }
            Err(error) => {
                failures = failures.saturating_add(1);
                warnings.push(format!(
                    "item {}: {}",
                    index.saturating_add(1),
                    error.message
                ));
                lines.push_str(
                    &json!({"index": index, "ok": false, "error": error.message}).to_string(),
                );
                lines.push('\n');
            }
        }
    }
    for warning in &warnings {
        output::warn(warning);
    }
    output::write_stdout(lines.as_bytes()).map_err(AppError::from)?;
    if failures == 0 {
        Ok(())
    } else {
        Err(AppError::invalid(format!("{failures} item(ns) falharam")))
    }
}

/// `--dry-run`: valida a configuração de cada item sem tocar no browser.
fn dry_run(items: &[Result<CaptureArgs, AppError>]) -> Result<(), AppError> {
    let mut lines = String::new();
    let mut failures: Vec<String> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let validation = match item {
            Ok(capture) => capture
                .clone()
                .into_config()
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Err(error) => Err(error.message.clone()),
        };
        match validation {
            Ok(()) => {
                lines.push_str(&json!({"index": index, "ok": true}).to_string());
                lines.push('\n');
            }
            Err(message) => {
                failures.push(format!("item {}: {message}", index.saturating_add(1)));
                lines.push_str(&json!({"index": index, "ok": false, "error": message}).to_string());
                lines.push('\n');
            }
        }
    }
    for failure in &failures {
        output::warn(failure);
    }
    output::write_stdout(lines.as_bytes()).map_err(AppError::from)?;
    if failures.is_empty() {
        Ok(())
    } else {
        Err(AppError::invalid(format!(
            "{} item(ns) inválidos",
            failures.len()
        )))
    }
}

/// `--json` é a saída de máquina: força um único snapshot JSON.
fn apply_global(mut item: CaptureArgs, global: &Cli) -> CaptureArgs {
    if global.json {
        item.output = "json".to_string();
        item.summary = false;
        item.no_summary = false;
    }
    item
}

/// Executa uma operação de captura no backend efetivo.
async fn run_item(args: &CaptureArgs) -> Result<Captured, AppError> {
    match args.effective_backend() {
        EffectiveBackend::Flutter => run_flutter(args).await,
        EffectiveBackend::Web => run_web(args).await,
    }
}

/// Web backend: Chromium over CDP.
async fn run_web(args: &CaptureArgs) -> Result<Captured, AppError> {
    let chrome_path = args.chrome.clone();
    let connect = args.connect.clone();
    let screenshot_path = args.screenshot.clone();
    let persist = args.persist.clone();
    let ignore_certificate_errors = args.ignore_certificate_errors;
    let config = args
        .clone()
        .into_config()
        .context("invalid configuration")
        .map_err(AppError::from)?;

    let outcome = if let Some(endpoint) = &connect {
        let sniffer = Sniffer::connect(endpoint)
            .await
            .context("connecting to browser")
            .map_err(AppError::from)?;
        sniffer.set_ignore_certificate_errors(ignore_certificate_errors);
        sniffer
            .sniff(&config)
            .await
            .context("sniffing via existing browser")
            .map_err(AppError::from)?
    } else {
        let opts = LaunchOptions {
            executable: chrome_path,
            headless: true,
            ignore_certificate_errors,
            ..Default::default()
        };
        let sniffer = Sniffer::launch(&opts)
            .await
            .context("launching browser")
            .map_err(AppError::from)?;
        sniffer
            .sniff(&config)
            .await
            .context("sniffing page")
            .map_err(AppError::from)?
    };

    emit(
        &config,
        &outcome,
        persist.as_deref(),
        screenshot_path.as_deref(),
        &config.url,
        &config.selector,
    )
    .map_err(AppError::from)
}

/// Flutter backend: a debug-mode Flutter app over the Dart VM Service.
async fn run_flutter(args: &CaptureArgs) -> Result<Captured, AppError> {
    use sniff_flutter::extractor::extract;
    use sniff_flutter::{
        EmulatorProcess, FlutterDriver, FlutterInspector, FlutterMachine, ViewportGuard,
    };

    let config = args
        .clone()
        .into_config()
        .context("invalid configuration")
        .map_err(AppError::from)?;

    // Resolve the target device: an AVD to launch, an attached serial, or the
    // one implied by the `flutter://<device>` URL.
    let device = if let Some(avd) = &args.avd {
        let emulator = EmulatorProcess::launch(avd)
            .await
            .context("launching emulator")
            .map_err(AppError::from)?;
        let serial = emulator
            .serial()
            .ok_or_else(|| AppError::internal("emulator did not report a serial"))?
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
        args.device
            .clone()
            .or_else(|| args.flutter_device())
            .ok_or_else(|| {
                AppError::invalid(
                    "flutter backend needs --device <serial>, --avd <name> or a flutter://<device> URL",
                )
            })?
    };

    // Apply --viewport on the device (`adb shell wm size`) before launching the
    // app so the Flutter MediaQuery/layout reflect the requested size. The
    // guard restores the previous size afterwards (and on any error path).
    let viewport_guard = match &args.viewport {
        Some(spec) => {
            let vp = sniff_core::Viewport::parse_cli(spec).map_err(AppError::from)?;
            Some(
                ViewportGuard::apply(&device, vp.width, vp.height)
                    .await
                    .context("applying --viewport on device (adb wm size)")
                    .map_err(AppError::from)?,
            )
        }
        None => None,
    };

    // Run (or attach to) the app and discover the VM Service URI.
    let project = args.project.clone().unwrap_or_else(|| {
        sniff_flutter::device::find_project_root(&args.target)
            .unwrap_or_else(|| PathBuf::from("."))
            .to_string_lossy()
            .into_owned()
    });
    let ws_uri = if args.attach {
        let mut machine =
            FlutterMachine::attach(Path::new(&project), &device).context("flutter attach")?;
        machine
            .wait_for_vm_service(std::time::Duration::from_secs(90))
            .await
            .context("waiting for VM Service on attach")
            .map_err(AppError::from)?
    } else {
        let mut machine = FlutterMachine::run(Path::new(&project), &args.target, &device)
            .context("flutter run")?;
        machine
            .wait_for_vm_service(std::time::Duration::from_secs(180))
            .await
            .context("waiting for VM Service (is the app built in debug mode?)")
            .map_err(AppError::from)?
    };

    // Connect the widget inspector (and the Flutter Driver extension when
    // actions are configured). Animations are frozen AFTER the actions so a
    // reveal transition (modal, dropdown) completes at normal speed first —
    // freezing first would capture the interaction mid-animation.
    let inspector = FlutterInspector::connect(&ws_uri)
        .await
        .context("connecting to VM Service")
        .map_err(AppError::from)?;
    // A previous capture may have left the app frozen (`timeDilation` 1e6),
    // which blocks driver actions (their `endOfFrame` pump never completes);
    // restore real time before doing anything.
    inspector
        .set_time_dilation(1.0)
        .await
        .context("restoring app time dilation")
        .map_err(AppError::from)?;
    let roots = if config.actions.is_empty() {
        inspector
            .freeze_animations()
            .await
            .context("freezing animations")
            .map_err(AppError::from)?;
        extract(&inspector, config.depth)
            .await
            .context("extracting Flutter widget tree")
            .map_err(AppError::from)?
    } else {
        let driver = FlutterDriver::connect(&ws_uri)
            .await
            .context("connecting to Flutter Driver extension")
            .map_err(AppError::from)?;
        if !driver.is_available().await {
            return Err(AppError::invalid(
                "flutter --action needs the app to call `enableFlutterDriverExtension()` in \
                 main() (add `flutter_driver` to dev_dependencies; see docs/flutter.md)",
            ));
        }
        driver
            .keep_frames_alive()
            .await
            .context("keeping app frames alive for actions")
            .map_err(AppError::from)?;
        for act in &config.actions {
            sniff_flutter::perform_action(&driver, act)
                .await
                .with_context(|| format!("performing {} `{}`", act.kind(), act.selector()))
                .map_err(AppError::from)?;
            tokio::time::sleep(std::time::Duration::from_millis(act.settle_ms())).await;
        }
        inspector
            .freeze_animations()
            .await
            .context("freezing animations after actions")
            .map_err(AppError::from)?;
        extract(&inspector, config.depth)
            .await
            .context("extracting Flutter widget tree after actions")
            .map_err(AppError::from)?
    };
    // Leave the app running at real time: the freeze above is only for a
    // deterministic capture, and a permanently-frozen app breaks later driver
    // actions and the developer's own hot-reload.
    inspector
        .set_time_dilation(1.0)
        .await
        .context("restoring app time dilation after capture")
        .map_err(AppError::from)?;
    let screenshot_path = args.screenshot.clone();
    let screenshot = if screenshot_path.is_some() {
        let serial = device.clone();
        let emulator =
            EmulatorProcess::attach(&serial).context("attaching device for screenshot")?;
        Some(
            emulator
                .screenshot()
                .await
                .context("adb screencap")
                .map_err(AppError::from)?,
        )
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
        args.persist.as_deref(),
        screenshot_path.as_deref(),
        &url,
        &config.selector,
    )
    .map_err(AppError::from)
}

/// Serialize `snapshots`, write the screenshot and (optionally) persist the
/// payload. Returns the serialized bytes and the persisted path, if any.
#[expect(
    clippy::too_many_arguments,
    reason = "agrega config, outcome e flags de saída"
)]
fn emit(
    config: &sniff_core::SniffConfig,
    outcome: &sniff_engine::extractor::SniffOutcome,
    persist: Option<&str>,
    screenshot_path: Option<&str>,
    url: &str,
    selector: &str,
) -> anyhow::Result<Captured> {
    let mut buf = Vec::new();
    {
        let mut out = BufWriter::new(&mut buf);
        write_output(&mut out, outcome, &config.output)?;
        out.flush()?;
    }

    if let Some(path) = screenshot_path
        && let Some(bytes) = &outcome.screenshot
    {
        std::fs::write(path, bytes).with_context(|| format!("writing screenshot to {path}"))?;
        sniff_core::diagnostic::write(format_args!("screenshot saved to {path}"));
    }

    let persisted = match persist {
        None => None,
        Some("") => {
            let root = snapshot_root();
            let rel = persist_snapshot(&root, url, selector, config.output.format, &buf)?;
            let abs = root.join(&rel);
            sniff_core::diagnostic::write(format_args!("snapshot saved to {}", abs.display()));
            Some(abs)
        }
        Some(dest) => {
            let path = PathBuf::from(dest);
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            std::fs::write(&path, &buf)
                .with_context(|| format!("writing snapshot to {}", path.display()))?;
            sniff_core::diagnostic::write(format_args!("snapshot saved to {}", path.display()));
            Some(path)
        }
    };

    Ok(Captured {
        bytes: buf,
        persisted,
    })
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
            None,
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
            None,
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
            None,
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
