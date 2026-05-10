//! Trace-hash contract for ternary primitives — `docs/ARCHITECTURE.md` § 0.8.
//!
//! ## Two versions, one contract
//!
//! Per ADR-0008 (BLAKE3-512 for long-lived commitments) we maintain
//! **two** trace-hash format versions side by side:
//!
//! - **v1** ([`v1::trace_hash_v1`]) — original. `weights_hash` is a
//!   32-byte BLAKE3-256 digest. **Frozen.** No new code should produce
//!   v1 trace hashes; v1 stays present and tested so historical traces
//!   (and the v1 sequencer code path during migration windows) remain
//!   independently verifiable. Modifying v1 is forbidden — see the v1
//!   known-answer test (KAT) including the adversarial-input case
//!   below.
//! - **v2** ([`v2::trace_hash_v2`]) — current canonical for new
//!   networks. `weights_hash` is a 64-byte BLAKE3-512 digest per
//!   ADR-0008. The trace-hash output itself stays 32 bytes
//!   (BLAKE3-256 over the v2 inputs); only the `weights_hash`
//!   commitment widens.
//!
//! ## Cutover policy
//!
//! There is currently no live PSL chain (v0.1.0 is audit-pending).
//! The cutover is therefore "any block produced under v2 sequencer
//! code". When a chain is operating, the cutover is recorded as a
//! block-height boundary in the chain's genesis-config addendum;
//! pre-cutover blocks verify under v1, at-or-after-cutover under v2.
//!
//! ## Why both
//!
//! ADRs 0001 and 0008 establish the precedent: when the canonical
//! contract changes, the previous contract is **frozen** and remains
//! verifiable, not deleted. The v1 verifier code is small (this
//! module + the `weights_hash` field on `WeightsHeader`) and the
//! testing cost is negligible. Maintaining the v1 path now means the
//! v0.2 → v0.3 migration story is already proven.
//!
//! ## Determinism (both versions)
//!
//! Because integer addition is associative and the forward pass uses
//! no fp, `trace_hash_v{1,2}(P, x)` is bit-identical across any
//! conformant verifier (x86_64, aarch64, FPGA, secure enclave, …).
//! This is what removes the canonical-engine pin that gate-8 had to
//! maintain.

use crate::network::TernaryNetwork;

/// Canonical input/output encoding: 4-byte BE length, then 8-byte BE
/// per i64. Shared between v1 and v2 — the encoding of `(input, output)`
/// is the same; the only thing that changes between versions is the
/// width of the `weights_hash` commitment fed into the hasher.
pub fn encode_canonical(values: &[i64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 8 * values.len());
    out.extend_from_slice(&(values.len() as u32).to_be_bytes());
    for &v in values {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out
}

/// **Trace-hash v1 (frozen).**
///
/// Per ADR-0001 / ADR-0008 freeze policy. Do not modify. New code
/// should call [`v2::trace_hash_v2`].
pub mod v1 {
    use super::{encode_canonical, TernaryNetwork};

    /// `trace_hash_v1(P, x, y)` — original contract. BLAKE3-256 over
    /// `weights_hash_v1 (32B) || canonical(x) || canonical(y)`.
    ///
    /// **Frozen** per ADR-0008. Caller-supplied `x` (input) and `y`
    /// (output) commit to the run; the network's `weights_hash`
    /// (32-byte BLAKE3-256) commits to the program.
    pub fn trace_hash_v1(net: &TernaryNetwork, input: &[i64], output: &[i64]) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(&net.header.weights_hash);
        h.update(&encode_canonical(input));
        h.update(&encode_canonical(output));
        let mut out = [0u8; 32];
        out.copy_from_slice(h.finalize().as_bytes());
        out
    }
}

/// **Trace-hash v2 (current canonical).**
///
/// Per ADR-0008. New trace-hash production goes through this module.
pub mod v2 {
    use super::{encode_canonical, TernaryNetwork};

    /// `trace_hash_v2(P, x, y)` — BLAKE3-256 over
    /// `weights_hash_v2 (64B) || canonical(x) || canonical(y)`.
    ///
    /// The trace-hash output itself stays 32 bytes (per ADR-0008
    /// short-lived hashes stay 256-bit); only the `weights_hash`
    /// commitment widens to 64 bytes (BLAKE3-512). This is the
    /// load-bearing change: irrevocable per-trace commitments to the
    /// program identity get the full 256-bit quantum margin (Grover-
    /// halved from 512-bit).
    pub fn trace_hash_v2(net: &TernaryNetwork, input: &[i64], output: &[i64]) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(&net.header.weights_hash_v2);
        h.update(&encode_canonical(input));
        h.update(&encode_canonical(output));
        let mut out = [0u8; 32];
        out.copy_from_slice(h.finalize().as_bytes());
        out
    }
}

