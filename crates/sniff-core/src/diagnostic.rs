//! Process-wide stderr diagnostics.
//!
//! `stdout` is reserved for the machine-readable capture data; every
//! human-facing message goes to `stderr`. Centralizing the write here keeps
//! `clippy::print_stderr` denied across the workspace: this is the single,
//! audited write point.

use std::fmt::Arguments;

/// Writes one diagnostic line to `stderr`.
///
/// Best effort, like the standard library's own `eprintln!`: a broken pipe or
/// closed descriptor is ignored rather than turned into a second error.
#[expect(
    clippy::print_stderr,
    reason = "ponto único de escrita de diagnóstico em stderr do workspace"
)]
pub fn write(args: Arguments<'_>) {
    eprintln!("{args}");
}
