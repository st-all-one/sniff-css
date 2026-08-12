//! On-disk snapshot store backing the MCP tools.
//!
//! Sniffs are persisted as `sniffCSS/[domain]/[UTC]-[path]-[selector].jsonl`
//! relative to the server's working directory (or `SNIFF_SNAPSHOT_DIR` when
//! set), so diff/check tools can operate on **paths** instead of round-tripping
//! full snapshots through the LLM context. The leading UTC stamp sorts the
//! filenames chronologically, making executions of any target trivially
//! ordered and the "latest snapshot" easy to find. The store root is
//! auto-ignored by git (a `.gitignore` with `*`), so the generated tree is
//! never tracked by version control.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use sniff_core::snapshot::{domain_of, ensure_gitignored, snapshot_file_name};
use sniff_core::{SniffConfig, SniffError, SniffResult};

/// Default root directory (relative to the server's CWD).
pub const DEFAULT_ROOT: &str = sniff_core::snapshot::DEFAULT_ROOT;

/// One persisted snapshot listed by [`SnapshotStore::list`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct SnapshotEntry {
    /// Sanitized host (e.g. `localhost_3000`) — the subdirectory name.
    pub domain: String,
    /// Stable identity of the captured target (`[path]-[selector]`).
    pub target: String,
    /// Path relative to the snapshot root, usable as `base_path`/`head_path`.
    pub path: String,
    /// UTC capture time, `YYYYMMDDTHHMMSSZ`.
    pub created_at: String,
    /// Bytes on disk.
    pub size: u64,
}

/// Shared handle to the structured snapshot directory.
#[derive(Debug, Clone)]
pub struct SnapshotStore {
    root: PathBuf,
}

impl SnapshotStore {
    /// Root from `SNIFF_SNAPSHOT_DIR` or `sniffCSS` under the CWD.
    pub fn from_env() -> Self {
        Self {
            root: sniff_core::snapshot::snapshot_root(),
        }
    }

    /// Root at an explicit path (used by tests to avoid polluting the repo).
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Absolute snapshot root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Ensure the store root exists and carries a `.gitignore` (`*`) so the
    /// whole generated tree is ignored by git. Called before any write.
    fn ensure_root_ready(&self) -> SniffResult<()> {
        ensure_gitignored(&self.root)
            .map_err(|e| SniffError::Other(format!("prepare store root: {e}")))
    }

    /// Persist a captured snapshot, returning its root-relative path.
    ///
    /// The write is atomic (temp file + rename) so concurrent captures never
    /// leave a torn file, and the name carries the UTC stamp for ordering.
    pub fn save(&self, config: &SniffConfig, jsonl: &str) -> SniffResult<PathBuf> {
        self.ensure_root_ready()?;
        let domain = domain_of(&config.url);
        let name = snapshot_file_name(&config.url, &config.selector, "jsonl");
        let rel = PathBuf::from(&domain).join(&name);
        let abs = self.resolve(&rel)?;

        let parent = abs
            .parent()
            .ok_or_else(|| SniffError::Other(format!("no parent for {}", abs.display())))?;
        fs::create_dir_all(parent)
            .map_err(|e| SniffError::Other(format!("create {}: {e}", parent.display())))?;

        let tmp = parent.join(format!(".{name}.tmp"));
        {
            let mut f = fs::File::create(&tmp)
                .map_err(|e| SniffError::Other(format!("create {}: {e}", tmp.display())))?;
            f.write_all(jsonl.as_bytes())
                .map_err(|e| SniffError::Other(format!("write {}: {e}", tmp.display())))?;
            f.sync_all()
                .map_err(|e| SniffError::Other(format!("sync {}: {e}", tmp.display())))?;
        }
        fs::rename(&tmp, &abs)
            .map_err(|e| SniffError::Other(format!("rename {}: {e}", abs.display())))?;
        Ok(rel)
    }

