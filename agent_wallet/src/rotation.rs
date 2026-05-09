//! Key rotation. The parent issues a `KeyRotation` tx that:
//!   1. revokes the old child pubkey (via the existing
//!      `revocation::Revocation` mechanism, monotonic).
//!   2. binds a new child pubkey to the same policy (or a new one).
//!   3. publishes the (old → new) mapping so outstanding contracts
//!      that reference the old pubkey can be migrated by separate
//!      issuer-signed migration txs.
//!
//! The rotation tx itself does not move the contracts — that is a
//! separate signed action. Rotation only commits the mapping. This
//! keeps each on-chain action small and atomic.

use crate::error::WalletError;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyRotation {
    #[serde(with = "BigArray")]
    pub parent_pubkey: [u8; 32],
    #[serde(with = "BigArray")]
    pub old_child_pubkey: [u8; 32],
    #[serde(with = "BigArray")]
    pub new_child_pubkey: [u8; 32],
    pub rotated_at_unix: u64,
    /// Monotonic version paired with the matching new policy version.
    pub policy_version: u64,
    #[serde(with = "BigArray")]
    pub sig: [u8; 64],
}

impl KeyRotation {
    fn canonical_bytes(
        parent_pubkey: &[u8; 32],
        old_child_pubkey: &[u8; 32],
        new_child_pubkey: &[u8; 32],
        rotated_at_unix: u64,
        policy_version: u64,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(160);
        out.extend_from_slice(b"PSL-KEY-ROTATION-V1");
        out.extend_from_slice(parent_pubkey);
        out.extend_from_slice(old_child_pubkey);
        out.extend_from_slice(new_child_pubkey);
        out.extend_from_slice(&rotated_at_unix.to_be_bytes());
        out.extend_from_slice(&policy_version.to_be_bytes());
        out
    }

    pub fn sign(
        parent: &SigningKey,
        old_child_pubkey: [u8; 32],
        new_child_pubkey: [u8; 32],
        rotated_at_unix: u64,
        policy_version: u64,
    ) -> Self {
        let parent_pubkey = parent.verifying_key().to_bytes();
        let body = Self::canonical_bytes(
            &parent_pubkey,
            &old_child_pubkey,
            &new_child_pubkey,
            rotated_at_unix,
            policy_version,
        );
        let sig = parent.sign(&body);
        Self {
            parent_pubkey,
            old_child_pubkey,
            new_child_pubkey,
            rotated_at_unix,
            policy_version,
            sig: sig.to_bytes(),
        }
    }

    pub fn verify(&self) -> Result<(), WalletError> {
        let parent_pk = VerifyingKey::from_bytes(&self.parent_pubkey)
            .map_err(|e| WalletError::Ed25519(format!("parent pubkey: {e}")))?;
        let sig = Signature::from_bytes(&self.sig);
        let body = Self::canonical_bytes(
            &self.parent_pubkey,
            &self.old_child_pubkey,
            &self.new_child_pubkey,
            self.rotated_at_unix,
            self.policy_version,
        );
        parent_pk.verify(&body, &sig).map_err(|_| WalletError::SignatureInvalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn sk(seed: u64) -> SigningKey {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        SigningKey::generate(&mut rng)
    }

    #[test]
    fn signed_rotation_round_trips() {
        let parent = sk(1);
        let old_child = sk(2);
        let new_child = sk(3);
        let r = KeyRotation::sign(
            &parent,
            old_child.verifying_key().to_bytes(),
            new_child.verifying_key().to_bytes(),
            1234,
            7,
        );
        r.verify().unwrap();
    }

    #[test]
    fn forged_rotation_rejected() {
        let parent = sk(1);
        let attacker = sk(99);
        let old_child = sk(2);
        let new_child = sk(3);
        let body = KeyRotation::canonical_bytes(
            &parent.verifying_key().to_bytes(),
            &old_child.verifying_key().to_bytes(),
            &new_child.verifying_key().to_bytes(),
            1234,
            7,
        );
        let bad_sig = attacker.sign(&body);
        let r = KeyRotation {
            parent_pubkey: parent.verifying_key().to_bytes(),
            old_child_pubkey: old_child.verifying_key().to_bytes(),
            new_child_pubkey: new_child.verifying_key().to_bytes(),
            rotated_at_unix: 1234,
            policy_version: 7,
            sig: bad_sig.to_bytes(),
        };
        assert!(matches!(r.verify(), Err(WalletError::SignatureInvalid)));
    }
}
