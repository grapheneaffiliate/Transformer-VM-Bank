//! ed25519 signature wrapper.
//!
//! Used for transaction signing (sender authorizes a tx), block-header
//! signing (sequencer or BFT validators sign committed blocks), and view-key
//! authentication (regulators authenticate to query account proofs).
//!
//! All sigs are verified natively — never inside the transformer trace.

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type PublicKey = [u8; 32];
pub type Signature = [u8; 64];

#[derive(Debug, Error)]
pub enum SigError {
    #[error("invalid public key bytes")]
    InvalidPublicKey,
    #[error("invalid signature bytes")]
    InvalidSignature,
    #[error("signature verification failed")]
    VerificationFailed,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct KeyPair {
    #[serde(with = "hex::serde")]
    secret: [u8; 32],
}

impl KeyPair {
    pub fn generate() -> Self {
        let signing = SigningKey::generate(&mut OsRng);
        Self { secret: signing.to_bytes() }
    }

    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self { secret: seed }
    }

    pub fn public(&self) -> PublicKey {
        let signing = SigningKey::from_bytes(&self.secret);
        signing.verifying_key().to_bytes()
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        let signing = SigningKey::from_bytes(&self.secret);
        signing.sign(message).to_bytes()
    }
}

pub fn sign(keypair: &KeyPair, message: &[u8]) -> Signature {
    keypair.sign(message)
}

pub fn verify(pubkey: &PublicKey, message: &[u8], signature: &Signature) -> Result<(), SigError> {
    let verifying = VerifyingKey::from_bytes(pubkey).map_err(|_| SigError::InvalidPublicKey)?;
    let sig = ed25519_dalek::Signature::from_bytes(signature);
    verifying
        .verify(message, &sig)
        .map_err(|_| SigError::VerificationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_round_trip() {
        let kp = KeyPair::generate();
        let pk = kp.public();
        let msg = b"transfer 100 to alice";
        let sig = kp.sign(msg);
        verify(&pk, msg, &sig).unwrap();
    }

    #[test]
    fn tampered_message_fails() {
        let kp = KeyPair::generate();
        let pk = kp.public();
        let sig = kp.sign(b"original");
        assert!(verify(&pk, b"tampered", &sig).is_err());
    }

    #[test]
    fn deterministic_from_seed() {
        let seed = [42u8; 32];
        let kp1 = KeyPair::from_seed(seed);
        let kp2 = KeyPair::from_seed(seed);
        assert_eq!(kp1.public(), kp2.public());
    }
}
