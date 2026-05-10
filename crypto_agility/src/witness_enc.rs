//! Forward-secret witness encryption per ADR-0011.
//!
//! Wraps [`crate::kem::HybridX25519MlKem768Kem`] with HKDF-SHA-512
//! KDF combiner + AES-256-GCM AEAD + canonical wire format.
//!
//! ## Wire format (locked by ADR-0011)
//!
//! ```text
//! encrypted_blob := varint(scheme_id)        // KemSchemeId, 0x02 = HybridX25519MlKem768
//!                || eph_x25519_pubkey         // 32 bytes
//!                || mlkem_ciphertext          // 1088 bytes per FIPS 203
//!                || nonce                     // 12 bytes (AES-GCM nonce)
//!                || aead_ciphertext           // variable; 16-byte trailing AEAD tag
//! ```
//!
//! Single-axis versioning: any wire-format change gets a new
//! `scheme_id` discriminant. No `format_version` byte; the
//! discriminant is the version. Per ADR-0011 § "Versioning rule".
//!
//! ## Decoder hard-fail rules
//!
//! - Unknown `scheme_id` → [`crate::errors::KemError::SchemeNotSupported`].
//! - Length below the minimum → [`crate::errors::KemError::TruncatedBlob`].
//! - AEAD authentication failure → [`crate::errors::KemError::AuthenticationFailed`].
//!
//! No silent truncation, no padding tolerance.
//!
//! ## Context strings (domain separation)
//!
//! Each use of the hybrid KEM derives keys via HKDF with a context-
//! string `info` parameter so the same shared secrets in different
//! contexts produce different keys. Per ADR-0011:
//!
//! - [`CONTEXT_WITNESS_ENC`] = `"PSL-WitnessEnc-v1"` — compliance-
//!   private witness payload encryption.
//! - [`CONTEXT_VIEW_KEY`] = `"PSL-ViewKey-v1"` — regulator view-
//!   key delivery.
//! - [`CONTEXT_TRAVEL_RULE`] = `"PSL-TravelRule-v1"` — travel-rule
//!   metadata encryption.
//!
//! Historical context strings stay active forever for decryption
//! (old encrypted material is not re-encrypted on context-version
//! bumps; both versions remain first-class decryption paths).

use crate::errors::KemError;

/// Domain-separation context for hybrid-KEM-derived AEAD keys. Per
/// ADR-0011 Refinement 9: contexts are a typed enum, not raw `&[u8]`
/// constants at call sites. Adding a new context requires adding a
/// variant here, which requires touching this file — the natural
/// speed bump that enforces "borderline contexts require an ADR
/// first."
///
/// **Historical context strings stay active forever.** When a v2 of
/// any context launches (e.g., `WitnessEncV2`), the v1 variant
/// remains a legitimate decryption path for as long as historical
/// encrypted material exists. Old encrypted witnesses are not
/// re-encrypted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContextString {
    /// Compliance-private witness payload encryption (v1).
    WitnessEncV1,
    /// Regulator view-key delivery (v1).
    ViewKeyV1,
    /// Travel-rule metadata encryption (v1).
    TravelRuleV1,
}

impl ContextString {
    /// On-wire bytes used as the HKDF `info` parameter for AEAD-key
    /// derivation. Per ADR-0011 Refinement 9 + § "Context strings".
    pub fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::WitnessEncV1 => b"PSL-WitnessEnc-v1",
            Self::ViewKeyV1 => b"PSL-ViewKey-v1",
            Self::TravelRuleV1 => b"PSL-TravelRule-v1",
        }
    }
}

/// HKDF salt — fixed domain-separation tag per ADR-0011. **Locked.**
pub const HKDF_SALT: &[u8] = b"PSL-hybrid-kem-salt-v1";

/// Length of the X25519 ephemeral public key on the wire.
pub const EPH_X25519_PUBKEY_BYTES: usize = 32;
/// Length of the ML-KEM-768 ciphertext per FIPS 203.
pub const MLKEM768_CIPHERTEXT_BYTES: usize = 1088;
/// Length of the AES-256-GCM nonce.
pub const AEAD_NONCE_BYTES: usize = 12;
/// Length of the AES-256-GCM authentication tag.
pub const AEAD_TAG_BYTES: usize = 16;
/// Minimum encrypted-blob length (with empty plaintext).
/// = 1 (varint scheme_id) + 32 + 1088 + 12 + 16
pub const MIN_BLOB_BYTES: usize =
    1 + EPH_X25519_PUBKEY_BYTES + MLKEM768_CIPHERTEXT_BYTES + AEAD_NONCE_BYTES + AEAD_TAG_BYTES;

use crate::kem::{HybridX25519MlKem768Kem, MlKemPublicKey, X25519PublicKey};

