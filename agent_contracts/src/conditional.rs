//! `time_locked_release`, `multisig_2of3`, `conditional_payment` —
//! three guarded transfer contracts whose only difference is the
//! guard predicate. All wrap the same `wrapped_transfer` helper.

use crate::error::ContractError;
use crate::guarded::{no_op_output, u64_ge, wrapped_transfer};
use crate::program::{ProgramHash, TernaryProgram};
use psl_ternary_vm::network::TernaryNetwork;
use psl_ternary_vm::primitives::{byte_add_with_carry, byte_sub_with_borrow};

fn build_subnets() -> (TernaryNetwork, TernaryNetwork) {
    (byte_add_with_carry::build(), byte_sub_with_borrow::build())
}
fn hash_program(name: &str, byte_add: &TernaryNetwork, byte_sub: &TernaryNetwork) -> ProgramHash {
    let mut h = blake3::Hasher::new();
    h.update(name.as_bytes());
    h.update(&byte_add.header.weights_hash);
    h.update(&byte_sub.header.weights_hash);
    let mut out = [0u8; 32];
    out.copy_from_slice(h.finalize().as_bytes());
    out
}

// ── time_locked_release ──────────────────────────────────────────────

/// `time_locked_release` — fire transfer iff `current_time ≥ unlock_time`.
///
/// Input: from(16) ‖ to(16) ‖ amount(16) ‖ nonce(8) ‖ current_time(8) ‖ unlock_time(8) (72 B)
/// Output: new_from(16) ‖ new_to(16) ‖ new_nonce(8) (40 B)
///
/// Time comparison uses the same byte-borrow chain as `transfer_check`.
pub struct TimeLockedRelease {
    pub byte_add: TernaryNetwork,
    pub byte_sub: TernaryNetwork,
    pub program_hash: ProgramHash,
}
impl TimeLockedRelease {
    pub fn build() -> Self {
        let (byte_add, byte_sub) = build_subnets();
        let program_hash = hash_program("time_locked_release", &byte_add, &byte_sub);
        Self {
            byte_add,
            byte_sub,
            program_hash,
        }
    }
}
impl TernaryProgram for TimeLockedRelease {
    fn name(&self) -> &'static str {
        "time_locked_release"
    }
    fn program_hash(&self) -> ProgramHash {
        self.program_hash
    }
    fn run(&self, input: &[u8]) -> Result<Vec<u8>, ContractError> {
        if input.len() != 72 {
            return Err(ContractError::InputShape {
                contract: "time_locked_release",
                got: input.len(),
                expected: 72,
            });
        }
        let mut current_time = [0u8; 8];
        let mut unlock_time = [0u8; 8];
        current_time.copy_from_slice(&input[56..64]);
        unlock_time.copy_from_slice(&input[64..72]);
        if u64_ge(&self.byte_sub, current_time, unlock_time)? != 1 {
            return Ok(no_op_output(40));
        }
        let mut from = [0u8; 16];
        let mut to = [0u8; 16];
        let mut amount = [0u8; 16];
        let mut nonce = [0u8; 8];
        from.copy_from_slice(&input[0..16]);
        to.copy_from_slice(&input[16..32]);
        amount.copy_from_slice(&input[32..48]);
        nonce.copy_from_slice(&input[48..56]);
        wrapped_transfer(&self.byte_add, &self.byte_sub, from, to, amount, nonce)
    }
}

// ── multisig_2of3 ────────────────────────────────────────────────────

