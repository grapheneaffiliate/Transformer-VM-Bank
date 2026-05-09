//! Per-primitive ternary network constructors. Each module here defines
//! one PSL primitive's analytical construction. The construction is a
//! pure function — same primitive spec yields bit-identical weights.
//!
//! Naming and shapes match the active primitives in `primitives/`
//! (the C-source set used by the transformer-VM path):
//!
//! - `byte_add_with_carry` — input `(a, b, carry_in)`, output `(sum_byte, carry_out)`
//! - `byte_sub_with_borrow` — input `(m, s, borrow_in)`, output `(diff_byte, borrow_out)`
//! - `transfer_check` — 16-byte u128 ≥ compare → 1-byte ok flag
//! - `transfer_finalize` — 8-byte u64 nonce → +1
//! - `freeze_setup` / `freeze_apply` — chained freeze pipeline
//! - `mpt_emit_record` — 64-byte pass-through
//!
//! Order of implementation per the strategic plan:
//! `byte_add → byte_sub → freeze_apply → transfer_finalize →
//!  transfer_check → mpt_emit_record → freeze_setup`.
//!
//! This iteration ships `byte_add_with_carry`. The remaining primitives
//! follow the same construction pattern and will land in subsequent
//! commits.

pub mod byte_add_with_carry;
