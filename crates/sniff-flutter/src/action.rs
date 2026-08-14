//! Performing `sniff_core::Action` interactions on a Flutter app.
//!
//! The web backend interacts via CDP (`Input.dispatchMouseEvent`, …); the
//! Flutter backend drives the app through the Flutter Driver extension
//! ([`crate::driver::FlutterDriver`]), which locates the target widget in-app
//! (by `ValueKey`/type/text) and dispatches a real gesture at its center.
//!
//! `hover` and `upload` are web-specific (there is no pointer cursor or file
//! input on a mobile Flutter surface) and fail with a clear error.

use crate::driver::{DriverFinder, FlutterDriver, finder_from_spec};
use crate::vm::{Result, VmError};
use sniff_core::config::Action;

/// Perform one interaction.
///
/// `finder_from_spec` resolves the action's selector (a widget identity from
/// the snapshot, e.g. `FilledButton-[<'counter'>][0]`) to a driver finder.
/// `Action::Type` taps the target first (to focus the field) then enters the
/// text. The caller is responsible for the `settle_ms` pause afterwards.
pub async fn perform(driver: &FlutterDriver, action: &Action) -> Result<()> {
    unsupported(action)?;
    match action {
        Action::Click { selector, .. } => {
            let finder = finder_from_spec(selector);
            driver.tap(&finder).await
        }
        Action::Type { selector, text, .. } => {
            let finder = finder_from_spec(selector);
            driver.tap(&finder).await?;
            driver.enter_text(text).await
        }
        Action::Hover { .. } | Action::Upload { .. } => unreachable!("checked by unsupported()"),
    }
}

/// Reject the web-only action kinds before any driver I/O (pure, testable).
pub fn unsupported(action: &Action) -> Result<()> {
    match action {
        Action::Hover { selector, .. } => Err(VmError::Other(format!(
            "flutter has no hover pointer: `{selector}` (hover actions are web-only)"
        ))),
        Action::Upload { selector, .. } => Err(VmError::Other(format!(
            "flutter has no file input: `{selector}` (upload actions are web-only)"
        ))),
        _ => Ok(()),
    }
}

/// The `DriverFinder` this action will target (used by tests and for error
/// messages).
pub fn target_finder(action: &Action) -> Option<DriverFinder> {
    match action {
        Action::Click { selector, .. } | Action::Type { selector, .. } => {
            Some(finder_from_spec(selector))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(kind: &str, selector: &str) -> Action {
        match kind {
            "hover" => Action::Hover {
                selector: selector.into(),
                timeout_ms: 10_000,
                settle_ms: 150,
            },
            "upload" => Action::Upload {
                selector: selector.into(),
                files: vec!["/tmp/x.png".into()],
                timeout_ms: 10_000,
                settle_ms: 150,
            },
            _ => Action::Click {
                selector: selector.into(),
                timeout_ms: 10_000,
                settle_ms: 150,
            },
        }
    }

    #[test]
    fn hover_and_upload_fail_with_clear_message() {
        let err = unsupported(&action("hover", "Text")).unwrap_err().to_string();
        assert!(err.contains("hover"), "err: {err}");
        assert!(err.contains("web-only"), "err: {err}");

        let err = unsupported(&action("upload", "Text")).unwrap_err().to_string();
        assert!(err.contains("file input"), "err: {err}");
    }

    #[test]
    fn click_and_type_pass_the_guard() {
        assert!(unsupported(&action("click", "Text[0]")).is_ok());
        assert!(unsupported(&Action::Type {
            selector: "TextField-[<'field'>][0]".into(),
            text: "hi".into(),
            timeout_ms: 10_000,
            settle_ms: 150,
        })
        .is_ok());
    }

    #[test]
    fn target_finder_uses_value_key_when_present() {
        assert_eq!(
            target_finder(&action("click", "FilledButton-[<'counter'>][0]")),
            Some(DriverFinder::ByValueKey("counter".into()))
        );
        assert_eq!(
            target_finder(&action("click", "Open modal")),
            Some(DriverFinder::ByText("Open modal".into()))
        );
        assert!(target_finder(&action("hover", "Text")).is_none());
    }
}
