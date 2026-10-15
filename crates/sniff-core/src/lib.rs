//! Core types, configuration and the CSS property catalog.
//!
//! This crate is intentionally dependency-light: it holds the data
//! structures and configuration used by [`sniff-engine`](sniff_engine)
//! and the `sniff` CLI, with no I/O or protocol logic.

#![forbid(unsafe_code)]

pub mod agents;
pub mod config;
pub mod contrast;
pub mod diagnostic;
pub mod env;
pub mod error;
pub mod numeric;
pub mod properties;
pub mod snapshot;
pub mod storage;
pub mod types;

pub use config::*;
pub use error::SniffError;
pub use types::*;

/// Result alias used across the workspace crates.
pub type SniffResult<T> = Result<T, SniffError>;
