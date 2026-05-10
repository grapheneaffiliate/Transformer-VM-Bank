//! # psl-crypto-agility
//!
//! The cryptographic agility layer for PSL. Every signature, every KEM
//! ciphertext, and every hash blob in PSL carries an explicit
//! varint-encoded scheme identifier. Verifiers dispatch on the scheme;
//! unknown schemes are refused with a typed error (never silent
//! acceptance, never best-effort fallback).
//!
//! Per ADR-0007: this crate is the only place in the workspace that
//! reaches into primitive crates (`ed25519-dalek`, `blake3`, eventually
//! `pqcrypto-mldsa` and `pqcrypto-mlkem`). All other crates depend on
//! the `Signer` / `Verifier` / `Kem` / `HashScheme_` traits here and
//! never see primitive types directly.
//!
//! ## Phase status
//!
//! v0.1.x ships the *agility infrastructure* (this crate) plus an
//! `Ed25519` impl of the signature traits, equivalent to today's
//! direct ed25519 usage. Hybrid ed25519 + ML-DSA-65 lands in the next
//! workstream (ADR-0006 phase 3); BLAKE3-512 lands as ADR-0008 phase
//! 2; hybrid X25519 + ML-KEM-768 KEM in phase 4. The trait shape is
//! stable now so call sites can be ported in advance.

#![warn(missing_docs)]

pub mod codec;
pub mod errors;
pub mod hash;
pub mod hybrid;
pub mod kem;
pub mod scheme;
pub mod signer;

pub use errors::{HashError, HybridFailure, KemError, SignerError, VerifierError};
pub use hash::{Blake3_256, Blake3_512, HashScheme, HashScheme_};
pub use hybrid::{
    decode_hybrid_blob, encode_hybrid_pubkey_blob, encode_hybrid_sig_blob, HybridSigner,
    HybridVerifier, ED25519_PUBKEY_BYTES, ED25519_SIG_BYTES, HYBRID_PUBKEY_BYTES, HYBRID_SIG_BYTES,
    MLDSA65_PUBKEY_BYTES, MLDSA65_SIG_BYTES,
};
pub use kem::{Kem, KemScheme, SharedSecret};
pub use scheme::{KemSchemeId, SignatureScheme};
pub use signer::{Ed25519Signer, Ed25519Verifier, Signer, Verifier, VerifierPolicy};
