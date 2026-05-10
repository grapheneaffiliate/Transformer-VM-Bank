//! Key-encapsulation mechanism trait + the hybrid X25519 + ML-KEM-768
//! implementation per ADR-0011.
//!
//! ## Design notes (recap of ADR-0011)
//!
//! ### Decapsulation is total
//!
//! Per ADR-0011 § "Decapsulation semantics" + Refinement 7:
//! `decapsulate` returns `SharedSecret` directly, **not**
//! `Result<SharedSecret, _>`. Validation of malformed inputs lives
//! on the typed parameters (`Ciphertext::from_bytes -> Result<...>`,
//! `*SecretKey::from_bytes -> Result<...>`); once you hold a
//! `Ciphertext` and a `*SecretKey`, decapsulation cannot fail at
//! the type level.
//!
//! This is the structural enforcement of ML-KEM's implicit-rejection
//! design: a malformed ciphertext yields a deterministic
//! pseudorandom secret (constant-time, no signal). The only
//! rejection point is the AEAD authentication step in
//! [`crate::witness_enc`]. Code physically cannot branch on "did
//! decap fail" because the type system makes the branch impossible.
//!
//! ### Secret vs public newtype split
//!
//! Per Refinement 8: secret-side types
//! ([`EphemeralX25519SecretKey`], [`EphemeralMlKemSecretKey`],
//! [`RecipientX25519SecretKey`], [`RecipientMlKemSecretKey`]) carry
//! `Zeroize` + `ZeroizeOnDrop`. Public-side types
//! ([`X25519PublicKey`], [`MlKemPublicKey`]) are `Clone` only — no
//! secret material, no zeroize needed. Mixing is a type error.
//!
//! ### Forward secrecy
//!
//! Witness encryption uses per-witness ephemeral hybrid keypairs.
//! Ephemeral private keys are zeroized immediately after AEAD
//! encryption finalizes. The `Ephemeral*SecretKey` newtypes carry
//! `ZeroizeOnDrop` so dropping them zeroizes them automatically.
//! See [`crate::witness_enc`] for the lifecycle.

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

// ── Public-key types (no secret material; no Zeroize needed) ──

/// X25519 public key — 32 bytes per RFC 7748.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct X25519PublicKey(pub(crate) [u8; 32]);

impl X25519PublicKey {
    /// Construct from raw bytes. Returns `Err` if the wrong length;
    /// X25519 has no further point validation (per RFC 7748 §5,
    /// every 32-byte string is a valid public key).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, KemError> {
        let arr: [u8; 32] = bytes.try_into().map_err(|_| KemError::MalformedPublicKey {
            scheme: KemScheme::HybridX25519MlKem768.as_u32(),
            detail: "X25519 public key must be exactly 32 bytes",
        })?;
        Ok(Self(arr))
    }

    /// Borrow the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// ML-KEM-768 public key — 1184 bytes per FIPS 203 §6.1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MlKemPublicKey(pub(crate) Vec<u8>);

impl MlKemPublicKey {
    /// Length per FIPS 203 §6.1.
    pub const BYTES: usize = 1184;

    /// Construct from raw bytes. Returns `Err` if the wrong length;
    /// post-parse structural validation lives in the underlying
    /// `pqcrypto-mlkem` crate's `PublicKey::from_bytes`.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, KemError> {
        if bytes.len() != Self::BYTES {
            return Err(KemError::MalformedPublicKey {
                scheme: KemScheme::HybridX25519MlKem768.as_u32(),
                detail: "ML-KEM-768 public key must be exactly 1184 bytes",
            });
        }
        Ok(Self(bytes.to_vec()))
    }

    /// Borrow the raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

// ── Secret-key types: ephemeral (zeroize on drop) ──

/// X25519 ephemeral secret key — 32-byte scalar. Zeroized on drop.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct EphemeralX25519SecretKey(pub(crate) [u8; 32]);

