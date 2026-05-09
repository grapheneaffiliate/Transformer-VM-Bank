//! PSL agent wallet — Phase 2 Layer 3.
//!
//! Hierarchical-key custody for autonomous agents:
//!
//! - [`slip10`] — SLIP-0010 ed25519 hierarchical key derivation. Master
//!   key from a seed; hardened child derivation; deterministic, with
//!   round-trip test vectors.
//! - [`policy`] — per-key spending policy: max total spend in a
//!   sliding window, allowed contract names, allowed counterparty
//!   pubkeys, expiry timestamp. Parent's signature over the policy is
//!   the key's authorization.
//! - [`revocation`] — parent-signed revocation set. Revocation is
//!   monotonic (a revoked key cannot be un-revoked except by parent
//!   issuing a fresh child).
//! - [`rotation`] — key rotation that preserves outstanding contract
//!   state (the migration tx itself is a separate signed action).
//!
//! All sensitive material (private keys, seeds) is wrapped in
//! `Zeroizing<…>` so it clears on drop.
//!
//! ## Architectural notes (per `docs/ARCHITECTURE.md` § ?)
//!
//! Sigs and keys are verified by **native code**, never by the
//! transformer / ternary trace. The wallet layer slots into the
//! sequencer's mempool validation path: every transaction submitted by
//! a child key is checked against the parent's signed policy before
//! the trace runs. A key whose policy is exhausted, whose expiry has
//! passed, or which has been revoked is rejected at validation time.

pub mod error;
pub mod policy;
pub mod revocation;
pub mod rotation;
pub mod slip10;

pub use error::WalletError;
pub use policy::{KeyPolicy, PolicyEnvelope, SpendingTracker};
pub use revocation::RevocationSet;
pub use rotation::KeyRotation;
pub use slip10::{Ed25519ChildKey, Ed25519MasterKey};
