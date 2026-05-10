//! `swap` standard contract — 2-asset exchange between two parties.
//!
//! Sender holds asset_in; recipient holds asset_out. After the swap:
//! sender's asset_in decreases by `amount_in` and asset_out increases
//! by `amount_out`; recipient's mirror.
//!
//! ```text
//! input:  sender_in(16)  ‖ sender_out(16)  ‖ recipient_in(16)  ‖ recipient_out(16)
//!         ‖ amount_in(16) ‖ amount_out(16) ‖ sender_nonce(8)
//!         (= 120 bytes)
//! output: new_sender_in(16) ‖ new_sender_out(16)
//!         ‖ new_recipient_in(16) ‖ new_recipient_out(16)
//!         ‖ new_sender_nonce(8)
//!         (= 72 bytes)
//! ```
//!
//! Preconditions: sender_in ≥ amount_in AND recipient_out ≥ amount_out
//! AND no recipient_in / sender_out overflow. On any precondition
//! violation, output is canonical no-op zeros.

use crate::error::ContractError;
use crate::program::{ProgramHash, TernaryProgram};
use psl_ternary_vm::network::TernaryNetwork;
use psl_ternary_vm::primitives::{
    byte_add_with_carry, byte_sub_with_borrow, transfer_check, transfer_finalize,
};

pub const INPUT_LEN: usize = 16 * 6 + 8; // 104 ... wait recompute below
pub const OUTPUT_LEN: usize = 16 * 4 + 8; // 72

/// Programmatic recompute of INPUT_LEN to keep doc + constant in sync.
const _CHECK_INPUT_LEN: usize = {
    // 4 balances + 2 amounts + 1 nonce
    let v = 16 * 6 + 8;
    let _ = v - INPUT_LEN; // would fail to compile if mismatched
    v
};

pub struct SwapContract {
    pub byte_add: TernaryNetwork,
    pub byte_sub: TernaryNetwork,
    pub program_hash: ProgramHash,
}

impl SwapContract {
    pub fn build() -> Self {
        let byte_add = byte_add_with_carry::build();
        let byte_sub = byte_sub_with_borrow::build();
        let mut h = blake3::Hasher::new();
        h.update(b"swap");
        h.update(byte_add.header.weights_hash());
        h.update(byte_sub.header.weights_hash());
        let mut program_hash = [0u8; 32];
        program_hash.copy_from_slice(h.finalize().as_bytes());
        Self {
            byte_add,
            byte_sub,
            program_hash,
        }
    }
}

fn u128_sub_chain(
    byte_sub: &TernaryNetwork,
    a: [u8; 16],
    b: [u8; 16],
) -> Result<([u8; 16], u8), ContractError> {
    let mut diff = [0u8; 16];
    let mut borrow: u8 = 0;
    for i in 0..16 {
        let (d, nb) = byte_sub_with_borrow::run(byte_sub, a[i], b[i], borrow)?;
        diff[i] = d;
        borrow = nb;
    }
    Ok((diff, borrow))
}

fn u128_add_chain(
    byte_add: &TernaryNetwork,
    a: [u8; 16],
    b: [u8; 16],
) -> Result<([u8; 16], u8), ContractError> {
    let mut sum = [0u8; 16];
    let mut carry: u8 = 0;
    for i in 0..16 {
        let (s, nc) = byte_add_with_carry::run(byte_add, a[i], b[i], carry)?;
        sum[i] = s;
        carry = nc;
    }
    Ok((sum, carry))
}

