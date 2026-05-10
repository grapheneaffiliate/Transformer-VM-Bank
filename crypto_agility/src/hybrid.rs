//! Hybrid ed25519 + ML-DSA-65 signature scheme per ADR-0006.
//!
//! ## Composition
//!
//! Concatenation combiner (NIST SP 800-227 draft, IETF
//! `draft-ietf-pquip-hybrid-signature-spectrums`):
//!
//! ```text
//! hybrid_sig(msg) = ed25519_sig(msg) || ml_dsa_sig(msg)
//! hybrid_pk      = ed25519_pk       || ml_dsa_pk
//! ```
//!
//! Verification accepts iff **both** components verify. No XOR-style
//! mixing, no novel combiner.
//!
//! ## Wire format (locked)
//!
//! Hybrid pubkey: 32 (ed25519) + 1952 (ML-DSA-65) = **1984 bytes**.
//! Hybrid signature: 64 (ed25519) + 3309 (ML-DSA-65) = **3373 bytes**.
//!
//! Both are fixed-length; no inner length prefixes. The decoder uses
//! the per-scheme constants below and **hard-fails** if the input is
//! shorter than the expected length (no silent truncation).
//!
//! Concatenation order is **ed25519 first, then ML-DSA-65**. This is
//! locked by the byte-exact wire-format round-trip test in
//! `tests/proptest_agility.rs` and by [`HYBRID_PUBKEY_BYTES`] /
//! [`HYBRID_SIG_BYTES`].
//!
//! ## Determinism invariant
//!
//! - **Verification is deterministic**: given `(msg, sig, pk)` the
//!   verify function returns the same answer on every call, on every
//!   architecture. This is what dispute-by-re-execution depends on.
//! - **Signing is randomized**: ML-DSA per FIPS 204 §5.4 (the standard
//!   `Sign` algorithm) injects 32 bytes of fresh randomness per call,
//!   so repeated sign(msg, sk) calls produce different bytes. This is
//!   FIPS-compliant and the recommended deployment mode for side-
//!   channel resistance during signing. The chain commits the
//!   signature bytes once (in the on-chain block / message); replays
//!   verify against the committed bytes.
//! - **Sign_internal (deterministic mode)** is not currently exposed by
//!   `pqcrypto-mldsa`. Tracked as a follow-up for the external
//!   cryptographer review per ADR-0006 acceptance criteria; switching
//!   to deterministic mode is a one-line change if the reviewer
//!   recommends it.
//!
//! ## Failure modes the test suite covers
//!
//! See `tests/proptest_agility.rs` `hybrid_*` and
//! `crypto_agility/src/hybrid.rs#tests`. The 10-scenario suite covers
//! the four base cases (both valid / only ed25519 / only ML-DSA /
//! neither) plus length-extension on each component, component swap,
//! cross-message replay between components, byte-exact wire-format
//! round-trip, and one-byte-short hard reject.

use crate::codec::{decode_varint, encode_varint};
use crate::errors::{HybridFailure, SignerError, VerifierError};
use crate::scheme::SignatureScheme;
use crate::signer::{Signer, Verifier};
use ed25519_dalek::{Signer as _, SigningKey, Verifier as _, VerifyingKey};
use pqcrypto_mldsa::mldsa65;
use pqcrypto_traits::sign::{DetachedSignature as _, PublicKey as _, SecretKey as _};
use rand::{rngs::OsRng, RngCore};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Length in bytes of an ed25519 public key.
pub const ED25519_PUBKEY_BYTES: usize = 32;
/// Length in bytes of an ed25519 signature.
pub const ED25519_SIG_BYTES: usize = 64;
/// Length in bytes of an ML-DSA-65 public key per FIPS 204.
pub const MLDSA65_PUBKEY_BYTES: usize = 1952;
/// Length in bytes of an ML-DSA-65 signature per FIPS 204.
pub const MLDSA65_SIG_BYTES: usize = 3309;
/// Length in bytes of the hybrid pubkey (ed25519 || ML-DSA-65).
pub const HYBRID_PUBKEY_BYTES: usize = ED25519_PUBKEY_BYTES + MLDSA65_PUBKEY_BYTES;
/// Length in bytes of the hybrid signature (ed25519 || ML-DSA-65).
pub const HYBRID_SIG_BYTES: usize = ED25519_SIG_BYTES + MLDSA65_SIG_BYTES;

