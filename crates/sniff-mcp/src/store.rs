//! On-disk snapshot store backing the MCP tools.
//!
//! Sniffs are persisted as `sniff-css/[domain]/[path]-[selector]-[UTC].jsonl`
//! relative to the server's working directory (or `SNIFF_SNAPSHOT_DIR` when
//! set), so diff/check tools can operate on **paths** instead of round-tripping
//! full snapshots through the LLM context. The UTC stamp sorts the filenames
//! chronologically, making the "latest snapshot for a target" trivially findable.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sniff_core::{SniffConfig, SniffError, SniffResult};

/// Default root directory (relative to the server's CWD).
pub const DEFAULT_ROOT: &str = "sniff-css";

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
    /// Root from `SNIFF_SNAPSHOT_DIR` or `sniff-css` under the CWD.
    pub fn from_env() -> Self {
        let root = std::env::var("SNIFF_SNAPSHOT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_ROOT));
        Self { root }
    }

    /// Root at an explicit path (used by tests to avoid polluting the repo).
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Absolute snapshot root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Persist a captured snapshot, returning its root-relative path.
    ///
    /// The write is atomic (temp file + rename) so concurrent captures never
    /// leave a torn file, and the name carries the UTC stamp for ordering.
    pub fn save(&self, config: &SniffConfig, jsonl: &str) -> SniffResult<PathBuf> {
        let domain = domain_of(&config.url);
        let name = snapshot_file_name(config);
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

/// Sanitized host of `url` (`localhost:3000` -> `localhost_3000`); `local`
/// for `file://` URLs and anything without an authority.
fn domain_of(url: &str) -> String {
    let rest = match url.find("://") {
        Some(i) => &url[i + 3..],
        None => url,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    let slug = slug(host, 80);
    if slug.is_empty() {
        "local".into()
    } else {
        slug.to_ascii_lowercase()
    }
}

/// Sanitized URL pathname (`/products/42` -> `products_42`); `index` for `/`.
fn path_of(url: &str) -> String {
    let rest = match url.find("://") {
        Some(i) => &url[i + 3..],
        None => url,
    };
    let path = rest.split(['?', '#']).next().unwrap_or("");
    let path = match path.find('/') {
        Some(i) => &path[i..],
        None => "/",
    };
    let slug = slug(path, 80);
    if slug.is_empty() {
        "index".into()
    } else {
        slug
    }
}

/// `[path]-[selector]-[UTC].jsonl`.
fn snapshot_file_name(config: &SniffConfig) -> String {
    format!(
        "{}-{}-{}.jsonl",
        path_of(&config.url),
        slug(&config.selector, 40),
        utc_now()
    )
}

/// Collapse arbitrary text into a filesystem-safe slug.
fn slug(raw: &str, max: usize) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_underscore = false;
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
            out.push(c);
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    while matches!(out.as_bytes().first(), Some(b'_' | b'.' | b'-')) {
        out.remove(0);
    }
    while matches!(out.as_bytes().last(), Some(b'_' | b'.' | b'-')) {
        out.pop();
    }
    if out.len() > max {
        out.truncate(max);
    }
    out
}

/// Split `[target]-YYYYMMDDTHHMMSSZ.jsonl` into its parts.
fn parse_snapshot_name(name: &str) -> Option<(String, String)> {
    let stem = name.strip_suffix(".jsonl")?;
    let created = stem.strip_suffix('Z')?;
    let created = created.get(created.len().checked_sub(15)?..)?;
    if !is_timestamp(created) {
        return None;
    }
    let target = stem.get(..stem.len().checked_sub(17)?)?;
    if target.is_empty() {
        return None;
    }
    Some((target.to_string(), format!("{created}Z")))
}

fn is_timestamp(s: &str) -> bool {
    s.len() == 15
        && s.bytes().take(8).all(|b| b.is_ascii_digit())
        && s.as_bytes()[8] == b'T'
        && s.bytes().skip(9).take(6).all(|b| b.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

/// Current UTC instant as `YYYYMMDDTHHMMSSZ` (Howard Hinnant's civil-from-days
/// algorithm; no chrono dependency needed for the store).
fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let sod = secs % 86_400;
    let (h, m, s) = (sod / 3600, (sod % 3600) / 60, sod % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}{mo:02}{d:02}T{h:02}{m:02}{s:02}Z")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
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
    use sniff_core::{ElementFilter, OutputConfig, SniffConfig, WaitStrategy};

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
            stabilize: false,
            ax_tree: false,
        }
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
        let name = snapshot_file_name(&cfg);
        let stripped = name
            .strip_suffix(".jsonl")
            .unwrap()
            .strip_suffix(&format!("-{}", utc_now()))
            .unwrap();
        assert_eq!(stripped, "foo_bar-data-testid_widget");
        assert_eq!(name.len(), stripped.len() + 1 + 16 + 6);
    }

    #[test]
    fn parse_snapshot_name_roundtrip() {
        let (target, ts) = parse_snapshot_name("foo_bar-card-20260812T101530Z.jsonl").unwrap();
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
    fn resolve_rejects_traversal() {
        let dir = temp_dir("snapshot_store_traversal");
        let store = SnapshotStore::new(dir.clone());
        assert!(store.resolve(Path::new("../escape.jsonl")).is_err());
        assert!(store.resolve(Path::new("a/../../escape.jsonl")).is_err());
        assert!(store.resolve(Path::new("localhost/foo.jsonl")).is_ok());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn utc_timestamp_is_formatted() {
        let ts = utc_now();
        assert_eq!(ts.len(), 16);
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.as_bytes()[8], b'T');
    }

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sniff-mcp-{label}-{}-{:?}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }
}
