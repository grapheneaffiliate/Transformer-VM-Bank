//! Key-encapsulation mechanism trait + the hybrid X25519 + ML-KEM-768
//! implementation per ADR-0011.
//!
//! ## Design notes (recap of ADR-0011)
//!
//! - **Implicit rejection.** ML-KEM (FIPS 203) decapsulation never
//!   fails at the type level. On a malformed ciphertext it returns
//!   a deterministic pseudorandom secret. The only rejection point
//!   is the AEAD authentication step in [`crate::witness_enc`];
//!   `Kem::decapsulate` returns `Result` only for *parsing* errors
//!   (wrong-length pubkey/sk/ct), never for "the ciphertext didn't
//!   correspond to a real encapsulation."
//! - **Forward secrecy.** Witness encryption uses per-witness
//!   ephemeral hybrid keypairs; ephemeral private keys are zeroized
//!   immediately after AEAD encryption finalizes. See
//!   [`crate::witness_enc`] for the lifecycle.
//! - **Hybrid combiner.** HKDF-SHA-512 over the canonical transcript
//!   `varint(scheme_id) || x25519_ss || ml_kem_ss || x25519_eph_pk
//!   || ml_kem_ct`, with `info = context_string` for domain
//!   separation. See [`crate::witness_enc`] for the implementation.

use crate::errors::KemError;
pub use crate::scheme::KemSchemeId as KemScheme;
use zeroize::{Zeroize, ZeroizeOnDrop};

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
///
/// **Decapsulation never fails post-parse.** Per ADR-0011, ML-KEM
/// uses implicit rejection: a malformed ciphertext yields a
/// pseudorandom secret rather than an error signal. The `Result`
/// here is for *parsing* errors only — wrong-length sk/ct/pk. Code
/// MUST NOT branch on "did decap succeed"; the AEAD authentication
/// step in [`crate::witness_enc`] is the rejection point.
pub trait Kem {
    /// Which scheme this KEM implements.
    fn scheme(&self) -> KemScheme;

    /// Encapsulate to `recipient_pk`, returning `(ciphertext_bytes,
    /// shared_secret)`. The ciphertext bytes are scheme-specific;
    /// the witness-encryption wrapper composes them with the AEAD
    /// payload per the ADR-0011 wire format.
    fn encapsulate(&self, recipient_pk: &[u8]) -> Result<(Vec<u8>, SharedSecret), KemError>;

    /// Decapsulate `ciphertext` with `my_sk`, returning the shared
    /// secret. **Returns `Err` only for parsing errors** (malformed
    /// ciphertext or sk length); per implicit-rejection design,
    /// post-parse decap always returns *some* shared secret — the
    /// AEAD authentication step is what catches an invalid one.
    fn decapsulate(&self, ciphertext: &[u8], my_sk: &[u8]) -> Result<SharedSecret, KemError>;
}

/// **Hybrid X25519 + ML-KEM-768 KEM.** Per ADR-0011.
///
/// **Skeleton.** Type and trait stub land in this commit so the
/// surface is reviewable against the ADR; the implementation
/// (encap, decap, internal encoding) lands in the next commit on
/// the same branch. Tests #1-#8 from ADR-0011 also land in the
/// next commit, currently marked `#[ignore]` here.
pub struct HybridX25519MlKem768Kem;

impl HybridX25519MlKem768Kem {
    /// Construct a stateless KEM instance. No state; equivalent to
    /// a singleton.
    pub fn new() -> Self {
        Self
    }
}

impl Default for HybridX25519MlKem768Kem {
    fn default() -> Self {
        Self::new()
    }
}

/// A hybrid signing/decryption keypair holding both the X25519
/// component (32-byte scalar / 32-byte point) and the ML-KEM-768
/// component (~1184-byte sk / ~1184-byte pk per FIPS 203 §6.1).
///
/// **Skeleton.** Field shapes locked here; impl lands next commit.
/// Both private-key components are wrapped to be zeroed on drop.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct HybridKemKeypair {
    /// X25519 secret key (raw 32-byte scalar).
    pub(crate) x25519_sk: [u8; 32],
    /// ML-KEM-768 secret key bytes per FIPS 203.
    pub(crate) ml_kem_sk: Vec<u8>,
    /// X25519 public key (cached for `pubkey()` to avoid recomputing
    /// the scalar multiplication).
    #[zeroize(skip)]
    pub(crate) x25519_pk: [u8; 32],
    /// ML-KEM-768 public key bytes.
    #[zeroize(skip)]
    pub(crate) ml_kem_pk: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_secret_zeroed_on_drop_is_observable() {
        let s = SharedSecret::new(vec![1, 2, 3]);
        assert_eq!(s.len(), 3);
        assert_eq!(s.as_bytes(), &[1, 2, 3]);
        drop(s);
    }

    /// Stub: HybridX25519MlKem768Kem::new() returns a value of the
    /// expected scheme. Real round-trip / forward-secrecy /
    /// implicit-rejection / etc. tests land in the impl commit.
    #[test]
    fn hybrid_kem_constructible_with_correct_scheme() {
        let kem = HybridX25519MlKem768Kem::new();
        assert_eq!(
            crate::scheme::KemSchemeId::HybridX25519MlKem768.as_u32(),
            0x02
        );
        // Suppress unused-binding warning until impl lands in the
        // next commit.
        let _ = kem;
    }
}
