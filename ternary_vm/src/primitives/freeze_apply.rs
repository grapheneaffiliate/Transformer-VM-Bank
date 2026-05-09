//! `freeze_apply` ternary network construction.
//!
//! Mirrors `primitives/freeze_apply.c`'s arithmetic:
//!
//! ```text
//! input:  flag ∈ [0, 1], byte47 ∈ [0, 255]
//! output: new_byte = (byte47 & 127) + (flag ? 128 : 0)
//! ```
//!
//! ## Construction (2 ternary layers)
//!
//! The non-trivial part is `byte47 & 127` (= `byte47 mod 128`). Direct
//! `sum(thermo_byte47[0..128]) − 1` only works when `byte47 < 128`; for
//! `byte47 ≥ 128` it saturates at 127 and we lose the lower bits. The
//! correct ternary identity is:
//!
//! ```text
//! Let h = thermo_byte47[128]   (1 iff byte47 ≥ 128)
//! thermo_low7[k] = thermo_byte47[k+128]
//!                + ReLU(thermo_byte47[k] − thermo_byte47[k+128] − h)
//!
//! For h=0 (byte47 < 128):
//!   thermo_byte47[k+128] = 0; ReLU(thermo_byte47[k] − 0 − 0) = thermo_byte47[k]
//!   ⇒ thermo_low7[k] = thermo_byte47[k]                     (= byte47's thermo)
//!
//! For h=1 (byte47 ≥ 128):
//!   thermo_byte47[k] = 1; ReLU(1 − thermo_byte47[k+128] − 1) = 0
//!   ⇒ thermo_low7[k] = thermo_byte47[k+128]                 (= (byte47-128)'s thermo)
//!
//! Either way thermo_low7 is a length-128 thermometer of LOW7 = byte47 & 127.
//! ```
//!
//! Layer 1 (258 → 257, ReLU):
//! - output[0..128]: ReLU(thermo_byte47[k] − thermo_byte47[k+128] − thermo_byte47[128])  for k ∈ [0,127]
//! - output[128..256]: passthrough thermo_byte47[k]  for k ∈ [128,255] (i.e., layer1_output[k] = thermo_byte47[k])
//! - output[256]: passthrough thermo_flag[1] (= flag)
//!
//! Layer 2 (257 → 2, no ReLU):
//! - output[0] = LOW7 = Σ_{k=0..127} (layer1[k] + layer1[k+128]) − 1
//! - output[1] = flag = layer1[256]
//!
//! All weights ∈ {−1, 0, +1}; all biases ∈ ℤ. Decoder computes
//! `new_byte = LOW7 + 128 · flag` (a pure integer op, deterministic).

use crate::error::TernaryError;
use crate::network::{SparseTernaryLayer, TernaryNetwork};
use crate::thermo;
use crate::weights::{pack_weights, WeightsHeader};

const FLAG_MAX: i64 = 1;
const BYTE_MAX: i64 = 255;

const FLAG_THERMO_LEN: usize = (FLAG_MAX + 1) as usize; // 2
const BYTE_THERMO_LEN: usize = (BYTE_MAX + 1) as usize; // 256
const LOW7_LEN: usize = 128;

pub const INPUT_DIM: usize = FLAG_THERMO_LEN + BYTE_THERMO_LEN; // 258
pub const OUTPUT_DIM: usize = 2;

pub fn build() -> TernaryNetwork {
    let layer1 = build_layer1();
    let layer2 = build_layer2();
    let layers = vec![layer1, layer2];
    let (_, digest) = pack_weights(
        "freeze_apply",
        INPUT_DIM as u32,
        OUTPUT_DIM as u32,
        &layers,
    );
    let header = WeightsHeader {
        version: 1,
        primitive: "freeze_apply".into(),
        input_dim: INPUT_DIM as u32,
        output_dim: OUTPUT_DIM as u32,
        weights_hash: digest,
    };
    TernaryNetwork::new(header, layers)
}

/// Layer 1: 258 → 257.
/// Output positions:
///   [0..128]    — ReLU(t[k] − t[k+128] − t[128])  for k ∈ [0, 127]
///   [128..256]  — passthrough thermo_byte47[k]    for k ∈ [128, 255]
///   [256]       — passthrough flag (thermo_flag[1])
fn build_layer1() -> SparseTernaryLayer {
    let mut pos_indices: Vec<Vec<u32>> = Vec::with_capacity(257);
    let mut neg_indices: Vec<Vec<u32>> = Vec::with_capacity(257);
    let bias = vec![0i64; 257];

    let byte_lo: u32 = FLAG_THERMO_LEN as u32; // input index 2 = thermo_byte47[0]
    let h_idx: u32 = byte_lo + 128; // input index 130 = thermo_byte47[128]

    // [0..128]: ReLU(t[k] − t[k+128] − t[128])
    for k in 0u32..128 {
        let t_k = byte_lo + k;
        let t_k128 = byte_lo + k + 128;
        pos_indices.push(vec![t_k]);
        neg_indices.push(vec![t_k128, h_idx]);
    }
    // [128..256]: passthrough thermo_byte47[k] for k ∈ [128, 255]
    for k in 128u32..256 {
        pos_indices.push(vec![byte_lo + k]);
        neg_indices.push(vec![]);
    }
    // [256]: flag
    pos_indices.push(vec![1u32]);
    neg_indices.push(vec![]);

    SparseTernaryLayer {
        input_dim: INPUT_DIM,
        output_dim: 257,
        pos_indices,
        neg_indices,
        bias,
        relu: true,
    }
}

