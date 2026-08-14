//! Flutter/Dart native sniffing backend.
//!
//! The web backend (sniff-engine) talks to Chrome over CDP; the Flutter
//! backend talks to a **debug-mode Flutter app** on an Android emulator or
//! device over the Dart **VM Service Protocol** (the same JSON-RPC-over-WebSocket
//! wire as CDP, hence the shared `sniff-cdp::jsonrpc::JsonRpcClient`).
//!
//! Flow: `flutter run --machine` reports the VM Service `ws://` URI, which
//! `VmService` connects to and drives via the `ext.flutter.inspector.*`
//! service extensions to read the widget/render trees.

pub mod action;
pub mod color;
pub mod device;
pub mod driver;
pub mod extractor;
pub mod inspector;
pub mod machine;
pub mod vm;

pub use action::{perform as perform_action, target_finder, unsupported};
pub use device::{
    Device, DeviceError, EmulatorProcess, ViewportGuard, adb, is_adb_available,
    is_flutter_available, list_devices, restore_wm_size, set_wm_size, wm_size,
};
pub use driver::{DriverFinder, FlutterDriver, finder_from_spec};
pub use inspector::FlutterInspector;
pub use machine::{FlutterMachine, MachineError, MachineEvent, parse_machine_line};
pub use sniff_core::types::ElementSnapshot;
pub use vm::{VmService, to_ws_uri};
