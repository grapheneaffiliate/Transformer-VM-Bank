//! Key revocation. Parent signs a revocation tx; sequencer commits the
//! pubkey + reason hash to a dedicated revocation subtree; mempool
//! rejects all subsequent transactions signed by revoked keys.
//!
//! Revocation is **monotonic**: once a key is in the set, it stays.
//! The only way to "un-revoke" is for the parent to issue a fresh
//! child key with a new pubkey. This invariant is exercised by
//! `revocation_is_monotonic` test below; the corresponding Lean theorem
//! is a follow-up.

use crate::error::WalletError;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use std::collections::HashMap;

/// One revocation entry — signed by the parent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Revocation {
    #[serde(with = "BigArray")]
    pub revoked_pubkey: [u8; 32],
    #[serde(with = "BigArray")]
    pub parent_pubkey: [u8; 32],
    /// 32-byte hash committing to off-chain documentation of why this
    /// key was revoked (e.g., compromise reason). PSL only stores the
    /// hash; the document itself lives at the issuer / regulator.
    pub reason_hash: [u8; 32],
    pub revoked_at_unix: u64,
    #[serde(with = "BigArray")]
    pub sig: [u8; 64],
}

impl Revocation {
    fn canonical_bytes(
        revoked_pubkey: &[u8; 32],
        parent_pubkey: &[u8; 32],
        reason_hash: &[u8; 32],
        revoked_at_unix: u64,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(128);
        out.extend_from_slice(b"PSL-KEY-REVOCATION-V1");
        out.extend_from_slice(revoked_pubkey);
        out.extend_from_slice(parent_pubkey);
        out.extend_from_slice(reason_hash);
        out.extend_from_slice(&revoked_at_unix.to_be_bytes());
        out
    }

    pub fn sign(
        parent: &SigningKey,
        revoked_pubkey: [u8; 32],
        reason_hash: [u8; 32],
        revoked_at_unix: u64,
    ) -> Self {
        let parent_pubkey = parent.verifying_key().to_bytes();
        let body = Self::canonical_bytes(
            &revoked_pubkey,
            &parent_pubkey,
            &reason_hash,
            revoked_at_unix,
        );
        let sig = parent.sign(&body);
        Self {
            revoked_pubkey,
            parent_pubkey,
            reason_hash,
            revoked_at_unix,
            sig: sig.to_bytes(),
        }
    }

    pub fn verify(&self) -> Result<(), WalletError> {
        let parent_pk = VerifyingKey::from_bytes(&self.parent_pubkey)
            .map_err(|e| WalletError::Ed25519(format!("parent pubkey: {e}")))?;
        let sig = Signature::from_bytes(&self.sig);
        let body = Self::canonical_bytes(
            &self.revoked_pubkey,
            &self.parent_pubkey,
            &self.reason_hash,
            self.revoked_at_unix,
        );
        parent_pk
            .verify(&body, &sig)
            .map_err(|_| WalletError::SignatureInvalid)
    }
}

/// In-memory revocation set. Production uses an MPT subtree; this
/// type is the runtime view used by the mempool to reject revoked
/// keys quickly.
#[derive(Default, Debug)]
pub struct RevocationSet {
    /// pubkey → first revocation that landed (we keep the first one
    /// because revocation is monotonic).
    by_pubkey: HashMap<[u8; 32], Revocation>,
}

impl RevocationSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a revocation. Verifies the signature and the
    /// monotonicity invariant. Returns `Ok(true)` if the revocation
    /// was newly added, `Ok(false)` if the pubkey was already revoked
    /// (no-op, monotonic), or an error if the signature is invalid.
    pub fn insert(&mut self, rev: Revocation) -> Result<bool, WalletError> {
        rev.verify()?;
        if self.by_pubkey.contains_key(&rev.revoked_pubkey) {
            return Ok(false);
        }
        self.by_pubkey.insert(rev.revoked_pubkey, rev);
        Ok(true)
    }

    pub fn is_revoked(&self, pubkey: &[u8; 32]) -> bool {
        self.by_pubkey.contains_key(pubkey)
    }

    /// Mempool gate: refuse the call if `pubkey` has been revoked.
    pub fn check(&self, pubkey: &[u8; 32]) -> Result<(), WalletError> {
        if self.is_revoked(pubkey) {
            Err(WalletError::KeyRevoked { pubkey: *pubkey })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::SeedableRng;

    fn sk(seed: u64) -> SigningKey {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        SigningKey::generate(&mut rng)
    }

    #[test]
    fn signed_revocation_round_trips() {
        let parent = sk(1);
        let child = sk(2);
        let rev = Revocation::sign(
            &parent,
            child.verifying_key().to_bytes(),
            [0xa1u8; 32],
            123456,
        );
        rev.verify().unwrap();
    }

    #[test]
    fn forged_revocation_rejected() {
        let parent = sk(1);
        let attacker = sk(99);
        let child = sk(2);
        // attacker signs over the same body but using their own key
        let body = Revocation::canonical_bytes(
            &child.verifying_key().to_bytes(),
            &parent.verifying_key().to_bytes(), // claims parent
            &[0u8; 32],
            0,
        );
        let bad_sig = attacker.sign(&body);
        let rev = Revocation {
            revoked_pubkey: child.verifying_key().to_bytes(),
            parent_pubkey: parent.verifying_key().to_bytes(),
            reason_hash: [0u8; 32],
            revoked_at_unix: 0,
            sig: bad_sig.to_bytes(),
        };
        assert!(matches!(rev.verify(), Err(WalletError::SignatureInvalid)));
    }

    #[test]
    fn revocation_is_monotonic() {
        let parent = sk(1);
        let child = sk(2);
        let mut set = RevocationSet::new();
        let rev1 = Revocation::sign(&parent, child.verifying_key().to_bytes(), [1u8; 32], 100);
        assert!(set.insert(rev1).unwrap()); // newly added
                                            // Even with a "newer" reason, monotonicity says: still revoked, no-op insert
        let rev2 = Revocation::sign(&parent, child.verifying_key().to_bytes(), [2u8; 32], 200);
        assert!(!set.insert(rev2).unwrap()); // not added (already present)
        assert!(set.is_revoked(&child.verifying_key().to_bytes()));
        // Cannot bypass — there is no `un-revoke` API on `RevocationSet`.
    }

    #[test]
    fn revoked_key_rejected_by_mempool_gate() {
        let parent = sk(1);
        let child = sk(2);
        let mut set = RevocationSet::new();
        let rev = Revocation::sign(&parent, child.verifying_key().to_bytes(), [0u8; 32], 0);
        set.insert(rev).unwrap();
        let r = set.check(&child.verifying_key().to_bytes());
        assert!(matches!(r, Err(WalletError::KeyRevoked { .. })));
    }
}
