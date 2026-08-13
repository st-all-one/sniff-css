//! Flutter widget-inspector client (`ext.flutter.inspector.*`).
//!
//! Drives the same service extensions that Flutter DevTools uses, over the
//! Dart VM Service protocol:
//!
//! - `getRootWidgetSummaryTree` — the whole widget tree in one response
//!   (deep `children`, summary nodes carry a `valueId`);
//! - `getChildrenSummaryTree(valueId)` — expand a subtree on demand;
//! - `getProperties(valueId)` — the widget's diagnostics properties;
//! - `getLayoutExplorerNode(valueId)` — geometry: `isBox`, `size`,
//!   `parentData` offset, `constraints`.
//!
//! Service-extension params are string maps; every response is wrapped as
//! `{"result": ...}` by the VM service.

use crate::vm::{Result, VmError, VmService};
use serde_json::{Map, Value, json};

/// A live session against a Flutter app's widget inspector.
///
/// Cheap to clone: the underlying [`VmService`] is a shared connection.
#[derive(Clone)]
pub struct FlutterInspector {
    vm: VmService,
    isolate_id: String,
    group: String,
}

impl std::fmt::Debug for FlutterInspector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlutterInspector")
            .field("isolate", &self.isolate_id)
            .field("group", &self.group)
            .finish()
    }
}

impl FlutterInspector {
    /// Connect to a VM Service URI, resolve the first isolate and open an
    /// object group for the session.
    pub async fn connect(ws_uri: &str) -> Result<Self> {
        let vm = VmService::connect(ws_uri).await?;
        let isolate_id = resolve_first_isolate(&vm).await?;
        // Group name scoped to this process for easy cleanup.
        let group = format!("sniff-flutter-{}", std::process::id());
        Ok(Self {
            vm,
            isolate_id,
            group,
        })
    }

    /// The isolate the inspector is attached to.
    pub fn isolate(&self) -> &str {
        &self.isolate_id
    }

    /// Fetch the whole widget tree as a summary (one root node with deep
    /// `children`). `None` when the app has no widget tree yet.
    pub async fn root_widget_summary_tree(&self) -> Result<Option<Value>> {
        let raw = self
            .vm
            .call(
                "ext.flutter.inspector.getRootWidgetSummaryTree",
                self.params(Map::new()),
            )
            .await?;
        Ok(null_to_none(raw))
    }

    /// Expand a node's children on demand (`valueId` → array of nodes).
    pub async fn children_summary_tree(&self, value_id: &str) -> Result<Vec<Value>> {
        let params = self.params(json!({ "arg": value_id }).as_object().cloned().unwrap());
        let raw = self
            .vm
            .call("ext.flutter.inspector.getChildrenSummaryTree", params)
            .await?;
        result_array(raw)
    }

    /// Fetch a node's diagnostics properties (`valueId` → array).
    pub async fn properties(&self, value_id: &str) -> Result<Vec<Value>> {
        let params = self.params(json!({ "arg": value_id }).as_object().cloned().unwrap());
        let raw = self
            .vm
            .call("ext.flutter.inspector.getProperties", params)
            .await?;
        result_array(raw)
    }

    /// Fetch a node's render-object geometry (size/offset/constraints).
    pub async fn layout_explorer_node(&self, value_id: &str) -> Result<Value> {
        let params = self.params(
            json!({
                "id": value_id,
                "subtreeDepth": "0",
                "groupName": self.group,
            })
            .as_object()
            .cloned()
            .unwrap(),
        );
        self.vm
            .call("ext.flutter.inspector.getLayoutExplorerNode", params)
            .await
    }

    /// Release all object references held by this session's group.
    pub async fn dispose_group(&self) -> Result<()> {
        let params = json!({ "objectGroup": self.group });
        self.vm
            .call("ext.flutter.inspector.disposeGroup", params)
            .await?;
        Ok(())
    }

    /// Set the app's global time dilation (the `ext.flutter.timeDilation`
    /// service extension). `1.0` restores real time.
    pub async fn set_time_dilation(&self, dilation: f64) -> Result<()> {
        let params = self.params(
            json!({ "timeDilation": dilation.to_string() })
                .as_object()
                .cloned()
                .unwrap(),
        );
        self.vm.call("ext.flutter.timeDilation", params).await?;
        Ok(())
    }

    /// Freeze animations before capture for deterministic snapshots (the
    /// Flutter analogue of the web `STABILIZE_JS`): a huge time dilation makes
    /// animation clocks advance so slowly the frame is effectively frozen.
    pub async fn freeze_animations(&self) -> Result<()> {
        self.set_time_dilation(1e6).await
    }

    /// Close the underlying VM Service connection.
    pub async fn close(&self) {
        self.vm.close().await;
    }

    /// Build a service-extension params map: `isolateId` + `objectGroup`,
    /// merged over any caller-supplied extra keys.
    fn params(&self, extra: Map<String, Value>) -> Value {
        build_params(&self.isolate_id, &self.group, extra)
    }
}

/// Build a service-extension params map (free function for testability).
fn build_params(isolate_id: &str, group: &str, extra: Map<String, Value>) -> Value {
    let mut map = Map::new();
    map.insert("isolateId".into(), Value::String(isolate_id.to_string()));
    map.insert("objectGroup".into(), Value::String(group.to_string()));
    map.extend(extra);
    Value::Object(map)
}

/// Resolve the first isolate id from `getVM`.
async fn resolve_first_isolate(vm: &VmService) -> Result<String> {
    let vm_info = vm.get_vm().await?;
    let isolate_id = vm_info
        .get("isolates")
        .and_then(Value::as_array)
        .and_then(|iso| iso.first())
        .and_then(|i| i.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            VmError::Other("getVM returned no isolates (is the Flutter app in debug mode?)".into())
        })?
        .to_string();
    Ok(isolate_id)
}

/// Turn a `Value::Null` into `None` (the VM Service already unwraps the
/// `result` envelope through `VmService::call`).
fn null_to_none(raw: Value) -> Option<Value> {
    match raw {
        Value::Null => None,
        v => Some(v),
    }
}

/// Require the VM Service `result` value (already unwrapped) to be an array.
fn result_array(raw: Value) -> Result<Vec<Value>> {
    match raw {
        Value::Array(items) => Ok(items),
        Value::Null => Ok(Vec::new()),
        other => Err(VmError::Other(format!(
            "expected array result, got {}",
            other
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_to_none_handles_null_and_value() {
        assert!(null_to_none(Value::Null).is_none());
        assert_eq!(null_to_none(json!({"a": 1})), Some(json!({"a": 1})));
    }

    #[test]
    fn result_array_rejects_objects() {
        assert!(result_array(json!([1, 2])).is_ok());
        assert!(result_array(Value::Null).is_ok());
        assert!(result_array(json!({"a": 1})).is_err());
    }

    #[test]
    fn params_include_isolate_and_group() {
        let params = build_params("iso-1", "g-1", Map::new());
        assert_eq!(params["isolateId"], "iso-1");
        assert_eq!(params["objectGroup"], "g-1");

        let extra = json!({"id": "inspector-3", "subtreeDepth": "0"});
        let params = build_params("iso-1", "g-1", extra.as_object().cloned().unwrap());
        assert_eq!(params["id"], "inspector-3");
        assert_eq!(params["subtreeDepth"], "0");
    }
}
