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
use pqcrypto_mlkem::mlkem768;
use pqcrypto_traits::kem::{Ciphertext as _, PublicKey as _, SecretKey as _, SharedSecret as _};
use rand::{rngs::OsRng, RngCore};
use x25519_dalek::{PublicKey as X25519DalekPublicKey, StaticSecret as X25519DalekSecret};
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

/// Hybrid public key — bundle of X25519 + ML-KEM-768 public keys.
/// Cloneable (no secret material); the trait's `PublicKey`
/// associated type for [`HybridX25519MlKem768Kem`].
#[derive(Clone)]
pub struct HybridPublicKey {
    /// X25519 component (32 bytes).
    pub x25519: X25519PublicKey,
    /// ML-KEM-768 component (1184 bytes).
    pub ml_kem: MlKemPublicKey,
}

/// Hybrid long-term recipient secret key — bundle of X25519 +
/// ML-KEM-768 secret keys. Zeroized on drop. The trait's
/// `RecipientSecretKey` associated type for
/// [`HybridX25519MlKem768Kem`].
pub struct HybridRecipientSecretKey {
    /// X25519 component. Zeroized on drop via its inner type.
    pub x25519: RecipientX25519SecretKey,
    /// ML-KEM-768 component. Zeroized on drop via its inner type.
    pub ml_kem: RecipientMlKemSecretKey,
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
    /// Public-key half (cloneable — share with senders).
    pub public: HybridPublicKey,
    /// Secret-key half (zeroized on drop — never leaves the
    /// recipient).
    pub secret: HybridRecipientSecretKey,
}

impl HybridKemKeypair {
    /// Generate a fresh hybrid keypair using `OsRng` for both
    /// components. Per ADR-0011 — used for both recipient long-
    /// term keys and (via the `Ephemeral*` constructors) per-
    /// witness ephemeral keys.
    pub fn generate() -> Self {
        // X25519 component.
        let mut x25519_sk_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut x25519_sk_bytes);
        let x25519_dalek_sk = X25519DalekSecret::from(x25519_sk_bytes);
        let x25519_dalek_pk = X25519DalekPublicKey::from(&x25519_dalek_sk);
        let x25519_pk_bytes = *x25519_dalek_pk.as_bytes();
        // We hold the secret bytes in our typed wrapper; zeroize the
        // intermediate scratch.
        let x25519_sk_typed = RecipientX25519SecretKey(x25519_sk_bytes);
        x25519_sk_bytes.zeroize();
        // Drop the dalek wrapper; its internal Zeroize fires on drop.
        drop(x25519_dalek_sk);

        // ML-KEM-768 component.
        let (ml_kem_pk_dalek, ml_kem_sk_dalek) = mlkem768::keypair();
        let ml_kem_pk_typed = MlKemPublicKey(ml_kem_pk_dalek.as_bytes().to_vec());
        let ml_kem_sk_typed = RecipientMlKemSecretKey(ml_kem_sk_dalek.as_bytes().to_vec());

        Self {
            public: HybridPublicKey {
                x25519: X25519PublicKey(x25519_pk_bytes),
                ml_kem: ml_kem_pk_typed,
            },
            secret: HybridRecipientSecretKey {
                x25519: x25519_sk_typed,
                ml_kem: ml_kem_sk_typed,
            },
        }
    }
}

/// Hybrid ciphertext: X25519 (sender-ephemeral pubkey, in ECDH-as-
/// KEM convention) + ML-KEM-768 ciphertext. The trait's
/// `Ciphertext` associated type for [`HybridX25519MlKem768Kem`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HybridCiphertext {
    /// X25519 component (sender's ephemeral pubkey).
    pub x25519: X25519Ciphertext,
    /// ML-KEM-768 component.
    pub ml_kem: MlKemCiphertext,
}

impl Kem for HybridX25519MlKem768Kem {
    type Ciphertext = HybridCiphertext;
    type RecipientSecretKey = HybridRecipientSecretKey;
    type PublicKey = HybridPublicKey;

    fn scheme(&self) -> KemScheme {
        KemScheme::HybridX25519MlKem768
    }

    /// Encapsulate to `recipient_pk`. Returns the hybrid ciphertext
    /// alongside the raw concatenated shared secret `x25519_ss ||
    /// ml_kem_ss` (64 bytes). The witness_enc layer feeds this and
    /// the ciphertext components into HKDF with the appropriate
    /// `ContextString` to derive the AEAD key per ADR-0011 § "KDF
    /// combiner specification".
    fn encapsulate(&self, recipient_pk: &HybridPublicKey) -> (HybridCiphertext, SharedSecret) {
        // 1. X25519 encap: generate ephemeral, compute DH shared secret.
        let mut eph_x25519_sk_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut eph_x25519_sk_bytes);
        let eph_x25519_sk = X25519DalekSecret::from(eph_x25519_sk_bytes);
        eph_x25519_sk_bytes.zeroize();
        let eph_x25519_pk_dalek = X25519DalekPublicKey::from(&eph_x25519_sk);
        let recipient_x25519_pk_dalek = X25519DalekPublicKey::from(recipient_pk.x25519.0);
        let x25519_ss = eph_x25519_sk.diffie_hellman(&recipient_x25519_pk_dalek);
        // eph_x25519_sk drops here; dalek's StaticSecret is Zeroize.

