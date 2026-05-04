//! PSL crypto primitives.
//!
//! - [`hash`]: BLAKE3 wrapper, the system-wide hash function (`Hash` = 32 bytes).
//! - [`signature`]: ed25519 wrapper for tx signing and block-header signing.
//! - [`smt`]: Sparse Merkle Tree (256-bit keys, BLAKE3 hashing) — the system
//!   state commitment. See [`smt::SparseMerkleTree`] for the API.
//! - [`account`]: 64-byte fixed account record + serialization to/from the
//!   wire bytes consumed by the transformer-trace primitives.

pub mod hash;
pub mod signature;
pub mod smt;
pub mod account;
pub mod trace_hash;

pub use hash::{Hash, hash_bytes, hash_concat};
pub use signature::{KeyPair, PublicKey, SigError, Signature, sign, verify};
pub use smt::{SparseMerkleTree, MerkleProof, SmtError};
pub use account::{Account, FROZEN_FLAG};
pub use trace_hash::{hash_trace, hash_trace_owned};
