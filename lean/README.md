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
  `supply_changes_only_via_authority`, all **fully proven, no `sorry`**
  (machine-checked, axiom-clean: `propext` + `Quot.sound` only). An audit
  found the originals were unsound/vacuous as stated; they are now proven
  under the genuinely-required hypotheses (`WellKeyed` state invariant,
  `live.Nodup`, distinct in-set endpoints), with no-axiom `decide`
  counterexamples showing each hypothesis is necessary.
- **`PSL/LedgerInvariants.lean`** — completes the supply-accounting story:
  `mint_increases_supply` / `burn_decreases_supply` (supply moves by *exactly*
  the authorized amount), `frozen_sender_transfer_noop` (freeze-authority
  enforcement), `transfer_success_increments_nonce` (replay/ordering). All
  proven, no `sorry`.
- **`PSL/BlockAccounting.lean`** — block-level supply accounting:
  `block_supply_accounting` (over any block, for every asset,
  `before + minted = after + burned` exactly) and the no-authority
  conservation corollary, built on axiom-free `WellKeyed`-preservation
  lemmas so the per-tx theorems chain legitimately across `applyBlock`.
- **`PSL/SMTModel.lean`** — functional model of the Sparse Merkle Tree
  (`rootHash` as a pure function of the key→value map, faithful to
  `crypto/src/smt.rs`). Proves `inclusion_proof_complete` (honest proofs
  verify; needs no collision-resistance), the capstone
  `inclusion_proof_correct` (anything that verifies carries exactly the
  stored value — non-inclusion soundness is the empty-value special case),
  and `smt_root_order_independent` (the commitment depends only on the final
  map). Includes the proven 256-bit key↔path round-trip.
- **`PSL/Audit.lean`** — the in-build axiom-footprint gate (see § Verification
  gate). Fails `lake build` if any load-bearing theorem gains a disallowed
  axiom (`sorry`/`native_decide`/unlisted axiom) or goes missing.
- **`PSL/Determinism.lean`** — trivial determinism (Lean functions are
  deterministic by construction); operational determinism is checked
  by the Rust test suite (`cargo test --workspace --release`) and by
  the cross-platform CI matrix.
- **`PSL/MPT.lean`** — Sparse Merkle Tree soundness as **value binding**
  (a committed `(root, key)` pins a unique value; forging another value
  that verifies breaks collision-resistance), **fully proven, no `sorry`**.
  `verifyProof` mirrors `crypto/src/smt.rs::verify_proof` (recompute the
  root by folding the 256 key-bit-ordered siblings up from `leafHash`).
  Conditioned on the explicit `hash_collision_resistant` and `hash_length`
  axioms. The previous `inclusion_proof_sound` was ill-posed (its
  `value.length ∈ {0,64}` conclusion is not enforced by any verifier) and is
  replaced by the correct binding statement.

## Verification gate (gate 3)

`lake build` succeeds against mathlib v4.12.0 with **zero `sorry` markers**
in the load-bearing theorems (the formal layer is now sorry-free; the only
declared axioms are `hash_collision_resistant` and `hash_length`, the
standard BLAKE3 assumptions). **Gate 3 is ✅**.

See [`docs/STATUS.md`](../docs/STATUS.md) gate 3 row for the
authoritative status, and [`CONTRIBUTING.md`](../CONTRIBUTING.md) for
the no-new-sorrys rule.