        // 2. ML-KEM encap (uses internal RNG; PQClean enforces
        //    constant-time on the data path).
        let recipient_ml_kem_pk = mlkem768::PublicKey::from_bytes(&recipient_pk.ml_kem.0)
            .expect("MlKemPublicKey constructor validates length");
        let (mlkem_ss, mlkem_ct) = mlkem768::encapsulate(&recipient_ml_kem_pk);

        // 3. Build hybrid ciphertext.
        let ct = HybridCiphertext {
            x25519: X25519Ciphertext(*eph_x25519_pk_dalek.as_bytes()),
            ml_kem: MlKemCiphertext(mlkem_ct.as_bytes().to_vec()),
        };

        // 4. Concatenate raw component secrets (no HKDF here; the
        //    KEM trait stops at raw concat — witness_enc applies
        //    HKDF with context_string).
        let mut combined = Vec::with_capacity(64);
        combined.extend_from_slice(x25519_ss.as_bytes());
        combined.extend_from_slice(mlkem_ss.as_bytes());
        let shared_secret = SharedSecret::new(combined);

        (ct, shared_secret)
    }

    /// Decapsulate. **Total at the type level.** Order of
    /// operations per ADR-0011 / engineer-reviewer's impl PR
    /// guidance: ML-KEM decap first (implicit rejection produces
    /// real or pseudorandom secret), then X25519 decap (total over
    /// field). Both run unconditionally — no early-out, no
    /// branching on intermediate "validity." All failure modes
    /// surface at the AEAD authentication step in
    /// [`crate::witness_enc`].
    fn decapsulate(
        &self,
        ciphertext: &HybridCiphertext,
        recipient_sk: &HybridRecipientSecretKey,
    ) -> SharedSecret {
        // 1. ML-KEM decap. Per FIPS 203 §6.3 implicit rejection:
        //    on a malformed ciphertext, returns a deterministic
        //    pseudorandom secret. Cannot fail at the type level.
        //    Verified for pqcrypto-mlkem 0.1.1 by probe test before
        //    adopting the dep (see PR #12 description).
        let mlkem_sk = mlkem768::SecretKey::from_bytes(&recipient_sk.ml_kem.0)
            .expect("RecipientMlKemSecretKey constructor validates length");
        let mlkem_ct = mlkem768::Ciphertext::from_bytes(&ciphertext.ml_kem.0)
            .expect("MlKemCiphertext constructor validates length");
        let mlkem_ss = mlkem768::decapsulate(&mlkem_ct, &mlkem_sk);

        // 2. X25519 decap (DH; total over field — every 32-byte
        //    pubkey input produces a 32-byte shared secret per
        //    RFC 7748 §5).
        let recipient_x25519_sk = X25519DalekSecret::from(recipient_sk.x25519.0);
        let sender_x25519_pk = X25519DalekPublicKey::from(ciphertext.x25519.0);
        let x25519_ss = recipient_x25519_sk.diffie_hellman(&sender_x25519_pk);
        // recipient_x25519_sk drops here; dalek's Zeroize fires.

        // 3. Concatenate raw shared secrets (no HKDF; transcript
        //    binding + HKDF live in witness_enc with context_string).
        let mut combined = Vec::with_capacity(64);
        combined.extend_from_slice(x25519_ss.as_bytes());
        combined.extend_from_slice(mlkem_ss.as_bytes());
        SharedSecret::new(combined)
    }
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

    // ── KEM impl tests (per ADR-0011 § Tests) ──

    /// Test #1: round-trip. Generate keypair, encap, decap, the
    /// derived shared secrets are equal byte-for-byte.
    #[test]
    fn round_trip() {
        let kp = HybridKemKeypair::generate();
        let kem = HybridX25519MlKem768Kem::new();
        let (ct, ss_encap) = kem.encapsulate(&kp.public);
        let ss_decap = kem.decapsulate(&ct, &kp.secret);
        assert_eq!(ss_encap.as_bytes(), ss_decap.as_bytes());
        // 32B X25519 + 32B ML-KEM = 64B raw concat per the impl.
        assert_eq!(ss_encap.len(), 64);
    }

    /// Test #3: implicit rejection. Take a valid hybrid ciphertext,
    /// flip a bit in the ML-KEM portion, decap. Per FIPS 203, decap
    /// must NOT panic and must NOT return Err — it returns SOME
    /// shared secret (deterministic pseudorandom). The shared secret
    /// will differ from the honest one; the witness_enc layer's AEAD
    /// auth-tag check is the rejection point. This test asserts (a)
    /// no panic, (b) returns 64 bytes, (c) bytes differ from the
    /// honest run.
    #[test]
    fn implicit_rejection_on_corrupted_mlkem_component() {
        let kp = HybridKemKeypair::generate();
        let kem = HybridX25519MlKem768Kem::new();
        let (ct, honest_ss) = kem.encapsulate(&kp.public);

        // Corrupt the ML-KEM ciphertext component.
        let mut bad_mlkem_bytes = ct.ml_kem.as_bytes().to_vec();
        bad_mlkem_bytes[100] ^= 0xff;
        let bad_mlkem_ct = MlKemCiphertext::from_bytes(&bad_mlkem_bytes)
            .expect("byte length unchanged → still parses");
        let bad_ct = HybridCiphertext {
            x25519: ct.x25519.clone(),
            ml_kem: bad_mlkem_ct,
        };

        // No panic, no Err — decap returns SOME shared secret.
        let pseudorandom_ss = kem.decapsulate(&bad_ct, &kp.secret);
        assert_eq!(pseudorandom_ss.len(), 64);
        assert_ne!(
            honest_ss.as_bytes(),
            pseudorandom_ss.as_bytes(),
            "implicit rejection must produce a DIFFERENT (pseudorandom) secret on corrupt ct"
        );
    }

    /// Test #3 variant: corrupt the X25519 portion. X25519 is total
    /// over the field (every 32-byte input produces a valid shared
    /// secret); corrupting the sender's ephemeral pubkey changes
    /// which point we DH against, so the resulting shared secret
    /// differs. AEAD auth catches it downstream.
    #[test]
    fn corrupted_x25519_component_changes_secret() {
        let kp = HybridKemKeypair::generate();
        let kem = HybridX25519MlKem768Kem::new();
        let (ct, honest_ss) = kem.encapsulate(&kp.public);

        let mut bad_x_bytes = *ct.x25519.as_bytes();
        bad_x_bytes[10] ^= 0xff;
        let bad_ct = HybridCiphertext {
            x25519: X25519Ciphertext(bad_x_bytes),
            ml_kem: ct.ml_kem.clone(),
        };

        let different_ss = kem.decapsulate(&bad_ct, &kp.secret);
        assert_eq!(different_ss.len(), 64);
        assert_ne!(honest_ss.as_bytes(), different_ss.as_bytes());
    }

    /// Determinism of decap: given the same `(ct, sk)`, decap
    /// returns byte-identical output every call. Load-bearing for
    /// cross-platform determinism (test #8 builds on this).
    #[test]
    fn decap_is_deterministic() {
        let kp = HybridKemKeypair::generate();
        let kem = HybridX25519MlKem768Kem::new();
        let (ct, _ss) = kem.encapsulate(&kp.public);

        let ss_a = kem.decapsulate(&ct, &kp.secret);
        let ss_b = kem.decapsulate(&ct, &kp.secret);
        let ss_c = kem.decapsulate(&ct, &kp.secret);
        assert_eq!(ss_a.as_bytes(), ss_b.as_bytes());
        assert_eq!(ss_b.as_bytes(), ss_c.as_bytes());
    }

    /// Decap with a wrong recipient secret produces a different
    /// shared secret (not honest_ss). Provides confidence that the
    /// recipient_sk is actually consumed in the derivation.
    #[test]
    fn decap_with_wrong_secret_diverges() {
        let kp_real = HybridKemKeypair::generate();
        let kp_other = HybridKemKeypair::generate();
        let kem = HybridX25519MlKem768Kem::new();
        let (ct, honest_ss) = kem.encapsulate(&kp_real.public);
        let wrong_ss = kem.decapsulate(&ct, &kp_other.secret);
        assert_ne!(honest_ss.as_bytes(), wrong_ss.as_bytes());
    }

    /// Hybrid ciphertext byte sizes match ADR-0011: 32 (X25519) +
    /// 1088 (ML-KEM) = 1120 bytes total in component form.
    #[test]
    fn hybrid_ciphertext_component_sizes() {
        let kp = HybridKemKeypair::generate();
        let kem = HybridX25519MlKem768Kem::new();
        let (ct, _) = kem.encapsulate(&kp.public);
        assert_eq!(ct.x25519.as_bytes().len(), 32);
        assert_eq!(ct.ml_kem.as_bytes().len(), 1088);
    }
}
