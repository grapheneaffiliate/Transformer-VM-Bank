//! Top-level greedy-argmax generation (Phase 1.5).
//!
//! Mirrors `Transformer-VM/transformer_vm/runner.py:run_model_program`.

use crate::transformer::Transformer;
use crate::weights::Weights;
use anyhow::{anyhow, Result};

pub struct GenerateConfig {
    pub max_new_tokens: usize,
}

impl Default for GenerateConfig {
    fn default() -> Self {
        Self {
            max_new_tokens: 50_000,
        }
    }
}

/// Run a token program (string sequence) through the model and return the
/// full predicted token sequence (including the input prompt — matches
/// Python `generate_with_cache`'s return value).
pub fn generate(w: &Weights, input_tokens: &[&str], cfg: &GenerateConfig) -> Result<Vec<String>> {
    let mut idx_seq = Vec::with_capacity(input_tokens.len());
    for &tok in input_tokens {
        let i = w
            .tok_to_idx
            .get(tok)
            .ok_or_else(|| anyhow!("unknown token in input: {tok:?}"))?;
        idx_seq.push(*i);
    }
    let model = Transformer::new(w);
    let result_ids = model.generate(&idx_seq, cfg.max_new_tokens);
    let result: Vec<String> = result_ids.iter().map(|&i| w.tokens[i].clone()).collect();
    Ok(result)
}
