//! Raw Chrome DevTools Protocol client.

pub mod browser;
pub mod client;
pub mod endpoint;
pub mod protocol;
pub mod session;

pub use browser::BrowserProcess;
pub use client::{CdpClient, CdpError};
pub use endpoint::resolve_endpoint;
pub use protocol::CdpEvent;
pub use session::{CdpSession, CdpSessionError};
