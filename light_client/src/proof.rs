//! Merkle-proof verification re-export.
//!
//! Light client's proof verification is identical to the sequencer's; we
//! re-export `psl_crypto::SparseMerkleTree::verify_proof` here so callers
//! can invoke it without pulling in the full crypto crate.

pub use psl_crypto::{MerkleProof, SparseMerkleTree};