    /// Persist a binary artifact (e.g. a page screenshot) next to where the
    /// matching snapshot would land, under `[UTC]-[path]-[selector].<ext>`.
    /// Returns the root-relative path.
    pub fn save_bytes(
        &self,
        config: &SniffConfig,
        ext: &str,
        bytes: &[u8],
    ) -> SniffResult<PathBuf> {
        self.ensure_root_ready()?;
        let domain = domain_of(&config.url);
        let name = snapshot_file_name(&config.url, &config.selector, ext);
        let rel = PathBuf::from(&domain).join(&name);
        let abs = self.resolve(&rel)?;

        let parent = abs
            .parent()
            .ok_or_else(|| SniffError::Other(format!("no parent for {}", abs.display())))?;
        fs::create_dir_all(parent)
            .map_err(|e| SniffError::Other(format!("create {}: {e}", parent.display())))?;

        let tmp = parent.join(format!(".{name}.tmp"));
        {
            let mut f = fs::File::create(&tmp)
                .map_err(|e| SniffError::Other(format!("create {}: {e}", tmp.display())))?;
            f.write_all(bytes)
                .map_err(|e| SniffError::Other(format!("write {}: {e}", tmp.display())))?;
            f.sync_all()
                .map_err(|e| SniffError::Other(format!("sync {}: {e}", tmp.display())))?;
        }
        fs::rename(&tmp, &abs)
            .map_err(|e| SniffError::Other(format!("rename {}: {e}", abs.display())))?;
        Ok(rel)
    }

    /// Resolve a tool-supplied path (relative to the root, or absolute) to an
    /// absolute path, rejecting anything that escapes the snapshot root.
    pub fn resolve(&self, rel: &Path) -> SniffResult<PathBuf> {
        let abs = if rel.is_absolute() {
            rel.to_path_buf()
        } else {
            self.root.join(rel)
        };
        ensure_contained(&self.root, &abs).map_err(|e| {
            SniffError::Other(format!("snapshot path `{}` rejected: {e}", rel.display()))
        })?;
        Ok(abs)
    }

    /// All persisted snapshots, newest first.
    pub fn list(&self) -> SniffResult<Vec<SnapshotEntry>> {
        let mut out = Vec::new();
        self.collect_dir(&self.root, &mut out)?;
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }

