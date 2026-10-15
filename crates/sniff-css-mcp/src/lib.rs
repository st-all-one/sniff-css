//! MCP server for computed-style sniffing.
//!
//! Exposes `sniff_page`, `sniff_diff`, `sniff_check`,
//! `sniff_snapshots` and `sniff_categories` as MCP tools over stdio,
//! backed by a shared headless Chrome pool, with phase progress streamed
//! asynchronously via `notifications/progress`.

pub mod browser;
pub mod progress;
pub mod server;
pub mod store;

pub use browser::ChromePool;
pub use server::SniffMcpServer;
pub use store::SnapshotStore;
