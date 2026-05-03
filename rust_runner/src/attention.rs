//! KV cache (Phase 1.5).
//!
//! Source: `Transformer-VM/transformer_vm/attention/standard_cache.{cpp,py}`.
//!
//! StandardKVCache is brute-force O(n) attention: every generated token
//! recomputes attention scores against all prior tokens. This is the
//! production cache for PSL because its outputs are pure-integer-arithmetic
//! deterministic, and its semantics match the Python tests for the same
//! cache class.
//!
//! HullKVCache is intentionally NOT ported — its convex-hull math has
//! float-ish internals that could drift across implementations. PSL pins
//! StandardKVCache (see docs/ARCHITECTURE.md § 0.3).
//!
//! TODO(phase-1.5):
//!   - StandardKVCache::layer_step.
//!   - Multi-layer cache management.
//!   - Bit-exact-verify against the C++ engine's StandardKVCache.

use anyhow::Result;

pub struct StandardKVCache {
    pub n_layers: usize,
    // TODO
}

impl StandardKVCache {
    pub fn new(n_layers: usize) -> Self {
        Self { n_layers }
    }

    pub fn layer_step(
        &mut self,
        _layer: usize,
        _keys: &[f64],
        _queries: &[f64],
        _values: &[f64],
    ) -> Result<Vec<f64>> {
        anyhow::bail!("StandardKVCache::layer_step not yet implemented")
    }
}
