//! Trace-hash contract for ternary primitives — `docs/ARCHITECTURE.md` § 0.8.
//!
//! ```text
//! trace_hash_ternary(P, x) := BLAKE3(
//!     weights_hash(P)
//!  || canonical_input_encoding(x)
//!  || canonical_output_encoding(y)
//! )
//! ```
//!
//! `y` is the output of the ternary forward pass on `x` for primitive
//! `P`. `weights_hash(P)` is the BLAKE3 over the canonical packed weight
//! payload (see `weights::pack_weights`).
//!
//! Canonical encodings are 4-byte big-endian length prefix + raw bytes
//! (one i64 = 8 BE bytes). This keeps the contract independent of the
//! host's word size and endianness.
//!
//! Because integer addition is associative and the forward pass uses no
//! fp, `trace_hash_ternary(P, x)` is bit-identical across any conformant
//! verifier (x86_64, aarch64, FPGA, secure enclave, …). This is what
//! removes the canonical-engine pin that gate-8 had to maintain.

use crate::network::TernaryNetwork;

/// Canonical input/output encoding: 4-byte BE length, then 8-byte BE per i64.
pub fn encode_canonical(values: &[i64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 8 * values.len());
    out.extend_from_slice(&(values.len() as u32).to_be_bytes());
    for &v in values {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out
}

/// `trace_hash_ternary(P, x, y)` — caller supplies x (input) and y
/// (output). The trace_hash also commits to the network's `weights_hash`
/// via the header.
pub fn trace_hash_ternary(net: &TernaryNetwork, input: &[i64], output: &[i64]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(&net.header.weights_hash);
    h.update(&encode_canonical(input));
    h.update(&encode_canonical(output));
    let mut out = [0u8; 32];
    out.copy_from_slice(h.finalize().as_bytes());
    out
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

    #[test]
    fn same_inputs_produce_same_hash() {
        let net = dummy_net();
        let h1 = trace_hash_ternary(&net, &[1, 2, 3], &[6]);
        let h2 = trace_hash_ternary(&net, &[1, 2, 3], &[6]);
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_inputs_produce_different_hash() {
        let net = dummy_net();
        let h1 = trace_hash_ternary(&net, &[1, 2, 3], &[6]);
        let h2 = trace_hash_ternary(&net, &[1, 2, 4], &[6]);
        assert_ne!(h1, h2);
    }

    #[test]
    fn different_outputs_produce_different_hash() {
        let net = dummy_net();
        let h1 = trace_hash_ternary(&net, &[1, 2, 3], &[6]);
        let h2 = trace_hash_ternary(&net, &[1, 2, 3], &[7]);
        assert_ne!(h1, h2);
    }
}
