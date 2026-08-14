//! Flutter Driver extension client (`ext.flutter.driver`).
//!
//! Drives user interactions (tap, enter text) inside a **debug-mode** Flutter
//! app over the Dart VM Service — the same service extension `flutter drive`
//! and DevTools use. The target widget is located in-app (by `ValueKey`, by
//! widget type or by text), so no pixel math is involved and the dispatched
//! gesture is always at the widget's real center.
//!
//! Requirement: the app must call `enableFlutterDriverExtension()` (from
//! `package:flutter_driver/driver_extension.dart`) in `main()` — the
//! sniff-flutter fixture does; real apps need one line. Without it
//! [`FlutterDriver::is_available`] is `false` and actions fail with a clear
//! message instead of silently doing nothing.

use crate::vm::{Result, VmError, VmService};
use serde_json::{Map, Value, json};
use std::time::Duration;

/// Which widget finder locates the interaction target inside the app.
///
/// Mirrors the serialization of the Flutter Driver `SerializableFinder`
/// classes (`ByValueKey`, `ByType`, `ByText`) so the params map can be sent
/// verbatim over the VM Service.
#[derive(Debug, Clone, PartialEq)]
pub enum DriverFinder {
    /// `ByValueKey` — a widget with a `ValueKey` (most precise).
    ByValueKey(String),
    /// `ByType` — first widget of a runtime type, e.g. `FilledButton`.
    ByType(String),
    /// `ByText` — a `Text`/`EditableText` with this string.
    ByText(String),
}

impl DriverFinder {
    fn serialize(&self) -> Map<String, Value> {
        let mut map = Map::new();
        match self {
            DriverFinder::ByValueKey(key) => {
                map.insert("finderType".into(), json!("ByValueKey"));
                map.insert("keyValueString".into(), json!(key));
                map.insert("keyValueType".into(), json!("String"));
            }
            DriverFinder::ByType(ty) => {
                map.insert("finderType".into(), json!("ByType"));
                map.insert("type".into(), json!(ty));
            }
            DriverFinder::ByText(text) => {
                map.insert("finderType".into(), json!("ByText"));
                map.insert("text".into(), json!(text));
            }
        }
        map
    }

    /// Human-readable description for error messages.
    pub fn describe(&self) -> String {
        match self {
            DriverFinder::ByValueKey(key) => format!("key `{key}`"),
            DriverFinder::ByType(ty) => format!("type `{ty}`"),
            DriverFinder::ByText(text) => format!("text `{text}`"),
        }
    }
}

/// Resolve an action target spec to a driver finder.
///
/// The spec is whatever the user passed to `--action`/`--click` — most often a
/// widget identity straight from the snapshot's `selector`/`path`, e.g.
/// `FilledButton-[<'counter'>][0]` or `TextField-[<'field'>][0]`. Resolution:
///
/// 1. A `ValueKey` embedded in the spec (`<'key'>`) → [`DriverFinder::ByValueKey`]
///    (exact and stable, preferred);
/// 2. A bare widget type (`Text`, `FilledButton`, possibly with an `[ordinal]`
///    suffix) → [`DriverFinder::ByType`] (first widget of that type);
/// 3. Anything else → [`DriverFinder::ByText`] (matches a label).
pub fn finder_from_spec(spec: &str) -> DriverFinder {
    if let Some(key) = extract_value_key(spec) {
        return DriverFinder::ByValueKey(key);
    }
    let class = spec.trim().split('[').next().unwrap_or(spec).trim();
    if is_widget_type(class) {
        return DriverFinder::ByType(class.to_string());
    }
    DriverFinder::ByText(spec.to_string())
}

/// The `ValueKey` string embedded in a widget diagnostics description
/// (`FilledButton-[<'counter'>]` → `Some("counter")`).
fn extract_value_key(spec: &str) -> Option<String> {
    let start = spec.find("<'")? + 2;
    let rest = &spec[start..];
    let end = rest.find("'>")?;
    Some(rest[..end].to_string())
}

/// Whether `s` looks like a Flutter widget runtime type (an identifier
/// starting with an uppercase letter).
fn is_widget_type(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().is_some_and(char::is_uppercase)
        && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

/// A session against the app's Flutter Driver extension.
///
/// Cheap to clone: the underlying [`VmService`] is a shared connection.
#[derive(Clone)]
pub struct FlutterDriver {
    vm: VmService,
    isolate_id: String,
}

impl std::fmt::Debug for FlutterDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlutterDriver")
            .field("isolate", &self.isolate_id)
            .finish()
    }
}

impl FlutterDriver {
    /// Connect to a VM Service URI and resolve the first isolate.
    pub async fn connect(ws_uri: &str) -> Result<Self> {
        let vm = VmService::connect(ws_uri).await?;
        let isolate_id = resolve_first_isolate(&vm).await?;
        Ok(Self { vm, isolate_id })
    }

