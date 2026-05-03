//! Pure-Rust port of Transformer-VM's specialized-model runner.
//!
//! Phase 1.5 of the PSL plan: replace the PyO3/subprocess bridge into Python
//! with a native Rust runner that produces bit-exact-identical token
//! sequences and runs ≥10× faster.
//!
//! ## Source-of-truth ports
//!
//! Each module here is a faithful port of the corresponding Python/C++
//! component in `Transformer-VM/transformer_vm/`. Bit-exact parity is
//! verified by the gate-8 test (`tests/test_runner_parity.rs`), which runs
//! both runners against the gate-1 vectors and asserts identical output.
//!
//! - [`weights`]: parser for the `.bin` format produced by
//!   `transformer_vm.model.weights::save_weights`. Layout is
//!   `(header || tok_to_idx_map || all_tokens || per-layer weights)`.
//! - [`transformer`]: forward pass matching `model/transformer.py` and
//!   `model/transformer.cpp`. Greedy argmax decoding only.
//! - [`attention`]: `StandardKVCache` only — `HullKVCache` deferred (its
//!   convex-hull math has float-ish internals that risk drift; the docs
//!   pin StandardKVCache as the production cache).
//! - [`generate`]: top-level `generate_with_cache` API, identical signature
//!   to the Python version.
//!
//! ## Status
//!
//! **Skeleton only.** The actual port is multi-day work:
//!   1. Reverse the binary weight format (read `weights.py` save_weights /
//!      load_weights to get the layout exactly).
//!   2. Port transformer forward pass (RMSNorm, attention, FFN with ReGLU).
//!   3. Port `StandardKVCache`.
//!   4. Bit-exact-verify on hello/collatz fixtures from Transformer-VM tests.
//!   5. Run gate-1 vectors through both Python and Rust runners; diff.
//!
//! Each module below has a TODO comment listing what's left.

pub mod weights;
pub mod transformer;
pub mod attention;
pub mod generate;

pub use generate::{generate, GenerateConfig};