    fn collect_dir(&self, dir: &Path, out: &mut Vec<SnapshotEntry>) -> SniffResult<()> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()), // missing root/domain dir => empty list
        };
        for entry in entries {
            let entry = entry.map_err(|e| SniffError::Other(e.to_string()))?;
            let path = entry.path();
            if path.is_dir() {
                self.collect_dir(&path, out)?;
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some((target, created_at)) = parse_snapshot_name(name) else {
                continue;
            };
            let domain = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|d| d.to_str())
                .unwrap_or("")
                .to_string();
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let rel = path
                .strip_prefix(&self.root)
                .unwrap_or(&path)
                .display()
                .to_string();
            out.push(SnapshotEntry {
                domain,
                target,
                path: rel,
                created_at,
                size,
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Naming
// ---------------------------------------------------------------------------

// `domain_of`, `path_of`, `snapshot_file_name`, `slug` and `utc_now` live in
// `sniff_core::snapshot` so the CLI `--persist` and the MCP store share the
// exact same file-tree contract.

/// Split `YYYYMMDDTHHMMSSZ-[target].jsonl` into its parts.
fn parse_snapshot_name(name: &str) -> Option<(String, String)> {
    let stem = name.strip_suffix(".jsonl")?;
    let created = stem.get(..15)?;
    if !is_timestamp(created) {
        return None;
    }
    let rest = stem.get(15..)?.strip_prefix('Z')?.strip_prefix('-')?;
    if rest.is_empty() {
        return None;
    }
    Some((rest.to_string(), format!("{created}Z")))
}

fn is_timestamp(s: &str) -> bool {
    s.len() == 15
        && s.bytes().take(8).all(|b| b.is_ascii_digit())
        && s.as_bytes()[8] == b'T'
        && s.bytes().skip(9).take(6).all(|b| b.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// Containment
// ---------------------------------------------------------------------------

fn ensure_contained(root: &Path, abs: &Path) -> Result<(), String> {
    if abs.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("`..` not allowed".into());
    }
    let root_components: Vec<_> = root.components().collect();
    let abs_components: Vec<_> = abs.components().collect();
    if abs_components.len() < root_components.len() || !abs_components.starts_with(&root_components)
    {
        return Err("escapes the snapshot root".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sniff_core::config::OutputFormat;
    use sniff_core::snapshot::{path_of, utc_now};
    use sniff_core::{ElementFilter, OutputConfig, SniffConfig, WaitStrategy};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn config(url: &str, selector: &str) -> SniffConfig {
        SniffConfig {
            url: url.into(),
            selector: selector.into(),
            depth: 0,
            categories: sniff_core::properties::StyleCategory::all().to_vec(),
            custom_properties: Vec::new(),
            pseudo_elements: Vec::new(),
            wait: vec![WaitStrategy::Delay { ms: 0 }],
            filter: ElementFilter::default(),
            output: OutputConfig {
                format: OutputFormat::JsonLines,
                compact: true,
                ..OutputConfig::default()
            },
            viewport: None,
            include_custom_properties: false,
            stable_key: None,
            attributes: vec![],
            stabilize: false,
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

    #[test]
    fn save_creates_gitignored_root() {
        let dir = temp_dir("snapshot_store_gitignore");
        let store = SnapshotStore::new(dir.clone());
        let cfg = config("http://localhost:3000/foo/bar", ".card");
        store.save(&cfg, "{\"id\":1}\n").unwrap();
        let gitignore = dir.join(".gitignore");
        assert!(gitignore.exists(), "store root must carry a .gitignore");
        assert_eq!(
            std::fs::read_to_string(&gitignore).unwrap(),
            "*\n",
            ".gitignore must ignore everything inside the snapshot tree"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn domain_and_path_naming() {
        assert_eq!(domain_of("http://localhost:3000/foo"), "localhost_3000");
        assert_eq!(domain_of("https://www.example.com/a/b"), "www.example.com");
        assert_eq!(domain_of("file:///tmp/page.html"), "local");
        assert_eq!(
            path_of("http://localhost:3000/products/42?id=1"),
            "products_42"
        );
        assert_eq!(path_of("http://localhost:3000/"), "index");
        assert_eq!(path_of("file:///tmp/page.html"), "tmp_page.html");
    }

    #[test]
    fn file_name_keeps_path_and_selector() {
        let cfg = config("http://localhost:3000/foo/bar", "[data-testid=\"widget\"]");
        let name = snapshot_file_name(&cfg.url, &cfg.selector, "jsonl");
        let stripped = name
            .strip_prefix(&format!("{}-", utc_now()))
            .unwrap()
            .strip_suffix(".jsonl")
            .unwrap();
        assert_eq!(stripped, "foo_bar-data-testid_widget");
        assert_eq!(name.len(), 16 + 1 + stripped.len() + 6);
    }

    #[test]
    fn parse_snapshot_name_roundtrip() {
        let (target, ts) = parse_snapshot_name("20260812T101530Z-foo_bar-card.jsonl").unwrap();
        assert_eq!(target, "foo_bar-card");
        assert_eq!(ts, "20260812T101530Z");
        assert!(parse_snapshot_name("foo_card.jsonl").is_none());
        assert!(parse_snapshot_name(".tmp.jsonl").is_none());
    }

    #[test]
    fn save_and_list_roundtrip_in_temp_dir() {
        let dir = temp_dir("snapshot_store_list");
        let store = SnapshotStore::new(dir.clone());
        let cfg = config("http://localhost:3000/foo/bar", ".card");
        let rel = store.save(&cfg, "{\"id\":1}\n").unwrap();
        assert_eq!(rel.parent().unwrap().as_os_str(), "localhost_3000");

        let entries = store.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].domain, "localhost_3000");
        assert_eq!(entries[0].target, "foo_bar-card");
        assert_eq!(entries[0].path, rel.display().to_string());
        assert!(entries[0].size > 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_bytes_persists_binary_beside_snapshot() {
        let dir = temp_dir("snapshot_store_bytes");
        let store = SnapshotStore::new(dir.clone());
        let cfg = config("http://localhost:3000/foo/bar", ".card");
        let rel = store.save_bytes(&cfg, "png", b"\x89PNG\r\n\x1a\n").unwrap();
        let abs = store.resolve(&rel).unwrap();
        assert_eq!(
            abs.extension().unwrap().to_str().unwrap(),
            "png",
            "extension must replace .jsonl"
        );
        let data = fs::read(&abs).unwrap();
        assert_eq!(data, b"\x89PNG\r\n\x1a\n");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_rejects_traversal() {
        let dir = temp_dir("snapshot_store_traversal");
        let store = SnapshotStore::new(dir.clone());
        assert!(store.resolve(Path::new("../escape.jsonl")).is_err());
        assert!(store.resolve(Path::new("a/../../escape.jsonl")).is_err());
        assert!(store.resolve(Path::new("localhost/foo.jsonl")).is_ok());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn store_uses_shared_snapshot_naming() {
        let ts = sniff_core::snapshot::utc_now();
        assert_eq!(ts.len(), 16);
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.as_bytes()[8], b'T');
    }

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sniffCSS-mcp-{label}-{}-{:?}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }
}
