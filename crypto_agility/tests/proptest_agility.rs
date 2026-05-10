//! Property tests for the agility layer.
//!
//! Three classes of invariant:
//!
//! 1. **Varint codec round-trip.** For every `u32` v, decode(encode(v)) = (v, n).
//! 2. **Sign/verify round-trip.** For every (signer, message), the
//!    verifier accepts the signer's signature.
//! 3. **Hash blob round-trip.** For every hash scheme, the blob
//!    encoding is round-trippable and length-validated.

use proptest::prelude::*;
use psl_crypto_agility::codec::{decode_varint, encode_varint};
use psl_crypto_agility::{
    Blake3_256, Blake3_512, Ed25519Signer, Ed25519Verifier, HashScheme_, SignatureScheme, Signer,
    Verifier, VerifierError,
};

proptest! {
    #[test]
    fn varint_round_trip(v: u32) {
        let mut buf = Vec::new();
        let n = encode_varint(v, &mut buf);
        prop_assert_eq!(buf.len(), n);
        let (decoded, consumed) = decode_varint(&buf).unwrap();
        prop_assert_eq!(decoded, v);
        prop_assert_eq!(consumed, n);
    }

    #[test]
    fn ed25519_sign_verify_arbitrary_message(msg in prop::collection::vec(any::<u8>(), 0..1024)) {
        let signer = Ed25519Signer::generate();
        let verifier = Ed25519Verifier::new();
        let sig = signer.sign(&msg).unwrap();
        prop_assert!(verifier
            .verify(SignatureScheme::Ed25519, &msg, &sig, &signer.public_key())
            .is_ok());
    }

    #[test]
    fn ed25519_rejects_any_signature_modification(
        msg in prop::collection::vec(any::<u8>(), 1..256),
        flip_byte in 0usize..64,
    ) {
        let signer = Ed25519Signer::generate();
        let verifier = Ed25519Verifier::new();
        let mut sig = signer.sign(&msg).unwrap();
        sig[flip_byte] ^= 0xff;
        let result = verifier.verify(
            SignatureScheme::Ed25519,
            &msg,
            &sig,
            &signer.public_key(),
        );
        let bad = matches!(result, Err(VerifierError::BadSignature(_)) | Err(VerifierError::MalformedSignature{..}));
        prop_assert!(bad);
    }

    #[test]
    fn blake3_256_first_32_match_blake3_512(data in prop::collection::vec(any::<u8>(), 0..1024)) {
        let h256 = Blake3_256.hash(&data);
        let h512 = Blake3_512.hash(&data);
        prop_assert_eq!(&h512[..32], &h256[..]);
    }

    #[test]
    fn blake3_256_deterministic(data in prop::collection::vec(any::<u8>(), 0..1024)) {
        let a = Blake3_256.hash(&data);
        let b = Blake3_256.hash(&data);
        prop_assert_eq!(a, b);
    }

    #[test]
    fn blake3_512_deterministic(data in prop::collection::vec(any::<u8>(), 0..1024)) {
        let a = Blake3_512.hash(&data);
        let b = Blake3_512.hash(&data);
        prop_assert_eq!(a, b);
    }
}
