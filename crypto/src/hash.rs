//! BLAKE3 wrapper. PSL's system-wide hash function.

use serde::{Deserialize, Serialize};

pub type Hash = [u8; 32];

pub fn hash_bytes(data: &[u8]) -> Hash {
    *blake3::hash(data).as_bytes()
}

pub fn hash_concat(a: &[u8], b: &[u8]) -> Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(a);
    hasher.update(b);
    *hasher.finalize().as_bytes()
}

pub fn hash_three(a: &[u8], b: &[u8], c: &[u8]) -> Hash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(a);
    hasher.update(b);
    hasher.update(c);
    *hasher.finalize().as_bytes()
}

pub const ZERO_HASH: Hash = [0u8; 32];

/// A wrapper for serde'ing a Hash as hex.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct HexHash(#[serde(with = "hex::serde")] pub Hash);

impl From<Hash> for HexHash {
    fn from(h: Hash) -> Self {
        HexHash(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_hash_is_blake3_of_empty() {
        let h = hash_bytes(b"");
        let expected = *blake3::hash(b"").as_bytes();
        assert_eq!(h, expected);
    }

    #[test]
    fn hash_concat_matches_streamed() {
        let a = b"hello";
        let b = b"world";
        let combined: Vec<u8> = [a.as_slice(), b.as_slice()].concat();
        assert_eq!(hash_concat(a, b), hash_bytes(&combined));
    }
}
