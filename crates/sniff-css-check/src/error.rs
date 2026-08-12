//! Error/result types for the check layer.

use thiserror::Error;

/// Errors produced by the check layer (mostly upstream loading errors).
#[derive(Debug, Error)]
pub enum CheckError {
    #[error("failed to load snapshot JSONL: {0}")]
    Load(#[from] sniff_css_diff::DiffError),

    #[error("{0}")]
    Other(String),
}

/// Result alias for the check layer.
pub type CheckResult<T> = Result<T, CheckError>;
