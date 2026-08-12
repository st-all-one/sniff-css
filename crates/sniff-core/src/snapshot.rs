//! On-disk snapshot naming and git-ignore helpers, shared by the CLI
//! (`--persist`) and the MCP snapshot store so both produce the same
//! sortable `sniffCSS/[domain]/[UTC]-[path]-[selector].<ext>` tree.
//!
//! Everything here is dependency-light (std only): pure string naming plus
//! two tiny filesystem helpers for creating/ignoring the root directory.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Default root directory (relative to the process CWD).
pub const DEFAULT_ROOT: &str = "sniffCSS";

/// Content of the `.gitignore` written into the snapshot root. A lone `*`
/// ignores everything inside the tree — including the `.gitignore` itself —
/// so the generated folder is never tracked by version control.
pub const GITIGNORE_CONTENT: &str = "*\n";

/// Root from `SNIFF_SNAPSHOT_DIR` or `sniffCSS` under the CWD.
pub fn snapshot_root() -> PathBuf {
    std::env::var("SNIFF_SNAPSHOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_ROOT))
}

/// Ensure `root` exists and carries a `.gitignore` that auto-ignores its
/// entire contents, so the snapshot tree stays out of git.
pub fn ensure_gitignored(root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    let gitignore = root.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(&gitignore, GITIGNORE_CONTENT)?;
    }
    Ok(())
}

/// Sanitized host of `url` (`localhost:3000` -> `localhost_3000`); `local`
/// for `file://` URLs and anything without an authority.
pub fn domain_of(url: &str) -> String {
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
pub fn path_of(url: &str) -> String {
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

/// `[UTC]-[path]-[selector].<ext>` — leading timestamp so file listings sort
/// chronologically across executions of the same target.
pub fn snapshot_file_name(url: &str, selector: &str, ext: &str) -> String {
    format!(
        "{}-{}-{}.{ext}",
        utc_now(),
        path_of(url),
        slug(selector, 40),
    )
}

/// Collapse arbitrary text into a filesystem-safe slug.
pub fn slug(raw: &str, max: usize) -> String {
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

/// Current UTC instant as `YYYYMMDDTHHMMSSZ` (Howard Hinnant's
/// civil-from-days algorithm; no chrono dependency needed for the store).
pub fn utc_now() -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn file_name_keeps_path_selector_and_extension() {
        let name = snapshot_file_name(
            "http://localhost:3000/foo/bar",
            "[data-testid=\"widget\"]",
            "jsonl",
        );
        let stripped = name
            .strip_prefix(&format!("{}-", utc_now()))
            .unwrap()
            .strip_suffix(".jsonl")
            .unwrap();
        assert_eq!(stripped, "foo_bar-data-testid_widget");
        assert_eq!(name.len(), 16 + 1 + stripped.len() + 6);

        let png = snapshot_file_name(
            "http://localhost:3000/foo/bar",
            "[data-testid=\"widget\"]",
            "png",
        );
        assert!(png.ends_with(".png"));
    }

    #[test]
    fn slug_collapses_and_trims_separators() {
        assert_eq!(slug("  a/b c  ", 80), "a_b_c");
        assert_eq!(slug("--.x..", 80), "x");
        assert_eq!(slug("abcdefghij", 5), "abcde");
        assert_eq!(slug("plain", 80), "plain");
    }

    #[test]
    fn utc_timestamp_is_formatted() {
        let ts = utc_now();
        assert_eq!(ts.len(), 16);
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.as_bytes()[8], b'T');
    }

    #[test]
    fn ensure_gitignored_creates_root_and_gitignore() {
        let dir = std::env::temp_dir().join(format!(
            "sniffCSS-snapshot-it-{}-{:?}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        ensure_gitignored(&dir).unwrap();
        let content = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
        assert_eq!(content, "*\n");
        // Idempotent: calling again does not error or rewrite.
        ensure_gitignored(&dir).unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }
}
