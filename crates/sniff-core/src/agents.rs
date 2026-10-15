//! Managed `AGENTS.md` block written by `sniff init`.
//!
//! Safety mirrors knudge: the block lives between `<!-- sniff:start -->` and
//! `<!-- sniff:end -->` markers and is inserted/replaced **idempotently**,
//! preserving everything else in the file. `--force` overwrites the whole file
//! with the block (explicit destructive reset).

use std::io;
use std::path::{Path, PathBuf};

/// Start marker of the managed block.
pub const MARKER_BEGIN: &str = "<!-- sniff:start -->";
/// End marker of the managed block.
pub const MARKER_END: &str = "<!-- sniff:end -->";
/// Prefix of the protocol version marker.
pub const VERSION_PREFIX: &str = "<!-- sniff:version:";
/// Protocol version embedded in the block.
pub const VERSION: u32 = 1;
/// File the block is written to.
pub const FILE: &str = "AGENTS.md";

/// What [`apply`] did to the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The file did not exist before.
    Created,
    /// The file existed and its content changed.
    Updated,
    /// The file already matched the block.
    Unchanged,
}

/// Result of [`apply`].
#[derive(Debug, Clone)]
pub struct Applied {
    /// Absolute or relative path that was (or would be) touched.
    pub path: PathBuf,
    /// What happened to the file.
    pub state: State,
}

/// The token-optimized protocol block written into `AGENTS.md`.
#[must_use]
pub fn protocol_block() -> String {
    format!(
        "{MARKER_BEGIN}\n\
         {VERSION_PREFIX} {VERSION} -->\n\
         # sniff — computed styles for LLMs\n\
         \n\
         `sniff capture` is the source of truth for layout/style — never guess CSS, capture it.\n\
         Cycle: `sniff capture` → `sniff check` → `sniff diff`; the AI reads only the delta.\n\
         \n\
         - Capture: `sniff capture -u <URL> -s <CSS> [--depth N] [--output summary|jsonl|json]`\n\
           (default `summary`, token-lean). Diff/check need `--no-summary` JSONL snapshots.\n\
         - Diff: `sniff diff <BASE> <HEAD> [--tolerance 0.5] [--stats-only]` (deterministic, offline).\n\
         - Checks: `sniff check -i <SNAP> [--rules] [--uniform]` (contrast, target size, focus, alt).\n\
         - Protocol: `sniff prime` (`--long` = full); `sniff self` = setup/completions/upgrade.\n\
         \n\
         Common cases:\n\
         - Self-signed TLS: add `--ignore-certificate-errors` (or set `SNIFF_IGNORE_CERTIFICATE_ERRORS=1`).\n\
         - Auth: `--header \"Name: Value\"` on every request; persist a login with `--storage-state FILE`\n\
           (write it once with `--save-storage-state FILE`).\n\
         - Action-only UI (modal/dropdown): `--click SEL`, or ordered `--action \"click:SEL:5000\"`.\n\
         - Flutter/Dart app: `-u flutter://<device> --project DIR --depth N` (debug build only).\n\
         - Persist a snapshot: `--persist` writes `sniff/[domain]/[UTC]-[path]-[selector].jsonl`.\n\
         \n\
         Contract: stdout = data (JSONL), stderr = logs; `--json` = \
         {{success, command, data?, error{{code,message,retryable}}, warnings?}}.\n\
         Exit: 2 invalid, 3 not_found, 5 io, 70 internal.\n\
         {MARKER_END}\n"
    )
}

/// Extracts the protocol version present in `text`, if any.
#[must_use]
pub fn version_in(text: &str) -> Option<u32> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(VERSION_PREFIX) {
            let digits: String = rest
                .trim_start()
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(value) = digits.parse::<u32>() {
                return Some(value);
            }
        }
    }
    None
}

/// Inserts/replaces the block delimited by `begin`/`end` in `text`.
///
/// - `block = Some(text)`: guarantees the block exists (replacing an old one).
/// - `block = None`: removes the block, if present.
///
/// Idempotent: applying it twice yields the same result. Slicing-free (walks
/// the lines) so it is safe under `indexing_slicing`.
#[must_use]
pub fn upsert(text: &str, begin: &str, end: &str, block: Option<&str>) -> String {
    let mut out = String::with_capacity(
        text.len()
            .saturating_add(block.map_or(0, str::len))
            .saturating_add(2),
    );
    let mut in_block = false;
    let mut found_begin = false;
    for line in text.split_inclusive('\n') {
        let marker = line.trim_end_matches(['\r', '\n']).trim();
        if !in_block && marker == begin {
            found_begin = true;
            in_block = true;
            if let Some(block) = block {
                out.push_str(block);
            }
            continue;
        }
        if in_block {
            if marker == end {
                in_block = false;
            }
            // A `begin` without `end` replaces to EOF (the tail is dropped).
            continue;
        }
        out.push_str(line);
    }
    if !found_begin && let Some(block) = block {
        if !out.is_empty() {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            if !out.ends_with("\n\n") {
                out.push('\n');
            }
        }
        out.push_str(block);
    }
    tidy(&out)
}

