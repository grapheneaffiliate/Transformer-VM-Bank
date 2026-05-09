//! SLIP-0010 hierarchical key derivation for ed25519.
//!
//! Reference: <https://github.com/satoshilabs/slips/blob/master/slip-0010.md>
//!
//! Master from seed:
//!   I = HMAC-SHA512(key="ed25519 seed", data=seed)
//!   master_priv = I[..32]; master_chain_code = I[32..]
//!
//! ed25519 supports **hardened-only** child derivation. Index must
//! satisfy `index ≥ 0x80000000`.
//!
//!   data = 0x00 || parent_priv || index_be
//!   I    = HMAC-SHA512(key=parent_chain_code, data=data)
//!   child_priv = I[..32]; child_chain_code = I[32..]
//!
//! `Ed25519MasterKey` and `Ed25519ChildKey` carry the 32-byte private
//! scalar wrapped in `Zeroizing<[u8;32]>` so it clears on drop. Public
//! key is derived on demand via the `ed25519_dalek::SigningKey`.

use crate::error::WalletError;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hmac::{Hmac, Mac};
use sha2::Sha512;
use zeroize::Zeroizing;

type HmacSha512 = Hmac<Sha512>;

const HARDENED_OFFSET: u32 = 0x80000000;

/// 32-byte ed25519 master private key + 32-byte chain code.
#[derive(Clone)]
pub struct Ed25519MasterKey {
    private: Zeroizing<[u8; 32]>,
    chain_code: Zeroizing<[u8; 32]>,
}

impl Ed25519MasterKey {
    /// Derive the master key from a seed of 16, 32, or 64 bytes (per
    /// SLIP-0010). Other lengths are rejected to surface seed-format
    /// bugs at the call site.
    pub fn from_seed(seed: &[u8]) -> Result<Self, WalletError> {
        if !matches!(seed.len(), 16 | 32 | 64) {
            return Err(WalletError::BadSeedLength { got: seed.len() });
        }
        let mut mac = <HmacSha512 as Mac>::new_from_slice(b"ed25519 seed")
            .map_err(|e| WalletError::Ed25519(format!("hmac init: {e}")))?;
        mac.update(seed);
        let i = mac.finalize().into_bytes();
        let mut private = Zeroizing::new([0u8; 32]);
        private.copy_from_slice(&i[..32]);
        let mut chain_code = Zeroizing::new([0u8; 32]);
        chain_code.copy_from_slice(&i[32..]);
        Ok(Self { private, chain_code })
    }

    /// Derive a hardened child key. `index` must be ≥ 0x80000000.
    pub fn derive_child(&self, index: u32) -> Result<Ed25519ChildKey, WalletError> {
        derive_hardened(&self.private, &self.chain_code, index)
    }

    pub fn signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.private)
    }

    pub fn public_key(&self) -> VerifyingKey {
        self.signing_key().verifying_key()
    }
}

/// 32-byte ed25519 child private key + chain code (for further descent).
#[derive(Clone)]
pub struct Ed25519ChildKey {
    private: Zeroizing<[u8; 32]>,
    chain_code: Zeroizing<[u8; 32]>,
    pub index: u32,
}

impl Ed25519ChildKey {
    pub fn derive_child(&self, index: u32) -> Result<Ed25519ChildKey, WalletError> {
        derive_hardened(&self.private, &self.chain_code, index)
    }

    pub fn signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.private)
    }

    pub fn public_key(&self) -> VerifyingKey {
        self.signing_key().verifying_key()
    }

    pub fn sign(&self, msg: &[u8]) -> Signature {
        self.signing_key().sign(msg)
    }

    pub fn verify(pubkey: &VerifyingKey, msg: &[u8], sig: &Signature) -> Result<(), WalletError> {
        pubkey.verify(msg, sig).map_err(|_| WalletError::SignatureInvalid)
    }
}

