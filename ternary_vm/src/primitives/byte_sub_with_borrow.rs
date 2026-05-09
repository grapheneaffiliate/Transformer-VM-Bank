//! `byte_sub_with_borrow` ternary network construction.
//!
//! Mirrors `primitives/byte_sub_with_borrow.c`'s arithmetic:
//!
//! ```text
//! input:  m ∈ [0, 255], s ∈ [0, 255], b_in ∈ [0, 1]
//! signed: d_signed = m - s - b_in ∈ [-256, 255]
//! output: diff_byte  = d_signed mod 256
//!         borrow_out = 1 iff d_signed < 0
//! ```
//!
//! Construction (4 ternary layers, same pattern as byte_add):
//!
//! ```text
//! THERMO_INPUT  = thermo(m, 255) ‖ thermo(s, 255) ‖ thermo(b_in, 1)   length 514
//!     │
//!     ▼ Layer 1 (514 → 1, no ReLU): +1 weights for thermo(m), −1 for thermo(s)
//!                                  and thermo(b_in), bias +257
//! TOTAL_SHIFTED = (m+1) − (s+1) − (b_in+1) + 257 = m − s − b_in + 256
//!                                                  ∈ [0, 511]
//!     │
//!     ▼ Layer 2 (1 → 1026, ReLU): parallel ReLU(t − i + 1) and ReLU(t − i)
//! PARALLEL_RELU                                                 for i ∈ [0, 512]
//!     │
//!     ▼ Layer 3 (1026 → 513, no ReLU): pair-subtract → thermo of TOTAL_SHIFTED
//! THERMO_T   thermo_t[i] = 1 iff TOTAL_SHIFTED ≥ i,   length 513
//!     │
//!     ▼ Layer 4 (513 → 257, no ReLU):
//! OUTPUT      diff_byte one-hot at indices 0..256, borrow_out at index 256
//! ```
//!
//! Layer 4 details:
//! - `one_hot_diff_byte[k]` for `k ∈ [0, 255]` = `(t[k] − t[k+1]) + (t[k+256] − t[k+257])`
//!   — same shape as byte_add's sum-byte projection because
//!   `diff_byte == k iff TOTAL_SHIFTED == k OR TOTAL_SHIFTED == k+256`.
//! - `borrow_out = 1 − thermo_t[256]`. In ternary form: weight −1 at
//!   `thermo_t[256]`, bias +1.

use crate::error::TernaryError;
use crate::network::{argmax, SparseTernaryLayer, TernaryNetwork};
use crate::thermo;
use crate::weights::{pack_weights, WeightsHeader};

const M_MAX: i64 = 255;
const S_MAX: i64 = 255;
const B_MAX: i64 = 1;
const TOTAL_SHIFTED_MAX: i64 = M_MAX + (S_MAX + 1) + (B_MAX + 1) - 1; // 511
const THERMO_LEN: usize = (TOTAL_SHIFTED_MAX + 1) as usize + 1; // 513

pub const INPUT_DIM: usize = (M_MAX + 1) as usize + (S_MAX + 1) as usize + (B_MAX + 1) as usize;
pub const OUTPUT_DIM: usize = 256 + 1;

pub fn build() -> TernaryNetwork {
    let layer1 = build_layer1_total_shifted();
    let layer2 = build_layer2_parallel_relu();
    let layer3 = build_layer3_thermo_decode();
    let layer4 = build_layer4_projection();
    let layers = vec![layer1, layer2, layer3, layer4];
    let (_, digest) = pack_weights(
        "byte_sub_with_borrow",
        INPUT_DIM as u32,
        OUTPUT_DIM as u32,
        &layers,
    );
    let header = WeightsHeader {
        version: 1,
        primitive: "byte_sub_with_borrow".into(),
        input_dim: INPUT_DIM as u32,
        output_dim: OUTPUT_DIM as u32,
        weights_hash: digest,
    };
    TernaryNetwork::new(header, layers)
}

/// Layer 1: 514 → 1.
/// Weights: +1 for the 256 thermo(m) inputs (indices 0..256),
///          −1 for the 256 thermo(s) inputs (indices 256..512),
///          −1 for the 2 thermo(b_in) inputs (indices 512..514).
/// Bias: +257.
/// Output = (m+1) − (s+1) − (b_in+1) + 257 = m − s − b_in + 256.
fn build_layer1_total_shifted() -> SparseTernaryLayer {
    let m_lo = 0u32;
    let m_hi = (M_MAX + 1) as u32; // 256
    let s_lo = m_hi; // 256
    let s_hi = s_lo + (S_MAX + 1) as u32; // 512
    let b_lo = s_hi; // 512
    let b_hi = b_lo + (B_MAX + 1) as u32; // 514
    let pos: Vec<u32> = (m_lo..m_hi).collect();
    let mut neg: Vec<u32> = (s_lo..s_hi).collect();
    neg.extend(b_lo..b_hi);
    SparseTernaryLayer {
        input_dim: INPUT_DIM,
        output_dim: 1,
        pos_indices: vec![pos],
        neg_indices: vec![neg],
        bias: vec![257],
        relu: false,
    }
}