/// Walks up from `start` looking for a `.git` entry; falls back to `start`.
#[must_use]
pub fn find_project_root(start: &Path) -> PathBuf {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join(".git").exists() {
            return dir;
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => return start.to_path_buf(),
        }
    }
}

/// Ensures the managed block in `root/AGENTS.md`.
///
/// With `force`, the whole file is overwritten with the block; otherwise only
/// the managed region is touched. Returns what happened.
///
/// # Errors
/// Propagates I/O errors from reading or writing the file.
pub fn apply(root: &Path, force: bool) -> io::Result<Applied> {
    let path = root.join(FILE);
    let existed = path.exists();
    let original = if existed {
        std::fs::read_to_string(&path)?
    } else {
        String::new()
    };
    let block = protocol_block();
    let updated = if force {
        block
    } else {
        upsert(&original, MARKER_BEGIN, MARKER_END, Some(&block))
    };
    let changed = updated != original;
    if changed {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_atomic(&path, updated.as_bytes())?;
    }
    Ok(Applied {
        path,
        state: if !existed {
            State::Created
        } else if changed {
            State::Updated
        } else {
            State::Unchanged
        },
    })
}

/// Writes `bytes` to `path` atomically (staging file + rename).
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let staging = path.with_extension("md.tmp");
    std::fs::write(&staging, bytes)?;
    std::fs::rename(&staging, path)?;
    Ok(())
}

/// Guarantees exactly one trailing `\n` and at most one blank line between blocks.
fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len().saturating_add(1));
    let mut pending_newlines: usize = 0;
    for ch in text.chars() {
        if ch == '\n' {
            pending_newlines = pending_newlines.saturating_add(1);
            if pending_newlines <= 2 {
                out.push(ch);
            }
        } else {
            pending_newlines = 0;
            out.push(ch);
        }
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_carries_current_version() {
        let block = protocol_block();
        assert_eq!(version_in(&block), Some(VERSION));
        assert!(block.contains(MARKER_BEGIN));
        assert!(block.contains(MARKER_END));
        assert!(block.contains("--ignore-certificate-errors"));
    }

    #[test]
    fn upsert_inserts_once_and_is_idempotent() {
        let block = protocol_block();
        let once = upsert("user = 1\n", MARKER_BEGIN, MARKER_END, Some(&block));
        assert!(once.contains("sniff capture"));
        assert!(once.starts_with("user = 1\n"));
        let twice = upsert(&once, MARKER_BEGIN, MARKER_END, Some(&block));
        assert_eq!(once, twice);
    }
    #[test]
    fn upsert_replaces_existing_block_and_keeps_surroundings() {
        let block = protocol_block();
        let original = format!("before\n{MARKER_BEGIN}\nold\n{MARKER_END}\nafter\n");
        let updated = upsert(&original, MARKER_BEGIN, MARKER_END, Some(&block));
        assert!(updated.starts_with("before\n"));
        assert!(updated.ends_with("after\n"));
        assert!(updated.contains("sniff capture"));
        assert!(!updated.contains("old"));
    }

    #[test]
    fn upsert_removes_block_when_block_is_none() {
        let original = format!("a\n{MARKER_BEGIN}\nx\n{MARKER_END}\nb\n");
        let removed = upsert(&original, MARKER_BEGIN, MARKER_END, None);
        assert_eq!(removed, "a\nb\n");
    }

    #[test]
    fn apply_writes_and_is_idempotent() {
        let root = std::env::temp_dir().join(format!("sniff-agents-{}", std::process::id()));
        let _cleanup = std::fs::remove_dir_all(&root);
        let first = apply(&root, false).unwrap();
        assert_eq!(first.state, State::Created);
        let path = root.join(FILE);
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains(MARKER_BEGIN));
        let second = apply(&root, false).unwrap();
        assert_eq!(second.state, State::Unchanged);
        let _cleanup = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_force_overwrites_user_content() {
        let root = std::env::temp_dir().join(format!("sniff-agents-force-{}", std::process::id()));
        let _cleanup = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(FILE);
        std::fs::write(&path, "keep me\n").unwrap();
        let applied = apply(&root, true).unwrap();
        assert_eq!(applied.state, State::Updated);
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("keep me"));
        assert!(content.starts_with(MARKER_BEGIN));
        let _cleanup = std::fs::remove_dir_all(&root);
    }
}
