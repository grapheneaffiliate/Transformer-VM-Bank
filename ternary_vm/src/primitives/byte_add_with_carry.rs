//! `byte_add_with_carry` ternary network construction.
//!
//! Mirrors `primitives/byte_add_with_carry.c`'s arithmetic:
//!
//! ```text
//! input:  a ∈ [0, 255], b ∈ [0, 255], c_in ∈ [0, 1]
//! output: sum_byte = (a + b + c_in) mod 256
//!         carry_out = (a + b + c_in) ≥ 256
//! ```
//!
//! Construction (4 ternary layers, single-shot, no autoregressive loop):
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │ encode_input(a, b, c)                                        │
//! │   → thermo(a, 255) ‖ thermo(b, 255) ‖ thermo(c, 1)           │  length 256+256+2 = 514
//! └──────────────────────────────────────────────────────────────┘
//!     │
//!     ▼ Layer 1 (514 → 1, no ReLU): all-ones weights, bias −3
//! TOTAL = a + b + c_in   ∈ [0, 511]
//!     │
//!     ▼ Layer 2 (1 → 1026, ReLU): parallel ReLU(TOTAL − i + 1) and ReLU(TOTAL − i)
//! PARALLEL_RELU                                                          for i ∈ [0, 512]
//!     │
//!     ▼ Layer 3 (1026 → 513, no ReLU): subtract paired ReLU outputs
//! THERMO_T   thermo_t[i] = 1 iff TOTAL ≥ i,   length 513
//!     │
//!     ▼ Layer 4 (513 → 257, no ReLU):
//! OUTPUT      one_hot_sum_byte[0..256], carry_out at index 256
//! ```
//!
//! Layer 4 details — one_hot_sum_byte[k] has +1 at thermo_t[k] and
//! thermo_t[k+256], -1 at thermo_t[k+1] and thermo_t[k+257]. carry_out
//! is just thermo_t[256] copied through. All weights ∈ {−1, 0, +1};
//! all biases ∈ ℤ; activations ∈ ℤ.
//!
//! The decoder takes the network output, argmaxes indices 0..256 to get
//! `sum_byte`, and reads index 256 as `carry_out`. Both are bytes by
//! construction.
//!
//! ## Bit-exact properties
//!
//! - Same input → same output on any conformant integer-arithmetic host.
//! - No fp anywhere.
//! - Total weight slots ≈ 514 + 1026 + 1026 + 1024 = 3590, all ternary.
//!   Sparsity: layer 1 is 100% nonzero; layers 2-4 each have ≤ 4
//!   nonzeros per row → ~99% sparse overall, matching the PoC numbers.

use crate::error::TernaryError;
use crate::network::{argmax, SparseTernaryLayer, TernaryNetwork};
use crate::thermo;
use crate::weights::{pack_weights, WeightsHeader};

const A_MAX: i64 = 255;
const B_MAX: i64 = 255;
const C_MAX: i64 = 1;
const TOTAL_MAX: i64 = A_MAX + B_MAX + C_MAX; // 511
const THERMO_LEN: usize = (TOTAL_MAX + 1) as usize + 1; // 513 (positions 0..512 inclusive)

/// `INPUT_DIM` for the network: 256 + 256 + 2 = 514.
pub const INPUT_DIM: usize = (A_MAX + 1) as usize + (B_MAX + 1) as usize + (C_MAX + 1) as usize;

/// `OUTPUT_DIM` for the network: 256 (sum_byte one-hot) + 1 (carry_out).
pub const OUTPUT_DIM: usize = 256 + 1;

/// Build the ternary network for `byte_add_with_carry`. The
/// construction is deterministic — calling this twice produces
/// bit-identical weights.
pub fn build() -> TernaryNetwork {
    let layer1 = build_layer1_total();
    let layer2 = build_layer2_parallel_relu();
    let layer3 = build_layer3_thermo_decode();
    let layer4 = build_layer4_projection();
    let layers = vec![layer1, layer2, layer3, layer4];

    // Compute weights_hash by serializing through pack_weights.
    let (_, digest) = pack_weights(
        "byte_add_with_carry",
        INPUT_DIM as u32,
        OUTPUT_DIM as u32,
        &layers,
    );
    let header = WeightsHeader {
        version: 1,
        primitive: "byte_add_with_carry".into(),
        input_dim: INPUT_DIM as u32,
        output_dim: OUTPUT_DIM as u32,
        weights_hash: digest,
    };
    TernaryNetwork::new(header, layers)
}

/// Layer 1: 514 → 1. All weights +1, bias −3.
/// Output = sum(thermo(a)) + sum(thermo(b)) + sum(thermo(c)) − 3
///        = (a+1) + (b+1) + (c+1) − 3 = a + b + c.
fn build_layer1_total() -> SparseTernaryLayer {
    let pos: Vec<u32> = (0..INPUT_DIM as u32).collect();
    SparseTernaryLayer {
        input_dim: INPUT_DIM,
        output_dim: 1,
        pos_indices: vec![pos],
        neg_indices: vec![vec![]],
        bias: vec![-3],
        relu: false,
    }
}

