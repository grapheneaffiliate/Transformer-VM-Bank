//! `transfer` standard contract — the single most important PSL
//! contract. Composes Layer 1 ternary primitives:
//!
//! ```text
//!   guard:  transfer_check(from_balance, amount)  →  ok ∈ {0, 1}
//!   if ok = 1:
//!     new_from_balance = from_balance − amount        (chained byte_sub × 16)
//!     new_to_balance   = to_balance   + amount        (chained byte_add × 16)
//!     new_nonce        = nonce + 1                    (transfer_finalize)
//!     output: new_from_balance ‖ new_to_balance ‖ new_nonce
//!   else:
//!     output: [0; 16+16+8] (canonical no-op)
//! ```
//!
//! Input/output byte layout (40 input, 40 output) keeps the contract
//! API minimal — issuer-id, epoch, account flags, and signing/auth
//! checks live outside the trace per `docs/ARCHITECTURE.md § 0`.
//!
//! ```text
//! input: from_balance(16) ‖ to_balance(16) ‖ amount(16) ‖ nonce(8)    (56 bytes)
//! output: new_from_balance(16) ‖ new_to_balance(16) ‖ new_nonce(8)    (40 bytes)
//! ```
//!
//! All u128/u64 fields are little-endian.

use crate::error::ContractError;
use crate::program::{ProgramHash, TernaryProgram};

use psl_ternary_vm::network::TernaryNetwork;
use psl_ternary_vm::primitives::{
    byte_add_with_carry, byte_sub_with_borrow, transfer_check, transfer_finalize,
};

pub const INPUT_LEN: usize = 16 + 16 + 16 + 8;
pub const OUTPUT_LEN: usize = 16 + 16 + 8;

/// `transfer` contract. Holds the embedded sub-networks and computes
/// the program_hash up front.
pub struct TransferContract {
    pub byte_add: TernaryNetwork,
    pub byte_sub: TernaryNetwork,
    pub program_hash: ProgramHash,
}

impl TransferContract {
    pub fn build() -> Self {
        let byte_add = byte_add_with_carry::build();
        let byte_sub = byte_sub_with_borrow::build();
        let program_hash = compute_program_hash(&byte_add, &byte_sub);
        Self {
            byte_add,
            byte_sub,
            program_hash,
        }
    }
}

fn compute_program_hash(byte_add: &TernaryNetwork, byte_sub: &TernaryNetwork) -> ProgramHash {
    let mut h = blake3::Hasher::new();
    h.update(b"transfer");
    h.update(&byte_add.header.weights_hash);
    h.update(&byte_sub.header.weights_hash);
    let mut out = [0u8; 32];
    out.copy_from_slice(h.finalize().as_bytes());
    out
}

/// Compute `from − amount` (16-byte little-endian u128 wrapping
/// subtraction) by chaining `byte_sub_with_borrow` 16 times. Returns
/// the difference + final borrow.
fn u128_sub_chain(
    byte_sub: &TernaryNetwork,
    a: [u8; 16],
    b: [u8; 16],
) -> Result<([u8; 16], u8), ContractError> {
    let mut diff = [0u8; 16];
    let mut borrow: u8 = 0;
    for i in 0..16 {
        let (d, new_borrow) = byte_sub_with_borrow::run(byte_sub, a[i], b[i], borrow)?;
        diff[i] = d;
        borrow = new_borrow;
    }
    Ok((diff, borrow))
}

/// Compute `to + amount` by chaining `byte_add_with_carry` 16 times.
fn u128_add_chain(
    byte_add: &TernaryNetwork,
    a: [u8; 16],
    b: [u8; 16],
) -> Result<([u8; 16], u8), ContractError> {
    let mut sum = [0u8; 16];
    let mut carry: u8 = 0;
    for i in 0..16 {
        let (s, new_carry) = byte_add_with_carry::run(byte_add, a[i], b[i], carry)?;
        sum[i] = s;
        carry = new_carry;
    }
    Ok((sum, carry))
}