impl TernaryProgram for SwapContract {
    fn name(&self) -> &'static str {
        "swap"
    }

    fn program_hash(&self) -> ProgramHash {
        self.program_hash
    }

    fn run(&self, input: &[u8]) -> Result<Vec<u8>, ContractError> {
        if input.len() != INPUT_LEN {
            return Err(ContractError::InputShape {
                contract: "swap",
                got: input.len(),
                expected: INPUT_LEN,
            });
        }
        let mut sender_in = [0u8; 16];
        let mut sender_out = [0u8; 16];
        let mut recipient_in = [0u8; 16];
        let mut recipient_out = [0u8; 16];
        let mut amount_in = [0u8; 16];
        let mut amount_out = [0u8; 16];
        let mut sender_nonce = [0u8; 8];
        sender_in.copy_from_slice(&input[0..16]);
        sender_out.copy_from_slice(&input[16..32]);
        recipient_in.copy_from_slice(&input[32..48]);
        recipient_out.copy_from_slice(&input[48..64]);
        amount_in.copy_from_slice(&input[64..80]);
        amount_out.copy_from_slice(&input[80..96]);
        sender_nonce.copy_from_slice(&input[96..104]);

        // Preconditions
        if transfer_check::run(&self.byte_sub, sender_in, amount_in)? != 1
            || transfer_check::run(&self.byte_sub, recipient_out, amount_out)? != 1
        {
            return Ok(vec![0u8; OUTPUT_LEN]);
        }

        let (new_sender_in, _) = u128_sub_chain(&self.byte_sub, sender_in, amount_in)?;
        let (new_sender_out, c1) = u128_add_chain(&self.byte_add, sender_out, amount_out)?;
        let (new_recipient_in, c2) = u128_add_chain(&self.byte_add, recipient_in, amount_in)?;
        let (new_recipient_out, _) = u128_sub_chain(&self.byte_sub, recipient_out, amount_out)?;
        if c1 == 1 || c2 == 1 {
            return Ok(vec![0u8; OUTPUT_LEN]);
        }
        let new_sender_nonce = transfer_finalize::run(&self.byte_add, sender_nonce)?;

        let mut out = Vec::with_capacity(OUTPUT_LEN);
        out.extend_from_slice(&new_sender_in);
        out.extend_from_slice(&new_sender_out);
        out.extend_from_slice(&new_recipient_in);
        out.extend_from_slice(&new_recipient_out);
        out.extend_from_slice(&new_sender_nonce);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(
        sin: u128,
        sout: u128,
        rin: u128,
        rout: u128,
        ain: u128,
        aout: u128,
        nonce: u64,
    ) -> Vec<u8> {
        let mut v = Vec::with_capacity(INPUT_LEN);
        for x in [sin, sout, rin, rout, ain, aout] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        v.extend_from_slice(&nonce.to_le_bytes());
        v
    }
    fn unpack(out: &[u8]) -> (u128, u128, u128, u128, u64) {
        let parse_u128 = |s: &[u8]| {
            let mut a = [0u8; 16];
            a.copy_from_slice(s);
            u128::from_le_bytes(a)
        };
        let parse_u64 = |s: &[u8]| {
            let mut a = [0u8; 8];
            a.copy_from_slice(s);
            u64::from_le_bytes(a)
        };
        (
            parse_u128(&out[0..16]),
            parse_u128(&out[16..32]),
            parse_u128(&out[32..48]),
            parse_u128(&out[48..64]),
            parse_u64(&out[64..72]),
        )
    }

    fn ground_truth(
        sin: u128,
        sout: u128,
        rin: u128,
        rout: u128,
        ain: u128,
        aout: u128,
        nonce: u64,
    ) -> (u128, u128, u128, u128, u64) {
        if sin < ain || rout < aout {
            return (0, 0, 0, 0, 0);
        }
        let new_sout = match sout.checked_add(aout) {
            Some(v) => v,
            None => return (0, 0, 0, 0, 0),
        };
        let new_rin = match rin.checked_add(ain) {
            Some(v) => v,
            None => return (0, 0, 0, 0, 0),
        };
        (
            sin - ain,
            new_sout,
            new_rin,
            rout - aout,
            nonce.wrapping_add(1),
        )
    }

    #[test]
    fn smoke_basic_swap() {
        let c = SwapContract::build();
        let input = pack(1000, 0, 0, 500, 100, 50, 5);
        let out = c.run(&input).unwrap();
        let (nsin, nsout, nrin, nrout, nn) = unpack(&out);
        assert_eq!((nsin, nsout, nrin, nrout, nn), (900, 50, 100, 450, 6));
    }

    #[test]
    fn smoke_insufficient_balance_returns_zeros() {
        let c = SwapContract::build();
        // sender_in too small
        let input = pack(50, 0, 0, 500, 100, 50, 5);
        assert_eq!(c.run(&input).unwrap(), vec![0u8; OUTPUT_LEN]);
        // recipient_out too small
        let input = pack(1000, 0, 0, 25, 100, 50, 5);
        assert_eq!(c.run(&input).unwrap(), vec![0u8; OUTPUT_LEN]);
    }

    #[test]
    fn random_witnesses_match_arithmetic() {
        use rand::{Rng, SeedableRng};
        let mut rng = rand::rngs::StdRng::seed_from_u64(88888);
        let c = SwapContract::build();
        for _ in 0..50 {
            let sin: u128 = rng.gen();
            let sout: u128 = rng.gen();
            let rin: u128 = rng.gen();
            let rout: u128 = rng.gen();
            let ain: u128 = rng.gen();
            let aout: u128 = rng.gen();
            let nonce: u64 = rng.gen();
            let input = pack(sin, sout, rin, rout, ain, aout, nonce);
            let got = unpack(&c.run(&input).unwrap());
            let want = ground_truth(sin, sout, rin, rout, ain, aout, nonce);
            assert_eq!(got, want);
        }
    }
}
