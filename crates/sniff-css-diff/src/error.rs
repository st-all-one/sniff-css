//! Error types for the diff library.

use std::io;

/// Errors produced while loading or diffing snapshots.
#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    /// Underlying I/O error while reading a snapshot.
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    /// Malformed JSONL line.
    #[error("invalid jsonl: {0}")]
    Json(#[from] serde_json::Error),
    /// A token-lean summary digest was passed to a full-snapshot consumer.
    #[error("summary snapshot is not valid input for check/diff; recapture with --output jsonl")]
    SummaryDigest,
    /// A single file mixed tree and flat layouts.
    #[error("snapshot mixes tree and flat layouts in a single file")]
    MixedLayout,
}

/// Result alias for the diff library.
pub type DiffResult<T> = Result<T, DiffError>;