/// `multisig_2of3` — fire transfer iff at least 2 of the 3 signature
/// flags are set. Each flag is a single byte ∈ {0, 1}; values outside
/// that range collapse to the canonical no-op (defensive — sequencer
/// is expected to normalize, but the contract checks).
///
/// Input: from(16) ‖ to(16) ‖ amount(16) ‖ nonce(8) ‖ sig0(1) ‖ sig1(1) ‖ sig2(1) (59 B)
/// Output: new_from(16) ‖ new_to(16) ‖ new_nonce(8) (40 B)
pub struct Multisig2of3 {
    pub byte_add: TernaryNetwork,
    pub byte_sub: TernaryNetwork,
    pub program_hash: ProgramHash,
}
impl Multisig2of3 {
    pub fn build() -> Self {
        let (byte_add, byte_sub) = build_subnets();
        let program_hash = hash_program("multisig_2of3", &byte_add, &byte_sub);
        Self {
            byte_add,
            byte_sub,
            program_hash,
        }
    }
}
impl TernaryProgram for Multisig2of3 {
    fn name(&self) -> &'static str {
        "multisig_2of3"
    }
    fn program_hash(&self) -> ProgramHash {
        self.program_hash
    }
    fn run(&self, input: &[u8]) -> Result<Vec<u8>, ContractError> {
        if input.len() != 59 {
            return Err(ContractError::InputShape {
                contract: "multisig_2of3",
                got: input.len(),
                expected: 59,
            });
        }
        let s0 = input[56];
        let s1 = input[57];
        let s2 = input[58];
        if s0 > 1 || s1 > 1 || s2 > 1 {
            return Ok(no_op_output(40));
        }
        let count = s0 as u32 + s1 as u32 + s2 as u32;
        if count < 2 {
            return Ok(no_op_output(40));
        }
        let mut from = [0u8; 16];
        let mut to = [0u8; 16];
        let mut amount = [0u8; 16];
        let mut nonce = [0u8; 8];
        from.copy_from_slice(&input[0..16]);
        to.copy_from_slice(&input[16..32]);
        amount.copy_from_slice(&input[32..48]);
        nonce.copy_from_slice(&input[48..56]);
        wrapped_transfer(&self.byte_add, &self.byte_sub, from, to, amount, nonce)
    }
}

// ── conditional_payment ──────────────────────────────────────────────

