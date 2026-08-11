//! MCP server for computed-style sniffing.
//!
//! Exposes `sniff_page`, `diff_snapshots` and `list_categories` as MCP
//! tools over stdio, backed by a shared headless Chrome pool, with phase
//! progress streamed asynchronously via `notifications/progress`.

pub mod browser;
pub mod progress;
pub mod server;

pub use browser::ChromePool;
pub use server::SniffMcpServer;
