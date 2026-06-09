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
> which models the ledger semantics, the Sparse Merkle Tree, and the mempool
> compliance policy. Properties verified *empirically* (bit-exact primitive
> re-execution, cross-platform determinism, ed25519 signature correctness) are
> tracked in `docs/STATUS.md` gates, not here. The honest boundary between
> "proved" and "tested" is the point.

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
| **Block-level supply accounting:** over ANY block, for every asset, `supply_before + total_minted = supply_after + total_burned`, where the totals sum exactly the successful mint/burn amounts. Supply moves only by authorized amounts no matter how transactions are interleaved. (Relies on `wellKeyed_applyTx`/`wellKeyed_applyBlock`: the `WellKeyed` invariant is preserved by every operation — proven with no axioms — so per-tx theorems legitimately chain.) | `block_supply_accounting` | `PSL/BlockAccounting.lean` | `propext`, `Quot.sound` |
| A block containing no mint or burn transactions cannot change the supply of any asset. | `block_without_authority_conserves` | `PSL/BlockAccounting.lean` | `propext`, `Quot.sound` |
| **Value binding:** for a committed `(root, key)`, no two proofs can verify with different values. A forged alternative value that still verifies would break collision-resistance. (Phone-side balance-proof soundness.) | `inclusion_proof_sound` | `PSL/MPT.lean` | `propext`, `Classical.choice`, `Quot.sound`, `hash_collision_resistant`, `hash_length` |
| **Completeness:** the honestly-generated proof for any key verifies against the model root. Purely structural — needs **no collision-resistance**. | `inclusion_proof_complete` | `PSL/SMTModel.lean` | `propext`, `Quot.sound`, `hash_length` |
| **Correctness (capstone):** *any* proof that verifies against a model root carries exactly the stored value `m key`. Soundness + completeness combined: the committed root pins down precisely the map's value at every key. With an absent key (`m key = []`) this **is** non-inclusion soundness. | `inclusion_proof_correct` | `PSL/SMTModel.lean` | `propext`, `Classical.choice`, `Quot.sound`, `hash_collision_resistant`, `hash_length` |
| The state commitment depends only on the final key→value map — writing two distinct keys in either order yields the same root (spec-level form of the Rust `put_order_independent_for_independent_keys` test). | `smt_root_order_independent` | `PSL/SMTModel.lean` | `propext`, `Quot.sound` |
| **Compliance admission policy** (`PSL/Compliance.lean`, modeling `sequencer/src/mempool.rs::validate`; signature verification abstracted as an opaque proposition). Nine theorems, **all axiom-free**: a high-value transfer without travel-rule metadata is rejected; a freeze needs both issuer authority and a court order; mint/burn need the (capability-enabled) issuer authority; a frozen sender's transfer/burn is rejected; a wrong-nonce transfer/burn is rejected; an invalid signature is rejected; and a fully-compliant transfer is admitted. | `travel_rule_high_value_rejected`, `freeze_non_authority_rejected`, `freeze_without_court_order_rejected`, `mint_non_authority_rejected`, `burn_non_authority_rejected`, `frozen_sender_rejected`, `nonce_mismatch_rejected`, `invalid_signature_rejected`, `compliant_transfer_admitted` | `PSL/Compliance.lean` | none |

Together these give a complete supply-accounting picture, per transaction
**and per block**: total supply is **invariant** under transfer and freeze,
moves by **precisely** the authorized amount under mint and burn, and over an
entire block the books balance exactly (`before + minted = after + burned`).
And the Merkle layer is closed in both directions: honest proofs verify
(completeness), and anything that verifies is the truth
(soundness/correctness).

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
  proof: `lean/PSL/Ledger.lean` mirrors the executable semantics in
  `sequencer/src/trace.rs` (composed from the `primitives/*.c` micro-ops);
  `lean/PSL/MPT.lean::verifyProof` and `lean/PSL/SMTModel.lean` mirror
  `crypto/src/smt.rs`. Divergence is guarded empirically (gate 1 bit-exact
  vectors) and by `tools/check_lean_drift.py`, which hashes every watched
  implementation source against a pinned manifest and **runs in CI** (the
  `formal-verification` job fails if a watched source changes without the
  manifest being re-reviewed).
- **Correspondence-audit findings (2026-06, recorded honestly rather than
  papered over):**
  1. The previous drift checker watched `ledger_*.c` files that never existed
     in this tree — it had **never run successfully**, so the hand-translation
     contract was unenforced until now. The watch list is fixed and pinned.
  2. `Account.assetId` is **model-only**: the Rust 64-byte account record has
     no asset_id field (asset_id exists only on transactions). The Lean
     assetId guards model the intended per-asset partitioning; the
     implementation enforces it at a different layer.
  3. Rust's transfer credit path uses u128 `wrapping_add` while Lean balances
     are ℕ — the conservation theorems **do not cover a u128 wraparound** of
     a recipient balance. Mint uses `checked_add` (fails safe). Whether wrap
     is reachable depends on issuer mint policy; treating this as out of
     scope of the formal claim is a documented decision, not an oversight.
- **The SMT model is functional, not imperative.** `PSL/SMTModel.lean` models
  the tree as a pure function of the key→value map (`rootHash`), faithful to
  `crypto/src/smt.rs`'s hashing scheme (leaf/internal/default-subtree rules,
  MSB-first key bits). Completeness, correctness, and order-independence are
  proven against this functional spec. The *imperative* node-store `put` in
  Rust agreeing with the functional spec remains an empirically-tested
  property (the 100k randomized-put determinism test), per the
  hand-translation contract.
- **Compliance: policy proven, signature abstracted.** `PSL/Compliance.lean`
  models the `mempool.rs::validate` admission policy and proves all nine
  regulator-facing gate properties (axiom-free). Signature *verification*
  itself is abstracted as an opaque proposition `SigValid` (the same treatment
  `hash` gets) — ed25519 correctness is a tested, not formalized, property.
- **Essentially the whole load-bearing surface is now formalized.** Supply
  conservation/accounting (per-tx and per-block), mint/burn exactness,
  freeze-authority, nonce/replay monotonicity, Merkle
  soundness/completeness/correctness, root order-independence, and the
  compliance admission policy all have machine-checked theorems above. The
  remaining honest gaps are the *correspondence* items: the hand-translation
  contract (guarded by the CI drift check), the opaque-`hash`/`SigValid`
  abstractions, the functional-vs-imperative SMT `put`, and the documented
  ℕ-vs-u128 wraparound scoping.

## Reproduce locally

```bash
cd lean
elan toolchain install $(cat lean-toolchain)   # one-time
lake exe cache get                             # prebuilt mathlib (~1-2 min)
lake build                                     # builds proofs + runs the audit gate
```

A passing build prints `✓ formal audit passed: 22 load-bearing theorems rest
only on the 5 allowed axioms`. To see the footprint yourself:

```bash
echo 'import PSL
open PSL PSL.MPT PSL.Compliance
#print axioms transfer_conserves
#print axioms supply_changes_only_via_authority
#print axioms mint_increases_supply
#print axioms block_supply_accounting
#print axioms inclusion_proof_sound
#print axioms inclusion_proof_correct
#print axioms smt_root_order_independent
#print axioms travel_rule_high_value_rejected
#print axioms freeze_without_court_order_rejected
#print axioms compliant_transfer_admitted' > /tmp/Ax.lean
lake env lean /tmp/Ax.lean
```
