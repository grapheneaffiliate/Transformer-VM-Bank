//! Transformer forward pass (Phase 1.5).
//!
//! Source: `Transformer-VM/transformer_vm/model/transformer.py` and
//! `transformer.cpp`. RMSNorm + multi-head attention + ReGLU FFN.
//!
//! TODO(phase-1.5):
//!   - RMSNorm (no LayerNorm bias).
//!   - Multi-head attention with the existing Python's projection layout.
//!   - ReGLU FFN: `ff_in` produces (gate, value); output = value · ReLU(gate);
//!     `ff_out` projects back to d_model.
//!   - Logit head (`head.weight`).
//!   - argmax over output_tokens (a subset of vocab — see `output_tokens`
//!     in the program graph).
//!   - All matmuls in f64 to match the Python path exactly.

use crate::weights::Weights;
use anyhow::{anyhow, Result};

pub struct Transformer<'a> {
    pub w: &'a Weights,
}

impl<'a> Transformer<'a> {
    pub fn new(w: &'a Weights) -> Self {
        Self { w }
    }

    pub fn forward(&self, _tokens: &[usize]) -> Result<Vec<f64>> {
        Err(anyhow!("rust_runner::transformer::forward not yet implemented"))
    }
}
