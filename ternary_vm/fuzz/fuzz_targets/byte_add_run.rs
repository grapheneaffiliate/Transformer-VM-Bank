//! Fuzz the `byte_add_with_carry::run` end-to-end input path.
//! Also covers `encode_input` and `decode_output`.
//!
//! Run for ≥ 1 CPU-hour per the audit checklist:
//!
//!     cargo +nightly fuzz run byte_add_run -- -max_total_time=3600

#![no_main]

use libfuzzer_sys::fuzz_target;
use psl_ternary_vm::primitives::byte_add_with_carry;

fuzz_target!(|data: &[u8]| {
    if data.len() < 3 {
        return;
    }
    let net = byte_add_with_carry::build();
    // Bias inputs into the documented range so the kernel exercises
    // the correctness path; carry > 1 should be rejected by encode.
    let _ = byte_add_with_carry::run(&net, data[0], data[1], data[2]);
});
