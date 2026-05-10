# PSL Lean proofs

Formal model of the PSL ledger semantics. Lean 4 + mathlib.

## Build

```bash
# One-time: install elan, then:
elan toolchain install $(cat lean-toolchain)
lake update
lake build
```

`lake build` compiles against mathlib v4.12.0 and finishes in ~15 min
cold (mathlib cache fetch dominates) or ~30 s warm. See
[`docs/REPRODUCIBILITY_REPORT.md`](../docs/REPRODUCIBILITY_REPORT.md)
for the per-gate timing reference.

## Files

- **`PSL/Account.lean`** — account record model.
- **`PSL/Ledger.lean`** — pure functional model of `transfer`, `mint`, `burn`,
  `freeze`, `applyBlock`. Hand-translated from `primitives/*.c`. The
  translation gap is the only place a Lean–C divergence can sneak in.
- **`PSL/Conservation.lean`** — `transfer_conserves`, `freeze_conserves`,
  `supply_changes_only_via_authority`. **2 of the 3 outstanding `sorry`
  markers live here** (lines 42 and 60). Target close dates per
  [`docs/STATUS.md`](../docs/STATUS.md): 2026-06-15 and 2026-07-15.
- **`PSL/Determinism.lean`** — trivial determinism (Lean functions are
  deterministic by construction); operational determinism is checked
  by the Rust test suite (`cargo test --workspace --release`) and by
  the cross-platform CI matrix.
- **`PSL/MPT.lean`** — Sparse Merkle Tree soundness, conditioned on
  hash collision-resistance. **The 3rd outstanding `sorry` is here**
  (line 58). Target close date: 2026-07-15.

## Verification gate (gate 3)

`lake build` succeeds against mathlib v4.12.0; 3 documented `sorry`
markers remain with target close dates. **Gate 3 is ✅** per the
operating principle: existing sorrys with tracked close dates are
acceptable; *new* sorrys in load-bearing theorems are not.

See [`docs/STATUS.md`](../docs/STATUS.md) gate 3 row for the
authoritative status, and [`CONTRIBUTING.md`](../CONTRIBUTING.md) for
the no-new-sorrys rule.