fn derive_hardened(
    parent_priv: &[u8; 32],
    parent_chain: &[u8; 32],
    index: u32,
) -> Result<Ed25519ChildKey, WalletError> {
    if index < HARDENED_OFFSET {
        return Err(WalletError::NotHardened { index });
    }
    let mut mac = <HmacSha512 as Mac>::new_from_slice(parent_chain)
        .map_err(|e| WalletError::Ed25519(format!("hmac init: {e}")))?;
    mac.update(&[0u8]);
    mac.update(parent_priv);
    mac.update(&index.to_be_bytes());
    let i = mac.finalize().into_bytes();
    let mut private = Zeroizing::new([0u8; 32]);
    private.copy_from_slice(&i[..32]);
    let mut chain_code = Zeroizing::new([0u8; 32]);
    chain_code.copy_from_slice(&i[32..]);
    Ok(Ed25519ChildKey { private, chain_code, index })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SLIP-0010 ed25519 test vector #1, master + m/0' (hardened).
    /// From <https://github.com/satoshilabs/slips/blob/master/slip-0010.md>.
    #[test]
    fn slip10_test_vector_1_master() {
        let seed = hex::decode("000102030405060708090a0b0c0d0e0f").unwrap();
        let master = Ed25519MasterKey::from_seed(&seed).unwrap();
        let expected_priv =
            hex::decode("2b4be7f19ee27bbf30c667b642d5f4aa69fd169872f8fc3059c08ebae2eb19e7").unwrap();
        let expected_chain =
            hex::decode("90046a93de5380a72b5e45010748567d5ea02bbf6522f979e05c0d8d8ca9fffb").unwrap();
        assert_eq!(&master.private[..], &expected_priv[..]);
        assert_eq!(&master.chain_code[..], &expected_chain[..]);
    }

    #[test]
    fn slip10_test_vector_1_first_child() {
        let seed = hex::decode("000102030405060708090a0b0c0d0e0f").unwrap();
        let master = Ed25519MasterKey::from_seed(&seed).unwrap();
        let child = master.derive_child(HARDENED_OFFSET).unwrap();
        let expected_priv =
            hex::decode("68e0fe46dfb67e368c75379acec591dad19df3cde26e63b93a8e704f1dade7a3").unwrap();
        let expected_chain =
            hex::decode("8b59aa11380b624e81507a27fedda59fea6d0b779a778918a2fd3590e16e9c69").unwrap();
        assert_eq!(&child.private[..], &expected_priv[..]);
        assert_eq!(&child.chain_code[..], &expected_chain[..]);
    }

    #[test]
    fn non_hardened_index_rejected() {
        let seed = hex::decode("000102030405060708090a0b0c0d0e0f").unwrap();
        let master = Ed25519MasterKey::from_seed(&seed).unwrap();
        let r = master.derive_child(0); // not hardened
        assert!(matches!(r, Err(WalletError::NotHardened { index: 0 })));
    }

    #[test]
    fn bad_seed_length_rejected() {
        assert!(matches!(
            Ed25519MasterKey::from_seed(&[0u8; 8]),
            Err(WalletError::BadSeedLength { got: 8 })
        ));
        assert!(matches!(
            Ed25519MasterKey::from_seed(&[0u8; 31]),
            Err(WalletError::BadSeedLength { got: 31 })
        ));
        // valid lengths
        assert!(Ed25519MasterKey::from_seed(&[0u8; 16]).is_ok());
        assert!(Ed25519MasterKey::from_seed(&[0u8; 32]).is_ok());
        assert!(Ed25519MasterKey::from_seed(&[0u8; 64]).is_ok());
    }

    #[test]
    fn child_signs_and_verifies() {
        let seed = [0xa5u8; 32];
        let master = Ed25519MasterKey::from_seed(&seed).unwrap();
        let child = master.derive_child(HARDENED_OFFSET + 7).unwrap();
        let pk = child.public_key();
        let sig = child.sign(b"hello agents");
        assert!(Ed25519ChildKey::verify(&pk, b"hello agents", &sig).is_ok());
        // tamper
        assert!(Ed25519ChildKey::verify(&pk, b"hello agents!", &sig).is_err());
    }

    #[test]
    fn deeper_derivation_paths_are_deterministic() {
        let seed = [0u8; 32];
        let m = Ed25519MasterKey::from_seed(&seed).unwrap();
        let a = m
            .derive_child(HARDENED_OFFSET + 1)
            .unwrap()
            .derive_child(HARDENED_OFFSET + 2)
            .unwrap();
        let b = m
            .derive_child(HARDENED_OFFSET + 1)
            .unwrap()
            .derive_child(HARDENED_OFFSET + 2)
            .unwrap();
        assert_eq!(&a.private[..], &b.private[..]);
        assert_eq!(&a.chain_code[..], &b.chain_code[..]);
    }
}