/// Backwards-compat re-export. Existing callers (none in production
/// as of v0.1.0; see scope analysis in PR description) get the
/// **v1 frozen** behavior. New code should call
/// [`v2::trace_hash_v2`] directly.
#[deprecated(
    since = "0.1.1",
    note = "use v2::trace_hash_v2 for new networks; v1 is frozen per ADR-0008. \
            This re-export exists only for v0.1.0 backwards compatibility and \
            will be removed in v0.2.0."
)]
pub fn trace_hash_ternary(net: &TernaryNetwork, input: &[i64], output: &[i64]) -> [u8; 32] {
    #[allow(deprecated)]
    v1::trace_hash_v1(net, input, output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::SparseTernaryLayer;
    use crate::weights::WeightsHeader;

    fn dummy_net() -> TernaryNetwork {
        let header = WeightsHeader {
            version: 1,
            primitive: "test".into(),
            input_dim: 1,
            output_dim: 1,
            weights_hash: [42u8; 32],
            weights_hash_v2: [43u8; 64],
        };
        let layer = SparseTernaryLayer {
            input_dim: 1,
            output_dim: 1,
            pos_indices: vec![vec![0]],
            neg_indices: vec![vec![]],
            bias: vec![0],
            relu: false,
        };
        TernaryNetwork::new(header, vec![layer])
    }

    // --- v1 tests (frozen behavior) ---

    #[test]
    fn v1_same_inputs_produce_same_hash() {
        let net = dummy_net();
        let h1 = v1::trace_hash_v1(&net, &[1, 2, 3], &[6]);
        let h2 = v1::trace_hash_v1(&net, &[1, 2, 3], &[6]);
        assert_eq!(h1, h2);
    }

    #[test]
    fn v1_different_inputs_produce_different_hash() {
        let net = dummy_net();
        let h1 = v1::trace_hash_v1(&net, &[1, 2, 3], &[6]);
        let h2 = v1::trace_hash_v1(&net, &[1, 2, 4], &[6]);
        assert_ne!(h1, h2);
    }

    #[test]
    fn v1_different_outputs_produce_different_hash() {
        let net = dummy_net();
        let h1 = v1::trace_hash_v1(&net, &[1, 2, 3], &[6]);
        let h2 = v1::trace_hash_v1(&net, &[1, 2, 3], &[7]);
        assert_ne!(h1, h2);
    }

    /// **Frozen v1 KAT (benign).** A documented input → digest pair
    /// that v1 must produce for all time. If this digest changes, the
    /// v1 verifier has drifted and historical block-header
    /// verification is broken.
    ///
    /// The KAT digest is computed once from the v1 algorithm against
    /// `(weights_hash=[42; 32], input=[1, 2, 3], output=[6])` and
    /// pinned here. Any code change that perturbs v1's bytes will
    /// fail this test.
    #[test]
    fn v1_frozen_kat_benign_input() {
        let net = dummy_net();
        let got = v1::trace_hash_v1(&net, &[1, 2, 3], &[6]);
        // Pinned 2026-05-10. If this assertion fails after a code change,
        // v1 has drifted. v1 is FROZEN per ADR-0008. Do not update this
        // expected value to match new behavior — fix the regression.
        let expected =
            hex::decode("f5b6e20e4bc39d21831b49ca6afcfd28c3fe1efda4f82e28e12ee8810b32f298")
                .unwrap();
        assert_eq!(
            got.as_slice(),
            expected.as_slice(),
            "v1 trace_hash KAT (benign) drifted — see ADR-0008. \n\
             expected: {expected:?}\n\
             got:      {got:?}\n\
             If this is a legitimate v2 migration, do NOT update v1 — \
             update the v2 caller path instead."
        );
    }

    /// **Frozen v1 KAT (adversarial).** A deliberately malformed
    /// network blob that v1's `unpack_weights` must reject. This
    /// catches "v1 silently started accepting invalid inputs" drift —
    /// a failure mode benign KATs miss.
    ///
    /// Three adversarial inputs, each must be rejected:
    ///
    /// 1. **Wrong magic.** First 8 bytes are zero instead of `TVMW0001`
    ///    — pack/unpack handshake fails.
    /// 2. **Truncated.** Blob shorter than the minimum length —
    ///    structural parse fails.
    /// 3. **Tampered digest.** Real packed blob with one bit flipped
    ///    in the trailing digest — integrity check fails.
    ///
    /// If any of these *succeed* in parsing, v1's verifier has
    /// silently weakened.
    #[test]
    fn v1_frozen_kat_adversarial_inputs_rejected() {
        use crate::weights::{pack_weights, unpack_weights};

        // (1) Wrong magic — 32 zero bytes, no valid TVMW header.
        let bad_magic = vec![0u8; 64];
        assert!(
            unpack_weights(&bad_magic).is_err(),
            "v1 unpack_weights silently accepted a wrong-magic blob — \
             the frozen v1 verifier has drifted. See ADR-0008."
        );

        // (2) Truncated — 8 bytes is below the minimum 32 + MAGIC=8 = 40.
        let truncated = vec![0u8; 8];
        assert!(
            unpack_weights(&truncated).is_err(),
            "v1 unpack_weights silently accepted a truncated blob"
        );

        // (3) Tampered digest — pack a real network, flip one bit in
        // the trailing 32-byte BLAKE3-256 digest, expect integrity
        // failure.
        let layer = SparseTernaryLayer {
            input_dim: 1,
            output_dim: 1,
            pos_indices: vec![vec![0]],
            neg_indices: vec![vec![]],
            bias: vec![0],
            relu: false,
        };
        let (mut packed, _digest) = pack_weights("kat", 1, 1, &[layer]);
        let last = packed.len() - 1;
        packed[last] ^= 0x01;
        assert!(
            unpack_weights(&packed).is_err(),
            "v1 unpack_weights silently accepted a tampered digest — \
             the frozen v1 integrity check has weakened. See ADR-0008."
        );
    }

    // --- v2 tests (current canonical) ---

    #[test]
    fn v2_same_inputs_produce_same_hash() {
        let net = dummy_net();
        let h1 = v2::trace_hash_v2(&net, &[1, 2, 3], &[6]);
        let h2 = v2::trace_hash_v2(&net, &[1, 2, 3], &[6]);
        assert_eq!(h1, h2);
    }

    #[test]
    fn v2_different_inputs_produce_different_hash() {
        let net = dummy_net();
        let h1 = v2::trace_hash_v2(&net, &[1, 2, 3], &[6]);
        let h2 = v2::trace_hash_v2(&net, &[1, 2, 4], &[6]);
        assert_ne!(h1, h2);
    }

    /// **v1 and v2 produce different digests on identical inputs.**
    /// This is the load-bearing property of the format break: the
    /// 32-byte weights_hash commitment vs the 64-byte one is the only
    /// thing that changes, but it changes the hashed prefix and so
    /// the output digest. A v1 verifier and a v2 verifier disagreeing
    /// on the same `(network, input, output)` is exactly what we want
    /// — it signals "this trace was produced under a different
    /// contract version, you need the matching verifier."
    #[test]
    fn v1_and_v2_disagree_on_identical_inputs() {
        let net = dummy_net();
        let h1 = v1::trace_hash_v1(&net, &[1, 2, 3], &[6]);
        let h2 = v2::trace_hash_v2(&net, &[1, 2, 3], &[6]);
        assert_ne!(
            h1, h2,
            "v1 and v2 trace_hash must produce different digests for \
             the same (input, output) — they commit to different \
             weights_hash widths"
        );
    }

    /// Backwards-compat: the deprecated `trace_hash_ternary` re-export
    /// returns identical bytes to `v1::trace_hash_v1`. This pins the
    /// re-export's behavior so any future "let's silently switch the
    /// re-export to v2" change fails this test loudly.
    #[test]
    #[allow(deprecated)]
    fn deprecated_re_export_is_v1_not_v2() {
        let net = dummy_net();
        let from_re_export = trace_hash_ternary(&net, &[1, 2, 3], &[6]);
        let from_v1 = v1::trace_hash_v1(&net, &[1, 2, 3], &[6]);
        assert_eq!(from_re_export, from_v1);
        let from_v2 = v2::trace_hash_v2(&net, &[1, 2, 3], &[6]);
        assert_ne!(from_re_export, from_v2);
    }
}
