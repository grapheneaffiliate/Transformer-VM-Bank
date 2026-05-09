//! Fuzz JSON deserialization of `ProtocolMessage`. The wire layer
//! must never panic on adversarial bytes; malformed input → typed
//! `serde_json::Error`, then the caller's signature check rejects.

#![no_main]

use libfuzzer_sys::fuzz_target;
use psl_agent_protocol::ProtocolMessage;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _: Result<ProtocolMessage, _> = serde_json::from_str(s);
    }
});
