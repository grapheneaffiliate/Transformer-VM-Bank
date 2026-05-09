//! PSL ternary execution engine — Phase 2.
//!
//! Pure-integer execution layer for PSL primitives. Each primitive is an
//! analytically constructed ternary network: weights ∈ {-1, 0, +1}, biases
//! and activations are i64, ReLU non-linearity, single-shot (non-
//! autoregressive) inference. The construction follows the thermometer-
//! encoding pattern proven in the PoC:
//!
//! ```text
//! input bytes
//!    │
//!    ▼  thermometer encoding (v → v+1 ones)
//! THERMO_INPUT
//!    │
//!    ▼  layer 1: ternary {+1}-weighted sum + bias
//! TOTAL  (single integer)
//!    │
//!    ▼  layer 2: parallel ReLU(t-i+1) and ReLU(t-i) for each position
//! PARALLEL_RELU
//!    │
//!    ▼  layer 3: subtract paired ReLU outputs
//! THERMO_T
//!    │
//!    ▼  layer 4: ternary projection to (output_byte one-hot, carry)
//! OUTPUT
//! ```
//!
//! Properties this design buys us, all of which are load-bearing for the
//! agent execution layer (`docs/ARCHITECTURE.md` § 0.8):
//!
//! - **Cross-platform determinism**: integer addition is associative;
//!   any conformant implementation produces bit-identical output.
//! - **No `halt`-token convergence problem**: each forward pass produces
//!   the answer directly. There is no autoregressive loop.
//! - **Edge-deployable**: typical primitive is ≤ 1 MB packed weights,
//!   zero multiplications (only `+`/`-`), well within microcontroller /
//!   secure-enclave / FPGA budgets.
//! - **No floating point**: removes the entire fp64 reduction-order
//!   surface that the gate-8 work showed is not reproducible across
//!   PyTorch / MKL / `transformer.cpp` hard-attention.
//!
//! ## Modules
//!
//! - [`error`] — typed error enum (no panics in production paths).
//! - [`weights`] — packed weight format + BLAKE3 weights hash.
//! - [`network`] — `SparseTernaryLayer`, `TernaryNetwork`, forward pass.
//! - [`primitives`] — per-primitive constructors. Each is a pure function
//!   from primitive spec → `TernaryNetwork`. See sub-modules.
//! - [`thermo`] — thermometer encoding helpers used by all primitives.
//! - [`trace_hash`] — `trace_hash_ternary(P, x)` per
//!   `docs/ARCHITECTURE.md` § 0.8.

pub mod error;
pub mod network;
pub mod primitives;
pub mod thermo;
pub mod trace_hash;
pub mod weights;

pub use error::TernaryError;
pub use network::{SparseTernaryLayer, TernaryNetwork};
pub use trace_hash::trace_hash_ternary;
