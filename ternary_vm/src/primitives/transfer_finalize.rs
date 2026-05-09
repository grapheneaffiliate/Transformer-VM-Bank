//! `transfer_finalize` ternary program — u64 little-endian nonce → nonce + 1 (mod 2^64).
//!
//! Mirrors `primitives/transfer_finalize.c`. Per the strategic plan
//! ("Reuse byte_add_with_carry chain logic for 8-byte add-by-1"),
//! `transfer_finalize` is a *ternary program*: it composes
//! `byte_add_with_carry`'s ternary network 8 times, threading the
//! carry from byte i to byte i+1.
//!
//! ```text
//! byte_0 = byte_add(nonce[0], 0, 1)         → (new[0], carry_0)
//! byte_1 = byte_add(nonce[1], 0, carry_0)   → (new[1], carry_1)
//! ⋮
//! byte_7 = byte_add(nonce[7], 0, carry_6)   → (new[7], carry_7)
//! ```
//!
//! `carry_7` is the high overflow and is discarded (mod 2^64). All
//! arithmetic is integer; same input always produces same output.
//!
//! Trace hash for the program:
//!   `trace_hash_ternary_program(P=transfer_finalize, x=nonce, y=new_nonce)`
//! — defined identically to `trace_hash_ternary` over the (input,
//! output) pair plus the BLAKE3 of the embedded byte_add weights.

use crate::error::TernaryError;
use crate::network::TernaryNetwork;
use crate::primitives::byte_add_with_carry;

/// Build the byte_add network used by this program. Returns the same
/// network shape every call (deterministic).
pub fn build() -> TernaryNetwork {
    byte_add_with_carry::build()
}

/// Run nonce + 1 mod 2^64. `nonce` is the 8 little-endian bytes of the
/// u64; output is the 8 little-endian bytes of `nonce + 1 mod 2^64`.
pub fn run(byte_add_net: &TernaryNetwork, nonce: [u8; 8]) -> Result<[u8; 8], TernaryError> {
    let mut new_nonce = [0u8; 8];
    let mut carry: u8 = 1;
    for i in 0..8 {
        let (sum, new_carry) = byte_add_with_carry::run(byte_add_net, nonce[i], 0, carry)?;
        new_nonce[i] = sum;
        carry = new_carry;
    }
    Ok(new_nonce)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground_truth(nonce: [u8; 8]) -> [u8; 8] {
        let n = u64::from_le_bytes(nonce);
        n.wrapping_add(1).to_le_bytes()
    }

    #[test]
    fn smoke_zero() {
        let n = build();
        let out = run(&n, [0u8; 8]).unwrap();
        assert_eq!(out, [1, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn smoke_carry_propagation() {
        let n = build();
        // 0xFF in byte 0 → wraps to 0, carry to byte 1
        let out = run(&n, [0xff, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(out, [0, 1, 0, 0, 0, 0, 0, 0]);
        // all 0xFF → wraps to all 0
        let out = run(&n, [0xff; 8]).unwrap();
        assert_eq!(out, [0u8; 8]);
        // 0xFF, 0xFF, 0x7F → wraps low two, ticks middle
        let out = run(&n, [0xff, 0xff, 0x7f, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(out, [0, 0, 0x80, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn random_witnesses_match_arithmetic() {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(11111);
        let n = build();
        for _ in 0..1000 {
            let mut nonce = [0u8; 8];
            for s in nonce.iter_mut() { *s = rng.gen(); }
            let got = run(&n, nonce).unwrap();
            assert_eq!(got, ground_truth(nonce), "nonce={nonce:?}");
        }
    }
}