/// Encrypt `plaintext` for the recipient under the given typed
/// context. AEAD AAD is `additional_data` (typically the encrypted
/// blob's identity in the surrounding protocol).
///
/// Returns the wire-format encrypted blob per the module docstring.
///
/// **Skeleton.** Implementation lands in the next commit. The
/// signature is locked per ADR-0011 so the surface is reviewable
/// against the spec — note typed parameters per Refinement 7 + 9
/// (recipient-key types instead of raw `&[u8]`; `ContextString`
/// enum instead of raw context bytes).
pub fn encrypt(
    _recipient_x25519_pk: &X25519PublicKey,
    _recipient_ml_kem_pk: &MlKemPublicKey,
    _plaintext: &[u8],
    _additional_data: &[u8],
    _context: ContextString,
) -> Result<Vec<u8>, KemError> {
    unimplemented!(
        "witness_enc::encrypt impl lands in the next commit on this branch \
         per ADR-0011 § Implementation commit order"
    )
}

/// Decrypt a wire-format encrypted blob using the recipient's typed
/// hybrid secret keys + the context used at encryption time. AEAD
/// AAD must match what was passed to `encrypt`.
///
/// Returns the plaintext on successful AEAD authentication. Returns
/// [`KemError::AuthenticationFailed`] for any failure (wrong sk,
/// tampered ciphertext, swapped components, wrong context,
/// malformed AEAD tag) — see ADR-0011 § "Decapsulation semantics"
/// for why the rejection point is the AEAD layer.
///
/// **Skeleton.** Implementation lands in the next commit.
pub fn decrypt(
    _recipient_x25519_sk: &crate::kem::RecipientX25519SecretKey,
    _recipient_ml_kem_sk: &crate::kem::RecipientMlKemSecretKey,
    _encrypted_blob: &[u8],
    _additional_data: &[u8],
    _context: ContextString,
) -> Result<Vec<u8>, KemError> {
    unimplemented!(
        "witness_enc::decrypt impl lands in the next commit on this branch \
         per ADR-0011 § Implementation commit order"
    )
}

/// **Centralized transcript construction** per ADR-0011 Refinement
/// 10. Single source of truth so encap-side and decap-side cannot
/// drift. Byte order documented inline matching ADR § "KDF combiner
/// specification".
///
/// ```text
/// transcript := varint(scheme_id)              // KemSchemeId, locked at 0x02
///            || x25519_shared_secret            // 32 bytes
///            || ml_kem_shared_secret            // 32 bytes
///            || x25519_ephemeral_pubkey         // 32 bytes
///            || ml_kem_ciphertext               // 1088 bytes
/// ```
///
/// Order is locked: shared secrets first in classical-then-PQ
/// order, then binding material in the same order. Matches IETF
/// `draft-ietf-tls-hybrid-design` byte-for-byte.
///
/// **Skeleton.** Implementation lands in the next commit; signature
/// + doc lock the contract.
#[allow(dead_code)] // Used by encrypt/decrypt impl in the next commit.
pub(crate) fn build_kem_transcript(
    _x25519_shared_secret: &[u8; 32],
    _ml_kem_shared_secret: &[u8; 32],
    _x25519_ephemeral_pubkey: &[u8; 32],
    _ml_kem_ciphertext: &[u8],
) -> Vec<u8> {
    unimplemented!(
        "build_kem_transcript impl lands in the next commit per \
         ADR-0011 § KDF combiner specification"
    )
}

#[allow(unused_imports)]
use HybridX25519MlKem768Kem as _;

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity: the canonical context strings are byte-distinct so
    /// HKDF produces different derived keys for different contexts.
    /// (The actual KDF-domain-separation property is tested in the
    /// wrong-context test #5 once impl lands.)
    #[test]
    fn context_strings_are_byte_distinct() {
        let we = ContextString::WitnessEncV1.as_bytes();
        let vk = ContextString::ViewKeyV1.as_bytes();
        let tr = ContextString::TravelRuleV1.as_bytes();
        assert_ne!(we, vk);
        assert_ne!(vk, tr);
        assert_ne!(we, tr);
    }

    /// Sanity: the minimum blob length matches ADR-0011's wire
    /// format: 1 + 32 + 1088 + 12 + 16 = 1149 bytes.
    #[test]
    fn min_blob_bytes_matches_adr_0011_wire_format() {
        assert_eq!(MIN_BLOB_BYTES, 1149);
    }

    /// Sanity: ML-KEM-768 ciphertext size matches FIPS 203 §6.1.
    #[test]
    fn mlkem768_ciphertext_size_matches_fips_203() {
        assert_eq!(MLKEM768_CIPHERTEXT_BYTES, 1088);
    }
}
