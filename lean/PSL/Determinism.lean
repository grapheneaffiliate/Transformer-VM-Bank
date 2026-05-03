/-
  Determinism: applyBlock is a function, so it is by definition deterministic.

  This file states the property explicitly and proves the trivial form.
  The non-trivial determinism we care about is between Lean and the C/WASM
  primitives, which is established empirically by the bit-exact test gate
  (`tests/test_bit_exact.py`).
-/

import PSL.Ledger

namespace PSL

theorem applyBlock_deterministic
  (s : State) (txs : List Tx) :
    applyBlock s txs = applyBlock s txs :=
  rfl

theorem applyTx_deterministic
  (s : State) (tx : Tx) :
    applyTx s tx = applyTx s tx :=
  rfl

/-- Two states equal pointwise iff their `accounts` functions agree everywhere.
    The Lean model defines `State` as a function from `PubKey` to `Account`;
    two states with the same function are definitionally equal in extensional
    settings (or via `funext`). -/
theorem state_extensional
  (s₁ s₂ : State)
  (h : ∀ pk, s₁.accounts pk = s₂.accounts pk)
  : s₁.accounts = s₂.accounts := by
  funext pk
  exact h pk

end PSL
