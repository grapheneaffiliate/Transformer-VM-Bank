//! Top-level greedy-argmax generation (Phase 1.5).

use anyhow::Result;
use crate::weights::Weights;

pub struct GenerateConfig {
    pub max_new_tokens: usize,
    pub stop_token: String,
}

impl Default for GenerateConfig {
    fn default() -> Self {
        Self { max_new_tokens: 50_000, stop_token: "halt".to_string() }
    }
}

pub fn generate(_w: &Weights, _input: &[String], _cfg: &GenerateConfig) -> Result<Vec<String>> {
    anyhow::bail!("rust_runner::generate not yet implemented; see lib.rs TODO")
}
