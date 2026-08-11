//! Sniffing engine: orchestrates wait strategies, element filters and
//! computed-style extraction against a CDP session.
//!
//! The engine owns the in-page JavaScript extraction pass, the wait
//! strategy executor and the output serializers. It is independent of
//! any CLI surface so it can be reused by binaries, watch modes or
//! servers.

pub mod extractor;
pub mod output;
pub mod sniffer;
pub mod waiter;

pub use output::write_output;
pub use sniffer::{Phase, Sniffer, sniff_session, sniff_session_with_progress};