/// Layer 2: 257 → 2.
/// output[0] = Σ_{k=0..127} (layer1[k] + layer1[k+128]) − 1   (LOW7)
/// output[1] = layer1[256]                                     (flag)
fn build_layer2() -> SparseTernaryLayer {
    // LOW7 sum: 128 ReLU outputs at indices [0..128] AND 128 thermo
    // passthrough outputs at [128..256]. Total 256 +1 indices in pos.
    let mut low7_pos: Vec<u32> = (0..128u32).collect();
    low7_pos.extend(128u32..256u32);
    SparseTernaryLayer {
        input_dim: 257,
        output_dim: 2,
        pos_indices: vec![low7_pos, vec![256u32]],
        neg_indices: vec![vec![], vec![]],
        bias: vec![-1, 0],
        relu: false,
    }
}

pub fn encode_input(flag: u8, byte47: u8) -> Result<Vec<i64>, TernaryError> {
    if flag > 1 {
        return Err(TernaryError::InputRange {
            primitive: "freeze_apply",
            value: flag as i64,
            max: 1,
        });
    }
    let mut v = Vec::with_capacity(INPUT_DIM);
    v.extend(thermo::encode(flag as i64, FLAG_MAX));
    v.extend(thermo::encode(byte47 as i64, BYTE_MAX));
    Ok(v)
}

/// Decode `[LOW7, flag]` → `new_byte = LOW7 + 128 · flag`.
pub fn decode_output(out: &[i64]) -> Result<u8, TernaryError> {
    if out.len() != OUTPUT_DIM {
        return Err(TernaryError::OutputDecode(format!(
            "freeze_apply expected {OUTPUT_DIM} outputs, got {}",
            out.len()
        )));
    }
    let low7 = out[0];
    let flag = out[1];
    if !(0..=127).contains(&low7) {
        return Err(TernaryError::OutputDecode(format!("LOW7 out of range: {low7}")));
    }
    if !(0..=1).contains(&flag) {
        return Err(TernaryError::OutputDecode(format!("flag out of range: {flag}")));
    }
    Ok((low7 + 128 * flag) as u8)
}

pub fn run(net: &TernaryNetwork, flag: u8, byte47: u8) -> Result<u8, TernaryError> {
    let input = encode_input(flag, byte47)?;
    let output = net.forward(&input)?;
    decode_output(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground_truth(flag: u8, byte47: u8) -> u8 {
        let low7 = byte47 & 0x7f;
        if flag != 0 {
            low7 | 0x80
        } else {
            low7
        }
    }

    #[test]
    fn smoke_basic() {
        let n = build();
        // flag=0: clear top bit
        assert_eq!(run(&n, 0, 0).unwrap(), 0);
        assert_eq!(run(&n, 0, 0xff).unwrap(), 0x7f);
        assert_eq!(run(&n, 0, 0x42).unwrap(), 0x42);
        // flag=1: set top bit, keep low 7
        assert_eq!(run(&n, 1, 0).unwrap(), 0x80);
        assert_eq!(run(&n, 1, 0xff).unwrap(), 0xff);
        assert_eq!(run(&n, 1, 0x42).unwrap(), 0xc2);
        // boundary: byte47 = 128 (LOW7 should be 0)
        assert_eq!(run(&n, 0, 128).unwrap(), 0);
        assert_eq!(run(&n, 1, 128).unwrap(), 0x80);
        // mid-high byte47 = 200 (LOW7 = 72)
        assert_eq!(run(&n, 0, 200).unwrap(), 72);
        assert_eq!(run(&n, 1, 200).unwrap(), 200);
    }

    #[test]
    fn random_witnesses_match_arithmetic() {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(98765);
        let n = build();
        for _ in 0..1000 {
            let flag: u8 = rng.gen_range(0..=1);
            let byte47: u8 = rng.gen();
            assert_eq!(
                run(&n, flag, byte47).unwrap(),
                ground_truth(flag, byte47),
                "(flag={flag}, byte47={byte47})"
            );
        }
    }
}
