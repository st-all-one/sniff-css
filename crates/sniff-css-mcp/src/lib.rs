//! MCP server for computed-style sniffing.
//!
//! Exposes `sniffCSS_page`, `sniffCSS_diff`, `sniffCSS_check`,
//! `sniffCSS_snapshots` and `sniffCSS_categories` as MCP tools over stdio,
//! backed by a shared headless Chrome pool, with phase progress streamed
//! asynchronously via `notifications/progress`.

pub mod browser;
pub mod progress;
pub mod server;
pub mod store;

pub use browser::ChromePool;
pub use server::SniffMcpServer;
pub use store::SnapshotStore;
