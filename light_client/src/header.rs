//! Block-header chain validation (light-client subset).
//!
//! Mirrors `psl_sequencer::block::BlockHeader` but redefined here so the
//! light_client crate doesn't depend on the sequencer (mobile target wants
//! a tiny dependency tree).

use psl_crypto::{hash_bytes, sign, verify, Hash, KeyPair, PublicKey, SigError, Signature};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Header {
    pub block_n: u64,
    #[serde(with = "hex::serde")]
    pub parent_hash: Hash,
    #[serde(with = "hex::serde")]
    pub prev_state_root: Hash,
    #[serde(with = "hex::serde")]
    pub tx_list_hash: Hash,
    #[serde(with = "hex::serde")]
    pub trace_hash: Hash,
    #[serde(with = "hex::serde")]
    pub new_state_root: Hash,
    #[serde(with = "hex::serde")]
    pub issuer_registry_root: Hash,
    pub timestamp_ms: u64,
    #[serde(with = "hex::serde")]
    pub sequencer_pubkey: PublicKey,
}

impl Header {
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);
        buf.extend_from_slice(&self.block_n.to_le_bytes());
        buf.extend_from_slice(&self.parent_hash);
        buf.extend_from_slice(&self.prev_state_root);
        buf.extend_from_slice(&self.tx_list_hash);
        buf.extend_from_slice(&self.trace_hash);
        buf.extend_from_slice(&self.new_state_root);
        buf.extend_from_slice(&self.issuer_registry_root);
        buf.extend_from_slice(&self.timestamp_ms.to_le_bytes());
        buf.extend_from_slice(&self.sequencer_pubkey);
        buf
    }

    /// Hash of the header BEFORE the sequencer signature is appended. Used
    /// internally; chain linking uses `SignedHeader::full_hash` (which
    /// matches `psl_sequencer::block::BlockHeader::header_hash`).
    pub fn unsigned_hash(&self) -> Hash {
        hash_bytes(&self.signing_bytes())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedHeader {
    pub header: Header,
    #[serde(with = "hex::serde")]
    pub signature: Signature,
}

impl SignedHeader {
    pub fn sign(header: Header, kp: &KeyPair) -> Self {
        let sig = sign(kp, &header.signing_bytes());
        Self {
            header,
            signature: sig,
        }
    }

    pub fn verify(&self, expected: &PublicKey) -> Result<(), SigError> {
        if &self.header.sequencer_pubkey != expected {
            return Err(SigError::VerificationFailed);
        }
        verify(
            &self.header.sequencer_pubkey,
            &self.header.signing_bytes(),
            &self.signature,
        )
    }

    /// Full hash of the signed header (signing_bytes || signature). Matches
    /// `psl_sequencer::block::BlockHeader::header_hash`. Used as `parent_hash`
    /// of the next block in the chain — modifying any byte of the signed
    /// header (including the signature) changes the downstream chain identity.
    pub fn full_hash(&self) -> Hash {
        let mut buf = self.header.signing_bytes();
        buf.extend_from_slice(&self.signature);
        hash_bytes(&buf)
    }
}
