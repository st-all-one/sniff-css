//! Deterministic diffing of `sniff-computed-style` JSONL snapshots.
//!
//! The extraction step produces the *exact* computed-style truth; this
//! crate answers "what changed" structurally and cheaply (matching by
//! stable selector + positional fallback, property-level comparison with
//! a subpixel tolerance), so an LLM only ever needs to see the small
//! delta. The semantic evaluation itself (positive/negative) stays in the
//! prompt layer, not in this crate.

pub mod diff;
pub mod error;
pub mod model;
pub mod output;

pub use diff::{DeltaLine, DiffOptions, DiffStats, diff_trees};
pub use error::{DiffError, DiffResult};
pub use model::{DiffNode, load_file};
pub use output::write_delta;
