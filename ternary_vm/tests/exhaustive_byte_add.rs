//! Exhaustive verification of `byte_add_with_carry`.
//!
//! Iterates all 256 × 256 × 2 = 131,072 input combinations and checks
//! that the ternary network's output matches `(a + b + c) mod 256` /
//! `(a + b + c) ≥ 256` for every one. This is the gate-10 acceptance
//! criterion for this primitive (see `docs/STATUS.md`).
//!
//! Also computes the frozen v1 trace hash (`trace_hash_v1`, the
//! behavior behind the deprecated `trace_hash_ternary` re-export) for
//! each witness to confirm determinism across re-runs and to seed the
//! cross-platform determinism CI test (gate 10 part 2 — landing in a
//! follow-up commit).
//!
//! Runtime: ~1 second on a modern x86_64 single thread (single-shot
//! inference, ~3590 ternary slots, no autoregressive loop).

use psl_ternary_vm::primitives::byte_add_with_carry::{build, run};
use psl_ternary_vm::primitives::byte_add_with_carry::{decode_output, encode_input};
use psl_ternary_vm::trace_hash::v1::trace_hash_v1;

/// `(a, b, c, got, want)` for the first failing combination.
type FailCase = (u8, u8, u8, (u8, u8), (u8, u8));

fn ground_truth(a: u8, b: u8, c: u8) -> (u8, u8) {
    let s = a as u16 + b as u16 + c as u16;
    ((s & 0xff) as u8, (s >> 8) as u8)
}

#[test]
fn byte_add_exhaustive_131072() {
    let net = build();
    let mut pass = 0u32;
    let mut fail = 0u32;
    let mut first_fail: Option<FailCase> = None;

    for a in 0u8..=255 {
        for b in 0u8..=255 {
            for c in 0u8..=1 {
                let got = run(&net, a, b, c).expect("run failed");
                let want = ground_truth(a, b, c);
                if got == want {
                    pass += 1;
                } else if first_fail.is_none() {
                    fail += 1;
                    first_fail = Some((a, b, c, got, want));
                } else {
                    fail += 1;
                }
            }
        }
    }
    assert_eq!(
        pass, 131_072,
        "expected exhaustive pass; first fail: {first_fail:?}"
    );
    assert_eq!(fail, 0);
}

#[test]
fn trace_hash_is_deterministic_across_runs() {
    let net1 = build();
    let net2 = build();
    let cases = [(0u8, 0u8, 0u8), (255, 255, 1), (1, 1, 0), (128, 128, 1)];
    for (a, b, c) in cases {
        let in_vec = encode_input(a, b, c).unwrap();
        let out1 = net1.forward(&in_vec).unwrap();
        let out2 = net2.forward(&in_vec).unwrap();
        assert_eq!(
            out1, out2,
            "forward differs across builds for ({a},{b},{c})"
        );

        let h1 = trace_hash_v1(&net1, &in_vec, &out1);
        let h2 = trace_hash_v1(&net2, &in_vec, &out2);
        assert_eq!(h1, h2);

        // also check decode round-trip
        let (s, co) = decode_output(&out1).unwrap();
        assert_eq!((s, co), ground_truth(a, b, c));
    }
}

#[test]
fn trace_hash_differs_for_different_inputs() {
    let net = build();
    let in_a = encode_input(1, 2, 0).unwrap();
    let in_b = encode_input(1, 3, 0).unwrap();
    let out_a = net.forward(&in_a).unwrap();
    let out_b = net.forward(&in_b).unwrap();
    let ha = trace_hash_v1(&net, &in_a, &out_a);
    let hb = trace_hash_v1(&net, &in_b, &out_b);
    assert_ne!(ha, hb);
}
