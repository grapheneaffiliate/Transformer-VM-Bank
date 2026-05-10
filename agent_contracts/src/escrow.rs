//! Escrow contract trio: `escrow_create`, `escrow_release`, `escrow_refund`.
//!
//! `escrow_create` is a balance-conserving transfer from the sender to
//! a designated escrow account. `escrow_release` and `escrow_refund`
//! are gated transfers from the escrow account — to recipient or back
//! to sender respectively — fired when the corresponding flag is set.
//!
//! The "escrow account" is just another PSL account from the trace's
//! point of view. Whether the sequencer treats it as locked /
//! non-spendable except via release/refund is a sequencer-side
//! invariant out of scope for this trace.

use crate::error::ContractError;
use crate::guarded::{no_op_output, wrapped_transfer};
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

// ── escrow_create ────────────────────────────────────────────────────

/// `escrow_create` — same wire shape as `transfer`. The caller-side
/// distinction is purely semantic (the "to" account is an escrow).
///
/// Input: from_balance(16) ‖ escrow_balance(16) ‖ amount(16) ‖ sender_nonce(8) (56 B)
/// Output: new_from(16) ‖ new_escrow(16) ‖ new_sender_nonce(8) (40 B)
pub struct EscrowCreate {
    pub byte_add: TernaryNetwork,
    pub byte_sub: TernaryNetwork,
    pub program_hash: ProgramHash,
}
impl EscrowCreate {
    pub fn build() -> Self {
        let (byte_add, byte_sub) = build_subnets();
        let program_hash = hash_program("escrow_create", &byte_add, &byte_sub);
        Self {
            byte_add,
            byte_sub,
            program_hash,
        }
    }
}
impl TernaryProgram for EscrowCreate {
    fn name(&self) -> &'static str {
        "escrow_create"
    }
    fn program_hash(&self) -> ProgramHash {
        self.program_hash
    }
    fn run(&self, input: &[u8]) -> Result<Vec<u8>, ContractError> {
        if input.len() != 56 {
            return Err(ContractError::InputShape {
                contract: "escrow_create",
                got: input.len(),
                expected: 56,
            });
        }
        let mut from_balance = [0u8; 16];
        let mut escrow_balance = [0u8; 16];
        let mut amount = [0u8; 16];
        let mut nonce = [0u8; 8];
        from_balance.copy_from_slice(&input[0..16]);
        escrow_balance.copy_from_slice(&input[16..32]);
        amount.copy_from_slice(&input[32..48]);
        nonce.copy_from_slice(&input[48..56]);
        wrapped_transfer(
            &self.byte_add,
            &self.byte_sub,
            from_balance,
            escrow_balance,
            amount,
            nonce,
        )
    }
}

// ── escrow_release / escrow_refund (same shape, different name) ──────

/// `escrow_release` — transfer escrow → recipient when `release_flag = 1`.
///
/// Input: escrow_balance(16) ‖ recipient_balance(16) ‖ amount(16) ‖ release_flag(1) ‖ nonce(8) (57 B)
/// Output: new_escrow(16) ‖ new_recipient(16) ‖ new_nonce(8) (40 B)
pub struct EscrowRelease {
    pub byte_add: TernaryNetwork,
    pub byte_sub: TernaryNetwork,
    pub program_hash: ProgramHash,
}
impl EscrowRelease {
    pub fn build() -> Self {
        let (byte_add, byte_sub) = build_subnets();
        let program_hash = hash_program("escrow_release", &byte_add, &byte_sub);
        Self {
            byte_add,
            byte_sub,
            program_hash,
        }
    }
}
impl TernaryProgram for EscrowRelease {
    fn name(&self) -> &'static str {
        "escrow_release"
    }
    fn program_hash(&self) -> ProgramHash {
        self.program_hash
    }
    fn run(&self, input: &[u8]) -> Result<Vec<u8>, ContractError> {
        if input.len() != 57 {
            return Err(ContractError::InputShape {
                contract: "escrow_release",
                got: input.len(),
                expected: 57,
            });
        }
        let release_flag = input[48];
        if release_flag != 1 {
            return Ok(no_op_output(40));
        }
        let mut escrow = [0u8; 16];
        let mut recipient = [0u8; 16];
        let mut amount = [0u8; 16];
        let mut nonce = [0u8; 8];
        escrow.copy_from_slice(&input[0..16]);
        recipient.copy_from_slice(&input[16..32]);
        amount.copy_from_slice(&input[32..48]);
        nonce.copy_from_slice(&input[49..57]);
        wrapped_transfer(
            &self.byte_add,
            &self.byte_sub,
            escrow,
            recipient,
            amount,
            nonce,
        )
    }
}

