/-
  Formal-verification integrity gate (machine-enforced, runs inside `lake build`).

  This module pins the *trust boundary* of the PSL formal layer: it asserts, at
  compile time, that every load-bearing theorem depends on nothing beyond an
  explicit allowlist of axioms. If a future change:

    • reintroduces a `sorry`            → adds `sorryAx`            → build fails
    • uses `native_decide`              → adds `Lean.ofReduceBool`  → build fails
    • adds a new `axiom`                → new name not on allowlist → build fails
    • deletes/renames a guarded theorem → `env.contains` fails      → build fails

  So the guarantees in `Conservation.lean` / `MPT.lean` cannot silently rot.
  The allowlist is Lean's three standard foundational axioms plus the two
  explicit BLAKE3 cryptographic assumptions — the entire honest trust boundary,
  in one place, checked on every build. See `VERIFICATION.md` for the human map.
-/

import Lean
import PSL.Conservation
import PSL.LedgerInvariants
import PSL.MPT
import PSL.SMTModel

open Lean Elab Command

namespace PSL.Audit

/-- The only axioms the load-bearing layer is permitted to rest on:
    Lean's standard foundations (`propext`, `Classical.choice`, `Quot.sound`)
    and the two declared BLAKE3 assumptions. Anything else — `sorryAx`,
    `Lean.ofReduceBool` (`native_decide`), or any newly-introduced `axiom` —
    is a build-breaking trust regression. -/
def allowedAxioms : List Name :=
  [``propext, ``Classical.choice, ``Quot.sound,
   ``PSL.MPT.hash_collision_resistant, ``PSL.MPT.hash_length]

/-- The theorems whose trust boundary we continuously enforce. Keep this in
    sync with `VERIFICATION.md`. -/
def loadBearing : List Name :=
  [``PSL.transfer_conserves,
   ``PSL.freeze_conserves,
   ``PSL.supply_changes_only_via_authority,
   ``PSL.mint_increases_supply,
   ``PSL.burn_decreases_supply,
   ``PSL.frozen_sender_transfer_noop,
   ``PSL.transfer_success_increments_nonce,
   ``PSL.MPT.inclusion_proof_sound,
   ``PSL.MPT.inclusion_proof_complete,
   ``PSL.MPT.inclusion_proof_correct,
   ``PSL.MPT.smt_root_order_independent]

/-- Axioms a constant transitively depends on. -/
def axiomsOf (env : Environment) (n : Name) : List Name :=
  (((Lean.CollectAxioms.collect n).run env).run {}).2.axioms.toList

run_cmd do
  let env ← getEnv
  let mut violations : Array MessageData := #[]
  for thm in loadBearing do
    unless env.contains thm do
      throwError "formal audit: guarded theorem `{thm}` is missing (renamed or deleted?)"
    for ax in axiomsOf env thm do
      unless allowedAxioms.contains ax do
        violations := violations.push m!"  • {thm}  depends on forbidden axiom  {ax}"
  unless violations.isEmpty do
    throwError m!"FORMAL AUDIT FAILED — load-bearing theorems gained disallowed axioms:\n{MessageData.joinSep violations.toList "\n"}\n\nAllowed axioms: {allowedAxioms}\nA `sorry` shows up as `sorryAx`; `native_decide` as `Lean.ofReduceBool`."
  logInfo m!"✓ formal audit passed: {loadBearing.length} load-bearing theorems rest only on the {allowedAxioms.length} allowed axioms"

end PSL.Audit