/// `conditional_payment` — fire transfer iff `condition_satisfied = 1`.
///
/// Input: from(16) ‖ to(16) ‖ amount(16) ‖ nonce(8) ‖ condition(1) (57 B)
/// Output: new_from(16) ‖ new_to(16) ‖ new_nonce(8) (40 B)
pub struct ConditionalPayment {
    pub byte_add: TernaryNetwork,
    pub byte_sub: TernaryNetwork,
    pub program_hash: ProgramHash,
}
impl ConditionalPayment {
    pub fn build() -> Self {
        let (byte_add, byte_sub) = build_subnets();
        let program_hash = hash_program("conditional_payment", &byte_add, &byte_sub);
        Self {
            byte_add,
            byte_sub,
            program_hash,
        }
    }
}
impl TernaryProgram for ConditionalPayment {
    fn name(&self) -> &'static str {
        "conditional_payment"
    }
    fn program_hash(&self) -> ProgramHash {
        self.program_hash
    }
    fn run(&self, input: &[u8]) -> Result<Vec<u8>, ContractError> {
        if input.len() != 57 {
            return Err(ContractError::InputShape {
                contract: "conditional_payment",
                got: input.len(),
                expected: 57,
            });
        }
        let cond = input[56];
        if cond != 1 {
            return Ok(no_op_output(40));
        }
        let mut from = [0u8; 16];
        let mut to = [0u8; 16];
        let mut amount = [0u8; 16];
        let mut nonce = [0u8; 8];
        from.copy_from_slice(&input[0..16]);
        to.copy_from_slice(&input[16..32]);
        amount.copy_from_slice(&input[32..48]);
        nonce.copy_from_slice(&input[48..56]);
        wrapped_transfer(&self.byte_add, &self.byte_sub, from, to, amount, nonce)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unpack(out: &[u8]) -> (u128, u128, u64) {
        let mut a = [0u8; 16];
        let mut b = [0u8; 16];
        let mut n = [0u8; 8];
        a.copy_from_slice(&out[0..16]);
        b.copy_from_slice(&out[16..32]);
        n.copy_from_slice(&out[32..40]);
        (
            u128::from_le_bytes(a),
            u128::from_le_bytes(b),
            u64::from_le_bytes(n),
        )
    }

    #[test]
    fn time_locked_fires_after_unlock() {
        let c = TimeLockedRelease::build();
        let mut input = Vec::with_capacity(72);
        input.extend_from_slice(&1000u128.to_le_bytes());
        input.extend_from_slice(&500u128.to_le_bytes());
        input.extend_from_slice(&100u128.to_le_bytes());
        input.extend_from_slice(&5u64.to_le_bytes());
        input.extend_from_slice(&200u64.to_le_bytes()); // current
        input.extend_from_slice(&100u64.to_le_bytes()); // unlock
        let out = c.run(&input).unwrap();
        assert_eq!(unpack(&out), (900, 600, 6));
    }
    #[test]
    fn time_locked_blocks_before_unlock() {
        let c = TimeLockedRelease::build();
        let mut input = Vec::with_capacity(72);
        input.extend_from_slice(&1000u128.to_le_bytes());
        input.extend_from_slice(&500u128.to_le_bytes());
        input.extend_from_slice(&100u128.to_le_bytes());
        input.extend_from_slice(&5u64.to_le_bytes());
        input.extend_from_slice(&50u64.to_le_bytes()); // current < unlock
        input.extend_from_slice(&100u64.to_le_bytes());
        let out = c.run(&input).unwrap();
        assert_eq!(out, vec![0u8; 40]);
    }

    #[test]
    fn multisig_fires_at_2_or_3_signatures() {
        let c = Multisig2of3::build();
        for (s0, s1, s2, expected_fire) in [
            (1u8, 1u8, 1u8, true),
            (1, 1, 0, true),
            (1, 0, 1, true),
            (0, 1, 1, true),
            (1, 0, 0, false),
            (0, 1, 0, false),
            (0, 0, 1, false),
            (0, 0, 0, false),
        ] {
            let mut input = Vec::with_capacity(59);
            input.extend_from_slice(&1000u128.to_le_bytes());
            input.extend_from_slice(&500u128.to_le_bytes());
            input.extend_from_slice(&100u128.to_le_bytes());
            input.extend_from_slice(&5u64.to_le_bytes());
            input.push(s0);
            input.push(s1);
            input.push(s2);
            let out = c.run(&input).unwrap();
            if expected_fire {
                assert_eq!(unpack(&out), (900, 600, 6), "sigs=({s0},{s1},{s2})");
            } else {
                assert_eq!(out, vec![0u8; 40], "sigs=({s0},{s1},{s2})");
            }
        }
    }

    #[test]
    fn multisig_rejects_out_of_range_flags() {
        let c = Multisig2of3::build();
        let mut input = Vec::with_capacity(59);
        input.extend_from_slice(&1000u128.to_le_bytes());
        input.extend_from_slice(&500u128.to_le_bytes());
        input.extend_from_slice(&100u128.to_le_bytes());
        input.extend_from_slice(&5u64.to_le_bytes());
        input.push(1);
        input.push(2); // out of range
        input.push(0);
        let out = c.run(&input).unwrap();
        assert_eq!(out, vec![0u8; 40]);
    }

    #[test]
    fn conditional_payment_fires_when_set() {
        let c = ConditionalPayment::build();
        let mut input = Vec::with_capacity(57);
        input.extend_from_slice(&1000u128.to_le_bytes());
        input.extend_from_slice(&500u128.to_le_bytes());
        input.extend_from_slice(&100u128.to_le_bytes());
        input.extend_from_slice(&5u64.to_le_bytes());
        input.push(1);
        let out = c.run(&input).unwrap();
        assert_eq!(unpack(&out), (900, 600, 6));
    }
    #[test]
    fn conditional_payment_blocks_when_clear() {
        let c = ConditionalPayment::build();
        let mut input = Vec::with_capacity(57);
        input.extend_from_slice(&1000u128.to_le_bytes());
        input.extend_from_slice(&500u128.to_le_bytes());
        input.extend_from_slice(&100u128.to_le_bytes());
        input.extend_from_slice(&5u64.to_le_bytes());
        input.push(0);
        let out = c.run(&input).unwrap();
        assert_eq!(out, vec![0u8; 40]);
    }
}
