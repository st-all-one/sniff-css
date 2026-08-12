//! Chromium process lifecycle management.

use crate::client::{CdpClient, CdpError, Result};
use crate::protocol::LaunchOptions;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// A launched (or externally-provided) Chromium instance.
pub struct BrowserProcess {
    child: Option<Child>,
    /// WebSocket endpoint for DevTools.
    pub ws_endpoint: String,
    /// Temp user-data dir to remove on drop (when we created one).
    temp_dir: Option<std::path::PathBuf>,
}

impl std::fmt::Debug for BrowserProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserProcess")
            .field("ws_endpoint", &self.ws_endpoint)
            .field("temp_dir", &self.temp_dir)
            .finish()
    }
}

impl BrowserProcess {
    /// Launch Chromium and wait for the DevTools WebSocket endpoint.
    pub async fn launch(opts: &LaunchOptions) -> Result<Self> {
        let executable = opts
            .executable
            .clone()
            .or_else(detect_chrome)
            .ok_or_else(|| {
                CdpError::Connect(
                    "no Chrome/Chromium binary found; set SNIFF_CHROME_PATH or pass --chrome"
                        .to_string(),
                )
            })?;

        let (user_data_dir, temp_dir) = match &opts.user_data_dir {
            Some(dir) => (dir.clone(), None),
            None => {
                let dir = std::env::temp_dir().join(format!(
                    "sniff-cdp-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0)
                ));
                std::fs::create_dir_all(&dir).map_err(CdpError::Io)?;
                (dir.to_string_lossy().into_owned(), Some(dir))
            }
        };

        let mut cmd = Command::new(&executable);
        cmd.args([
            "--remote-debugging-port=0",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-networking",
            "--disable-default-apps",
            "--disable-extensions",
            "--disable-sync",
            "--disable-translate",
            "--no-sandbox",
            "--disable-dev-shm-usage",
            "--remote-debugging-address=127.0.0.1",
            &format!("--user-data-dir={user_data_dir}"),
            "about:blank",
        ]);
        if opts.headless {
            cmd.args(["--headless=new"]);
        }
        cmd.args(&opts.extra_args);
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            if let Some(dir) = &temp_dir {
                let _ = std::fs::remove_dir_all(dir);
            }
            CdpError::Connect(format!("failed to spawn {executable}: {e}"))
        })?;

        let endpoint = match wait_for_endpoint(
            &mut child,
            &temp_dir,
            Duration::from_millis(opts.launch_timeout_ms),
        )
        .await
        {
            Ok(ep) => ep,
            Err(e) => {
                let _ = child.start_kill();
                let _ = child.try_wait();
                if let Some(dir) = &temp_dir {
                    let _ = std::fs::remove_dir_all(dir);
                }
                return Err(e);
            }
        };

        Ok(Self {
            child: Some(child),
            ws_endpoint: endpoint,
            temp_dir,
        })
    }

    /// Connect to this browser's DevTools endpoint.
    pub async fn connect(&self) -> Result<CdpClient> {
        CdpClient::connect(&self.ws_endpoint).await
    }

    /// Build a placeholder that references an external browser and will
    /// not attempt to kill anything on drop.
    pub fn placeholder(endpoint: &str) -> Self {
        Self {
            child: None,
            ws_endpoint: endpoint.to_string(),
            temp_dir: None,
        }
    }

    /// Try to detect a Chrome/Chromium binary on this system.
    pub fn available() -> Option<String> {
        detect_chrome()
    }
}

impl Drop for BrowserProcess {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.start_kill();
            // Give the child a moment to release file handles before we
            // attempt to remove the user-data dir.
            for _ in 0..40 {
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            let _ = child.try_wait();
        }
        if let Some(dir) = &self.temp_dir {
            for _ in 0..40 {
                match std::fs::remove_dir_all(dir) {
                    Ok(()) => break,
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(25)),
                }
            }
        }
    }
}

/// Read Chromium's stderr until the DevTools WebSocket endpoint appears
/// or the child exits.
async fn wait_for_endpoint(
    child: &mut Child,
    _temp_dir: &Option<std::path::PathBuf>,
    timeout: Duration,
) -> Result<String> {
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CdpError::Io(std::io::Error::other("no stderr pipe")))?;
    let mut reader = BufReader::new(stderr).lines();

    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        let line = tokio::time::timeout(
            deadline.saturating_duration_since(tokio::time::Instant::now()),
            reader.next_line(),
        )
        .await
        .map_err(|_| CdpError::Timeout("DevTools endpoint".into()))?
        .map_err(CdpError::Io)?;

        match line {
            Some(text) => {
                if let Some(pos) = text.find("DevTools listening on ") {
                    let rest = &text[pos + "DevTools listening on ".len()..];
                    return Ok(rest.trim().to_string());
                }
                // Child exited before printing the endpoint.
                if let Some(status) = child.try_wait()? {
                    return Err(CdpError::Connect(format!(
                        "browser exited with {status} before printing the DevTools endpoint"
                    )));
                }
            }
            None => break,
        }
    }
    Err(CdpError::Timeout("DevTools endpoint".into()))
}

/// Locate a usable Chrome/Chromium executable.
pub fn detect_chrome() -> Option<String> {
    if let Ok(path) = std::env::var("SNIFF_CHROME_PATH")
        && !path.is_empty()
        && Path::new(&path).exists()
    {
        return Some(path);
    }
    const CANDIDATES: &[&str] = &[
        "google-chrome-stable",
        "google-chrome",
        "chromium-browser",
        "chromium",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium-browser",
        "/usr/bin/chromium",
        "/usr/sbin/chromium-browser",
        "/opt/google/chrome/chrome",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    ];
    for cand in CANDIDATES {
        if Path::new(cand).exists() {
            return Some((*cand).to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_detection_returns_something_or_none() {
        // Should not panic and should find at least a real binary on CI.
        let _ = BrowserProcess::available();
    }

    #[test]
    fn temp_dir_removed_on_drop() {
        let dir = std::env::temp_dir().join(format!("sniff-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        {
            let _proc = BrowserProcess {
                child: None,
                ws_endpoint: "ws://unused".into(),
                temp_dir: Some(dir.clone()),
            };
        }
        assert!(!dir.exists());
    }
}
