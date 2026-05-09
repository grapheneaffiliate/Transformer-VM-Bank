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
//!
//! ## Layout
//!
//! Keys / values are stored as flat `Vec<f64>` per layer (row-major
//! `[t, d_model]`). One `extend_from_slice` per step replaces the per-step
//! `Array1` heap allocation of the first-pass implementation.
//!
//! Scratch score / weight / output buffers are owned by the cache and grown
//! lazily — no per-step allocation in the hot path.
//!
//! Summation order is identical to the first-pass implementation
//! (which already passes bit-exact parity on byte_add / byte_sub /
//! mpt_emit_record), so this is a pure perf rewrite.

use ndarray::Array1;

pub struct StandardKVCache {
    n_heads: usize,
    d_head: usize,
    d_model: usize,
    keys: Vec<Vec<f64>>, // keys[layer]: row-major [t, d_model]
    vals: Vec<Vec<f64>>, // vals[layer]: row-major [t, d_model]
    sizes: Vec<usize>,   // current t per layer
    // Scratch buffers — reused across steps, grown on demand.
    score_buf: Vec<f64>,  // [t, n_heads] row-major
    weight_buf: Vec<f64>, // [t, n_heads] row-major
    out_buf: Vec<f64>,    // [d_model]
}

impl StandardKVCache {
    pub fn new(n_layers: usize, n_heads: usize, d_model: usize) -> Self {
        assert_eq!(d_model % n_heads, 0, "d_model must be divisible by n_heads");
        Self {
            n_heads,
            d_head: d_model / n_heads,
            d_model,
            keys: (0..n_layers).map(|_| Vec::new()).collect(),
            vals: (0..n_layers).map(|_| Vec::new()).collect(),
            sizes: vec![0; n_layers],
            score_buf: Vec::new(),
            weight_buf: Vec::new(),
            out_buf: vec![0.0; d_model],
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
        let n_heads = self.n_heads;
        let d_head = self.d_head;
        let d_model = self.d_model;

        let k_slice = k.as_slice().expect("k must be contiguous");
        let v_slice = v.as_slice().expect("v must be contiguous");
        let q_slice = q.as_slice().expect("q must be contiguous");

        self.keys[layer].extend_from_slice(k_slice);
        self.vals[layer].extend_from_slice(v_slice);
        self.sizes[layer] += 1;
        let t = self.sizes[layer];
        let keys_flat: &[f64] = &self.keys[layer];
        let vals_flat: &[f64] = &self.vals[layer];

        let needed = t * n_heads;
        if self.score_buf.len() < needed {
            self.score_buf.resize(needed, 0.0);
            self.weight_buf.resize(needed, 0.0);
        }
        let scores = &mut self.score_buf[..needed];
        let weights = &mut self.weight_buf[..needed];

        // scores[t, h] = Σ_i K[t, h*d_head + i] * Q[h*d_head + i]
        // Inner-inner d_head loop fully unrolls when d_head is small (d_head=2
        // for all current primitives — analytical models pin n_heads = d_model/2).
        for time in 0..t {
            let kt = &keys_flat[time * d_model..(time + 1) * d_model];
            let s_row = &mut scores[time * n_heads..(time + 1) * n_heads];
            for h in 0..n_heads {
                let base = h * d_head;
                let mut s = 0.0;
                for i in 0..d_head {
                    s += kt[base + i] * q_slice[base + i];
                }
                s_row[h] = s;
            }
        }

        // softmax over time axis (per head). Subtract max for numerical
        // stability (matches PyTorch's F.softmax behavior). Summation order:
        // increasing time, identical to first-pass implementation.
        for h in 0..n_heads {
            let mut max_score = f64::NEG_INFINITY;
            for time in 0..t {
                let v = scores[time * n_heads + h];
                if v > max_score {
                    max_score = v;
                }
            }
            let mut sum = 0.0;
            for time in 0..t {
                let e = (scores[time * n_heads + h] - max_score).exp();
                weights[time * n_heads + h] = e;
                sum += e;
            }
            // Note: must use `/= sum`, not `*= 1.0/sum` — two roundings vs
            // one breaks bit-exactness with PyTorch's softmax.
            for time in 0..t {
                weights[time * n_heads + h] /= sum;
            }
        }

        // out[base+i] = Σ_t weights[t, h] * V[t, base+i]
        // Restructured to t-outer / (h, i)-inner for cache locality (V is
        // t-major contiguous). Per-output-element summation order over t
        // remains 0,1,2,…, so bit-exact with the first-pass implementation.
        let out = &mut self.out_buf[..d_model];
        for x in out.iter_mut() {
            *x = 0.0;
        }
        for time in 0..t {
            let vt = &vals_flat[time * d_model..(time + 1) * d_model];
            let w_row = &weights[time * n_heads..(time + 1) * n_heads];
            for h in 0..n_heads {
                let w = w_row[h];
                let base = h * d_head;
                for i in 0..d_head {
                    out[base + i] += w * vt[base + i];
                }
            }
        }

        Array1::from_vec(out.to_vec())
    }
}
