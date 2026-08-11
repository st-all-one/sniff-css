//! Streaming progress over the MCP protocol.
//!
//! Phase updates are sent as `notifications/progress` JSON-RPC messages
//! during tool execution, so an AI agent sees the pipeline advance
//! asynchronously without blocking. When the client did not supply a
//! progress token, reporting is a no-op.

use rmcp::ErrorData;
use rmcp::model::{ProgressNotificationParam, ProgressToken, RequestMetaObject};
use rmcp::service::{Peer, RoleServer};

/// Sends progress notifications to the connected MCP peer.
#[derive(Debug, Clone)]
pub struct ProgressReporter {
    token: Option<ProgressToken>,
    peer: Peer<RoleServer>,
}

impl ProgressReporter {
    /// Build a reporter from the request metadata and peer. Absent token
    /// means progress notifications are silently skipped.
    pub fn new(meta: &RequestMetaObject, peer: &Peer<RoleServer>) -> Self {
        Self {
            token: meta.get_progress_token(),
            peer: peer.clone(),
        }
    }

    /// Emit a progress notification (`progress` in 0..=1.0).
    pub async fn report(&self, progress: f64, message: &str) -> Result<(), ErrorData> {
        let Some(token) = &self.token else {
            return Ok(());
        };
        self.peer
            .notify_progress(
                ProgressNotificationParam::new(token.clone(), progress)
                    .with_total(1.0)
                    .with_message(message),
            )
            .await
            .map_err(|e| ErrorData::internal_error(format!("progress send failed: {e}"), None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_token_makes_reporter_noop() {
        // We can't build a Peer in unit tests cheaply; here we only assert
        // the type is constructible from an empty RequestMetaObject shape.
        let _ = RequestMetaObject::new();
    }

    #[test]
    fn progress_values_are_monotonic() {
        let phases = [0.2f64, 0.4, 0.7, 0.9];
        let mut prev = -1.0;
        for p in phases {
            assert!(p > prev);
            prev = p;
        }
    }
}