/// Layer 2: 1 → 1026 (same pattern as byte_add).
fn build_layer2_parallel_relu() -> SparseTernaryLayer {
    let mut pos_indices = Vec::with_capacity(2 * THERMO_LEN);
    let mut neg_indices = Vec::with_capacity(2 * THERMO_LEN);
    let mut bias = Vec::with_capacity(2 * THERMO_LEN);
    for i in 0..THERMO_LEN as i64 {
        pos_indices.push(vec![0u32]);
        neg_indices.push(vec![]);
        bias.push(1 - i);
        pos_indices.push(vec![0u32]);
        neg_indices.push(vec![]);
        bias.push(-i);
    }
    SparseTernaryLayer {
        input_dim: 1,
        output_dim: 2 * THERMO_LEN,
        pos_indices,
        neg_indices,
        bias,
        relu: true,
    }
}

/// Layer 3: 1026 → 513.
fn build_layer3_thermo_decode() -> SparseTernaryLayer {
    let mut pos_indices = Vec::with_capacity(THERMO_LEN);
    let mut neg_indices = Vec::with_capacity(THERMO_LEN);
    let bias = vec![0i64; THERMO_LEN];
    for i in 0..THERMO_LEN {
        pos_indices.push(vec![(2 * i) as u32]);
        neg_indices.push(vec![(2 * i + 1) as u32]);
    }
    SparseTernaryLayer {
        input_dim: 2 * THERMO_LEN,
        output_dim: THERMO_LEN,
        pos_indices,
        neg_indices,
        bias,
        relu: false,
    }
}

/// Layer 4: 513 → 257.
/// - `diff_byte` one-hot[k]: `(t[k] − t[k+1]) + (t[k+256] − t[k+257])`
/// - `borrow_out` = `1 − t[256]` (bias +1, weight −1 at column 256).
fn build_layer4_projection() -> SparseTernaryLayer {
    let mut pos_indices = Vec::with_capacity(OUTPUT_DIM);
    let mut neg_indices = Vec::with_capacity(OUTPUT_DIM);
    let mut bias = vec![0i64; OUTPUT_DIM];
    for k in 0..256u32 {
        pos_indices.push(vec![k, k + 256]);
        neg_indices.push(vec![k + 1, k + 257]);
    }
    // borrow_out
    pos_indices.push(vec![]);
    neg_indices.push(vec![256u32]);
    bias[256] = 1;
    SparseTernaryLayer {
        input_dim: THERMO_LEN,
        output_dim: OUTPUT_DIM,
        pos_indices,
        neg_indices,
        bias,
        relu: false,
    }
}

pub fn encode_input(m: u8, s: u8, b: u8) -> Result<Vec<i64>, TernaryError> {
    if b > 1 {
        return Err(TernaryError::InputRange {
            primitive: "byte_sub_with_borrow",
            value: b as i64,
            max: 1,
        });
    }
    let mut v = Vec::with_capacity(INPUT_DIM);
    v.extend(thermo::encode(m as i64, M_MAX));
    v.extend(thermo::encode(s as i64, S_MAX));
    v.extend(thermo::encode(b as i64, B_MAX));
    Ok(v)
}

pub fn decode_output(out: &[i64]) -> Result<(u8, u8), TernaryError> {
    if out.len() != OUTPUT_DIM {
        return Err(TernaryError::OutputDecode(format!(
            "byte_sub expected {OUTPUT_DIM} outputs, got {}",
            out.len()
        )));
    }
    let diff_byte = argmax(&out[..256])?;
    let borrow = out[256];
    if !(0..=1).contains(&borrow) {
        return Err(TernaryError::OutputDecode(format!(
            "byte_sub borrow_out must be 0 or 1, got {borrow}"
        )));
    }
    Ok((diff_byte as u8, borrow as u8))
}

pub fn run(net: &TernaryNetwork, m: u8, s: u8, b: u8) -> Result<(u8, u8), TernaryError> {
    let input = encode_input(m, s, b)?;
    let output = net.forward(&input)?;
    decode_output(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground_truth(m: u8, s: u8, b: u8) -> (u8, u8) {
        let d = m as i32 - s as i32 - b as i32;
        if d < 0 {
            ((d + 256) as u8, 1)
        } else {
            (d as u8, 0)
        }
    }

    #[test]
    fn smoke_no_borrow() {
        let n = build();
        // 5 - 2 - 0 = 3, no borrow
        assert_eq!(run(&n, 5, 2, 0).unwrap(), (3, 0));
        // 5 - 5 - 0 = 0, no borrow
        assert_eq!(run(&n, 5, 5, 0).unwrap(), (0, 0));
        // 255 - 0 - 0 = 255, no borrow
        assert_eq!(run(&n, 255, 0, 0).unwrap(), (255, 0));
    }

    #[test]
    fn smoke_borrow_boundary() {
        let n = build();
        // 0 - 0 - 1 = -1 → diff_byte = 255, borrow = 1
        assert_eq!(run(&n, 0, 0, 1).unwrap(), (255, 1));
        // 0 - 1 - 0 = -1 → 255, borrow = 1
        assert_eq!(run(&n, 0, 1, 0).unwrap(), (255, 1));
        // 0 - 255 - 1 = -256 → diff_byte = 0, borrow = 1
        assert_eq!(run(&n, 0, 255, 1).unwrap(), (0, 1));
    }

    #[test]
    fn random_witnesses_match_arithmetic() {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(54321);
        let n = build();
        for _ in 0..1000 {
            let m: u8 = rng.gen();
            let s: u8 = rng.gen();
            let b: u8 = rng.gen_range(0..=1);
            let got = run(&n, m, s, b).unwrap();
            assert_eq!(got, ground_truth(m, s, b), "(m={m}, s={s}, b={b})");
        }
    }
}
