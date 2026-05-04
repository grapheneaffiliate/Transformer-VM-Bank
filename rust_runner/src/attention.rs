//! StandardKVCache port — Phase 1.5.
//!
//! Source: `Transformer-VM/transformer_vm/attention/standard_cache.py:21`.
//!
//! Per-layer softmax KV cache. For each generated position, the cache appends
//! the new (key, value) pair, then computes attention output as:
//!     scores  = Σ_i K[t, h, i] · Q[h, i]            shape [t, n_heads]
//!     weights = softmax(scores, dim=0)
//!     out     = Σ_t weights[t, h] · V[t, h, :]      shape [n_heads, d_head]
//!
//! No 1/sqrt(d_head) scaling — that's the convention in the analytically
//! constructed Transformer-VM models (matching standard_cache.py exactly).
//!
//! HullKVCache is intentionally NOT ported. PSL pins StandardKVCache (see
//! docs/ARCHITECTURE.md § 0.3) because its semantics are deterministic
//! across implementations.

use ndarray::{Array1, Array2};

pub struct StandardKVCache {
    n_heads: usize,
    d_head: usize,
    keys: Vec<Vec<Array1<f64>>>,
    vals: Vec<Vec<Array1<f64>>>,
}

impl StandardKVCache {
    pub fn new(n_layers: usize, n_heads: usize, d_model: usize) -> Self {
        assert_eq!(d_model % n_heads, 0, "d_model must be divisible by n_heads");
        Self {
            n_heads,
            d_head: d_model / n_heads,
            keys: (0..n_layers).map(|_| Vec::new()).collect(),
            vals: (0..n_layers).map(|_| Vec::new()).collect(),
        }
    }

    /// Append (k, v) to the layer's cache and compute attention output.
    /// `q`, `k`, `v` are each shape [d_model] (= n_heads * d_head).
    /// Returns [d_model] output (heads concatenated, matching `.flatten()`).
    pub fn layer_step(
        &mut self,
        layer: usize,
        k: Array1<f64>,
        q: Array1<f64>,
        v: Array1<f64>,
    ) -> Array1<f64> {
        self.keys[layer].push(k);
        self.vals[layer].push(v);
        let t = self.keys[layer].len();
        let n_heads = self.n_heads;
        let d_head = self.d_head;

        // scores [t, n_heads]: scores[time, h] = Σ_i K[time, h, i] · Q[h, i]
        let mut scores = Array2::<f64>::zeros((t, n_heads));
        for time in 0..t {
            let kt = &self.keys[layer][time];
            for h in 0..n_heads {
                let mut s = 0.0;
                let base = h * d_head;
                for i in 0..d_head {
                    s += kt[base + i] * q[base + i];
                }
                scores[[time, h]] = s;
            }
        }

        // softmax over time axis (per head). Subtract max for numerical
        // stability (matches PyTorch's F.softmax behavior).
        let mut weights = Array2::<f64>::zeros((t, n_heads));
        for h in 0..n_heads {
            let mut max_score = f64::NEG_INFINITY;
            for time in 0..t {
                if scores[[time, h]] > max_score {
                    max_score = scores[[time, h]];
                }
            }
            let mut sum = 0.0;
            for time in 0..t {
                let e = (scores[[time, h]] - max_score).exp();
                weights[[time, h]] = e;
                sum += e;
            }
            for time in 0..t {
                weights[[time, h]] /= sum;
            }
        }

        // out [n_heads, d_head] flattened to [d_model]
        let mut out = Array1::<f64>::zeros(n_heads * d_head);
        for h in 0..n_heads {
            let base = h * d_head;
            for i in 0..d_head {
                let mut acc = 0.0;
                for time in 0..t {
                    acc += weights[[time, h]] * self.vals[layer][time][base + i];
                }
                out[base + i] = acc;
            }
        }
        out
    }
}
