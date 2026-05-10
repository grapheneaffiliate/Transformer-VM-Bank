//! Property tests for the ternary execution kernel.
//!
//! Asserts the load-bearing invariants from `docs/SECURITY_REVIEW.md` § 3
//! hold for every randomized case:
//!
//! - I1: same input → same output (determinism).
//! - I2: forward-pass kernel never panics; overflow returns `Err`.
//! - I3: weights file integrity — flipping any non-trailing byte
//!   makes `unpack_weights` fail.

use proptest::prelude::*;
use psl_ternary_vm::{
    network::{argmax, SparseTernaryLayer, TernaryNetwork},
    primitives::{
        byte_add_with_carry, byte_sub_with_borrow, freeze_apply, freeze_setup, mpt_emit_record,
        transfer_check, transfer_finalize,
    },
    thermo,
    weights::{pack_weights, unpack_weights, WeightsHeader},
    TernaryError,
};

proptest! {
    /// I1 — byte_add: ternary matches arithmetic on every random
    /// (a, b, c_in). 256 cases per run is overkill given the
    /// exhaustive test exists, but it exercises the run-twice-and-
    /// compare path that proves determinism even in this format.
    #[test]
    fn byte_add_deterministic_and_correct(a in any::<u8>(), b in any::<u8>(), c in 0u8..=1) {
        let net = byte_add_with_carry::build();
        let (s1, co1) = byte_add_with_carry::run(&net, a, b, c).unwrap();
        let (s2, co2) = byte_add_with_carry::run(&net, a, b, c).unwrap();
        prop_assert_eq!((s1, co1), (s2, co2));
        let want_sum = (a as u16 + b as u16 + c as u16) as u16;
        prop_assert_eq!(s1 as u16, want_sum & 0xff);
        prop_assert_eq!(co1 as u16, want_sum >> 8);
    }

    /// I1 — byte_sub.
    #[test]
    fn byte_sub_deterministic_and_correct(m in any::<u8>(), s in any::<u8>(), b in 0u8..=1) {
        let net = byte_sub_with_borrow::build();
        let (d1, bo1) = byte_sub_with_borrow::run(&net, m, s, b).unwrap();
        let (d2, bo2) = byte_sub_with_borrow::run(&net, m, s, b).unwrap();
        prop_assert_eq!((d1, bo1), (d2, bo2));
        let signed = m as i32 - s as i32 - b as i32;
        if signed < 0 {
            prop_assert_eq!(d1, (signed + 256) as u8);
            prop_assert_eq!(bo1, 1);
        } else {
            prop_assert_eq!(d1, signed as u8);
            prop_assert_eq!(bo1, 0);
        }
    }

    /// I1 — freeze_apply.
    #[test]
    fn freeze_apply_deterministic_and_correct(flag in 0u8..=1, byte47 in any::<u8>()) {
        let net = freeze_apply::build();
        let r = freeze_apply::run(&net, flag, byte47).unwrap();
        let want = if flag != 0 { (byte47 & 0x7f) | 0x80 } else { byte47 & 0x7f };
        prop_assert_eq!(r, want);
    }

    /// I1 — transfer_finalize: u64 nonce + 1 mod 2^64.
    #[test]
    fn transfer_finalize_deterministic_and_correct(nonce in any::<u64>()) {
        let net = transfer_finalize::build();
        let nonce_bytes = nonce.to_le_bytes();
        let r = transfer_finalize::run(&net, nonce_bytes).unwrap();
        let want = nonce.wrapping_add(1).to_le_bytes();
        prop_assert_eq!(r, want);
    }

    /// I1 — transfer_check: u128 ≥ comparison.
    #[test]
    fn transfer_check_deterministic_and_correct(from in any::<u128>(), amount in any::<u128>()) {
        let net = transfer_check::build();
        let r = transfer_check::run(&net, from.to_le_bytes(), amount.to_le_bytes()).unwrap();
        let want = if from >= amount { 1u8 } else { 0u8 };
        prop_assert_eq!(r, want);
    }

    /// I1 — mpt_emit_record: 64-byte pass-through.
    #[test]
    fn mpt_emit_record_deterministic_and_correct(record in proptest::collection::vec(any::<u8>(), 64..=64)) {
        let net = mpt_emit_record::build();
        let mut a = [0u8; 64];
        a.copy_from_slice(&record);
        let r = mpt_emit_record::run(&net, &a).unwrap();
        prop_assert_eq!(r, a);
    }

    /// I1 — freeze_setup extracts (flag, byte47).
    #[test]
    fn freeze_setup_deterministic_and_correct(
        flag in 0u8..=1,
        bytes in proptest::collection::vec(any::<u8>(), 64..=64),
    ) {
        let net = freeze_setup::build();
        let mut w = [0u8; 65];
        w[0] = flag;
        w[1..].copy_from_slice(&bytes);
        let (f, b47) = freeze_setup::run(&net, &w).unwrap();
        prop_assert_eq!(f, flag);
        prop_assert_eq!(b47, bytes[47]);
    }

    /// I3 — weights file integrity. Flip any non-trailing byte → unpack fails.
    #[test]
    fn flipping_a_byte_in_weights_payload_fails_load(
        flip_byte in any::<u8>(),
    ) {
        let layer = SparseTernaryLayer {
            input_dim: 4,
            output_dim: 2,
            pos_indices: vec![vec![0, 2], vec![1]],
            neg_indices: vec![vec![3], vec![]],
            bias: vec![5, -3],
            relu: false,
        };
        let (mut bytes, _) = pack_weights("integrity_test", 4, 2, &[layer]);
        let payload_end = bytes.len() - 32; // last 32 bytes are the digest
        prop_assume!(payload_end > 0);
        // Flip a single bit at a stable offset in the payload.
        let idx = (flip_byte as usize) % payload_end;
        let original = bytes[idx];
        bytes[idx] = original ^ 0x01;
        let r = unpack_weights(&bytes);
        let is_mismatch = matches!(r, Err(TernaryError::WeightsHashMismatch { .. }));
        prop_assert!(is_mismatch);
    }
}

