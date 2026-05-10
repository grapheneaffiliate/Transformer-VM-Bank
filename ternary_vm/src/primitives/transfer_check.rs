//! `transfer_check` ternary program — 16-byte u128 ≥ compare → 1-byte ok flag.
//!
//! Mirrors `primitives/transfer_check.c`. Per the strategic plan,
//! "Build as a thermometer-difference network: encode (a, b) as
//! concatenated thermometers, compute sign(a - b) byte-by-byte with a
//! small ternary state machine, AND the per-byte results to get final
//! ok bit." The simplest realization re-uses `byte_sub_with_borrow`
//! chained 16 times — `from ≥ amount` iff the final byte-wise borrow
//! is 0.
//!
//! ```text
//! byte_0 = byte_sub(from[0],  amount[0],  0)         → (diff, borrow_0)
//! byte_1 = byte_sub(from[1],  amount[1],  borrow_0)  → (diff, borrow_1)
//! ⋮
//! byte_15 = byte_sub(from[15], amount[15], borrow_14) → (diff, borrow_15)
//! ok = 1 − borrow_15
//! ```
//!
//! `from` and `amount` are little-endian u128 byte arrays (matches
//! `tools/run_per_byte_10k.py`). Output is a single byte 0/1.

use crate::error::TernaryError;
use crate::network::TernaryNetwork;
use crate::primitives::byte_sub_with_borrow;

pub fn build() -> TernaryNetwork {
    byte_sub_with_borrow::build()
}

pub fn run(
    byte_sub_net: &TernaryNetwork,
    from: [u8; 16],
    amount: [u8; 16],
) -> Result<u8, TernaryError> {
    let mut borrow: u8 = 0;
    for i in 0..16 {
        let (_diff, new_borrow) =
            byte_sub_with_borrow::run(byte_sub_net, from[i], amount[i], borrow)?;
        borrow = new_borrow;
    }
    Ok(1 - borrow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground_truth(from: [u8; 16], amount: [u8; 16]) -> u8 {
        let f = u128::from_le_bytes(from);
        let a = u128::from_le_bytes(amount);
        if f >= a {
            1
        } else {
            0
        }
    }

    #[test]
    fn smoke_basic() {
        let n = build();
        // from > amount
        let from = {
            let mut a = [0u8; 16];
            a[0] = 100;
            a
        };
        let amount = {
            let mut a = [0u8; 16];
            a[0] = 50;
            a
        };
        assert_eq!(run(&n, from, amount).unwrap(), 1);

        // from < amount
        assert_eq!(run(&n, amount, from).unwrap(), 0);

        // from == amount
        assert_eq!(run(&n, from, from).unwrap(), 1);

        // from = 0, amount = 0
        assert_eq!(run(&n, [0u8; 16], [0u8; 16]).unwrap(), 1);

        // from = MAX, amount = MAX
        assert_eq!(run(&n, [0xffu8; 16], [0xffu8; 16]).unwrap(), 1);

        // from = MAX, amount = 1
        assert_eq!(
            run(&n, [0xffu8; 16], {
                let mut a = [0u8; 16];
                a[0] = 1;
                a
            })
            .unwrap(),
            1
        );

        // from = 0, amount = 1 (16-byte borrow chain)
        assert_eq!(
            run(&n, [0u8; 16], {
                let mut a = [0u8; 16];
                a[0] = 1;
                a
            })
            .unwrap(),
            0
        );
    }

    #[test]
    fn random_witnesses_match_arithmetic() {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(22222);
        let n = build();
        for _ in 0..500 {
            let mut from = [0u8; 16];
            let mut amount = [0u8; 16];
            for s in from.iter_mut() {
                *s = rng.gen();
            }
            for s in amount.iter_mut() {
                *s = rng.gen();
            }
            let got = run(&n, from, amount).unwrap();
            assert_eq!(
                got,
                ground_truth(from, amount),
                "from={from:?} amount={amount:?}"
            );
        }
    }
}
