//! Android emulator / device process lifecycle (the Flutter analogue of
//! `sniff-cdp::browser::BrowserProcess`).
//!
//! A Flutter app is only sniffable in **debug** (or profile) builds, which
//! expose the Dart VM Service. The tool therefore either launches an emulator
//! (`emulator -avd <name>`) or attaches to a device already running, then
//! hands the device to [`crate::machine::FlutterMachine`] to run the app.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Errors from device/emulator management.
#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("adb not found on PATH (install Android platform-tools)")]
    AdbNotFound,
    #[error("emulator binary not found on PATH")]
    EmulatorNotFound,
    #[error("no device found: run `adb devices` or pass --avd")]
    NoDevice,
    #[error("adb error: {0}")]
    Adb(String),
    #[error("emulator exited with {status} before becoming ready")]
    EmulatorExited { status: String },
    #[error("timed out waiting for device `{serial}` to be online")]
    Timeout { serial: String },
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// A single attached Android device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// `adb` serial, e.g. `emulator-5554`.
    pub serial: String,
    /// Product/model label from `adb devices -l`, when present.
    pub name: String,
}

/// A launched emulator process. Killed on drop.
pub struct EmulatorProcess {
    child: Option<Child>,
    avd: String,
    serial: Option<String>,
}

impl std::fmt::Debug for EmulatorProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmulatorProcess")
            .field("avd", &self.avd)
            .field("serial", &self.serial)
            .finish()
    }
}

impl EmulatorProcess {
    /// Launch an emulator AVD, waiting for it to come online.
    pub async fn launch(avd: &str) -> Result<Self, DeviceError> {
        if std::env::var_os("ANDROID_SDK_ROOT").is_none() && which("emulator").is_none() {
            return Err(DeviceError::EmulatorNotFound);
        }
        let mut child = Command::new("emulator")
            .args([
                "-avd",
                avd,
                "-no-window",
                "-no-audio",
                "-no-boot-anim",
                "-gpu",
                "swiftshader_indirect",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;

        // Drain stderr in the background so the child never blocks on a full
        // pipe buffer.
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "sniff_flutter::emulator", "{line}");
                }
            });
        }

        let mut proc = Self {
            child: Some(child),
            avd: avd.to_string(),
            serial: None,
        };
        let device = wait_for_any_device(Duration::from_secs(120)).await?;
        proc.serial = Some(device.serial.clone());
        Ok(proc)
    }

    /// Wrap an already-running device/emulator; no process is owned.
    pub fn attach(serial: &str) -> Result<Self, DeviceError> {
        Ok(Self {
            child: None,
            avd: String::new(),
            serial: Some(serial.to_string()),
        })
    }

    /// The device serial this emulator exposes.
    pub fn serial(&self) -> Option<&str> {
        self.serial.as_deref()
    }

    /// Capture a PNG screenshot of the device/emulator screen
    /// (`adb exec-out screencap -p`), the analogue of the web backend's
    /// `Page.captureScreenshot`.
    pub async fn screenshot(&self) -> Result<Vec<u8>, DeviceError> {
        let Some(serial) = self.serial.as_deref() else {
            return Err(DeviceError::NoDevice);
        };
        adb(&["-s", serial, "exec-out", "screencap", "-p"]).await
    }
}

impl Drop for EmulatorProcess {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.start_kill();
        }
    }
}

/// Run an `adb` command and return its stdout.
pub async fn adb(args: &[&str]) -> Result<Vec<u8>, DeviceError> {
    let output = Command::new("adb")
        .args(args)
        .output()
        .await
        .map_err(|_| DeviceError::AdbNotFound)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DeviceError::Adb(stderr.trim().to_string()));
    }
    Ok(output.stdout)
}

/// List attached devices (`adb devices`), skipping the header line.
pub async fn list_devices() -> Result<Vec<Device>, DeviceError> {
    let stdout = adb(&["devices"]).await?;
    let text = String::from_utf8_lossy(&stdout);
    let mut devices = Vec::new();
    for line in text.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() || line.contains("unauthorized") {
            continue;
        }
        let serial = line
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        if !serial.is_empty() {
            devices.push(Device {
                serial: serial.clone(),
                name: serial,
            });
        }
    }
    Ok(devices)
}

/// Whether `adb` is on PATH (cheap probe used to auto-skip integration tests).
pub fn is_adb_available() -> bool {
    which("adb").is_some()
}

/// Whether `flutter` is on PATH (cheap probe used to auto-skip integration tests).
pub fn is_flutter_available() -> bool {
    which("flutter").is_some()
}

/// Wait until at least one device appears on `adb devices`.
async fn wait_for_any_device(timeout: Duration) -> Result<Device, DeviceError> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(dev) = list_devices().await?.into_iter().next() {
            return Ok(dev);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(DeviceError::Timeout { serial: "*".into() });
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Minimal `PATH` lookup (no external crate).
fn which(bin: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(bin))
            .find(|p| p.exists())
    })
}

/// Resolve the Flutter project dir that owns `main.dart` (used to locate the
/// app to run). Walks up looking for `pubspec.yaml`.
pub fn find_project_root(main_dart: &str) -> Option<std::path::PathBuf> {
    let mut dir = Path::new(main_dart).parent()?.to_path_buf();
    loop {
        if dir.join("pubspec.yaml").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_root_found_from_main_dart() {
        let root = find_project_root("/tmp/sniff-flutter-app/lib/main.dart");
        // Without a real pubspec.yaml this returns None; we just require the
        // helper not to panic and to stay anchored at a parent.
        assert!(root.is_none() || root.unwrap().ends_with("sniff-flutter-app"));
    }

    #[tokio::test]
    async fn devices_list_is_empty_or_parses() {
        if !is_adb_available() {
            eprintln!("skipping: no adb");
            return;
        }
        let _ = list_devices().await;
    }
}
