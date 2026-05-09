# LEGACY — `rust_runner` (frozen)

**Status:** frozen per **ADR-0001** (`docs/decisions/0001-retire-legacy-fp64-runner.md`).

This is the Phase 1.5 fp64 autoregressive runner. It still builds and
its tests still pass — the contract is "frozen", not "broken" — but
**no new code depends on it.** The Phase 2 ternary-integer engine
(`ternary_vm/`) is the canonical execution layer for every primitive.

## Why frozen, not deleted

- Backward-compatibility verification path for primitives compiled
  through the original Transformer-VM toolchain.
- Reproducibility of the gate-1 / gate-8 historical results.
- Any future Phase 2 audit may want to cross-check the ternary
  engine's output against this runner on the short-primitive subset
  where they agree.

## Migration table

See `src/lib.rs` for the per-primitive mapping to
`psl_ternary_vm::primitives::*`.

## CI guard

`tools/ci/check_legacy_isolation.sh` rejects any new code outside
the `legacy/` subtree that imports from this crate. If you legitimately
need a legacy import in production code, you almost certainly want
the ternary equivalent instead. If you really need the legacy runner,
add a row to `legacy/EXEMPTIONS.md` with justification and a target
date for migration.

## Supported operations

What works (will keep working):
- All 7 primitives that shipped in Phase 1 specialize / `wasm-run`.
- Bit-exact parity vs `wasm-run --python --nohull` on the short
  primitives (byte_add 117 tok, byte_sub 402 tok, mpt_emit 3678 tok).

What's known broken (will stay broken — see ADR-0001):
- Long-primitive parity (freeze_setup ≥ 17.5k tok, freeze_apply ≥
  7.7k tok) drifts vs PyTorch+MKL due to soft-attention fp64
  reduction-order differences.
- We did not port hull-based hard attention; `transformer.cpp`
  default uses it, this runner does not.
