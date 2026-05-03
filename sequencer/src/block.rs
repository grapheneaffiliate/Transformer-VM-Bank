//! Block format + canonical encoding.

use psl_crypto::{hash_bytes, Hash, PublicKey, Signature};
use serde::{Deserialize, Serialize};

use crate::tx::SignedTx;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockHeader {
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
    #[serde(with = "hex::serde")]
    pub sequencer_sig: Signature,
}

impl BlockHeader {
    /// Canonical encoding excluding the signature — the bytes we sign.
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

    /// Hash of the entire signed header — what `parent_hash` of the next block points at.
    pub fn header_hash(&self) -> Hash {
        let mut buf = self.signing_bytes();
        buf.extend_from_slice(&self.sequencer_sig);
        hash_bytes(&buf)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub txs: Vec<SignedTx>,
}

pub fn tx_list_hash(txs: &[SignedTx]) -> Hash {
    let mut hasher = blake3::Hasher::new();
    for tx in txs {
        let canonical = tx.canonical();
        hasher.update(&(canonical.len() as u32).to_le_bytes());
        hasher.update(&canonical);
        hasher.update(&tx.signature);
    }
    *hasher.finalize().as_bytes()
}

pub fn combined_trace_hash(per_tx_traces: &[Hash]) -> Hash {
    let mut hasher = blake3::Hasher::new();
    for h in per_tx_traces {
        hasher.update(h);
    }
    *hasher.finalize().as_bytes()
}
