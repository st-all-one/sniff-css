//! Core types, configuration and the CSS property catalog.
//!
//! This crate is intentionally dependency-light: it holds the data
//! structures and configuration used by [`sniff-engine`](sniff_engine)
//! and the `sniffCSS` CLI, with no I/O or protocol logic.

pub mod config;
pub mod contrast;
pub mod error;
pub mod properties;
pub mod snapshot;
pub mod storage;
pub mod types;

pub use config::*;
pub use error::SniffError;
pub use types::*;

/// Result alias used across the workspace crates.
pub type SniffResult<T> = Result<T, SniffError>;