/// Layer 2: 1 → 1026. For each i ∈ [0, 512]:
///   output[2i]   = ReLU(TOTAL − i + 1)
///   output[2i+1] = ReLU(TOTAL − i)
fn build_layer2_parallel_relu() -> SparseTernaryLayer {
    let mut pos_indices = Vec::with_capacity(2 * THERMO_LEN);
    let mut neg_indices = Vec::with_capacity(2 * THERMO_LEN);
    let mut bias = Vec::with_capacity(2 * THERMO_LEN);
    for i in 0..THERMO_LEN as i64 {
        // parallel_pos_i: weight +1 from input 0, bias = -(i - 1) = 1 - i
        pos_indices.push(vec![0u32]);
        neg_indices.push(vec![]);
        bias.push(1 - i);
        // parallel_neg_i: weight +1 from input 0, bias = -i
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

/// Layer 3: 1026 → 513. thermo_t[i] = parallel_pos_i − parallel_neg_i.
/// Result: thermo_t[i] = 1 iff TOTAL ≥ i, else 0.
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

/// Layer 4: 513 → 257. one_hot_sum_byte[k] = (t[k]−t[k+1]) + (t[k+256]−t[k+257])
/// for k ∈ [0, 255]. carry_out = t[256] (index 256 of output).
fn build_layer4_projection() -> SparseTernaryLayer {
    let mut pos_indices = Vec::with_capacity(OUTPUT_DIM);
    let mut neg_indices = Vec::with_capacity(OUTPUT_DIM);
    let bias = vec![0i64; OUTPUT_DIM];
    for k in 0..256u32 {
        // one_hot_sum_byte[k]
        pos_indices.push(vec![k, k + 256]);
        neg_indices.push(vec![k + 1, k + 257]);
    }
    // carry_out = thermo_t[256]
    pos_indices.push(vec![256u32]);
    neg_indices.push(vec![]);
    SparseTernaryLayer {
        input_dim: THERMO_LEN,
        output_dim: OUTPUT_DIM,
        pos_indices,
        neg_indices,
        bias,
        relu: false,
    }
}

/// Encode `(a, b, c)` as the network's THERMO_INPUT vector (length 514).
pub fn encode_input(a: u8, b: u8, c: u8) -> Result<Vec<i64>, TernaryError> {
    if c > 1 {
        return Err(TernaryError::InputRange {
            primitive: "byte_add_with_carry",
            value: c as i64,
            max: 1,
        });
    }
    let mut v = Vec::with_capacity(INPUT_DIM);
    v.extend(thermo::encode(a as i64, A_MAX));
    v.extend(thermo::encode(b as i64, B_MAX));
    v.extend(thermo::encode(c as i64, C_MAX));
    Ok(v)
}

/// Decode the network output `[one_hot_sum_byte (256), carry_out (1)]`
/// into `(sum_byte, carry_out)`.
pub fn decode_output(out: &[i64]) -> Result<(u8, u8), TernaryError> {
    if out.len() != OUTPUT_DIM {
        return Err(TernaryError::OutputDecode(format!(
            "byte_add expected {OUTPUT_DIM} outputs, got {}",
            out.len()
        )));
    }
    let sum_byte = argmax(&out[..256])?;
    let carry = out[256];
    if !(0..=1).contains(&carry) {
        return Err(TernaryError::OutputDecode(format!(
            "byte_add carry_out must be 0 or 1, got {carry}"
        )));
    }
    Ok((sum_byte as u8, carry as u8))
}

/// One-shot convenience: build the network once if needed, then run
/// `(a, b, c_in) → (sum_byte, carry_out)` on it.
pub fn run(net: &TernaryNetwork, a: u8, b: u8, c: u8) -> Result<(u8, u8), TernaryError> {
    let input = encode_input(a, b, c)?;
    let output = net.forward(&input)?;
    decode_output(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground_truth(a: u8, b: u8, c: u8) -> (u8, u8) {
        let s = a as u16 + b as u16 + c as u16;
        ((s & 0xff) as u8, (s >> 8) as u8)
    }

    #[test]
    fn build_produces_deterministic_weights() {
        let n1 = build();
        let n2 = build();
        assert_eq!(n1.header.weights_hash, n2.header.weights_hash);
    }

    #[test]
    fn weights_hash_is_nonzero() {
        let n = build();
        assert_ne!(n.header.weights_hash, [0u8; 32]);
    }

    #[test]
    fn shape_matches_constants() {
        let n = build();
        assert_eq!(n.layers.len(), 4);
        assert_eq!(n.layers[0].input_dim, INPUT_DIM);
        assert_eq!(n.layers.last().unwrap().output_dim, OUTPUT_DIM);
    }

    #[test]
    fn smoke_zero_zero_zero() {
        let n = build();
        let (s, c) = run(&n, 0, 0, 0).unwrap();
        assert_eq!((s, c), (0, 0));
    }

    #[test]
    fn smoke_max_max_one() {
        // 255 + 255 + 1 = 511 → sum_byte = 255, carry = 1
        let n = build();
        let (s, c) = run(&n, 255, 255, 1).unwrap();
        assert_eq!((s, c), (255, 1));
    }

    #[test]
    fn smoke_carry_boundary() {
        // 255 + 1 + 0 = 256 → sum_byte = 0, carry = 1
        let n = build();
        assert_eq!(run(&n, 255, 1, 0).unwrap(), (0, 1));
        // 255 + 0 + 0 = 255 → sum_byte = 255, carry = 0
        assert_eq!(run(&n, 255, 0, 0).unwrap(), (255, 0));
        // 128 + 128 + 0 = 256 → sum_byte = 0, carry = 1
        assert_eq!(run(&n, 128, 128, 0).unwrap(), (0, 1));
    }

    #[test]
    fn random_witnesses_match_arithmetic() {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(12345);
        let n = build();
        for _ in 0..1000 {
            let a: u8 = rng.gen();
            let b: u8 = rng.gen();
            let c: u8 = rng.gen_range(0..=1);
            let got = run(&n, a, b, c).unwrap();
            assert_eq!(got, ground_truth(a, b, c), "(a={a}, b={b}, c={c})");
        }
    }
}