/// Hybrid ed25519 + ML-DSA-65 signing key.
///
/// Both component private keys are wrapped to be zeroed on drop.
#[derive(ZeroizeOnDrop)]
pub struct HybridSigner {
    ed25519: SigningKey,
    // pqcrypto types don't impl Zeroize directly; we wrap the bytes and
    // re-construct on every sign. The bytes are kept inside a Vec that
    // is zeroized on drop.
    mldsa_sk_bytes: Vec<u8>,
    // Cache the public key bytes so we don't have to reconstruct from sk
    // on every public_key() call (it's cheap-ish but unnecessary).
    #[zeroize(skip)]
    mldsa_pk_bytes: [u8; MLDSA65_PUBKEY_BYTES],
}

impl HybridSigner {
    /// Generate a fresh random hybrid keypair using `OsRng` for ed25519
    /// and the pqcrypto-mldsa internal RNG for ML-DSA-65.
    pub fn generate() -> Self {
        let mut ed_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut ed_bytes);
        let ed25519 = SigningKey::from_bytes(&ed_bytes);
        ed_bytes.zeroize();

        let (pk, sk) = mldsa65::keypair();
        let mut pk_arr = [0u8; MLDSA65_PUBKEY_BYTES];
        pk_arr.copy_from_slice(pk.as_bytes());

        Self {
            ed25519,
            mldsa_sk_bytes: sk.as_bytes().to_vec(),
            mldsa_pk_bytes: pk_arr,
        }
    }

    /// Reconstruct the ML-DSA secret key from its raw bytes for signing.
    fn mldsa_sk(&self) -> Result<mldsa65::SecretKey, SignerError> {
        mldsa65::SecretKey::from_bytes(&self.mldsa_sk_bytes)
            .map_err(|e| SignerError::Primitive(format!("ML-DSA-65 sk reconstruction: {e:?}")))
    }
}

impl Signer for HybridSigner {
    fn scheme(&self) -> SignatureScheme {
        SignatureScheme::HybridEd25519MlDsa65
    }

    fn public_key(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HYBRID_PUBKEY_BYTES);
        out.extend_from_slice(&self.ed25519.verifying_key().to_bytes());
        out.extend_from_slice(&self.mldsa_pk_bytes);
        out
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, SignerError> {
        let ed_sig = self.ed25519.sign(message);
        let mldsa_sk = self.mldsa_sk()?;
        let mldsa_sig = mldsa65::detached_sign(message, &mldsa_sk);

        let mldsa_sig_bytes = mldsa_sig.as_bytes();
        if mldsa_sig_bytes.len() != MLDSA65_SIG_BYTES {
            return Err(SignerError::Primitive(format!(
                "ML-DSA-65 produced unexpected sig length: {} (expected {})",
                mldsa_sig_bytes.len(),
                MLDSA65_SIG_BYTES
            )));
        }

        let mut out = Vec::with_capacity(HYBRID_SIG_BYTES);
        out.extend_from_slice(&ed_sig.to_bytes());
        out.extend_from_slice(mldsa_sig_bytes);
        debug_assert_eq!(out.len(), HYBRID_SIG_BYTES);
        Ok(out)
    }
}

/// Hybrid ed25519 + ML-DSA-65 verifier.
///
/// Verifies iff **both** components pass. On failure, returns the
/// opaque [`VerifierError::HybridSignatureInvalid`] regardless of
/// which component(s) failed — disclosing which component failed
/// would be a side-channel oracle for an adversary probing
/// independent component compromise. Inner detail logs at
/// `tracing::trace` level for local diagnostics; never serialized,
/// never returned across an error boundary.
pub struct HybridVerifier;

impl HybridVerifier {
    /// Construct a verifier. No state; equivalent to a singleton.
    pub fn new() -> Self {
        Self
    }
}