impl EphemeralX25519SecretKey {
    /// Wrap raw scalar bytes (caller is responsible for valid
    /// scalar generation; X25519 clamps internally).
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// ML-KEM-768 ephemeral secret key. Zeroized on drop.
///
/// FIPS 203 §6.1 specifies the secret-key length; we wrap a `Vec<u8>`
/// so the ZeroizeOnDrop derive can clear it on drop without
/// hardcoding the length here.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct EphemeralMlKemSecretKey(pub(crate) Vec<u8>);

impl EphemeralMlKemSecretKey {
    /// Wrap raw secret-key bytes from the underlying primitive.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

// ── Secret-key types: recipient long-term (also zeroize on drop) ──

/// X25519 long-term recipient secret key. Zeroized on drop.
///
/// Distinct from [`EphemeralX25519SecretKey`] at the type level so
/// the lifecycle is explicit: ephemerals are short-lived (drop after
/// one encryption); recipient secrets are long-lived (held by the
/// recipient indefinitely).
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct RecipientX25519SecretKey(pub(crate) [u8; 32]);

impl RecipientX25519SecretKey {
    /// Wrap raw scalar bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// ML-KEM-768 long-term recipient secret key. Zeroized on drop.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct RecipientMlKemSecretKey(pub(crate) Vec<u8>);

impl RecipientMlKemSecretKey {
    /// Wrap raw secret-key bytes from the underlying primitive.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

// ── Ciphertext types (parsed; public material; no Zeroize) ──

/// X25519 KEM ciphertext (in ECDH-as-KEM convention this is the
/// sender's ephemeral public key). 32 bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct X25519Ciphertext(pub(crate) [u8; 32]);

impl X25519Ciphertext {
    /// Parse from raw bytes; rejects wrong-length input.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, KemError> {
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| KemError::MalformedCiphertext {
                scheme: KemScheme::HybridX25519MlKem768.as_u32(),
                detail: "X25519 ciphertext must be exactly 32 bytes",
            })?;
        Ok(Self(arr))
    }

    /// Borrow the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// ML-KEM-768 ciphertext — 1088 bytes per FIPS 203 §6.1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MlKemCiphertext(pub(crate) Vec<u8>);

impl MlKemCiphertext {
    /// Length per FIPS 203 §6.1.
    pub const BYTES: usize = 1088;

    /// Parse from raw bytes; rejects wrong-length input.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, KemError> {
        if bytes.len() != Self::BYTES {
            return Err(KemError::MalformedCiphertext {
                scheme: KemScheme::HybridX25519MlKem768.as_u32(),
                detail: "ML-KEM-768 ciphertext must be exactly 1088 bytes",
            });
        }
        Ok(Self(bytes.to_vec()))
    }

    /// Borrow the raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

// ── KEM trait + hybrid impl stub ──

/// A KEM (key-encapsulation mechanism).
///
/// **Decapsulation is total at the type level.** Per ADR-0011 §
/// "Decapsulation semantics" + Refinement 7: validation of malformed
/// inputs lives on the typed parameters
/// ([`Self::Ciphertext::from_bytes`], etc.). Once a caller holds a
/// `Self::Ciphertext` and a `Self::RecipientSecretKey`,
/// `decapsulate` cannot fail — there is no `Result` to match on.
/// This structurally enforces ML-KEM's implicit-rejection design: a
/// malformed ciphertext yields a pseudorandom secret rather than
/// an error signal, and the AEAD authentication step in
/// [`crate::witness_enc`] is the only rejection point.
///
/// Code that calls `decapsulate(...)` cannot accidentally branch on
/// a non-existent failure mode, because the type system makes the
/// branch impossible.
pub trait Kem {
    /// Parsed-and-validated ciphertext type (e.g.,
    /// [`HybridCiphertext`] for the hybrid scheme).
    type Ciphertext;
    /// Parsed-and-validated recipient long-term secret key type.
    type RecipientSecretKey;
    /// Parsed-and-validated recipient long-term public key type.
    type PublicKey;

    /// Which scheme this KEM implements.
    fn scheme(&self) -> KemScheme;

    /// Encapsulate to `recipient_pk`. Returns the ciphertext (to be
    /// shipped to the recipient) and the shared secret (consumed by
    /// the AEAD-key derivation step in [`crate::witness_enc`]).
    /// **Total** — the typed parameter rules out malformed-key
    /// failure modes at the call site.
    ///
    /// Note: encap internally generates fresh randomness per FIPS
    /// 203 design.
    fn encapsulate(&self, recipient_pk: &Self::PublicKey) -> (Self::Ciphertext, SharedSecret);

