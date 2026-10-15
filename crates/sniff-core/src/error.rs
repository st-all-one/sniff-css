//! Workspace-wide error type.

use thiserror::Error;

/// Errors produced by configuration, engine and CLI layers.
#[derive(Debug, Error)]
pub enum SniffError {
    /// Unknown style category name.
    #[error("unknown style category: {0}")]
    UnknownCategory(String),

    /// Malformed or unsupported wait strategy.
    #[error("invalid wait strategy: {0}")]
    InvalidWaitStrategy(String),

    /// Malformed or unsupported ready condition.
    #[error("invalid ready condition: {0}")]
    InvalidReadyCondition(String),

    /// Malformed or unsupported user action.
    #[error("invalid action: {0}")]
    InvalidAction(String),

    /// Unknown output format name.
    #[error("invalid output format: {0}")]
    InvalidOutputFormat(String),

    /// Malformed element filter.
    #[error("invalid filter: {0}")]
    InvalidFilter(String),

    /// The selector matched no element.
    #[error("no elements matched selector `{selector}`")]
    NoMatch {
        /// The selector that produced no match.
        selector: String,
    },

    /// A wait/readiness phase timed out.
    #[error("timeout while waiting for {0}")]
    Timeout(String),

    /// Error surfaced by the CDP layer.
    #[error("CDP error: {0}")]
    Cdp(String),

    /// Error surfaced by the browser/process layer.
    #[error("browser error: {0}")]
    Browser(String),

    /// JSON (de)serialization failed.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// I/O error reading an operand, file or stdin.
    #[error("I/O error: {0}")]
    Io(String),

    /// Catch-all for errors without a dedicated variant.
    #[error("{0}")]
    Other(String),
}

impl From<serde_json::Error> for SniffError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}