/// `escrow_refund` — transfer escrow → sender when `refund_flag = 1`.
/// Same wire shape as escrow_release; semantically routes funds back
/// to the original payer when the escrow's release condition is not met.
pub struct EscrowRefund {
    pub byte_add: TernaryNetwork,
    pub byte_sub: TernaryNetwork,
    pub program_hash: ProgramHash,
}
impl EscrowRefund {
    pub fn build() -> Self {
        let (byte_add, byte_sub) = build_subnets();
        let program_hash = hash_program("escrow_refund", &byte_add, &byte_sub);
        Self {
            byte_add,
            byte_sub,
            program_hash,
        }
    }
}
impl TernaryProgram for EscrowRefund {
    fn name(&self) -> &'static str {
        "escrow_refund"
    }
    fn program_hash(&self) -> ProgramHash {
        self.program_hash
    }
    fn run(&self, input: &[u8]) -> Result<Vec<u8>, ContractError> {
        if input.len() != 57 {
            return Err(ContractError::InputShape {
                contract: "escrow_refund",
                got: input.len(),
                expected: 57,
            });
        }
        let refund_flag = input[48];
        if refund_flag != 1 {
            return Ok(no_op_output(40));
        }
        let mut escrow = [0u8; 16];
        let mut sender = [0u8; 16];
        let mut amount = [0u8; 16];
        let mut nonce = [0u8; 8];
        escrow.copy_from_slice(&input[0..16]);
        sender.copy_from_slice(&input[16..32]);
        amount.copy_from_slice(&input[32..48]);
        nonce.copy_from_slice(&input[49..57]);
        wrapped_transfer(
            &self.byte_add,
            &self.byte_sub,
            escrow,
            sender,
            amount,
            nonce,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_create(from: u128, escrow: u128, amount: u128, nonce: u64) -> Vec<u8> {
        let mut v = Vec::with_capacity(56);
        v.extend_from_slice(&from.to_le_bytes());
        v.extend_from_slice(&escrow.to_le_bytes());
        v.extend_from_slice(&amount.to_le_bytes());
        v.extend_from_slice(&nonce.to_le_bytes());
        v
    }
    fn pack_gated(a: u128, b: u128, amount: u128, flag: u8, nonce: u64) -> Vec<u8> {
        let mut v = Vec::with_capacity(57);
        v.extend_from_slice(&a.to_le_bytes());
        v.extend_from_slice(&b.to_le_bytes());
        v.extend_from_slice(&amount.to_le_bytes());
        v.push(flag);
        v.extend_from_slice(&nonce.to_le_bytes());
        v
    }
    fn unpack_40(out: &[u8]) -> (u128, u128, u64) {
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
    fn escrow_create_locks_funds() {
        let c = EscrowCreate::build();
        let out = c.run(&pack_create(1000, 200, 300, 5)).unwrap();
        assert_eq!(unpack_40(&out), (700, 500, 6));
    }

    #[test]
    fn escrow_create_no_balance_returns_zeros() {
        let c = EscrowCreate::build();
        let out = c.run(&pack_create(50, 200, 300, 5)).unwrap();
        assert_eq!(out, vec![0u8; 40]);
    }

    #[test]
    fn escrow_release_with_flag_set() {
        let c = EscrowRelease::build();
        let out = c.run(&pack_gated(500, 100, 300, 1, 5)).unwrap();
        assert_eq!(unpack_40(&out), (200, 400, 6));
    }

    #[test]
    fn escrow_release_with_flag_clear_returns_zeros() {
        let c = EscrowRelease::build();
        let out = c.run(&pack_gated(500, 100, 300, 0, 5)).unwrap();
        assert_eq!(out, vec![0u8; 40]);
    }

    #[test]
    fn escrow_refund_with_flag_set() {
        let c = EscrowRefund::build();
        let out = c.run(&pack_gated(500, 100, 300, 1, 5)).unwrap();
        assert_eq!(unpack_40(&out), (200, 400, 6));
    }

    #[test]
    fn escrow_refund_with_flag_clear_returns_zeros() {
        let c = EscrowRefund::build();
        let out = c.run(&pack_gated(500, 100, 300, 0, 5)).unwrap();
        assert_eq!(out, vec![0u8; 40]);
    }
}
