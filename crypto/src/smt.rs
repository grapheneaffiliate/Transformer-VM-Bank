//! Sparse Merkle Tree over 256-bit keys.
//!
//! The state commitment for PSL. Keys are ed25519 pubkeys (32 bytes); values
//! are serialized 64-byte account records. Hash function: BLAKE3.
//!
//! Internal node: `H = BLAKE3(left_hash || right_hash)`.
//! Leaf: `H = BLAKE3(0x00 || key || value_hash)` where
//!       `value_hash = BLAKE3(value_bytes)`.
//! Default subtree at depth `d`: `D[d] = BLAKE3(D[d+1] || D[d+1])`.
//!     `D[256] = BLAKE3(b"")`.
//!
//! Sparse representation: only non-default nodes are stored. The `nodes` map
//! is keyed by node-hash; each entry is `(left, right)` — two `Hash` values.
//! Leaves are stored in `leaves` map: `key → value_bytes`.
//!
//! Inclusion proofs are 256 sibling hashes (one per bit of the key).

use crate::hash::{hash_bytes, hash_concat, hash_three, Hash};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

pub const KEY_BITS: usize = 256;

#[derive(Debug, Error)]
pub enum SmtError {
    #[error("internal node not found in store: {0}")]
    NodeMissing(String),
}

