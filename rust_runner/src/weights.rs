//! Weights file format parser (Phase 1.5).
//!
//! Source: `Transformer-VM/transformer_vm/model/weights.py`.
//!
//! The format (as of 2026-05-03 — re-read save_weights() to confirm):
//!   1. Header: `{ "vocab": int, "d_model": int, "n_heads": int,
//!                 "n_layers": int, "d_ffn_per_layer": [int, ...] }` as JSON,
//!      length-prefixed by a u32.
//!   2. Token table: length-prefixed list of UTF-8 strings (one per vocab idx).
//!   3. Per-layer tensors as packed f64 arrays:
//!        - `tok.weight`           [vocab, d_model]
//!        - `head.weight`          [vocab, d_model]
//!        - `attn[i].in_proj_weight`  [3*d_model, d_model]
//!        - `attn[i].out_proj.weight` [d_model, d_model]
//!        - `ff_in[i].weight`         [2*d_ffn, d_model]    (RMSNorm + ReGLU gate)
//!        - `ff_out[i].weight`        [d_model, d_ffn]
//!
//! TODO(phase-1.5):
//!   - Implement load_weights() returning a `Weights` struct.
//!   - Bit-exact-verify on Transformer-VM/transformer_vm/tests/fixtures/
//!     by loading the same .bin in both Python and Rust and asserting
//!     equal floats (or, more practically, equal generated token sequences).

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelHeader {
    pub vocab: usize,
    pub d_model: usize,
    pub n_heads: usize,
    pub n_layers: usize,
    pub d_ffn_per_layer: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct Weights {
    pub header: ModelHeader,
    pub tokens: Vec<String>,
    // TODO: per-layer ndarray fields; left out until the binary layout is
    // fully reversed.
}

pub fn load_weights(_path: &std::path::Path) -> Result<Weights> {
    Err(anyhow!("rust_runner::weights::load_weights not yet implemented; see TODO in lib.rs"))
}
