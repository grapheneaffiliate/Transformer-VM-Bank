//! Account record (64 bytes) — wire compatible with primitives/common.h.
//!
//! Layout (must match primitives/common.h exactly):
//!   [0..32)   pubkey         (32 bytes, ed25519)
//!   [32..48)  balance        (16 bytes, u128 little-endian; high bit = frozen)
//!   [48..56)  nonce          (8  bytes, u64  little-endian)
//!   [56..64)  last_active    (8  bytes, u64  little-endian)
//!
//! The `frozen` flag lives in bit 7 of byte 47 (the high byte of balance).
//! This is a v1 simplification — v2 may extend the record to 96 bytes.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

pub const ACCOUNT_BYTES: usize = 64;
pub const PUBKEY_OFFSET: usize = 0;
pub const PUBKEY_LEN: usize = 32;
pub const BALANCE_OFFSET: usize = 32;
pub const BALANCE_LEN: usize = 16;
pub const NONCE_OFFSET: usize = 48;
pub const NONCE_LEN: usize = 8;
pub const LAST_ACTIVE_OFFSET: usize = 56;
pub const LAST_ACTIVE_LEN: usize = 8;
pub const FLAGS_BYTE: usize = 47;
pub const FROZEN_FLAG: u8 = 0x80;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Account {
    #[serde(with = "BigArray")]
    pub bytes: [u8; ACCOUNT_BYTES],
}

impl Default for Account {
    fn default() -> Self {
        Self { bytes: [0u8; ACCOUNT_BYTES] }
    }
}

impl Account {
    pub fn new(pubkey: [u8; 32]) -> Self {
        let mut bytes = [0u8; ACCOUNT_BYTES];
        bytes[PUBKEY_OFFSET..PUBKEY_OFFSET + PUBKEY_LEN].copy_from_slice(&pubkey);
        Self { bytes }
    }

    pub fn pubkey(&self) -> [u8; 32] {
        let mut pk = [0u8; 32];
        pk.copy_from_slice(&self.bytes[PUBKEY_OFFSET..PUBKEY_OFFSET + PUBKEY_LEN]);
        pk
    }

    pub fn balance(&self) -> u128 {
        // High bit of byte 47 is the frozen flag, not a balance bit.
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&self.bytes[BALANCE_OFFSET..BALANCE_OFFSET + BALANCE_LEN]);
        bytes[FLAGS_BYTE - BALANCE_OFFSET] &= !FROZEN_FLAG;
        u128::from_le_bytes(bytes)
    }

    pub fn set_balance(&mut self, balance: u128) {
        let frozen = self.is_frozen();
        let bytes = balance.to_le_bytes();
        self.bytes[BALANCE_OFFSET..BALANCE_OFFSET + BALANCE_LEN].copy_from_slice(&bytes);
        if frozen {
            self.set_frozen(true);
        }
    }

    pub fn nonce(&self) -> u64 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.bytes[NONCE_OFFSET..NONCE_OFFSET + NONCE_LEN]);
        u64::from_le_bytes(bytes)
    }

    pub fn set_nonce(&mut self, nonce: u64) {
        self.bytes[NONCE_OFFSET..NONCE_OFFSET + NONCE_LEN]
            .copy_from_slice(&nonce.to_le_bytes());
    }

    pub fn last_active(&self) -> u64 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(
            &self.bytes[LAST_ACTIVE_OFFSET..LAST_ACTIVE_OFFSET + LAST_ACTIVE_LEN],
        );
        u64::from_le_bytes(bytes)
    }

    pub fn set_last_active(&mut self, epoch: u64) {
        self.bytes[LAST_ACTIVE_OFFSET..LAST_ACTIVE_OFFSET + LAST_ACTIVE_LEN]
            .copy_from_slice(&epoch.to_le_bytes());
    }

    pub fn is_frozen(&self) -> bool {
        self.bytes[FLAGS_BYTE] & FROZEN_FLAG != 0
    }

    pub fn set_frozen(&mut self, frozen: bool) {
        if frozen {
            self.bytes[FLAGS_BYTE] |= FROZEN_FLAG;
        } else {
            self.bytes[FLAGS_BYTE] &= !FROZEN_FLAG;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_balance() {
        let mut a = Account::new([1u8; 32]);
        a.set_balance(123_456_789_012_345_678_901_234_u128);
        assert_eq!(a.balance(), 123_456_789_012_345_678_901_234_u128);
    }

    #[test]
    fn frozen_flag_does_not_corrupt_balance() {
        let mut a = Account::default();
        a.set_balance(u128::MAX >> 1);
        a.set_frozen(true);
        assert!(a.is_frozen());
        assert_eq!(a.balance(), u128::MAX >> 1);
        a.set_frozen(false);
        assert!(!a.is_frozen());
        assert_eq!(a.balance(), u128::MAX >> 1);
    }

    #[test]
    fn nonce_round_trip() {
        let mut a = Account::default();
        a.set_nonce(42);
        assert_eq!(a.nonce(), 42);
    }
}