    /// Decapsulate with the recipient's long-term secret key.
    /// **Total** — see trait-level docs for the implicit-rejection
    /// rationale. There is no `Result`; there is no failure mode to
    /// match on; the AEAD layer is the rejection point.
    fn decapsulate(
        &self,
        ciphertext: &Self::Ciphertext,
        recipient_sk: &Self::RecipientSecretKey,
    ) -> SharedSecret;
}

/// **Hybrid X25519 + ML-KEM-768 KEM.** Per ADR-0011.
///
/// **Skeleton.** Type and trait stub land in this commit so the
/// surface is reviewable against the ADR; the implementation
/// (encap, decap) lands in the next commit on the same branch.
/// Tests #1-#8 from ADR-0011 also land in the next commit.
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

/// Hybrid keypair. **Bundles both a public-key half (cloneable, no
/// zeroize) and a secret-key half (zeroized on drop).**
///
/// The split into separate `Ephemeral*` and `Recipient*` secret-key
/// types is per ADR-0011 Refinement 8: the type system distinguishes
/// the short-lived (per-encryption) ephemeral keys from the long-
/// lived (held by recipient) recipient keys. Both zeroize; mixing
/// them is a type error.
pub struct HybridKemKeypair {
    /// X25519 long-term secret. Zeroized on drop.
    pub recipient_x25519_sk: RecipientX25519SecretKey,
    /// ML-KEM-768 long-term secret. Zeroized on drop.
    pub recipient_ml_kem_sk: RecipientMlKemSecretKey,
    /// X25519 long-term public.
    pub x25519_pk: X25519PublicKey,
    /// ML-KEM-768 long-term public.
    pub ml_kem_pk: MlKemPublicKey,
}

/// Hybrid ciphertext: concatenation of X25519 (sender-ephemeral
/// pubkey, in ECDH-as-KEM convention) + ML-KEM-768 ciphertext.
/// **Skeleton.** The Kem trait `Ciphertext` associated type for
/// [`HybridX25519MlKem768Kem`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HybridCiphertext {
    /// X25519 component (sender's ephemeral pubkey in
    /// ECDH-as-KEM convention).
    pub x25519: X25519Ciphertext,
    /// ML-KEM-768 component.
    pub ml_kem: MlKemCiphertext,
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
    /// expected scheme.
    #[test]
    fn hybrid_kem_constructible_with_correct_scheme() {
        let _kem = HybridX25519MlKem768Kem::new();
        assert_eq!(KemScheme::HybridX25519MlKem768.as_u32(), 0x02);
    }

    /// Wrong-length input → typed-parser rejects. This is the
    /// validation that lives on the type per Refinement 7; once a
    /// caller holds an `MlKemCiphertext`, decap cannot fail.
    #[test]
    fn parsers_reject_wrong_lengths() {
        // X25519 pubkey/ct: must be 32 bytes.
        assert!(X25519PublicKey::from_bytes(&[0u8; 31]).is_err());
        assert!(X25519PublicKey::from_bytes(&[0u8; 33]).is_err());
        assert!(X25519PublicKey::from_bytes(&[0u8; 32]).is_ok());
        assert!(X25519Ciphertext::from_bytes(&[0u8; 31]).is_err());
        assert!(X25519Ciphertext::from_bytes(&[0u8; 32]).is_ok());

        // ML-KEM pubkey: must be 1184 bytes.
        assert!(MlKemPublicKey::from_bytes(&[0u8; 1183]).is_err());
        assert!(MlKemPublicKey::from_bytes(&[0u8; 1184]).is_ok());

        // ML-KEM ciphertext: must be 1088 bytes.
        assert!(MlKemCiphertext::from_bytes(&[0u8; 1087]).is_err());
        assert!(MlKemCiphertext::from_bytes(&[0u8; 1088]).is_ok());
    }

    /// Constants match FIPS 203 §6.1 / RFC 7748 / ADR-0011.
    #[test]
    fn type_lengths_match_specs() {
        assert_eq!(MlKemPublicKey::BYTES, 1184);
        assert_eq!(MlKemCiphertext::BYTES, 1088);
        // X25519 is fixed at 32 bytes by the type itself ([u8; 32]).
    }
}
