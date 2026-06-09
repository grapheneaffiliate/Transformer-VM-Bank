/-
  Lean model of the PSL ledger semantics.

  Each function mirrors the executable transaction semantics in
  `sequencer/src/trace.rs` (`NativeTraceExecutor`), which is itself composed
  from the `primitives/*.c` micro-ops (transfer_check = the balance≥amount
  compare, transfer_finalize = the nonce increment, freeze_setup/apply, …):
    - transfer       ↔ trace.rs::transfer  (frozen/balance guards, debit,
                       credit, nonce+1, lastActive)
    - mint           ↔ trace.rs::mint
    - burn           ↔ trace.rs::burn
    - freeze         ↔ trace.rs::freeze
    - applyBlock     ↔ block-level composition in sequencer/src/node.rs

  Known model ↔ implementation deltas (found in the 2026-06 correspondence
  audit; full discussion in VERIFICATION.md):
    • `Account.assetId` is MODEL-ONLY: the Rust 64-byte account record
      (crypto/src/account.rs) has no asset_id field — asset_id exists only on
      transactions. The Lean assetId guards model the intended per-asset
      partitioning; the implementation enforces it at a different layer.
    • Balances here are ℕ; Rust uses u128 with `wrapping_add` on the transfer
      credit path and `checked_add` on mint. The conservation theorems
      therefore do not cover a u128 wraparound of a recipient balance.

  These are pure, side-effect-free functions; Lean proves invariants over
  them directly. The hand-translation contract: when a watched source
  changes, this file MUST be updated and the corresponding theorems re-proven.
  Drift detection: `tools/check_lean_drift.py` (runs in CI).

  Historical note: this header previously claimed correspondence to
  `ledger_transfer.c` / `ledger_mint.c` / `ledger_burn.c` / `ledger_freeze.c`
  — files that never existed in this tree.
-/

import PSL.Account

namespace PSL

structure State where
  accounts : PubKey → Account
  -- The state could carry the issuer registry, an epoch counter, etc.
  -- For the conservation theorem we only need the account map.

namespace State

def lookup (s : State) (pk : PubKey) : Account := s.accounts pk

def update (s : State) (a : Account) : State :=
  { s with accounts := fun pk => if pk = a.pubkey then a else s.accounts pk }

end State

/-! ### transfer -/

structure TransferTx where
  from_     : PubKey
  to        : PubKey
  amount    : Balance
  assetId   : AssetId
  epoch     : Epoch

/-- Transfer's effect on state. Returns the new state and a success flag.
    On failure (frozen sender, insufficient balance, or asset_id mismatch),
    the state is unchanged. -/
def transfer (s : State) (tx : TransferTx) : State × Bool :=
  let from_ := s.lookup tx.from_
  let to    := s.lookup tx.to
  if from_.frozen
     ∨ from_.balance < tx.amount
     ∨ from_.assetId ≠ tx.assetId
     ∨ to.assetId ≠ tx.assetId then
    (s, false)
  else
    let from' := { from_ with
                   balance    := from_.balance - tx.amount
                   nonce      := from_.nonce + 1
                   lastActive := tx.epoch }
    let to'   := { to with
                   balance    := to.balance + tx.amount
                   lastActive := tx.epoch }
    (s.update from' |>.update to', true)

/-! ### mint -/

structure MintTx where
  to       : PubKey
  amount   : Balance
  assetId  : AssetId
  epoch    : Epoch

def mint (s : State) (tx : MintTx) : State × Bool :=
  let to := s.lookup tx.to
  if to.assetId ≠ tx.assetId then (s, false)
  else
    let to' := { to with balance := to.balance + tx.amount, lastActive := tx.epoch }
    (s.update to', true)

/-! ### burn -/

structure BurnTx where
  from_    : PubKey
  amount   : Balance
  assetId  : AssetId
  epoch    : Epoch

def burn (s : State) (tx : BurnTx) : State × Bool :=
  let from_ := s.lookup tx.from_
  if from_.balance < tx.amount ∨ from_.assetId ≠ tx.assetId then (s, false)
  else
    let from' := { from_ with balance := from_.balance - tx.amount, lastActive := tx.epoch }
    (s.update from', true)

/-! ### freeze -/

structure FreezeTx where
  account  : PubKey
  flag     : Bool

def freeze (s : State) (tx : FreezeTx) : State × Bool :=
  let a := s.lookup tx.account
  let a' := { a with frozen := tx.flag }
  (s.update a', true)

/-! ### Block-level composition -/

inductive Tx
  | transfer (t : TransferTx)
  | mint     (t : MintTx)
  | burn     (t : BurnTx)
  | freeze   (t : FreezeTx)

def applyTx (s : State) : Tx → State × Bool
  | .transfer t => transfer s t
  | .mint     t => mint     s t
  | .burn     t => burn     s t
  | .freeze   t => freeze   s t

def applyBlock (s : State) : List Tx → State
  | [] => s
  | t :: rest => applyBlock (applyTx s t).1 rest

end PSL