// ── I2 — kernel never panics on adversarial input ──────────────────────

proptest! {
    /// Forward pass on a hand-built tiny network — even adversarial
    /// integer inputs should yield Ok or a typed Err, never a panic.
    #[test]
    fn tiny_network_handles_arbitrary_inputs(input in proptest::collection::vec(any::<i64>(), 8..=8)) {
        // 1-layer network: y[i] = x[i] (identity) with relu = false
        let layer = SparseTernaryLayer {
            input_dim: 8,
            output_dim: 8,
            pos_indices: (0..8u32).map(|i| vec![i]).collect(),
            neg_indices: vec![vec![]; 8],
            bias: vec![0; 8],
            relu: false,
        };
        let net = TernaryNetwork::new(
            WeightsHeader {
                version: 1,
                primitive: "tiny".into(),
                input_dim: 8,
                output_dim: 8,
                weights_hash: [0; 32],
                weights_hash_v2: [0; 64],
            },
            vec![layer],
        );
        let r = net.forward(&input);
        // Either Ok (no overflow) or a typed overflow error; never a panic.
        match r {
            Ok(out) => prop_assert_eq!(out, input),
            Err(TernaryError::Overflow { .. }) => {} // acceptable
            Err(e) => {
                let msg = format!("unexpected error: {e}");
                prop_assert!(false, "{}", msg);
            }
        }
    }

    /// argmax never panics; ties go to lowest index.
    #[test]
    fn argmax_never_panics(values in proptest::collection::vec(any::<i64>(), 1..=64)) {
        let r = argmax(&values).unwrap();
        // r must point to one of the maximum values
        let max = values.iter().max().unwrap();
        prop_assert_eq!(values[r], *max);
        // Tie-break: lowest index
        for i in 0..r {
            prop_assert!(values[i] < *max);
        }
    }

    /// Thermometer encode/decode round-trip on arbitrary (v, max).
    #[test]
    fn thermo_encode_decode_roundtrip(
        max_val in 1i64..1024,
        v in 0i64..1024,
    ) {
        prop_assume!(v <= max_val);
        let t = thermo::encode(v, max_val);
        prop_assert_eq!(t.len(), (max_val + 1) as usize);
        prop_assert_eq!(t.iter().sum::<i64>(), v + 1);
        prop_assert_eq!(thermo::decode(&t), v);
    }
}
