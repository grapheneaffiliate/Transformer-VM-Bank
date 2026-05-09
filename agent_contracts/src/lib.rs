//! PSL standard contract library — Phase 2 Layer 2.
//!
//! Each contract is a `TernaryProgram`: a deterministic, integer-only,
//! single-shot composition of `psl-ternary-vm` primitives. Same inputs
//! produce bit-identical outputs on any conformant integer-arithmetic
//! verifier.
//!
//! ## Layer 2 design
//!
//! The strategic plan describes a fully parsed DSL with a typechecker,
//! interpreter, and a compiler that lowers to ternary networks. This
//! crate ships the *runtime substrate* for that DSL:
//!
//! - The `TernaryProgram` trait — a pure function from `&[u8] → Result<Vec<u8>>`
//!   plus a `weights_hash` commitment that covers all embedded sub-networks.
//! - A standard library of contracts hand-coded against this trait.
//! - Each contract carries a `program_hash()` (BLAKE3 over the
//!   contract's name + sub-network weights_hashes in canonical order)
//!   that plays the same role as `weights_hash(P)` in
//!   `trace_hash_ternary` (`docs/ARCHITECTURE.md § 0.8`).
//!
//! Once the parsed DSL lands (gate 11), the compiler will *produce*
//! `TernaryProgram` instances; the trait is the stable interface.
//!
//! ## Standard contracts (per the strategic plan)
//!
//! - `transfer` — debit sender, credit recipient, increment sender nonce.
//! - `swap`, `escrow_create`, `escrow_release`, `escrow_refund`,
//!   `time_locked_release`, `multisig_2of3`, `conditional_payment` —
//!   landing in subsequent commits using the same composition pattern.
//!
//! This iteration ships `transfer` end-to-end with random-witness
//! verification.

pub mod conditional;
pub mod error;
pub mod escrow;
pub mod guarded;
pub mod program;
pub mod swap;
pub mod transfer;

pub use conditional::{ConditionalPayment, Multisig2of3, TimeLockedRelease};
pub use error::ContractError;
pub use escrow::{EscrowCreate, EscrowRefund, EscrowRelease};
pub use program::{ProgramHash, TernaryProgram};
pub use swap::SwapContract;
pub use transfer::TransferContract;