impl TernaryProgram for TransferContract {
    fn name(&self) -> &'static str {
        "transfer"
    }

    fn program_hash(&self) -> ProgramHash {
        self.program_hash
    }

    fn run(&self, input: &[u8]) -> Result<Vec<u8>, ContractError> {
        if input.len() != INPUT_LEN {
            return Err(ContractError::InputShape {
                contract: "transfer",
                got: input.len(),
                expected: INPUT_LEN,
            });
        }
        let mut from_balance = [0u8; 16];
        let mut to_balance = [0u8; 16];
        let mut amount = [0u8; 16];
        let mut nonce = [0u8; 8];
        from_balance.copy_from_slice(&input[0..16]);
        to_balance.copy_from_slice(&input[16..32]);
        amount.copy_from_slice(&input[32..48]);
        nonce.copy_from_slice(&input[48..56]);

        // Precondition: from_balance ≥ amount.
        let ok = transfer_check::run(&self.byte_sub, from_balance, amount)?;
        if ok != 1 {
            return Ok(vec![0u8; OUTPUT_LEN]);
        }

        let (new_from, _) = u128_sub_chain(&self.byte_sub, from_balance, amount)?;
        let (new_to, carry) = u128_add_chain(&self.byte_add, to_balance, amount)?;
        if carry == 1 {
            // recipient balance overflow — canonical no-op
            return Ok(vec![0u8; OUTPUT_LEN]);
        }
        let new_nonce = transfer_finalize::run(&self.byte_add, nonce)?;

        let mut out = Vec::with_capacity(OUTPUT_LEN);
        out.extend_from_slice(&new_from);
        out.extend_from_slice(&new_to);
        out.extend_from_slice(&new_nonce);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_input(from: u128, to: u128, amount: u128, nonce: u64) -> Vec<u8> {
        let mut v = Vec::with_capacity(INPUT_LEN);
        v.extend_from_slice(&from.to_le_bytes());
        v.extend_from_slice(&to.to_le_bytes());
        v.extend_from_slice(&amount.to_le_bytes());
        v.extend_from_slice(&nonce.to_le_bytes());
        v
    }

    fn unpack_output(out: &[u8]) -> (u128, u128, u64) {
        let mut from = [0u8; 16];
        let mut to = [0u8; 16];
        let mut nonce = [0u8; 8];
        from.copy_from_slice(&out[0..16]);
        to.copy_from_slice(&out[16..32]);
        nonce.copy_from_slice(&out[32..40]);
        (
            u128::from_le_bytes(from),
            u128::from_le_bytes(to),
            u64::from_le_bytes(nonce),
        )
    }

    fn ground_truth(from: u128, to: u128, amount: u128, nonce: u64) -> (u128, u128, u64) {
        if from < amount {
            return (0, 0, 0);
        }
        let new_to = match to.checked_add(amount) {
            Some(v) => v,
            None => return (0, 0, 0),
        };
        (from - amount, new_to, nonce.wrapping_add(1))
    }

    #[test]
    fn smoke_basic_transfer() {
        let c = TransferContract::build();
        let input = pack_input(1000, 500, 250, 7);
        let out = c.run(&input).unwrap();
        let (nf, nt, nn) = unpack_output(&out);
        assert_eq!((nf, nt, nn), (750, 750, 8));
    }

    #[test]
    fn smoke_insufficient_balance_returns_zeros() {
        let c = TransferContract::build();
        let input = pack_input(100, 500, 250, 7);
        let out = c.run(&input).unwrap();
        assert_eq!(out, vec![0u8; OUTPUT_LEN]);
    }

    #[test]
    fn smoke_recipient_overflow_returns_zeros() {
        let c = TransferContract::build();
        // to = u128::MAX, amount = 1 → overflow
        let input = pack_input(10, u128::MAX, 1, 0);
        let out = c.run(&input).unwrap();
        assert_eq!(out, vec![0u8; OUTPUT_LEN]);
    }

    #[test]
    fn random_witnesses_match_arithmetic() {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(77777);
        let c = TransferContract::build();
        for _ in 0..100 {
            let from: u128 = rng.gen();
            let to: u128 = rng.gen();
            let amount: u128 = rng.gen();
            let nonce: u64 = rng.gen();
            let input = pack_input(from, to, amount, nonce);
            let out = c.run(&input).unwrap();
            let (nf, nt, nn) = unpack_output(&out);
            let (gf, gt, gn) = ground_truth(from, to, amount, nonce);
            assert_eq!(
                (nf, nt, nn),
                (gf, gt, gn),
                "from={from} to={to} amount={amount} nonce={nonce}"
            );
        }
    }

    #[test]
    fn trace_hash_is_deterministic() {
        let c1 = TransferContract::build();
        let c2 = TransferContract::build();
        assert_eq!(c1.program_hash(), c2.program_hash());
        let input = pack_input(1000, 500, 250, 7);
        let out1 = c1.run(&input).unwrap();
        let out2 = c2.run(&input).unwrap();
        assert_eq!(out1, out2);
        assert_eq!(c1.trace_hash(&input, &out1), c2.trace_hash(&input, &out2));
    }
}
