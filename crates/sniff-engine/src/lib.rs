//! Sniffing engine: orchestrates wait strategies, element filters and
//! computed-style extraction against a CDP session.
//!
//! The engine owns the in-page JavaScript extraction pass, the wait
//! strategy executor and the output serializers. It is independent of
//! any CLI surface so it can be reused by binaries, watch modes or
//! servers.
pub mod declared;
pub mod document;
pub mod env;
pub mod js;

pub mod action;
pub mod ax;
pub mod effects;
pub mod extractor;
pub mod output;
pub mod sniffer;
pub mod waiter;

pub use output::write_output;
pub use sniffer::{
    Phase, Sniffer, sniff_matrix_with_progress, sniff_session, sniff_session_with_progress,
    trim_for_summary,
};
