//! Signature traits and the Ed25519 implementation.
//!
//! This is the only signature scheme implemented in v0.1.x. The
//! hybrid ed25519 + ML-DSA-65 implementation lands in the next
//! workstream per ADR-0006 phase 3 and will plug in as another
//! `Signer` / `Verifier` impl with a different `SignatureScheme`
//! discriminant.

use crate::codec::{decode_varint, encode_varint};
use crate::errors::{SignerError, VerifierError};
use crate::scheme::SignatureScheme;
use ed25519_dalek::{Signer as _, SigningKey, Verifier as _, VerifyingKey};
use rand::{rngs::OsRng, RngCore};
use std::collections::HashSet;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A signing key bound to a specific `SignatureScheme`.
pub trait Signer {
    /// Which scheme this signer produces.
    fn scheme(&self) -> SignatureScheme;
    /// Public key bytes (without scheme prefix).
    fn public_key(&self) -> Vec<u8>;
    /// Sign `message`, returning the signature bytes (without scheme
    /// prefix). Use [`sign_with_prefix`] for the wire-format encoding.
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, SignerError>;
}

/// A verifier capable of verifying one or more registered schemes.
pub trait Verifier {
    /// Verify `signature` against `message` and `public_key` under
    /// `scheme`. Implementations must return
    /// [`VerifierError::SchemeNotAccepted`] if the scheme is not in
    /// this verifier's policy, even if the underlying primitive could
    /// verify.
    fn verify(
        &self,
        scheme: SignatureScheme,
        message: &[u8],
        signature: &[u8],
        public_key: &[u8],
    ) -> Result<(), VerifierError>;
}

/// Convenience: encode `(scheme, signature)` into the wire format
/// (varint scheme prefix || signature bytes).
pub fn sign_with_prefix<S: Signer>(s: &S, message: &[u8]) -> Result<Vec<u8>, SignerError> {
    let scheme = s.scheme();
    let sig = s.sign(message)?;
    let mut out = Vec::with_capacity(sig.len() + 1);
    encode_varint(scheme.as_u32(), &mut out);
    out.extend_from_slice(&sig);
    Ok(out)
}

/// Convenience: decode a wire-format `(scheme, signature)` blob and
/// dispatch to a verifier.
pub fn verify_prefixed<V: Verifier>(
    v: &V,
    message: &[u8],
    prefixed_sig: &[u8],
    prefixed_pubkey: &[u8],
) -> Result<(), VerifierError> {
    let (sig_scheme_u32, sig_off) =
        decode_varint(prefixed_sig).map_err(|_| VerifierError::MalformedSignature {
            scheme: 0,
            detail: "varint scheme prefix on signature is malformed",
        })?;
    let (pk_scheme_u32, pk_off) =
        decode_varint(prefixed_pubkey).map_err(|_| VerifierError::MalformedPublicKey {
            scheme: 0,
            detail: "varint scheme prefix on public key is malformed",
        })?;
    if sig_scheme_u32 != pk_scheme_u32 {
        return Err(VerifierError::SchemeNotAccepted(sig_scheme_u32));
    }
    let scheme = SignatureScheme::from_u32(sig_scheme_u32)?;
    v.verify(
        scheme,
        message,
        &prefixed_sig[sig_off..],
        &prefixed_pubkey[pk_off..],
    )
}

/// Policy controlling which schemes a verifier will accept.
#[derive(Debug, Clone)]
pub struct VerifierPolicy {
    accepted: HashSet<SignatureScheme>,
}

impl VerifierPolicy {
    /// Empty policy — accepts no scheme. Use the builder methods to
    /// add accepted schemes explicitly.
    pub fn empty() -> Self {
        Self {
            accepted: HashSet::new(),
        }
    }

    /// Pre-PQ policy: accept only ed25519.
    pub fn ed25519_only() -> Self {
        let mut s = HashSet::new();
        s.insert(SignatureScheme::Ed25519);
        Self { accepted: s }
    }

    /// Migration-window policy: accept ed25519 OR hybrid. Use during
    /// ADR-0006 phase 3-5 transition.
    pub fn ed25519_or_hybrid() -> Self {
        let mut s = HashSet::new();
        s.insert(SignatureScheme::Ed25519);
        s.insert(SignatureScheme::HybridEd25519MlDsa65);
        Self { accepted: s }
    }

