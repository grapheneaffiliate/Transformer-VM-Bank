//! `freeze_setup` ternary network — extract `(flag, byte[47])` from a
//! 65-byte witness `(flag, b0, b1, …, b63)`.
//!
//! Mirrors `primitives/freeze_setup.c`. The original C primitive
//! parses an ASCII-decimal text input, but the ternary engine
//! operates on raw bytes directly — the "parsing-heavy work" the
//! transformer trace had to do (skip 47 fields, parse a decimal
//! number) collapses to a positional index here.
//!
//! ## Construction (single ternary layer)
//!
//! ```text
//! INPUT  = thermo(flag, 1) ‖ thermo(b0, 255) ‖ … ‖ thermo(b63, 255)
//!          length 2 + 64·256 = 16386
//!
//! Layer 1 (16386 → 2, no ReLU):
//!   output[0] = flag       = thermo_flag[1]
//!   output[1] = byte47     = sum(thermo_b47[0..256]) − 1
//! ```
//!
//! Per the strategic plan, the "small ternary attention block" for
//! variable-position gather is unnecessary because byte 47's
//! position is fixed. The construction is a positional projection.

use crate::error::TernaryError;
use crate::network::{SparseTernaryLayer, TernaryNetwork};
use crate::thermo;
use crate::weights::{pack_weights_dual, WeightsHeader};

const FLAG_THERMO_LEN: usize = 2;
const BYTE_THERMO_LEN: usize = 256;
const N_BYTES: usize = 64;
const TARGET_BYTE: usize = 47;

pub const INPUT_DIM: usize = FLAG_THERMO_LEN + N_BYTES * BYTE_THERMO_LEN; // 16386
pub const OUTPUT_DIM: usize = 2;

pub fn build() -> TernaryNetwork {
    let layer1 = build_layer1();
    let layers = vec![layer1];
    let (_, digest, digest_v2) =
        pack_weights_dual("freeze_setup", INPUT_DIM as u32, OUTPUT_DIM as u32, &layers);
    let header = WeightsHeader {
        version: 1,
        primitive: "freeze_setup".into(),
        input_dim: INPUT_DIM as u32,
        output_dim: OUTPUT_DIM as u32,
        weights_hash: digest,
        weights_hash_v2: digest_v2,
    };
    TernaryNetwork::new(header, layers)
}

fn build_layer1() -> SparseTernaryLayer {
    let byte_lo = (FLAG_THERMO_LEN + TARGET_BYTE * BYTE_THERMO_LEN) as u32;
    let byte_hi = byte_lo + BYTE_THERMO_LEN as u32;
    let pos_byte47: Vec<u32> = (byte_lo..byte_hi).collect();
    SparseTernaryLayer {
        input_dim: INPUT_DIM,
        output_dim: 2,
        pos_indices: vec![vec![1u32], pos_byte47],
        neg_indices: vec![vec![], vec![]],
        bias: vec![0, -1],
        relu: false,
    }
}

pub fn encode_input(witness: &[u8; 65]) -> Result<Vec<i64>, TernaryError> {
    let flag = witness[0];
    if flag > 1 {
        return Err(TernaryError::InputRange {
            primitive: "freeze_setup",
            value: flag as i64,
            max: 1,
        });
    }
    let mut v = Vec::with_capacity(INPUT_DIM);
    v.extend(thermo::encode(flag as i64, 1));
    for i in 0..N_BYTES {
        v.extend(thermo::encode(witness[1 + i] as i64, 255));
    }
    Ok(v)
}

pub fn decode_output(out: &[i64]) -> Result<(u8, u8), TernaryError> {
    if out.len() != OUTPUT_DIM {
        return Err(TernaryError::OutputDecode(format!(
            "freeze_setup expected {OUTPUT_DIM} outputs, got {}",
            out.len()
        )));
    }
    let flag = out[0];
    let byte47 = out[1];
    if !(0..=1).contains(&flag) {
        return Err(TernaryError::OutputDecode(format!(
            "flag out of range: {flag}"
        )));
    }
    if !(0..=255).contains(&byte47) {
        return Err(TernaryError::OutputDecode(format!(
            "byte47 out of range: {byte47}"
        )));
    }
    Ok((flag as u8, byte47 as u8))
}

pub fn run(net: &TernaryNetwork, witness: &[u8; 65]) -> Result<(u8, u8), TernaryError> {
    let input = encode_input(witness)?;
    let output = net.forward(&input)?;
    decode_output(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground_truth(witness: &[u8; 65]) -> (u8, u8) {
        let flag = witness[0];
        let byte47 = witness[1 + 47];
        (flag, byte47)
    }

    #[test]
    fn smoke_basic() {
        let n = build();
        let mut w = [0u8; 65];
        w[0] = 1; // flag
        w[1 + 47] = 0xab; // byte47
        assert_eq!(run(&n, &w).unwrap(), (1, 0xab));

        let mut w = [0u8; 65];
        w[0] = 0;
        w[1 + 47] = 0;
        assert_eq!(run(&n, &w).unwrap(), (0, 0));

        let mut w = [0u8; 65];
        w[0] = 0;
        w[1 + 47] = 0xff;
        assert_eq!(run(&n, &w).unwrap(), (0, 0xff));
    }

    #[test]
    fn random_witnesses_match_arithmetic() {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(44444);
        let n = build();
        for _ in 0..1000 {
            let mut w = [0u8; 65];
            w[0] = rng.gen_range(0..=1);
            for i in 0..64 {
                w[1 + i] = rng.gen();
            }
            assert_eq!(run(&n, &w).unwrap(), ground_truth(&w));
        }
    }
}
