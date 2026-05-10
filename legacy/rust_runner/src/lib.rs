// The whole crate is `#[deprecated]` per ADR-0001; its own internals
// reference each other and would otherwise emit deprecation warnings
// from inside the crate itself. Suppress at crate root so the legacy
// build remains warning-free without needing per-callsite #[allow].
#![allow(deprecated)]

//! **LEGACY — frozen per ADR-0001 (`docs/decisions/0001-retire-legacy-fp64-runner.md`).**
//!
//! Pure-Rust port of Transformer-VM's specialized-model runner. This
//! is the Phase 1.5 fp64 autoregressive runner. Gates 10-16 of the
//! Phase 2 work (`ternary_vm/`) made it architecturally redundant for
//! all primitives that have a ternary equivalent. It stays in tree
//! for backward-compatibility verification only.
//!
//! New code MUST NOT depend on this crate. The CI guard
//! `tools/ci/check_legacy_isolation.sh` enforces this — any
//! `use psl_rust_runner::…` or `psl-rust-runner = …` outside the
//! `legacy/` subtree fails the build.
//!
//! ## Migration
//!
//! Use `psl_ternary_vm` instead. Each Phase 1 primitive has a
//! drop-in replacement constructor in
//! `psl_ternary_vm::primitives::*`:
//!
//! | Legacy primitive       | Ternary replacement                                      |
//! |------------------------|----------------------------------------------------------|
//! | byte_add_with_carry    | `psl_ternary_vm::primitives::byte_add_with_carry::build` |
//! | byte_sub_with_borrow   | `psl_ternary_vm::primitives::byte_sub_with_borrow::build`|
//! | freeze_apply           | `psl_ternary_vm::primitives::freeze_apply::build`        |
//! | freeze_setup           | `psl_ternary_vm::primitives::freeze_setup::build`        |
//! | mpt_emit_record        | `psl_ternary_vm::primitives::mpt_emit_record::build`     |
//! | transfer_check         | `psl_ternary_vm::primitives::transfer_check::build`      |
//! | transfer_finalize      | `psl_ternary_vm::primitives::transfer_finalize::build`   |
//!
//! The trace-hash contract for the ternary engine
//! (`docs/ARCHITECTURE.md § 0.2`) replaces the autoregressive
//! token-sequence contract this crate implemented.

#![deprecated(
    since = "0.1.0",
    note = "use `psl_ternary_vm` instead — see docs/decisions/0001-retire-legacy-fp64-runner.md"
)]

pub mod attention;
pub mod generate;
pub mod sparse;
pub mod transformer;
pub mod weights;

pub use generate::{generate, GenerateConfig};