    /// Post-migration policy: hybrid only. Use after ADR-0006 phase 6
    /// deadline.
    pub fn hybrid_only() -> Self {
        let mut s = HashSet::new();
        s.insert(SignatureScheme::HybridEd25519MlDsa65);
        Self { accepted: s }
    }

    /// Add a scheme to the accept-list.
    pub fn allow(mut self, scheme: SignatureScheme) -> Self {
        self.accepted.insert(scheme);
        self
    }

    /// Whether this policy accepts `scheme`.
    pub fn accepts(&self, scheme: SignatureScheme) -> bool {
        self.accepted.contains(&scheme)
    }
}

impl Default for VerifierPolicy {
    fn default() -> Self {
        // The default during the v0.1.x → v0.2 window is the migration
        // window: ed25519 OR hybrid. Production deployments override
        // explicitly per their own deadline.
        Self::ed25519_or_hybrid()
    }
}

/// Ed25519 signer. Wraps `ed25519_dalek::SigningKey`.
///
/// Private key bytes are wrapped in `Zeroizing<…>` and zeroed on drop.
#[derive(ZeroizeOnDrop)]
pub struct Ed25519Signer {
    sk: SigningKey,
}

impl Ed25519Signer {
    /// Generate a fresh random ed25519 keypair using `OsRng`.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        let sk = SigningKey::from_bytes(&bytes);
        bytes.zeroize();
        Self { sk }
    }

    /// Construct from a 32-byte secret key.
    pub fn from_secret(secret: &[u8; 32]) -> Self {
        Self {
            sk: SigningKey::from_bytes(secret),
        }
    }
}

impl Signer for Ed25519Signer {
    fn scheme(&self) -> SignatureScheme {
        SignatureScheme::Ed25519
    }

    fn public_key(&self) -> Vec<u8> {
        self.sk.verifying_key().to_bytes().to_vec()
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, SignerError> {
        Ok(self.sk.sign(message).to_bytes().to_vec())
    }
}

/// Ed25519 verifier with a configurable scheme policy.
pub struct Ed25519Verifier {
    policy: VerifierPolicy,
}

impl Ed25519Verifier {
    /// Construct a verifier accepting only ed25519.
    pub fn new() -> Self {
        Self {
            policy: VerifierPolicy::ed25519_only(),
        }
    }

