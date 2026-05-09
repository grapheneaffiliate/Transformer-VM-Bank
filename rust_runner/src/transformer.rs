//! Transformer forward pass (Phase 1.5).
//!
//! Source: `Transformer-VM/transformer_vm/model/transformer.py:52`.
//!
//! Per-token forward pass (greedy argmax decoding). The Python
//! `generate_with_cache` runs this loop:
//!
//!     x = tok.weight[token_id].clone()
//!     add_position_encoding(x, pos)               // x[0] += pos; x[1] += k(pos); x[2] += pos²
//!     for layer in 0..n_layers:
//!         q,k,v = (in_proj @ x).chunk(3, dim=-1)
//!         out   = cache.layer_step(layer, k, q, v)
//!         x     = x + out_proj @ out
//!         g, v  = (ff_in @ x).chunk(2, dim=-1)
//!         x     = x + ff_out @ (relu(g) * v)
//!     if generating: return argmax(head @ x)
//!
//! No layer norm, no attention scaling, no biases — matches the analytical
//! construction.

use crate::attention::StandardKVCache;
use crate::weights::Weights;
use ndarray::{s, Array1};

pub struct Transformer<'a> {
    pub w: &'a Weights,
}

#[inline]
fn add_position_encoding(x: &mut Array1<f64>, pos: usize) {
    let p = pos as f64;
    x[0] += p;
    x[1] += 1.0 / (2.0_f64).ln() - 1.0 / ((p + 2.0).ln());
    x[2] += p * p;
}

impl<'a> Transformer<'a> {
    pub fn new(w: &'a Weights) -> Self {
        Self { w }
    }

    /// One forward step: returns post-block residual `x` ready for either
    /// the head or the next position.
    fn step(&self, cache: &mut StandardKVCache, token_id: usize, pos: usize) -> Array1<f64> {
        let d_model = self.w.header.d_model;
        let mut x: Array1<f64> = self.w.tok_embed.row(token_id).to_owned();
        add_position_encoding(&mut x, pos);

        for (li, layer) in self.w.layers.iter().enumerate() {
            // Attention. in_proj is [3*d_model, d_model]; result [3*d_model].
            // Sparse path active when ≥50% of W entries are zero (the common
            // case for analytical-construction weights — see crate::sparse).
            let proj: Array1<f64> = layer.in_proj.matvec(&x);
            // chunk(3) along last dim ⇒ contiguous splits of size d_model.
            let q = proj.slice(s![0..d_model]).to_owned();
            let k = proj.slice(s![d_model..2 * d_model]).to_owned();
            let v = proj.slice(s![2 * d_model..3 * d_model]).to_owned();

            let attn_out = cache.layer_step(li, k, q, v);
            // out_proj is Linear(d_model, d_model) ⇒ weight [d_model, d_model].
            let out_proj = layer.out_proj.matvec(&attn_out);
            x = &x + &out_proj;

            // FFN. ff_in is [2*width, d_model]; result [2*width].
            let ffn_proj: Array1<f64> = layer.ff_in.matvec(&x);
            let width = self.w.header.d_ffn_per_layer[li];
            let gate = ffn_proj.slice(s![0..width]);
            let val = ffn_proj.slice(s![width..2 * width]);
            // ReGLU: relu(gate) * val (element-wise).
            let mut act: Array1<f64> = Array1::zeros(width);
            for i in 0..width {
                act[i] = gate[i].max(0.0) * val[i];
            }
            // ff_out is Linear(width, d_model) ⇒ weight [d_model, width].
            let ff_out_vec = layer.ff_out.matvec(&act);
            x = &x + &ff_out_vec;
        }
        x
    }

    /// Greedy generate. Consumes `prompt` as the warm-up KV cache, then
    /// emits new tokens by argmax-decoding `head @ x` until either
    /// `stop_token_id` or `max_new_tokens` is hit.
    /// Returns the full token id sequence (including the prompt).
    pub fn generate(&self, prompt: &[usize], max_new_tokens: usize) -> Vec<usize> {
        let n_layers = self.w.header.n_layers;
        let n_heads = self.w.header.n_heads;
        let d_model = self.w.header.d_model;
        let stop = self.w.header.stop_token_id;
        let mut cache = StandardKVCache::new(n_layers, n_heads, d_model);

        let mut idx_list: Vec<usize> = prompt.to_vec();
        let prompt_len = prompt.len();
        let total_steps = prompt_len + max_new_tokens;
        let mut pos = 0usize;
        while pos < total_steps {
            if pos >= idx_list.len() {
                break;
            }
            let x = self.step(&mut cache, idx_list[pos], pos);
            // Generate next only after the prompt is fully consumed; matches
            // the Python condition `if pos + 1 == len(idx_list)`.
            if pos + 1 == idx_list.len() {
                let logits = self.w.head.matvec(&x);
                let mut best_idx = 0usize;
                let mut best = f64::NEG_INFINITY;
                for (i, &v) in logits.iter().enumerate() {
                    if v > best {
                        best = v;
                        best_idx = i;
                    }
                }
                idx_list.push(best_idx);
                if best_idx as i32 == stop {
                    break;
                }
            }
            pos += 1;
        }
        idx_list
    }
}
