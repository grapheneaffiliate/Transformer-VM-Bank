//! PSL light client.
//!
//! Verifies an account's balance on a phone or auditor service. The phone
//! does NOT run the transformer — that's the auditor's responsibility. The
//! phone trusts the *signed block-header chain* and verifies a Merkle proof
//! against the latest header's `new_state_root`.
//!
//! API (FFI-stable for UniFFI):
//! - `verify_balance(genesis_root, headers, account_pubkey, account_bytes, proof)`
//! - `verify_block_header(parent_hash, header)`

pub mod header;
pub mod proof;

use psl_crypto::{Account, Hash, MerkleProof, SparseMerkleTree};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("genesis root mismatch")]
    GenesisMismatch,
    #[error("header chain broken at index {0}")]
    HeaderChainBroken(usize),
    #[error("invalid sequencer signature on header {0}")]
    InvalidSignature(u64),
    #[error("merkle proof failed for account")]
    ProofFailed,
}

pub fn verify_balance(
    genesis_root: Hash,
    headers: &[header::SignedHeader],
    expected_signer: &[u8; 32],
    account_pubkey: &[u8; 32],
    proof: &MerkleProof,
) -> Result<u128, VerifyError> {
    if headers.is_empty() {
        return Err(VerifyError::HeaderChainBroken(0));
    }
    let mut prev_hash: Option<Hash> = None;
    for (i, h) in headers.iter().enumerate() {
        if let Some(p) = prev_hash {
            if h.header.parent_hash != p {
                return Err(VerifyError::HeaderChainBroken(i));
            }
        } else if h.header.prev_state_root != genesis_root {
            return Err(VerifyError::GenesisMismatch);
        }
        h.verify(expected_signer)
            .map_err(|_| VerifyError::InvalidSignature(h.header.block_n))?;
        prev_hash = Some(h.full_hash());
    }
    let head = &headers[headers.len() - 1].header;
    if !SparseMerkleTree::verify_proof(&head.new_state_root, account_pubkey, proof) {
        return Err(VerifyError::ProofFailed);
    }
    let bytes = &proof.value;
    if bytes.len() == 64 {
        let mut a = Account::default();
        a.bytes.copy_from_slice(bytes);
        Ok(a.balance())
    } else {
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psl_crypto::{KeyPair, SparseMerkleTree};

    #[test]
    fn verify_balance_round_trip() {
        let kp = KeyPair::from_seed([7u8; 32]);
        let mut smt = SparseMerkleTree::new();
        let key = [42u8; 32];
        let mut acc = Account::new(key);
        acc.set_balance(1_000_000);
        smt.put(key, acc.bytes.to_vec());
        let proof = smt.proof(&key);
        let new_root = smt.root();

        let header = header::Header {
            block_n: 0,
            parent_hash: [0u8; 32],
            prev_state_root: [0u8; 32],
            tx_list_hash: [0u8; 32],
            trace_hash: [0u8; 32],
            new_state_root: new_root,
            issuer_registry_root: [0u8; 32],
            timestamp_ms: 1,
            sequencer_pubkey: kp.public(),
        };
        let signed = header::SignedHeader::sign(header, &kp);
        let bal = verify_balance([0u8; 32], &[signed], &kp.public(), &key, &proof).unwrap();
        assert_eq!(bal, 1_000_000);
    }
}
