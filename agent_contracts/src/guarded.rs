//! Shared helpers for "guarded transfer" contracts.
//!
//! escrow_release / escrow_refund / time_locked_release / multisig_2of3
//! / conditional_payment all collapse to: "evaluate a guard predicate,
//! and if it holds AND the inner balance/overflow checks pass, execute
//! a transfer." This module factors the wrapped transfer + the chained
//! u128 arithmetic out so each contract reduces to its specific guard.

use crate::error::ContractError;
use psl_ternary_vm::network::TernaryNetwork;
use psl_ternary_vm::primitives::{
    byte_add_with_carry, byte_sub_with_borrow, transfer_check, transfer_finalize,
};

pub(crate) fn u128_sub_chain(
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

pub(crate) fn u128_add_chain(
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

/// 8-byte u64 ≥ compare via the same byte_sub borrow chain trick used
/// by `transfer_check` (zero-padded to a 16-byte u128 compare).
pub(crate) fn u64_ge(byte_sub: &TernaryNetwork, a: [u8; 8], b: [u8; 8]) -> Result<u8, ContractError> {
    let mut a16 = [0u8; 16];
    let mut b16 = [0u8; 16];
    a16[..8].copy_from_slice(&a);
    b16[..8].copy_from_slice(&b);
    Ok(transfer_check::run(byte_sub, a16, b16)?)
}

/// Output payload returned on guard-fail or arithmetic-fail.
pub(crate) fn no_op_output(len: usize) -> Vec<u8> {
    vec![0u8; len]
}

/// Run a wrapped transfer once we've already evaluated the guard.
/// Returns the canonical 40-byte transfer output OR a no-op on inner
/// precondition violation (insufficient balance / recipient overflow).
pub(crate) fn wrapped_transfer(
    byte_add: &TernaryNetwork,
    byte_sub: &TernaryNetwork,
    from_balance: [u8; 16],
    to_balance: [u8; 16],
    amount: [u8; 16],
    nonce: [u8; 8],
) -> Result<Vec<u8>, ContractError> {
    if transfer_check::run(byte_sub, from_balance, amount)? != 1 {
        return Ok(no_op_output(40));
    }
    let (new_from, _) = u128_sub_chain(byte_sub, from_balance, amount)?;
    let (new_to, carry) = u128_add_chain(byte_add, to_balance, amount)?;
    if carry == 1 {
        return Ok(no_op_output(40));
    }
    let new_nonce = transfer_finalize::run(byte_add, nonce)?;
    let mut out = Vec::with_capacity(40);
    out.extend_from_slice(&new_from);
    out.extend_from_slice(&new_to);
    out.extend_from_slice(&new_nonce);
    Ok(out)
}
