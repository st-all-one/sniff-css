//! Error types for the diff library.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid jsonl: {0}")]
    Json(#[from] serde_json::Error),
    #[error("snapshot mixes tree and flat layouts in a single file")]
    MixedLayout,
}

pub type DiffResult<T> = Result<T, DiffError>;
