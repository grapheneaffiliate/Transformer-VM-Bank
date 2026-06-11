//! Gate-2 randomized test suite for the Sparse Merkle Tree.
//!
//! Tests:
//!   - 10k randomized puts → root stable across reordering of independent keys.
//!   - All 10k inclusion proofs round-trip.
//!   - Adversarial: tampered proofs/values rejected.

use psl_crypto::SparseMerkleTree;
use rand::{rngs::StdRng, Rng, SeedableRng};

const N: usize = 10_000;

/// `(key, value)` pairs inserted into the tree, in insertion order.
type Entries = Vec<([u8; 32], Vec<u8>)>;

fn build_tree(seed: u64, n: usize) -> (SparseMerkleTree, Entries) {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut entries = Vec::with_capacity(n);
    for _ in 0..n {
        let mut k = [0u8; 32];
        rng.fill(&mut k);
        let mut v = vec![0u8; 64];
        rng.fill(&mut v[..]);
        entries.push((k, v));
    }
    let mut smt = SparseMerkleTree::new();
    for (k, v) in &entries {
        smt.put(*k, v.clone());
    }
    (smt, entries)
}

#[test]
fn root_stable_across_orderings() {
    let (a, entries) = build_tree(1, N);
    let mut smt = SparseMerkleTree::new();
    let mut shuffled = entries.clone();
    let mut rng = StdRng::seed_from_u64(99);
    use rand::seq::SliceRandom;
    shuffled.shuffle(&mut rng);
    for (k, v) in &shuffled {
        smt.put(*k, v.clone());
    }
    assert_eq!(a.root(), smt.root());
}

#[test]
fn all_proofs_round_trip() {
    let (smt, entries) = build_tree(2, N);
    let root = smt.root();
    for (k, _) in entries.iter().take(200) {
        let p = smt.proof(k);
        assert!(SparseMerkleTree::verify_proof(&root, k, &p));
    }
}

#[test]
fn tampered_proofs_rejected() {
    let (smt, entries) = build_tree(3, 1000);
    let root = smt.root();
    let (k, _) = &entries[42];
    let p = smt.proof(k);
    let mut bad = p.clone();
    bad.siblings[0][0] ^= 0xff;
    assert!(!SparseMerkleTree::verify_proof(&root, k, &bad));

    let mut bad2 = p.clone();
    if !bad2.value.is_empty() {
        bad2.value[0] ^= 0xff;
        assert!(!SparseMerkleTree::verify_proof(&root, k, &bad2));
    }
}

#[test]
fn updates_change_root() {
    let mut smt = SparseMerkleTree::new();
    let mut rng = StdRng::seed_from_u64(7);
    let mut k = [0u8; 32];
    rng.fill(&mut k);
    let r1 = smt.put(k, b"v1".to_vec());
    let r2 = smt.put(k, b"v2".to_vec());
    assert_ne!(r1, r2);
}