impl Default for HybridVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Verifier for HybridVerifier {
    fn verify(
        &self,
        scheme: SignatureScheme,
        message: &[u8],
        signature: &[u8],
        public_key: &[u8],
    ) -> Result<(), VerifierError> {
        if scheme != SignatureScheme::HybridEd25519MlDsa65 {
            return Err(VerifierError::SchemeNotAccepted(scheme.as_u32()));
        }

        // Hard-reject anything that isn't exactly the expected length.
        // No silent truncation, no padding tolerance.
        if signature.len() != HYBRID_SIG_BYTES {
            return Err(VerifierError::MalformedSignature {
                scheme: scheme.as_u32(),
                detail: "hybrid signature must be exactly 3373 bytes (ed25519 || ML-DSA-65)",
            });
        }
        if public_key.len() != HYBRID_PUBKEY_BYTES {
            return Err(VerifierError::MalformedPublicKey {
                scheme: scheme.as_u32(),
                detail: "hybrid pubkey must be exactly 1984 bytes (ed25519 || ML-DSA-65)",
            });
        }

        // Slice the components by fixed offsets — concatenation order
        // is ed25519 first, then ML-DSA-65 (locked).
        let ed_pk_bytes = &public_key[..ED25519_PUBKEY_BYTES];
        let mldsa_pk_bytes = &public_key[ED25519_PUBKEY_BYTES..];
        let ed_sig_bytes = &signature[..ED25519_SIG_BYTES];
        let mldsa_sig_bytes = &signature[ED25519_SIG_BYTES..];

        // Ed25519 component.
        let ed_pk_arr: [u8; ED25519_PUBKEY_BYTES] =
            ed_pk_bytes
                .try_into()
                .map_err(|_| VerifierError::MalformedPublicKey {
                    scheme: scheme.as_u32(),
                    detail: "ed25519 pubkey slice length mismatch (internal bug)",
                })?;
        let ed_vk = VerifyingKey::from_bytes(&ed_pk_arr).map_err(|_| {
            VerifierError::MalformedPublicKey {
                scheme: scheme.as_u32(),
                detail: "ed25519 component pubkey is not a valid point",
            }
        })?;
        let ed_sig_arr: [u8; ED25519_SIG_BYTES] =
            ed_sig_bytes
                .try_into()
                .map_err(|_| VerifierError::MalformedSignature {
                    scheme: scheme.as_u32(),
                    detail: "ed25519 sig slice length mismatch (internal bug)",
                })?;
        let ed_sig = ed25519_dalek::Signature::from_bytes(&ed_sig_arr);
        if ed_vk.verify(message, &ed_sig).is_err() {
            // Trace-only diagnostic; never returned across an error
            // boundary. Outer error stays opaque so an adversary
            // probing for selective-component failures gets no signal.
            tracing::trace!(
                target: "psl_crypto_agility::hybrid",
                component = HybridFailure::ClassicalComponent.as_str(),
                "hybrid signature: classical (ed25519) component failed verification"
            );
            return Err(VerifierError::HybridSignatureInvalid);
        }

        // ML-DSA-65 component.
        let mldsa_pk = mldsa65::PublicKey::from_bytes(mldsa_pk_bytes).map_err(|_| {
            VerifierError::MalformedPublicKey {
                scheme: scheme.as_u32(),
                detail: "ML-DSA-65 component pubkey rejected by primitive",
            }
        })?;
        let mldsa_sig = mldsa65::DetachedSignature::from_bytes(mldsa_sig_bytes).map_err(|_| {
            VerifierError::MalformedSignature {
                scheme: scheme.as_u32(),
                detail: "ML-DSA-65 component sig rejected by primitive",
            }
        })?;
        if mldsa65::verify_detached_signature(&mldsa_sig, message, &mldsa_pk).is_err() {
            tracing::trace!(
                target: "psl_crypto_agility::hybrid",
                component = HybridFailure::PqComponent.as_str(),
                "hybrid signature: PQ (ML-DSA-65) component failed verification"
            );
            return Err(VerifierError::HybridSignatureInvalid);
        }

        Ok(())
    }
}

/// Wire-format helper: serialize `(scheme, hybrid_pubkey)` as
/// `varint(scheme) || hybrid_pubkey_bytes`. Pinned by the byte-exact
/// round-trip tests.
pub fn encode_hybrid_pubkey_blob(pk_bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + HYBRID_PUBKEY_BYTES);
    encode_varint(SignatureScheme::HybridEd25519MlDsa65.as_u32(), &mut out);
    out.extend_from_slice(pk_bytes);
    out
}

