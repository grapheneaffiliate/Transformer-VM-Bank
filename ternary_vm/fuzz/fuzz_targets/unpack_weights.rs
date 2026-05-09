//! Fuzz harness for `psl_ternary_vm::weights::unpack_weights`.
//!
//! The decoder must NEVER panic on adversarial input. Every byte
//! sequence either round-trips cleanly or returns a typed
//! `TernaryError`. Run for ≥ 1 CPU-hour per the audit checklist:
//!
//!     cargo +nightly fuzz run unpack_weights -- -max_total_time=3600

#![no_main]

use libfuzzer_sys::fuzz_target;
use psl_ternary_vm::weights::unpack_weights;

fuzz_target!(|data: &[u8]| {
    let _ = unpack_weights(data);
});
