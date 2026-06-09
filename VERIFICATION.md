# PSL Verification Ledger

**What is mathematically proven, what it rests on, and how that's enforced.**

PSL's value proposition is *verifiable trust*. This file is the honest,
single-source map of the formal layer: every load-bearing theorem, the exact
property it establishes, and the complete set of axioms it depends on. It is
**machine-checked** — `lean/PSL/Audit.lean` re-derives the axiom footprint of
each theorem below at build time and fails the build if it drifts, and the
`formal-verification` CI job runs that build on every PR. So this table cannot
silently go stale: if it disagrees with the code, CI is red.

> Scope note. This ledger covers the **Lean 4 formal layer** (`lean/PSL/`),
> which models the ledger semantics and the Sparse Merkle Tree. Properties
> verified *empirically* (bit-exact primitive re-execution, cross-platform
> determinism, compliance enforcement) are tracked in `docs/STATUS.md` gates,
> not here. The honest boundary between "proved" and "tested" is the point.

## Trust boundary: the allowed axioms

Every load-bearing theorem rests on a subset of exactly these five axioms —
nothing else (no `sorry`, no `native_decide`):

| Axiom | Kind | Why it's trusted |
| --- | --- | --- |
| `propext` | Lean foundation | Propositional extensionality — one of Lean's three standard axioms. |
| `Quot.sound` | Lean foundation | Quotient soundness — standard Lean axiom. |
| `Classical.choice` | Lean foundation | Classical choice — standard Lean axiom (used by `by_cases`/mathlib). |
| `PSL.MPT.hash_collision_resistant` | Crypto assumption | BLAKE3 is collision-resistant. Explicit, declared in `MPT.lean`. |
| `PSL.MPT.hash_length` | Crypto assumption | BLAKE3-256 emits a fixed 32-byte digest. Explicit, declared in `MPT.lean`. |

The first three are the standard trusted base of essentially all Lean/mathlib
developments. The last two are the *only* domain assumptions, and they are the
standard, expected form for a hash-based proof. **There is no `sorryAx` and no
`Lean.ofReduceBool` (`native_decide`) anywhere in the load-bearing set.**

## The theorems

| Property (what a regulator / integrator can rely on) | Theorem | File | Axioms |
| --- | --- | --- | --- |
| A transfer never changes the total supply of **any** asset (given a well-keyed state, a duplicate-free working set, and distinct endpoints both in that set). | `transfer_conserves` | `PSL/Conservation.lean` | `propext`, `Quot.sound` |
| A freeze never changes any balance (given a well-keyed state). | `freeze_conserves` | `PSL/Conservation.lean` | `Quot.sound` |
| The total supply of an asset can change **only** via mint or burn — a transfer or freeze that changes supply is impossible. | `supply_changes_only_via_authority` | `PSL/Conservation.lean` | `propext`, `Quot.sound` |
| A mint increases the asset's total supply by **exactly** the minted amount. | `mint_increases_supply` | `PSL/LedgerInvariants.lean` | `propext`, `Quot.sound` |
| A burn decreases the asset's total supply by **exactly** the burned amount. | `burn_decreases_supply` | `PSL/LedgerInvariants.lean` | `propext`, `Quot.sound` |
| A frozen sender cannot move funds — its transfer is a no-op (state and success flag unchanged). Freeze-authority enforcement. | `frozen_sender_transfer_noop` | `PSL/LedgerInvariants.lean` | none |
| A successful transfer strictly advances the sender's nonce by one (replay/ordering monotonicity). | `transfer_success_increments_nonce` | `PSL/LedgerInvariants.lean` | none |
| **Value binding:** for a committed `(root, key)`, no two proofs can verify with different values. A forged alternative value that still verifies would break collision-resistance. (Phone-side balance-proof soundness.) | `inclusion_proof_sound` | `PSL/MPT.lean` | `propext`, `Classical.choice`, `Quot.sound`, `hash_collision_resistant`, `hash_length` |

Together these give a complete supply-accounting picture: total supply is
**invariant** under transfer and freeze, and moves by **precisely** the
authorized amount under mint and burn.

### Why these statements, and not stronger-sounding ones

The formal layer states what is *actually true of the model*, with the
hypotheses that are genuinely required — and proves each hypothesis is
necessary with a counterexample, rather than quietly assuming it away:

- **`WellKeyed`** (`∀ pk, (s.accounts pk).pubkey = pk`) — the model writes an
  account at index `a.pubkey`, so a mis-keyed state lets `freeze` clobber a
  different slot. Counterexample: `freeze_not_conserves_without_wellkeyed`
  (no axioms — pure `decide`).
- **`live.Nodup`** — a duplicated key double-counts the moved delta.
  Counterexample: `transfer_not_conserves_without_nodup` (no axioms).
- The previous `supply_changes_only_via_authority` was **vacuous** (its
  conclusion held for the constant `"mint"`, ignoring its hypothesis);
  `original_authority_conclusion_is_vacuous` demonstrates that defect, and the
  current statement is the honest one.
- The previous MPT `inclusion_proof_sound` was **ill-posed** (its conclusion
  about `value.length` is enforced by no verifier); it is replaced by value
  binding, the property the SMT actually guarantees and that the
  `tampered_value_fails_proof` crypto test exercises.

## Modeling assumptions and known gaps (honest list)

- **`hash` is `opaque`** in Lean — the proofs treat BLAKE3 as a collision-
  resistant fixed-length function (the two axioms above), not by modeling the
  compression function. This is standard; closing it would require a verified
  BLAKE3, which mathlib does not provide.
- **Lean ↔ Rust/C correspondence** is a *hand-translation contract*, not a
  proof: `lean/PSL/Ledger.lean` mirrors `primitives/*.c` and `crypto/src/smt.rs`
  by hand. `lean/PSL/MPT.lean::verifyProof` mirrors `SparseMerkleTree::
  verify_proof`. Divergence is guarded empirically (gate 1 bit-exact vectors)
  and by `tools/check_lean_drift.py` (not yet wired into CI — see STATUS notes).
- **Not yet formalized** (tested in Rust only): SMT root determinism /
  order-independence (the Lean MPT model has `verifyProof` but not the tree
  `put`), and compliance (travel-rule) invariants. These are candidates for
  future proof work; until then they are empirical, not formal, guarantees.
  (Freeze-authority enforcement and nonce/replay monotonicity are now
  formalized — see the table above.)

## Reproduce locally

```bash
cd lean
elan toolchain install $(cat lean-toolchain)   # one-time
lake exe cache get                             # prebuilt mathlib (~1-2 min)
lake build                                     # builds proofs + runs the audit gate
```

A passing build prints `✓ formal audit passed: 8 load-bearing theorems rest
only on the 5 allowed axioms`. To see the footprint yourself:

```bash
echo 'import PSL
open PSL PSL.MPT
#print axioms transfer_conserves
#print axioms freeze_conserves
#print axioms supply_changes_only_via_authority
#print axioms mint_increases_supply
#print axioms burn_decreases_supply
#print axioms frozen_sender_transfer_noop
#print axioms transfer_success_increments_nonce
#print axioms inclusion_proof_sound' > /tmp/Ax.lean
lake env lean /tmp/Ax.lean
```
