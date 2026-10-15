#![no_main]
//! Fuzz do decodificador de frames JSON-RPC (`sniff-cdp`): nenhuma entrada
//! UTF-8 pode causar panic (`decode_incoming` é total).

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = sniff_cdp::decode_incoming(text);
    }
});
