//! Typed error variants for the agility layer.

use thiserror::Error;

/// Errors emitted by [`crate::Signer`] implementations.
#[derive(Debug, Error)]
pub enum SignerError {
    /// The scheme requested by the caller is not implemented in this
    /// build of `crypto_agility`. Carries the raw `u32` discriminant.
    #[error("signature scheme {0:#x} is not implemented in this build")]
    SchemeNotImplemented(u32),
    /// The signing key is malformed or has the wrong length for the
    /// chosen scheme.
    #[error("malformed signing key for scheme {scheme:#x}: {detail}")]
    MalformedKey {
        /// The scheme whose key validation failed.
        scheme: u32,
        /// Free-text detail (length, parity bit, etc.).
        detail: &'static str,
    },
    /// The underlying primitive returned an error during sign.
    #[error("primitive error during sign: {0}")]
    Primitive(String),
}

/// Errors emitted by [`crate::Verifier`] implementations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerifierError {
    /// The scheme prefix on the wire does not match a registered
    /// scheme. **Verifiers must never silently fall back**; this is
    /// the explicit rejection path per ADR-0007.
    #[error("unknown scheme {0:#x} on the wire (no fallback by policy)")]
    UnknownScheme(u32),
    /// The verifier's policy does not accept this scheme even though
    /// the scheme is registered. Used during migration windows.
    #[error("scheme {0:#x} is not in this verifier's accept-list")]
    SchemeNotAccepted(u32),
    /// The signature blob is malformed (wrong length for the scheme,
    /// truncated body, parse failure on the inner format).
    #[error("malformed signature for scheme {scheme:#x}: {detail}")]
    MalformedSignature {
        /// Scheme being verified.
        scheme: u32,
        /// Free-text detail.
        detail: &'static str,
    },
    /// The public-key blob is malformed.
    #[error("malformed public key for scheme {scheme:#x}: {detail}")]
    MalformedPublicKey {
        /// Scheme being verified.
        scheme: u32,
        /// Free-text detail.
        detail: &'static str,
    },
    /// The signature is well-formed but does not verify against the
    /// message+pubkey under the chosen scheme.
    #[error("signature does not verify under scheme {0:#x}")]
    BadSignature(u32),
    /// Hybrid signature did not verify. **Opaque by design** — does
    /// not disclose which of the two components failed, because
    /// disclosing that becomes a side-channel oracle for an adversary
    /// probing which component has been independently compromised.
    /// (E.g., "the classical component failed but PQ passed" is a
    /// signal to a quantum adversary that ed25519 is the live attack
    /// surface; the inverse signals classical-time forgery on
    /// ML-DSA.)
    ///
    /// For local diagnostics only, the inner detail is logged at
    /// `tracing::trace` level via [`HybridFailure`] — never
    /// serialized into this error variant, never returned across a
    /// process boundary, never exposed over the wire.
    ///
    /// **Both** components must verify for the hybrid signature to
    /// verify (concatenation combiner per ADR-0006).
    #[error("hybrid signature did not verify")]
    HybridSignatureInvalid,
}

/// Trace-level diagnostic for hybrid-signature verification failures.
///
/// Emitted via `tracing::trace!()` from the hybrid verifier when a
/// component fails. **Never** carried in [`VerifierError`]; never
/// serialized; never returned across a process boundary. Local
/// observability only.
///
/// If you need this information at warn/error level (for example, in
/// a development environment where you want to know which component
/// is breaking), enable a tracing subscriber that captures
/// `tracing::trace` events from the `psl_crypto_agility::hybrid`
/// target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HybridFailure {
    /// The classical (ed25519) component did not verify; the PQ
    /// component was not checked or also failed.
    ClassicalComponent,
    /// The PQ (ML-DSA-65) component did not verify; the classical
    /// component verified successfully.
    PqComponent,
}

impl HybridFailure {
    /// Short tag for trace-level logging.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClassicalComponent => "classical",
            Self::PqComponent => "pq",
        }
    }
}

/// Errors emitted by [`crate::Kem`] implementations and by the
/// witness-encryption layer ([`crate::witness_enc`]).
///
/// **Note on the absent `DecapFailed` variant.** ML-KEM (FIPS 203)
/// implements *implicit rejection*: decapsulation never fails at
/// the type level. On a malformed ciphertext, decap returns a
/// deterministic pseudorandom secret (constant-time, no signal to
/// the caller). The only rejection point is
/// [`KemError::AuthenticationFailed`] from the AEAD layer. Per
/// ADR-0011 § "Decapsulation semantics".
#[derive(Debug, Error)]
pub enum KemError {
    /// The KEM scheme requested is not implemented in this build.
    /// Covers both "scheme reserved but not yet implemented" and
    /// "wire blob has an unknown scheme prefix."
    #[error("KEM scheme {0:#x} is not supported in this build")]
    SchemeNotSupported(u32),
    /// The recipient's public key is malformed (wrong length,
    /// invalid encoding for the scheme).
    #[error("malformed recipient public key for scheme {scheme:#x}: {detail}")]
    MalformedPublicKey {
        /// Scheme.
        scheme: u32,
        /// Detail.
        detail: &'static str,
    },
    /// The ciphertext or wire blob is malformed (wrong length,
    /// invalid encoding for the scheme).
    #[error("malformed ciphertext/blob for scheme {scheme:#x}: {detail}")]
    MalformedCiphertext {
        /// Scheme.
        scheme: u32,
        /// Detail.
        detail: &'static str,
    },
    /// The encrypted blob is shorter than the minimum size for the
    /// declared scheme. Hard-fail per ADR-0011 § "Decoder hard-fail
    /// rules" — no silent truncation tolerance.
    #[error("encrypted blob is truncated for scheme {scheme:#x}: got {got} bytes, expected at least {min}")]
    TruncatedBlob {
        /// Scheme.
        scheme: u32,
        /// Actual blob length.
        got: usize,
        /// Minimum acceptable length for the scheme.
        min: usize,
    },
    /// AEAD authentication failed. **This is the load-bearing
    /// rejection point** for the witness-encryption layer.
    /// Implicit-rejection ML-KEM means decap can't fail visibly;
    /// AEAD auth-tag verification is what catches malformed
    /// ciphertexts, wrong contexts, swapped components, and any
    /// other transcript-binding violation.
    #[error("AEAD authentication failed (witness ciphertext rejected)")]
    AuthenticationFailed,
}

/// Errors emitted by [`crate::HashScheme_`] implementations.
#[derive(Debug, Error)]
pub enum HashError {
    /// Unknown hash scheme on the wire.
    #[error("unknown hash scheme {0:#x}")]
    UnknownScheme(u32),
    /// Hash blob has the wrong length for its declared scheme.
    #[error("hash blob has length {actual} but scheme {scheme:#x} requires {expected}")]
    WrongLength {
        /// Scheme on the wire.
        scheme: u32,
        /// Expected hash length in bytes.
        expected: usize,
        /// Actual blob length.
        actual: usize,
    },
}
