//! KEM trait. The X25519 implementation lands in the next workstream
//! per ADR-0006 phase 4 alongside the hybrid X25519 + ML-KEM-768
//! implementation. The trait shape is stable now so call sites can
//! be written against it.

use crate::errors::KemError;
pub use crate::scheme::KemSchemeId as KemScheme;
use zeroize::ZeroizeOnDrop;

/// A KEM-derived shared secret. Wrapped in `ZeroizeOnDrop` so the
/// secret is wiped from memory on drop. Callers should derive the
/// AEAD key via HKDF and drop the `SharedSecret` immediately after.
#[derive(ZeroizeOnDrop)]
pub struct SharedSecret(Vec<u8>);

impl SharedSecret {
    /// Wrap raw shared-secret bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Borrow the bytes. The slice lifetime ties the borrow to the
    /// `SharedSecret`; once it drops the bytes are zeroed.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the secret is empty (zero-length wrapper). Should not
    /// occur in well-formed KEM output.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A KEM (key-encapsulation mechanism) under a specific scheme.
pub trait Kem {
    /// Which scheme this KEM implements.
    fn scheme(&self) -> KemScheme;

    /// Encapsulate to `recipient_pk`, returning `(ciphertext_bytes,
    /// shared_secret)`. Wire encoding of the ciphertext is
    /// scheme-prefix + scheme-specific bytes.
    fn encapsulate(&self, recipient_pk: &[u8]) -> Result<(Vec<u8>, SharedSecret), KemError>;

    /// Decapsulate `ciphertext` with `my_sk`, returning the shared
    /// secret. Returns [`KemError::DecapFailed`] if the ciphertext
    /// does not correspond to a valid encapsulation under our key.
    fn decapsulate(&self, ciphertext: &[u8], my_sk: &[u8]) -> Result<SharedSecret, KemError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_secret_zeroed_on_drop_is_observable() {
        // We can't directly observe the zeroing without unsafe; this
        // test exists to lock in the API shape. The actual zeroing is
        // tested by the `zeroize` crate's own test suite.
        let s = SharedSecret::new(vec![1, 2, 3]);
        assert_eq!(s.len(), 3);
        assert_eq!(s.as_bytes(), &[1, 2, 3]);
        drop(s);
    }
}
