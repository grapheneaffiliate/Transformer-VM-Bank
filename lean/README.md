# PSL Lean proofs

Formal model of the PSL ledger semantics. Lean 4 + mathlib.

## Build

```bash
# One-time: install elan, then:
elan toolchain install $(cat lean-toolchain)
lake update
lake build
```

## Files

- **`PSL/Account.lean`** — account record model.
- **`PSL/Ledger.lean`** — pure functional model of `transfer`, `mint`, `burn`,
  `freeze`, `applyBlock`. Hand-translated from `primitives/*.c`. The
  translation gap is the only place a Lean–C divergence can sneak in;
  `tools/check_lean_drift.py` (TBD) will hash the C primitives and refuse to
  build if the hashes differ from the last-known-translated values.
- **`PSL/Conservation.lean`** — `transfer_conserves`, `freeze_conserves`,
  `supply_changes_only_via_authority`. Theorems are stated; full proofs are
  TODO (currently `sorry`-marked) — these are the most important proofs to
  finish before any production deployment.
- **`PSL/Determinism.lean`** — trivial determinism (Lean functions are
  deterministic by construction); the bit-exact gate (`tests/test_bit_exact.py`)
  is the operational determinism check between Lean and C.
- **`PSL/MPT.lean`** — Sparse Merkle Tree soundness, conditioned on hash
  collision-resistance.

## Verification gate

Gate 3 in `docs/ARCHITECTURE.md`: `lake build` succeeds with zero `sorry` in
the conservation, supply, and determinism theorems. Gate currently NOT met —
several proofs are skeletal. See per-file TODO comments.
