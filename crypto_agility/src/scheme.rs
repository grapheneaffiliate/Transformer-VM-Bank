//! Scheme identifiers for signatures and KEMs.
//!
//! Discriminants are part of the wire format and **never reused**. A
//! retired scheme's discriminant stays retired forever; a successor
//! takes a new ID.

use crate::errors::{SignerError, VerifierError};

/// Identifier for a signature scheme. Encoded as a varint prefix on
/// every signature blob and every public-key blob (per ADR-0007).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignatureScheme {
    /// Pure ed25519. Pre-PQ scheme; remains supported during the
    /// hybrid transition window per ADR-0006 phase 6.
    Ed25519 = 0x01,
    /// Hybrid ed25519 + ML-DSA-65 (FIPS 204) concatenation combiner.
    /// The post-quantum default per ADR-0006. Reserved; not yet
    /// implemented in this crate.
    HybridEd25519MlDsa65 = 0x02,
    /// SLH-DSA-128s (FIPS 205) — hash-based signatures. Reserved for
    /// validator-only use cases per ADR-0006. Not yet implemented.
    SlhDsa128s = 0x03,
}

impl SignatureScheme {
    /// Decode a scheme discriminant from its on-wire `u32`.
    pub fn from_u32(v: u32) -> Result<Self, VerifierError> {
        match v {
            0x01 => Ok(Self::Ed25519),
            0x02 => Ok(Self::HybridEd25519MlDsa65),
            0x03 => Ok(Self::SlhDsa128s),
            other => Err(VerifierError::UnknownScheme(other)),
        }
    }

    /// Wire encoding of this scheme as a `u32` discriminant.
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    /// Whether this scheme is implemented in the current crate
    /// version. Reserved-but-unimplemented schemes return `false`.
    pub fn is_implemented(self) -> bool {
        matches!(self, Self::Ed25519)
    }
}

impl From<SignatureScheme> for SignerError {
    fn from(s: SignatureScheme) -> Self {
        SignerError::SchemeNotImplemented(s.as_u32())
    }
}

/// Identifier for a key-encapsulation mechanism. Same wire-format
/// pattern as `SignatureScheme`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KemSchemeId {
    /// Pure X25519 ECDH KEM. Pre-PQ scheme; supported during transition.
    X25519 = 0x01,
    /// Hybrid X25519 + ML-KEM-768 (FIPS 203) HKDF combiner.
    /// Post-quantum default per ADR-0006. Reserved; not yet implemented.
    HybridX25519MlKem768 = 0x02,
}

impl KemSchemeId {
    /// Decode a KEM-scheme discriminant from its on-wire `u32`.
    pub fn from_u32(v: u32) -> Result<Self, VerifierError> {
        match v {
            0x01 => Ok(Self::X25519),
            0x02 => Ok(Self::HybridX25519MlKem768),
            other => Err(VerifierError::UnknownScheme(other)),
        }
    }

    /// Wire encoding as `u32`.
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    /// Whether implemented in the current crate version.
    pub fn is_implemented(self) -> bool {
        false
    }
}
