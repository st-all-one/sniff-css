//! Deterministic UI checks over `sniff-computed-style` snapshots.
//!
//! Two kinds of checks, both offline and LLM-free (the LLM only interprets
//! the results):
//!
//! - **Uniformity** ([`uniformity`]): with 2+ sibling instances of a
//!   selector, finds the "odd one out" against the group norm.
//! - **Derived rules** ([`rules`]): measured WCAG/UX heuristics (contrast,
//!   target size, focus indicator, hidden focusables, empty alt).

pub mod error;
pub mod rules;
pub mod uniformity;

pub use error::{CheckError, CheckResult};
pub use rules::{CheckLine, RuleStatus, run_rules};
pub use uniformity::{UniformityReport, check_uniformity};