    /// Construct with an explicit policy.
    pub fn with_policy(policy: VerifierPolicy) -> Self {
        Self { policy }
    }
}

impl Default for Ed25519Verifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Verifier for Ed25519Verifier {
    fn verify(
        &self,
        scheme: SignatureScheme,
        message: &[u8],
        signature: &[u8],
        public_key: &[u8],
    ) -> Result<(), VerifierError> {
        if !self.policy.accepts(scheme) {
            return Err(VerifierError::SchemeNotAccepted(scheme.as_u32()));
        }
        match scheme {
            SignatureScheme::Ed25519 => {
                let pk_bytes: [u8; 32] =
                    public_key
                        .try_into()
                        .map_err(|_| VerifierError::MalformedPublicKey {
                            scheme: scheme.as_u32(),
                            detail: "ed25519 pubkey must be 32 bytes",
                        })?;
                let vk = VerifyingKey::from_bytes(&pk_bytes).map_err(|_| {
                    VerifierError::MalformedPublicKey {
                        scheme: scheme.as_u32(),
                        detail: "ed25519 pubkey is not a valid point",
                    }
                })?;
                let sig_bytes: [u8; 64] =
                    signature
                        .try_into()
                        .map_err(|_| VerifierError::MalformedSignature {
                            scheme: scheme.as_u32(),
                            detail: "ed25519 signature must be 64 bytes",
                        })?;
                let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
                vk.verify(message, &sig)
                    .map_err(|_| VerifierError::BadSignature(scheme.as_u32()))
            }
            SignatureScheme::HybridEd25519MlDsa65 | SignatureScheme::SlhDsa128s => {
                Err(VerifierError::SchemeNotAccepted(scheme.as_u32()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ed25519_round_trip() {
        let signer = Ed25519Signer::generate();
        let verifier = Ed25519Verifier::new();
        let msg = b"hello PSL";
        let sig = signer.sign(msg).unwrap();
        verifier
            .verify(SignatureScheme::Ed25519, msg, &sig, &signer.public_key())
            .unwrap();
    }

    #[test]
    fn ed25519_rejects_modified_message() {
        let signer = Ed25519Signer::generate();
        let verifier = Ed25519Verifier::new();
        let sig = signer.sign(b"original").unwrap();
        let err = verifier
            .verify(
                SignatureScheme::Ed25519,
                b"modified",
                &sig,
                &signer.public_key(),
            )
            .unwrap_err();
        assert!(matches!(err, VerifierError::BadSignature(_)));
    }

    #[test]
    fn ed25519_rejects_modified_signature() {
        let signer = Ed25519Signer::generate();
        let verifier = Ed25519Verifier::new();
        let mut sig = signer.sign(b"x").unwrap();
        sig[0] ^= 0xff;
        let err = verifier
            .verify(SignatureScheme::Ed25519, b"x", &sig, &signer.public_key())
            .unwrap_err();
        assert!(matches!(err, VerifierError::BadSignature(_)));
    }

    #[test]
    fn ed25519_rejects_wrong_pubkey() {
        let signer = Ed25519Signer::generate();
        let other = Ed25519Signer::generate();
        let verifier = Ed25519Verifier::new();
        let sig = signer.sign(b"x").unwrap();
        let err = verifier
            .verify(SignatureScheme::Ed25519, b"x", &sig, &other.public_key())
            .unwrap_err();
        assert!(matches!(err, VerifierError::BadSignature(_)));
    }

    #[test]
    fn rejects_unknown_scheme_via_policy() {
        let signer = Ed25519Signer::generate();
        let verifier = Ed25519Verifier::with_policy(VerifierPolicy::hybrid_only());
        let sig = signer.sign(b"x").unwrap();
        let err = verifier
            .verify(SignatureScheme::Ed25519, b"x", &sig, &signer.public_key())
            .unwrap_err();
        assert_eq!(err, VerifierError::SchemeNotAccepted(0x01));
    }

    #[test]
    fn rejects_unimplemented_scheme_explicitly() {
        let signer = Ed25519Signer::generate();
        let verifier = Ed25519Verifier::with_policy(
            VerifierPolicy::ed25519_only().allow(SignatureScheme::HybridEd25519MlDsa65),
        );
        let err = verifier
            .verify(
                SignatureScheme::HybridEd25519MlDsa65,
                b"x",
                &signer.sign(b"x").unwrap(),
                &signer.public_key(),
            )
            .unwrap_err();
        // Hybrid scheme is in policy but not implemented in Ed25519Verifier;
        // dispatch returns SchemeNotAccepted (the verifier doesn't handle it).
        assert!(matches!(err, VerifierError::SchemeNotAccepted(_)));
    }

    #[test]
    fn wire_format_round_trip() {
        let signer = Ed25519Signer::generate();
        let verifier = Ed25519Verifier::new();
        let msg = b"prefixed";
        let prefixed_sig = sign_with_prefix(&signer, msg).unwrap();
        let mut prefixed_pk = Vec::new();
        encode_varint(signer.scheme().as_u32(), &mut prefixed_pk);
        prefixed_pk.extend_from_slice(&signer.public_key());
        verify_prefixed(&verifier, msg, &prefixed_sig, &prefixed_pk).unwrap();
    }

    #[test]
    fn wire_format_rejects_scheme_mismatch() {
        let signer = Ed25519Signer::generate();
        let verifier = Ed25519Verifier::new();
        let prefixed_sig = sign_with_prefix(&signer, b"x").unwrap();
        let mut prefixed_pk = Vec::new();
        encode_varint(
            SignatureScheme::HybridEd25519MlDsa65.as_u32(),
            &mut prefixed_pk,
        );
        prefixed_pk.extend_from_slice(&signer.public_key());
        let err = verify_prefixed(&verifier, b"x", &prefixed_sig, &prefixed_pk).unwrap_err();
        assert!(matches!(err, VerifierError::SchemeNotAccepted(_)));
    }

    #[test]
    fn wire_format_rejects_unknown_scheme() {
        let verifier = Ed25519Verifier::new();
        let mut prefixed_sig = Vec::new();
        encode_varint(0xdead_beef, &mut prefixed_sig);
        prefixed_sig.extend_from_slice(&[0u8; 64]);
        let mut prefixed_pk = Vec::new();
        encode_varint(0xdead_beef, &mut prefixed_pk);
        prefixed_pk.extend_from_slice(&[0u8; 32]);
        let err = verify_prefixed(&verifier, b"x", &prefixed_sig, &prefixed_pk).unwrap_err();
        assert_eq!(err, VerifierError::UnknownScheme(0xdead_beef));
    }
}