/// Default subtree hashes for each depth (0 = root, 256 = empty leaf).
/// Computed once via `default_hashes()`.
fn default_hashes() -> [Hash; KEY_BITS + 1] {
    let mut d = [[0u8; 32]; KEY_BITS + 1];
    d[KEY_BITS] = hash_bytes(b"");
    let mut i = KEY_BITS;
    while i > 0 {
        i -= 1;
        d[i] = hash_concat(&d[i + 1], &d[i + 1]);
    }
    d
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct InternalNode {
    #[serde(with = "hex::serde")]
    left: Hash,
    #[serde(with = "hex::serde")]
    right: Hash,
}

#[derive(Clone, Debug)]
pub struct SparseMerkleTree {
    /// Internal-node store keyed by node hash.
    nodes: HashMap<Hash, InternalNode>,
    /// Leaf store: key → value bytes (caller decides serialization).
    leaves: HashMap<Hash, Vec<u8>>,
    /// Current root hash.
    root: Hash,
    /// Cached default hashes per depth.
    defaults: [Hash; KEY_BITS + 1],
}

impl Default for SparseMerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

impl SparseMerkleTree {
    pub fn new() -> Self {
        let defaults = default_hashes();
        Self {
            nodes: HashMap::new(),
            leaves: HashMap::new(),
            root: defaults[0],
            defaults,
        }
    }

    pub fn root(&self) -> Hash {
        self.root
    }

    /// Hash of a leaf containing `value`. Empty value → `defaults[KEY_BITS]`
    /// (= the SMT default leaf), so absent keys verify against the empty-tree
    /// progression. Standard SMT semantics: putting `(key, [])` is equivalent
    /// to removing `key`.
    fn leaf_hash(key: &Hash, value: &[u8]) -> Hash {
        if value.is_empty() {
            hash_bytes(b"")
        } else {
            let value_hash = hash_bytes(value);
            hash_three(&[0u8], key, &value_hash)
        }
    }

    /// Bit at position `i` (0 = MSB, 255 = LSB) of a 32-byte key.
    fn bit(key: &Hash, i: usize) -> bool {
        let byte = i / 8;
        let bit = 7 - (i % 8);
        (key[byte] >> bit) & 1 == 1
    }

    pub fn get(&self, key: &Hash) -> Option<&[u8]> {
        self.leaves.get(key).map(|v| v.as_slice())
    }

    /// Insert/update `key → value`. Returns the new root.
    /// Putting an empty `value` removes the key (standard SMT semantics).
    pub fn put(&mut self, key: Hash, value: Vec<u8>) -> Hash {
        let new_leaf_hash = Self::leaf_hash(&key, &value);
        let path = self.compute_path_to_leaf(&key);
        let mut current = new_leaf_hash;
        for i in (0..KEY_BITS).rev() {
            let bit = Self::bit(&key, i);
            let sibling = path[i];
            let (left, right) = if bit {
                (sibling, current)
            } else {
                (current, sibling)
            };
            let parent = hash_concat(&left, &right);
            self.nodes.insert(parent, InternalNode { left, right });
            current = parent;
        }
        if value.is_empty() {
            self.leaves.remove(&key);
        } else {
            self.leaves.insert(key, value);
        }
        self.root = current;
        self.root
    }

    /// For each depth `d` (0..=255), compute the sibling hash on the path to `key`.
    /// `path[d]` = sibling at depth `d` (the subtree NOT containing `key`).
    fn compute_path_to_leaf(&self, key: &Hash) -> [Hash; KEY_BITS] {
        let mut siblings = [[0u8; 32]; KEY_BITS];
        let mut current = self.root;
        for d in 0..KEY_BITS {
            if let Some(node) = self.nodes.get(&current) {
                let bit = Self::bit(key, d);
                if bit {
                    siblings[d] = node.left;
                    current = node.right;
                } else {
                    siblings[d] = node.right;
                    current = node.left;
                }
            } else {
                // current is a default subtree at depth `d`; all siblings below
                // are also defaults.
                for k in d..KEY_BITS {
                    siblings[k] = self.defaults[k + 1];
                }
                break;
            }
        }
        siblings
    }

    /// Generate an inclusion proof for `key`. Returns the 256 sibling hashes
    /// and the value bytes (or empty vec if absent).
    pub fn proof(&self, key: &Hash) -> MerkleProof {
        let siblings = self.compute_path_to_leaf(key);
        let value = self.leaves.get(key).cloned().unwrap_or_default();
        MerkleProof {
            siblings: siblings.to_vec(),
            value,
        }
    }

    /// Verify an inclusion (or non-inclusion) proof.
    /// For inclusion: pass the value bytes that were stored.
    /// For non-inclusion: pass an empty `Vec<u8>`.
    pub fn verify_proof(root: &Hash, key: &Hash, proof: &MerkleProof) -> bool {
        if proof.siblings.len() != KEY_BITS {
            return false;
        }
        let leaf_hash = Self::leaf_hash(key, &proof.value);
        let mut current = leaf_hash;
        for d in (0..KEY_BITS).rev() {
            let sibling = proof.siblings[d];
            let bit = Self::bit(key, d);
            let (left, right) = if bit {
                (sibling, current)
            } else {
                (current, sibling)
            };
            current = hash_concat(&left, &right);
        }
        current == *root
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MerkleProof {
    /// 256 sibling hashes, one per depth (0 = root child, 255 = leaf parent).
    pub siblings: Vec<Hash>,
    /// Stored value (empty if non-inclusion proof).
    pub value: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_from_byte(b: u8) -> Hash {
        let mut k = [0u8; 32];
        k[0] = b;
        k
    }

    #[test]
    fn empty_root_is_default() {
        let smt = SparseMerkleTree::new();
        assert_eq!(smt.root(), smt.defaults[0]);
    }

    #[test]
    fn put_then_get() {
        let mut smt = SparseMerkleTree::new();
        let key = key_from_byte(1);
        smt.put(key, b"alice".to_vec());
        assert_eq!(smt.get(&key), Some(b"alice".as_ref()));
    }

    #[test]
    fn update_changes_root() {
        let mut smt = SparseMerkleTree::new();
        let key = key_from_byte(1);
        let r1 = smt.put(key, b"v1".to_vec());
        let r2 = smt.put(key, b"v2".to_vec());
        assert_ne!(r1, r2);
    }

    #[test]
    fn put_order_independent_for_independent_keys() {
        let mut a = SparseMerkleTree::new();
        let mut b = SparseMerkleTree::new();
        for k in [3u8, 7, 1, 9, 5] {
            a.put(key_from_byte(k), vec![k]);
        }
        for k in [9u8, 5, 7, 3, 1] {
            b.put(key_from_byte(k), vec![k]);
        }
        assert_eq!(a.root(), b.root());
    }

    #[test]
    fn inclusion_proof_round_trips() {
        let mut smt = SparseMerkleTree::new();
        for k in 0u8..32 {
            smt.put(key_from_byte(k), vec![k]);
        }
        let target = key_from_byte(7);
        let proof = smt.proof(&target);
        assert!(SparseMerkleTree::verify_proof(&smt.root(), &target, &proof));
    }

    #[test]
    fn tampered_value_fails_proof() {
        let mut smt = SparseMerkleTree::new();
        let k = key_from_byte(1);
        smt.put(k, b"correct".to_vec());
        let mut proof = smt.proof(&k);
        proof.value = b"tampered".to_vec();
        assert!(!SparseMerkleTree::verify_proof(&smt.root(), &k, &proof));
    }

    #[test]
    fn non_inclusion_proof() {
        let mut smt = SparseMerkleTree::new();
        smt.put(key_from_byte(1), b"present".to_vec());
        let absent = key_from_byte(2);
        let proof = smt.proof(&absent);
        assert!(proof.value.is_empty());
        assert!(SparseMerkleTree::verify_proof(&smt.root(), &absent, &proof));
    }
}
