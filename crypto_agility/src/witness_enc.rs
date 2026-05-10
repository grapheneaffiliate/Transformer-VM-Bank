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

/// Context string for compliance-private witness payload encryption.
pub const CONTEXT_WITNESS_ENC: &[u8] = b"PSL-WitnessEnc-v1";

/// Context string for regulator view-key delivery.
pub const CONTEXT_VIEW_KEY: &[u8] = b"PSL-ViewKey-v1";

/// Context string for travel-rule metadata encryption.
pub const CONTEXT_TRAVEL_RULE: &[u8] = b"PSL-TravelRule-v1";

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

/// Encrypt `plaintext` for the recipient identified by `recipient_pk`
/// (the recipient's hybrid public key bytes), under the given
/// `context_string` for KDF domain separation. AEAD AAD is
/// `additional_data` (typically the encrypted blob's identity in
/// the surrounding protocol).
///
/// Returns the wire-format encrypted blob per the module docstring.
///
/// **Skeleton.** Implementation lands in the next commit. The
/// signature is locked per ADR-0011 so the surface is reviewable
/// against the spec.
pub fn encrypt(
    _recipient_pk: &[u8],
    _plaintext: &[u8],
    _additional_data: &[u8],
    _context_string: &[u8],
) -> Result<Vec<u8>, KemError> {
    unimplemented!(
        "witness_enc::encrypt impl lands in the next commit on this branch \
         per ADR-0011 § Implementation commit order"
    )
}

/// Decrypt a wire-format encrypted blob using the recipient's
/// hybrid secret key + the context_string used at encryption time.
/// AEAD AAD must match what was passed to `encrypt`.
///
/// Returns the plaintext on successful AEAD authentication.
/// Returns [`KemError::AuthenticationFailed`] for any failure
/// (wrong sk, tampered ciphertext, swapped components, wrong
/// context, malformed AEAD tag) — see ADR-0011 § "Decapsulation
/// semantics" for why the rejection point is the AEAD layer.
///
/// **Skeleton.** Implementation lands in the next commit.
pub fn decrypt(
    _recipient_sk: &[u8],
    _encrypted_blob: &[u8],
    _additional_data: &[u8],
    _context_string: &[u8],
) -> Result<Vec<u8>, KemError> {
    unimplemented!(
        "witness_enc::decrypt impl lands in the next commit on this branch \
         per ADR-0011 § Implementation commit order"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity: the canonical context strings are byte-distinct so
    /// HKDF produces different derived keys for different contexts.
    /// (The actual KDF-domain-separation property is tested in the
    /// wrong-context test #5 once impl lands.)
    #[test]
    fn context_strings_are_byte_distinct() {
        assert_ne!(CONTEXT_WITNESS_ENC, CONTEXT_VIEW_KEY);
        assert_ne!(CONTEXT_VIEW_KEY, CONTEXT_TRAVEL_RULE);
        assert_ne!(CONTEXT_WITNESS_ENC, CONTEXT_TRAVEL_RULE);
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
