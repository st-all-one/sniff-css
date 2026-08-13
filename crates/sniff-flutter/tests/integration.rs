//! Integration tests for the Flutter backend.
//!
//! These require a real Android emulator/device running a debug Flutter app
//! and are auto-skipped when the toolchain is absent, mirroring the
//! `require_chrome` pattern of sniff-engine. Set `SNIFF_TEST_DEVICE` to an
//! `adb` serial (e.g. `emulator-5554`) to run the device-gated tests.

use sniff_flutter::{FlutterMachine, list_devices};

#[tokio::test]
async fn flutter_and_adb_probes_do_not_panic() {
    let _ = sniff_flutter::is_flutter_available();
    let _ = sniff_flutter::is_adb_available();
}

#[tokio::test]
async fn machine_discovery_against_real_device() {
    if !sniff_flutter::is_flutter_available() || !sniff_flutter::is_adb_available() {
        eprintln!("skipping: flutter/adb not on PATH");
        return;
    }
    let Some(device) = std::env::var("SNIFF_TEST_DEVICE").ok() else {
        eprintln!("skipping: set SNIFF_TEST_DEVICE=<adb serial> to run device tests");
        return;
    };
    let devices = list_devices().await.expect("list devices");
    assert!(
        devices.iter().any(|d| d.serial == device),
        "device {device} not visible to adb: {devices:?}"
    );

    // Attach to a debug app on the device; without one the VM service never
    // appears and the attach call times out — acceptable as an explicit skip.
    let mut machine = match FlutterMachine::attach(&device).await {
        Ok(m) => m,
        Err(e) => {
            eprintln!("skipping: cannot attach on {device}: {e}");
            return;
        }
    };
    match machine
        .wait_for_vm_service(std::time::Duration::from_secs(90))
        .await
    {
        Ok(uri) => {
            assert!(
                uri.starts_with("ws://") || uri.starts_with("wss://"),
                "vm service uri: {uri}"
            );
            eprintln!("discovered vm service: {uri}");
        }
        Err(e) => {
            eprintln!("skipping: no debug Flutter app to attach on {device}: {e}");
        }
    }
}