/// Wire-format helper: serialize `(scheme, hybrid_signature)`.
pub fn encode_hybrid_sig_blob(sig_bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + HYBRID_SIG_BYTES);
    encode_varint(SignatureScheme::HybridEd25519MlDsa65.as_u32(), &mut out);
    out.extend_from_slice(sig_bytes);
    out
}

/// Wire-format helper: parse a `(scheme, body)` blob and return the
/// body slice if the scheme is `HybridEd25519MlDsa65`. Hard-rejects
/// length mismatches and unknown schemes.
pub fn decode_hybrid_blob(blob: &[u8], expected_body_len: usize) -> Result<&[u8], VerifierError> {
    let (scheme_u32, off) = decode_varint(blob).map_err(|_| VerifierError::MalformedSignature {
        scheme: 0,
        detail: "hybrid blob: malformed varint scheme prefix",
    })?;
    if scheme_u32 != SignatureScheme::HybridEd25519MlDsa65.as_u32() {
        return Err(VerifierError::UnknownScheme(scheme_u32));
    }
    let body = &blob[off..];
    if body.len() != expected_body_len {
        return Err(VerifierError::MalformedSignature {
            scheme: scheme_u32,
            detail: "hybrid blob: body length mismatch (no silent truncation)",
        });
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Base case 1: both components valid → accept.
    #[test]
    fn hybrid_round_trip_accepts() {
        let signer = HybridSigner::generate();
        let verifier = HybridVerifier::new();
        let msg = b"the dispute mechanism rests on this";
        let sig = signer.sign(msg).unwrap();
        let pk = signer.public_key();
        verifier
            .verify(SignatureScheme::HybridEd25519MlDsa65, msg, &sig, &pk)
            .expect("freshly signed hybrid signature must verify");
    }

    /// Base case 2: ed25519 valid, ML-DSA forged -> reject opaquely.
    /// Outer error is `HybridSignatureInvalid` and does NOT disclose
    /// which component failed (per the side-channel hardening; inner
    /// detail logs at trace level only).
    #[test]
    fn hybrid_rejects_forged_pq_component() {
        let signer = HybridSigner::generate();
        let verifier = HybridVerifier::new();
        let msg = b"x";
        let mut sig = signer.sign(msg).unwrap();
        sig[ED25519_SIG_BYTES + 100] ^= 0xff;
        let err = verifier
            .verify(
                SignatureScheme::HybridEd25519MlDsa65,
                msg,
                &sig,
                &signer.public_key(),
            )
            .unwrap_err();
        assert_eq!(err, VerifierError::HybridSignatureInvalid);
    }

    /// Base case 3: ML-DSA valid, ed25519 forged -> reject opaquely.
    #[test]
    fn hybrid_rejects_forged_classical_component() {
        let signer = HybridSigner::generate();
        let verifier = HybridVerifier::new();
        let msg = b"x";
        let mut sig = signer.sign(msg).unwrap();
        sig[10] ^= 0xff;
        let err = verifier
            .verify(
                SignatureScheme::HybridEd25519MlDsa65,
                msg,
                &sig,
                &signer.public_key(),
            )
            .unwrap_err();
        assert_eq!(err, VerifierError::HybridSignatureInvalid);
    }

    /// Base case 4: both components forged -> reject opaquely.
    #[test]
    fn hybrid_rejects_both_forged() {
        let signer = HybridSigner::generate();
        let verifier = HybridVerifier::new();
        let msg = b"x";
        let mut sig = signer.sign(msg).unwrap();
        sig[10] ^= 0xff;
        sig[ED25519_SIG_BYTES + 100] ^= 0xff;
        let err = verifier
            .verify(
                SignatureScheme::HybridEd25519MlDsa65,
                msg,
                &sig,
                &signer.public_key(),
            )
            .unwrap_err();
        assert_eq!(err, VerifierError::HybridSignatureInvalid);
    }

    /// Side-channel hardening: the outer error variant is
    /// indistinguishable for "only classical failed" vs "only PQ
    /// failed" vs "both failed". An adversary observing only the
    /// returned error gets no signal about which component is the
    /// live attack surface. Inner detail is available via
    /// `tracing::trace` for local diagnostics only.
    #[test]
    fn hybrid_failure_outer_error_is_indistinguishable_across_modes() {
        let signer = HybridSigner::generate();
        let verifier = HybridVerifier::new();
        let msg = b"side-channel-test";
        let good_sig = signer.sign(msg).unwrap();
        let pk = signer.public_key();

        let mut classical_only = good_sig.clone();
        classical_only[10] ^= 0xff;
        let mut pq_only = good_sig.clone();
        pq_only[ED25519_SIG_BYTES + 100] ^= 0xff;
        let mut both = good_sig.clone();
        both[10] ^= 0xff;
        both[ED25519_SIG_BYTES + 100] ^= 0xff;

        let e1 = verifier
            .verify(
                SignatureScheme::HybridEd25519MlDsa65,
                msg,
                &classical_only,
                &pk,
            )
            .unwrap_err();
        let e2 = verifier
            .verify(SignatureScheme::HybridEd25519MlDsa65, msg, &pq_only, &pk)
            .unwrap_err();
        let e3 = verifier
            .verify(SignatureScheme::HybridEd25519MlDsa65, msg, &both, &pk)
            .unwrap_err();

        assert_eq!(e1, e2);
        assert_eq!(e2, e3);
        assert_eq!(e1, VerifierError::HybridSignatureInvalid);
    }

    /// Length-extension #1: junk appended to ed25519 component (i.e.,
    /// total signature length is wrong) → hard reject.
    #[test]
    fn hybrid_rejects_length_extension_on_total() {
        let signer = HybridSigner::generate();
        let verifier = HybridVerifier::new();
        let mut sig = signer.sign(b"x").unwrap();
        sig.extend_from_slice(&[0xaa; 16]);
        let err = verifier
            .verify(
                SignatureScheme::HybridEd25519MlDsa65,
                b"x",
                &sig,
                &signer.public_key(),
            )
            .unwrap_err();
        match err {
            VerifierError::MalformedSignature { detail, .. } => {
                assert!(detail.contains("3373"));
            }
            other => panic!("expected MalformedSignature, got {other:?}"),
        }
    }

    /// Length-extension #2: short by one byte → hard reject (no silent
    /// truncation, fixed-length decoder must hard-fail per the
    /// signature-malleability defense).
    #[test]
    fn hybrid_rejects_one_byte_short() {
        let signer = HybridSigner::generate();
        let verifier = HybridVerifier::new();
        let mut sig = signer.sign(b"x").unwrap();
        sig.pop(); // remove last byte
        let err = verifier
            .verify(
                SignatureScheme::HybridEd25519MlDsa65,
                b"x",
                &sig,
                &signer.public_key(),
            )
            .unwrap_err();
        assert!(matches!(err, VerifierError::MalformedSignature { .. }));
    }

    /// Component swap: take a valid hybrid signature, swap the order
    /// of the two components → must reject. This catches a class of
    /// bugs where the verifier doesn't enforce concatenation order.
    #[test]
    fn hybrid_rejects_component_swap() {
        let signer = HybridSigner::generate();
        let verifier = HybridVerifier::new();
        let msg = b"swap-test";
        let sig = signer.sign(msg).unwrap();
        // Swap: put ML-DSA bytes first, then ed25519.
        let mut swapped = Vec::with_capacity(HYBRID_SIG_BYTES);
        swapped.extend_from_slice(&sig[ED25519_SIG_BYTES..]);
        swapped.extend_from_slice(&sig[..ED25519_SIG_BYTES]);
        let err = verifier
            .verify(
                SignatureScheme::HybridEd25519MlDsa65,
                msg,
                &swapped,
                &signer.public_key(),
            )
            .unwrap_err();
        // Swap puts the ed25519 sig in the ML-DSA slot — first 64 bytes
        // are now interpreted as ed25519 sig, which won't match. The
        // classical component fails first; outer error is opaque.
        assert!(matches!(
            err,
            VerifierError::HybridSignatureInvalid | VerifierError::MalformedSignature { .. }
        ));
    }

    /// Cross-message replay: sign A and B separately, take the
    /// ed25519 part from sig(A) and the ML-DSA part from sig(B),
    /// concatenate, verify against A. Must reject (this catches
    /// combiner bugs where each component is independently checked
    /// but not bound to the same message digest).
    #[test]
    fn hybrid_rejects_cross_message_replay() {
        let signer = HybridSigner::generate();
        let verifier = HybridVerifier::new();
        let sig_a = signer.sign(b"message-A").unwrap();
        let sig_b = signer.sign(b"message-B").unwrap();
        // Frankenstein: ed25519 of A + ML-DSA of B.
        let mut frank = Vec::with_capacity(HYBRID_SIG_BYTES);
        frank.extend_from_slice(&sig_a[..ED25519_SIG_BYTES]);
        frank.extend_from_slice(&sig_b[ED25519_SIG_BYTES..]);
        // Verify against A: ed25519 of A passes, ML-DSA of B fails.
        // Outer error is opaque (HybridSignatureInvalid).
        let err_against_a = verifier
            .verify(
                SignatureScheme::HybridEd25519MlDsa65,
                b"message-A",
                &frank,
                &signer.public_key(),
            )
            .unwrap_err();
        assert_eq!(err_against_a, VerifierError::HybridSignatureInvalid);
        // Verify against B: ed25519 of A fails (it signed A, not B).
        // Same opaque outer error.
        let err_against_b = verifier
            .verify(
                SignatureScheme::HybridEd25519MlDsa65,
                b"message-B",
                &frank,
                &signer.public_key(),
            )
            .unwrap_err();
        assert_eq!(err_against_b, VerifierError::HybridSignatureInvalid);
        // Both errors are equal, demonstrating the side-channel
        // hardening: an adversary observing only the error gets no
        // signal about which component verified.
        assert_eq!(err_against_a, err_against_b);
    }

    /// Wire-format byte-exact round-trip: encode then decode produces
    /// identical bytes; the scheme prefix is correctly placed and the
    /// expected length is enforced.
    #[test]
    fn hybrid_wire_format_round_trip() {
        let signer = HybridSigner::generate();
        let pk = signer.public_key();
        let sig = signer.sign(b"wire-format-test").unwrap();

        let pk_blob = encode_hybrid_pubkey_blob(&pk);
        let sig_blob = encode_hybrid_sig_blob(&sig);

        // Discriminant is 0x02 (locked); varint encoding of 2 is 1 byte.
        assert_eq!(pk_blob[0], 0x02);
        assert_eq!(sig_blob[0], 0x02);
        assert_eq!(pk_blob.len(), 1 + HYBRID_PUBKEY_BYTES);
        assert_eq!(sig_blob.len(), 1 + HYBRID_SIG_BYTES);

        let pk_body = decode_hybrid_blob(&pk_blob, HYBRID_PUBKEY_BYTES).unwrap();
        let sig_body = decode_hybrid_blob(&sig_blob, HYBRID_SIG_BYTES).unwrap();
        assert_eq!(pk_body, pk.as_slice());
        assert_eq!(sig_body, sig.as_slice());
    }

    /// Wire-format hard-reject: input one byte short → MalformedSignature,
    /// not silent truncation. This is the signature-malleability defense.
    #[test]
    fn hybrid_wire_format_one_byte_short_hard_reject() {
        let signer = HybridSigner::generate();
        let sig = signer.sign(b"x").unwrap();
        let mut sig_blob = encode_hybrid_sig_blob(&sig);
        sig_blob.pop();
        let err = decode_hybrid_blob(&sig_blob, HYBRID_SIG_BYTES).unwrap_err();
        assert!(matches!(err, VerifierError::MalformedSignature { .. }));
    }

    /// Verification determinism (the load-bearing invariant): given
    /// the same (sig, msg, pk), verify returns the same answer on
    /// every call.
    #[test]
    fn hybrid_verification_is_deterministic() {
        let signer = HybridSigner::generate();
        let verifier = HybridVerifier::new();
        let sig = signer.sign(b"det-test").unwrap();
        let pk = signer.public_key();
        for _ in 0..32 {
            verifier
                .verify(
                    SignatureScheme::HybridEd25519MlDsa65,
                    b"det-test",
                    &sig,
                    &pk,
                )
                .unwrap();
        }
    }
}
