//! Raw Chrome DevTools Protocol client.

pub mod browser;
pub mod client;
pub mod protocol;
pub mod session;

pub use browser::BrowserProcess;
pub use client::{CdpClient, CdpError};
pub use protocol::CdpEvent;
pub use session::{CdpSession, CdpSessionError};
