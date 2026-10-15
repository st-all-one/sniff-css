//! Process environment access.
//!
//! The environment is process-global, and mutating it (`set_var`) is `unsafe`
//! in Edition 2024 because it can race with reads in other threads. Every read
//! therefore goes through this one auditable boundary instead of being
//! scattered across the crates. Adapters that need deterministic tests should
//! read the value once and pass it down instead of re-reading.

/// Reads `name` as a UTF-8 `String`; `None` when unset or not valid UTF-8.
#[expect(
    clippy::disallowed_methods,
    reason = "ponto único de leitura do ambiente do processo"
)]
pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Reads `name` as an `OsString`; `None` when unset.
#[expect(
    clippy::disallowed_methods,
    reason = "ponto único de leitura do ambiente do processo"
)]
pub fn var_os(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(name)
}