    /// Whether the app registered the `ext.flutter.driver` extension — i.e.
    /// `main()` calls `enableFlutterDriverExtension()`. Checked via the
    /// isolate's `extensionRPCs`, so no command is actually sent.
    pub async fn is_available(&self) -> bool {
        match self
            .vm
            .call(
                "getIsolate",
                json!({ "isolateId": self.isolate_id }),
            )
            .await
        {
            Ok(info) => info
                .get("extensionRPCs")
                .and_then(Value::as_array)
                .is_some_and(|exts| {
                    exts.iter()
                        .any(|e| e.as_str() == Some("ext.flutter.driver"))
                }),
            Err(_) => false,
        }
    }

    /// Tap the center of the widget located by `finder`.
    pub async fn tap(&self, finder: &DriverFinder) -> Result<()> {
        self.call_command("tap", Some(finder), Map::new()).await?;
        Ok(())
    }

    /// Enter `text` into the currently focused field (tap the field first to
    /// focus it — `Action::Type` does this before calling here).
    pub async fn enter_text(&self, text: &str) -> Result<()> {
        let mut extra = Map::new();
        extra.insert("text".into(), json!(text));
        self.call_command("enter_text", None, extra).await?;
        Ok(())
    }

    /// Wait until the widget located by `finder` appears (or `timeout` elapses).
    pub async fn wait_for(&self, finder: &DriverFinder, timeout: Duration) -> Result<()> {
        let mut extra = Map::new();
        extra.insert("timeout".into(), json!(timeout.as_millis() as u64));
        self.call_command("waitFor", Some(finder), extra).await?;
        Ok(())
    }

    /// Send one driver command and return its `response` payload, turning a
    /// driver-side `isError` response into a [`VmError`].
    async fn call_command(
        &self,
        kind: &str,
        finder: Option<&DriverFinder>,
        extra: Map<String, Value>,
    ) -> Result<Value> {
        let mut params = Map::new();
        params.insert("isolateId".into(), Value::String(self.isolate_id.clone()));
        params.insert("command".into(), Value::String(kind.to_string()));
        if let Some(f) = finder {
            params.extend(f.serialize());
        }
        params.extend(extra);
        let raw = self
            .vm
            .call("ext.flutter.driver", Value::Object(params))
            .await?;
        if raw.get("isError").and_then(Value::as_bool) == Some(true) {
            let message = raw
                .get("response")
                .and_then(Value::as_str)
                .unwrap_or("flutter driver command failed");
            return Err(VmError::Other(format!("flutter driver {kind}: {message}")));
        }
        Ok(raw.get("response").cloned().unwrap_or(Value::Null))
    }

    /// Close the underlying VM Service connection.
    pub async fn close(&self) {
        self.vm.close().await;
    }
}

/// Resolve the first isolate id from `getVM`.
async fn resolve_first_isolate(vm: &VmService) -> Result<String> {
    let vm_info = vm.get_vm().await?;
    vm_info
        .get("isolates")
        .and_then(Value::as_array)
        .and_then(|iso| iso.first())
        .and_then(|i| i.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            VmError::Other("getVM returned no isolates (is the Flutter app in debug mode?)".into())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_value_key_from_identity() {
        assert_eq!(
            extract_value_key("FilledButton-[<'counter'>][0]"),
            Some("counter".to_string())
        );
        assert_eq!(extract_value_key("TextField-[<'field'>][0]"), Some("field".to_string()));
        assert_eq!(extract_value_key("Text[0]"), None);
        assert_eq!(extract_value_key("Open modal"), None);
    }

    #[test]
    fn resolves_finder_preferring_key_then_type_then_text() {
        assert_eq!(
            finder_from_spec("FilledButton-[<'counter'>][0]"),
            DriverFinder::ByValueKey("counter".into())
        );
        assert_eq!(finder_from_spec("Text"), DriverFinder::ByType("Text".into()));
        assert_eq!(finder_from_spec("Text[0]"), DriverFinder::ByType("Text".into()));
        assert_eq!(
            finder_from_spec("Open modal"),
            DriverFinder::ByText("Open modal".into())
        );
        assert_eq!(
            finder_from_spec("ColoredBox"),
            DriverFinder::ByType("ColoredBox".into())
        );
    }

    #[test]
    fn finder_serializes_like_the_dart_classes() {
        let params = DriverFinder::ByValueKey("counter".into()).serialize();
        assert_eq!(params["finderType"], "ByValueKey");
        assert_eq!(params["keyValueString"], "counter");
        assert_eq!(params["keyValueType"], "String");

        let params = DriverFinder::ByType("FilledButton".into()).serialize();
        assert_eq!(params["finderType"], "ByType");
        assert_eq!(params["type"], "FilledButton");

        let params = DriverFinder::ByText("Counter: 1".into()).serialize();
        assert_eq!(params["finderType"], "ByText");
        assert_eq!(params["text"], "Counter: 1");
    }
}
