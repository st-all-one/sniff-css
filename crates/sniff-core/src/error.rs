//! Workspace-wide error type.

use thiserror::Error;

/// Errors produced by configuration, engine and CLI layers.
#[derive(Debug, Error)]
pub enum SniffError {
    #[error("unknown style category: {0}")]
    UnknownCategory(String),

    #[error("invalid wait strategy: {0}")]
    InvalidWaitStrategy(String),

    #[error("invalid ready condition: {0}")]
    InvalidReadyCondition(String),

    #[error("invalid action: {0}")]
    InvalidAction(String),

    #[error("invalid output format: {0}")]
    InvalidOutputFormat(String),

    #[error("invalid filter: {0}")]
    InvalidFilter(String),

    #[error("no elements matched selector `{selector}`")]
    NoMatch { selector: String },

    #[error("timeout while waiting for {0}")]
    Timeout(String),

    #[error("CDP error: {0}")]
    Cdp(String),

    #[error("browser error: {0}")]
    Browser(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("{0}")]
    Other(String),
}

impl From<serde_json::Error> for SniffError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}
