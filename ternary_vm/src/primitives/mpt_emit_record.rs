//! `mpt_emit_record` ternary network — 64-byte pass-through.
//!
//! Mirrors `primitives/mpt_emit_record.c`: emits the input record
//! verbatim. The "structured emit" framing in the strategic plan
//! collapses to byte-by-byte identity here because the
//! analytical-construction model already guarantees positional order.
//!
//! ## Construction (single ternary layer)
//!
//! ```text
//! INPUT = thermo(byte[0], 255) ‖ thermo(byte[1], 255) ‖ … ‖ thermo(byte[63], 255)
//!         length 64 · 256 = 16384
//!
//! Layer 1 (16384 → 64, no ReLU):
//!   output[i] = sum(thermo[i*256 .. (i+1)*256]) − 1 = byte[i]
//! ```
//!
//! All weights are +1 (one per input position, never −1); biases are
//! all −1. Strictly ternary.

use crate::error::TernaryError;
use crate::network::{SparseTernaryLayer, TernaryNetwork};
use crate::thermo;
use crate::weights::{pack_weights, WeightsHeader};

const RECORD_LEN: usize = 64;
const BYTE_THERMO_LEN: usize = 256;

pub const INPUT_DIM: usize = RECORD_LEN * BYTE_THERMO_LEN; // 16384
pub const OUTPUT_DIM: usize = RECORD_LEN;

pub fn build() -> TernaryNetwork {
    let layer1 = build_layer1();
    let layers = vec![layer1];
    let (_, digest) = pack_weights(
        "mpt_emit_record",
        INPUT_DIM as u32,
        OUTPUT_DIM as u32,
        &layers,
    );
    let header = WeightsHeader {
        version: 1,
        primitive: "mpt_emit_record".into(),
        input_dim: INPUT_DIM as u32,
        output_dim: OUTPUT_DIM as u32,
        weights_hash: digest,
    };
    TernaryNetwork::new(header, layers)
}

fn build_layer1() -> SparseTernaryLayer {
    let mut pos_indices = Vec::with_capacity(RECORD_LEN);
    let mut neg_indices = Vec::with_capacity(RECORD_LEN);
    let bias = vec![-1i64; RECORD_LEN];
    for i in 0..RECORD_LEN as u32 {
        let lo = i * BYTE_THERMO_LEN as u32;
        let hi = lo + BYTE_THERMO_LEN as u32;
        pos_indices.push((lo..hi).collect());
        neg_indices.push(vec![]);
    }
    SparseTernaryLayer {
        input_dim: INPUT_DIM,
        output_dim: OUTPUT_DIM,
        pos_indices,
        neg_indices,
        bias,
        relu: false,
    }
}

pub fn encode_input(record: &[u8; RECORD_LEN]) -> Vec<i64> {
    let mut v = Vec::with_capacity(INPUT_DIM);
    for &b in record {
        v.extend(thermo::encode(b as i64, 255));
    }
    v
}

pub fn decode_output(out: &[i64]) -> Result<[u8; RECORD_LEN], TernaryError> {
    if out.len() != OUTPUT_DIM {
        return Err(TernaryError::OutputDecode(format!(
            "mpt_emit expected {OUTPUT_DIM} outputs, got {}",
            out.len()
        )));
    }
    let mut record = [0u8; RECORD_LEN];
    for i in 0..RECORD_LEN {
        let b = out[i];
        if !(0..=255).contains(&b) {
            return Err(TernaryError::OutputDecode(format!(
                "mpt_emit byte[{i}] out of range: {b}"
            )));
        }
        record[i] = b as u8;
    }
    Ok(record)
}

pub fn run(
    net: &TernaryNetwork,
    record: &[u8; RECORD_LEN],
) -> Result<[u8; RECORD_LEN], TernaryError> {
    let input = encode_input(record);
    let output = net.forward(&input)?;
    decode_output(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_zero_record() {
        let n = build();
        let r = [0u8; 64];
        assert_eq!(run(&n, &r).unwrap(), r);
    }

    #[test]
    fn smoke_max_record() {
        let n = build();
        let r = [0xffu8; 64];
        assert_eq!(run(&n, &r).unwrap(), r);
    }

    #[test]
    fn smoke_pattern() {
        let n = build();
        let mut r = [0u8; 64];
        for i in 0..64 {
            r[i] = (i * 7 + 13) as u8;
        }
        assert_eq!(run(&n, &r).unwrap(), r);
    }

    #[test]
    fn random_witnesses_pass_through() {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(33333);
        let n = build();
        for _ in 0..1000 {
            let mut r = [0u8; 64];
            for slot in r.iter_mut() {
                *slot = rng.gen();
            }
            assert_eq!(run(&n, &r).unwrap(), r);
        }
    }
}
