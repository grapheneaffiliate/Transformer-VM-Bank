//! Hash schemes per ADR-0008.
//!
//! Two-tier policy:
//! - `Blake3_256` for short-lived surfaces (trace hashes, MPT roots,
//!   block headers).
//! - `Blake3_512` for long-lived irrevocable commitments
//!   (`weights_hash`, long-lived contract `program_hash`).
//!
//! Both are direct invocations of the standard BLAKE3 construction
//! at different output lengths — no special variant.

use crate::codec::{decode_varint, encode_varint};
use crate::errors::HashError;

/// Identifier for a hash scheme. Wire-format same as the signature
/// and KEM scheme identifiers (varint prefix).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashScheme {
    /// 256-bit BLAKE3. For short-lived hashes.
    Blake3_256 = 0x01,
    /// 512-bit BLAKE3. For long-lived irrevocable commitments per
    /// ADR-0008.
    Blake3_512 = 0x02,
}

impl HashScheme {
    /// Wire encoding as `u32`.
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    /// Decode from on-wire `u32`.
    pub fn from_u32(v: u32) -> Result<Self, HashError> {
        match v {
            0x01 => Ok(Self::Blake3_256),
            0x02 => Ok(Self::Blake3_512),
            other => Err(HashError::UnknownScheme(other)),
        }
    }

    /// Output length in bytes for this hash scheme.
    pub fn output_len(self) -> usize {
        match self {
            Self::Blake3_256 => 32,
            Self::Blake3_512 => 64,
        }
    }
}

/// Trait for hashing under a specific scheme.
pub trait HashScheme_ {
    /// Which scheme this hasher implements.
    fn scheme(&self) -> HashScheme;
    /// Hash `data`, returning a buffer of length [`HashScheme::output_len`].
    fn hash(&self, data: &[u8]) -> Vec<u8>;
}

/// 256-bit BLAKE3.
pub struct Blake3_256;

impl HashScheme_ for Blake3_256 {
    fn scheme(&self) -> HashScheme {
        HashScheme::Blake3_256
    }

    fn hash(&self, data: &[u8]) -> Vec<u8> {
        blake3::hash(data).as_bytes().to_vec()
    }
}

/// 512-bit BLAKE3 (XOF read at 64 bytes).
pub struct Blake3_512;

impl HashScheme_ for Blake3_512 {
    fn scheme(&self) -> HashScheme {
        HashScheme::Blake3_512
    }

    fn hash(&self, data: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; 64];
        let mut hasher = blake3::Hasher::new();
        hasher.update(data);
        let mut xof = hasher.finalize_xof();
        xof.fill(&mut out);
        out
    }
}

/// Encode a hash blob in wire format: `varint(scheme) || hash_bytes`.
pub fn encode_hash_blob<H: HashScheme_>(h: &H, data: &[u8]) -> Vec<u8> {
    let bytes = h.hash(data);
    let mut out = Vec::with_capacity(bytes.len() + 1);
    encode_varint(h.scheme().as_u32(), &mut out);
    out.extend_from_slice(&bytes);
    out
}

/// Decode a wire-format hash blob into `(scheme, hash_bytes)`. Length
/// of `hash_bytes` is validated against the scheme's expected length.
pub fn decode_hash_blob(blob: &[u8]) -> Result<(HashScheme, &[u8]), HashError> {
    let (scheme_u32, off) = decode_varint(blob).map_err(|_| HashError::UnknownScheme(0))?;
    let scheme = HashScheme::from_u32(scheme_u32)?;
    let body = &blob[off..];
    if body.len() != scheme.output_len() {
        return Err(HashError::WrongLength {
            scheme: scheme_u32,
            expected: scheme.output_len(),
            actual: body.len(),
        });
    }
    Ok((scheme, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blake3_256_known_vector() {
        // BLAKE3 of empty input.
        let h = Blake3_256.hash(&[]);
        assert_eq!(
            hex::encode(&h),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn blake3_512_length_is_64() {
        let h = Blake3_512.hash(b"x");
        assert_eq!(h.len(), 64);
    }

    #[test]
    fn blake3_512_first_32_bytes_equal_blake3_256() {
        // The 64-byte XOF read of BLAKE3 has the property that its first 32
        // bytes equal the standard 32-byte hash. Critical determinism property.
        let data = b"agility-layer-determinism-test";
        let h256 = Blake3_256.hash(data);
        let h512 = Blake3_512.hash(data);
        assert_eq!(&h512[..32], &h256[..]);
    }

    #[test]
    fn blob_round_trip_256() {
        let blob = encode_hash_blob(&Blake3_256, b"hello");
        let (scheme, bytes) = decode_hash_blob(&blob).unwrap();
        assert_eq!(scheme, HashScheme::Blake3_256);
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn blob_round_trip_512() {
        let blob = encode_hash_blob(&Blake3_512, b"hello");
        let (scheme, bytes) = decode_hash_blob(&blob).unwrap();
        assert_eq!(scheme, HashScheme::Blake3_512);
        assert_eq!(bytes.len(), 64);
    }

    #[test]
    fn blob_rejects_wrong_length() {
        let mut blob = Vec::new();
        encode_varint(HashScheme::Blake3_512.as_u32(), &mut blob);
        blob.extend_from_slice(&[0u8; 32]);  // wrong: 512 expects 64
        let err = decode_hash_blob(&blob).unwrap_err();
        match err {
            HashError::WrongLength { expected, actual, .. } => {
                assert_eq!(expected, 64);
                assert_eq!(actual, 32);
            }
            _ => panic!("expected WrongLength"),
        }
    }

    #[test]
    fn blob_rejects_unknown_scheme() {
        let mut blob = Vec::new();
        encode_varint(0xfeed_beef, &mut blob);
        blob.extend_from_slice(&[0u8; 32]);
        assert!(matches!(decode_hash_blob(&blob), Err(HashError::UnknownScheme(_))));
    }
}
