//! Compliance primitives: view keys, travel-rule, freeze authority.
//!
//! Most compliance logic lives in the mempool (pre-validation) and in the
//! issuer_registry. This module provides:
//!
//!   - `ViewKey`: regulator-scoped read access. Issuers grant a view key to
//!     a regulator pubkey; the sequencer's RPC enforces the filter and
//!     returns Merkle proofs.
//!
//!   - `RegulatorAccessProof`: bundle of (block header, account, MPT proof)
//!     that a regulator stores as audit evidence.
//!
//!   - `FreezeRecord`: court-order metadata logged alongside freeze blocks
//!     so any auditor can trace why an account was frozen.

use psl_crypto::{Hash, MerkleProof, PublicKey};
use serde::{Deserialize, Serialize};

use crate::block::BlockHeader;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ViewKey {
    pub regulator_pubkey: PublicKey,
    pub asset_id: u32,
    /// Optional account-pubkey filter: if non-empty, the regulator may only
    /// read these accounts. Empty = any account holding `asset_id`.
    pub account_filter: Vec<PublicKey>,
    /// Issuer's authorization signature over (regulator_pubkey, asset_id, filter).
    #[serde(with = "hex::serde")]
    pub issuer_signature: [u8; 64],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegulatorAccessProof {
    pub header: BlockHeader,
    #[serde(with = "hex::serde")]
    pub account_pubkey: [u8; 32],
    pub proof: MerkleProof,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FreezeRecord {
    pub block_n: u64,
    #[serde(with = "hex::serde")]
    pub frozen_account: [u8; 32],
    #[serde(with = "hex::serde")]
    pub court_order_hash: Hash,
    pub asset_id: u32,
    pub timestamp_ms: u64,
    pub issuer_pubkey: PublicKey,
}
