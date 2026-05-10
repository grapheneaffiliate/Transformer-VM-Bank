//! PSL crypto primitives.
//!
//! - [`hash`]: BLAKE3 wrapper, the system-wide hash function (`Hash` = 32 bytes).
//! - [`signature`]: ed25519 wrapper for tx signing and block-header signing.
//! - [`smt`]: Sparse Merkle Tree (256-bit keys, BLAKE3 hashing) — the system
//!   state commitment. See [`smt::SparseMerkleTree`] for the API.
//! - [`account`]: 64-byte fixed account record + serialization to/from the
//!   wire bytes consumed by the transformer-trace primitives.

pub mod account;
pub mod hash;
pub mod signature;
pub mod smt;
pub mod trace_hash;

pub use account::{Account, FROZEN_FLAG};
pub use hash::{hash_bytes, hash_concat, Hash};
pub use signature::{sign, verify, KeyPair, PublicKey, SigError, Signature};
pub use smt::{MerkleProof, SmtError, SparseMerkleTree};
pub use trace_hash::{hash_trace, hash_trace_owned};
