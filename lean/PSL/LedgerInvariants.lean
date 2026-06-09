/-
  Additional load-bearing ledger invariants (machine-checked, core Lean only).

  Complements `Conservation.lean`. Where conservation shows transfer/freeze do
  NOT change supply, this file pins down the rest of the supply-accounting and
  authority story that the Rust suite only checks empirically:

    • mint_increases_supply / burn_decreases_supply — mint and burn change total
      supply by EXACTLY the authorized amount (not just "by something").
    • frozen_sender_transfer_noop — a frozen sender's transfer is a no-op
      (freeze-authority enforcement).
    • transfer_success_increments_nonce — a successful transfer strictly
      advances the sender's nonce (replay/ordering monotonicity).

  Together with the conservation theorems this gives a complete picture: supply
  is invariant under transfer/freeze and moves by precisely the minted/burned
  amount under mint/burn.
-/

import PSL.Conservation

namespace PSL

/-- Single-point supply update (additive form, to avoid `Nat` subtraction):
    if `s` and `s'` give equal contributions everywhere on a `Nodup` `live`
    except at `b ∈ live`, the totals differ by exactly the change at `b`. -/
theorem totalSupply_single_update
    (s s' : State) (asset : AssetId) (b : PubKey) (live : List PubKey)
    (hb : b ∈ live) (hnd : live.Nodup)
    (hother : ∀ pk, pk ≠ b →
        (if (s'.accounts pk).assetId = asset then (s'.accounts pk).balance else 0)
      = (if (s.accounts pk).assetId = asset then (s.accounts pk).balance else 0))
    : totalSupply s' asset live
        + (if (s.accounts b).assetId = asset then (s.accounts b).balance else 0)
    = totalSupply s asset live
        + (if (s'.accounts b).assetId = asset then (s'.accounts b).balance else 0) := by
  unfold totalSupply
  rw [foldl_add_acc _ 0, foldl_add_acc _ 0, Nat.zero_add, Nat.zero_add]
  exact foldr_one _ _ b live hb hnd (fun x _ hxb => hother x hxb)

/-! ### Freeze-authority enforcement -/

/-- A frozen sender cannot move funds: the transfer is a no-op, both the state
    and the success flag. -/
theorem frozen_sender_transfer_noop
    (s : State) (tx : TransferTx) (hfrozen : (s.accounts tx.from_).frozen = true) :
    transfer s tx = (s, false) := by
  dsimp only [transfer, State.lookup]
  rw [if_pos (Or.inl hfrozen)]

/-! ### Nonce monotonicity (replay/ordering) -/

/-- A successful transfer strictly advances the sender's nonce by one. -/
theorem transfer_success_increments_nonce
    (s : State) (tx : TransferTx) (hwk : WellKeyed s) (hd : tx.from_ ≠ tx.to)
    (hsucc : (transfer s tx).2 = true) :
    ((transfer s tx).1.accounts tx.from_).nonce = (s.accounts tx.from_).nonce + 1 := by
  by_cases hfail :
      (s.accounts tx.from_).frozen ∨ (s.accounts tx.from_).balance < tx.amount
      ∨ (s.accounts tx.from_).assetId ≠ tx.assetId ∨ (s.accounts tx.to).assetId ≠ tx.assetId
  · exfalso
    have h0 : (transfer s tx).2 = false := by dsimp only [transfer, State.lookup]; rw [if_pos hfail]
    rw [h0] at hsucc; exact Bool.noConfusion hsucc
  · have hacc : (transfer s tx).1.accounts tx.from_ =
        { s.accounts tx.from_ with
          balance := (s.accounts tx.from_).balance - tx.amount,
          nonce := (s.accounts tx.from_).nonce + 1, lastActive := tx.epoch } := by
      dsimp only [transfer, State.lookup]
      rw [if_neg hfail]
      dsimp only [State.update]
      rw [hwk tx.to, hwk tx.from_, if_neg hd, if_pos rfl]
    rw [hacc]

/-! ### Mint / burn change supply by exactly the authorized amount -/

/-- Mint increases the total supply of its asset by exactly `tx.amount`. -/
theorem mint_increases_supply
    (s : State) (tx : MintTx) (live : List PubKey)
    (hwk : WellKeyed s) (hnd : live.Nodup) (hin : tx.to ∈ live)
    (hsucc : (mint s tx).2 = true) :
    totalSupply (mint s tx).1 tx.assetId live = totalSupply s tx.assetId live + tx.amount := by
  by_cases hfail : (s.accounts tx.to).assetId ≠ tx.assetId
  · exfalso
    have h0 : (mint s tx).2 = false := by dsimp only [mint, State.lookup]; rw [if_pos hfail]
    rw [h0] at hsucc; exact Bool.noConfusion hsucc
  · have hCto : (s.accounts tx.to).assetId = tx.assetId := Decidable.not_not.mp hfail
    have hacc : ∀ pk, (mint s tx).1.accounts pk =
        if pk = tx.to then
          { s.accounts tx.to with balance := (s.accounts tx.to).balance + tx.amount, lastActive := tx.epoch }
        else s.accounts pk := by
      intro pk
      dsimp only [mint, State.lookup]
      rw [if_neg hfail]
      dsimp only [State.update]
      rw [hwk tx.to]
    -- contributions agree away from tx.to
    have hother : ∀ pk, pk ≠ tx.to →
        (if ((mint s tx).1.accounts pk).assetId = tx.assetId then ((mint s tx).1.accounts pk).balance else 0)
      = (if (s.accounts pk).assetId = tx.assetId then (s.accounts pk).balance else 0) := by
      intro pk hpk; rw [hacc pk, if_neg hpk]
    have heq := totalSupply_single_update s (mint s tx).1 tx.assetId tx.to live hin hnd hother
    -- contribution at tx.to: before = balance, after = balance + amount
    have c_s : (if (s.accounts tx.to).assetId = tx.assetId then (s.accounts tx.to).balance else 0)
             = (s.accounts tx.to).balance := if_pos hCto
    have c_s' : (if ((mint s tx).1.accounts tx.to).assetId = tx.assetId then ((mint s tx).1.accounts tx.to).balance else 0)
              = (s.accounts tx.to).balance + tx.amount := by
      rw [hacc tx.to, if_pos rfl]; dsimp only []; rw [if_pos hCto]
    rw [c_s, c_s'] at heq
    -- heq : totalSupply (mint).1 + balance = totalSupply s + (balance + amount)
    rw [Nat.add_comm (s.accounts tx.to).balance tx.amount, ← Nat.add_assoc] at heq
    exact Nat.add_right_cancel heq

/-- Burn decreases the total supply of its asset by exactly `tx.amount`
    (stated additively: supply before = supply after + burned amount). -/
theorem burn_decreases_supply
    (s : State) (tx : BurnTx) (live : List PubKey)
    (hwk : WellKeyed s) (hnd : live.Nodup) (hin : tx.from_ ∈ live)
    (hsucc : (burn s tx).2 = true) :
    totalSupply s tx.assetId live = totalSupply (burn s tx).1 tx.assetId live + tx.amount := by
  by_cases hfail : (s.accounts tx.from_).balance < tx.amount ∨ (s.accounts tx.from_).assetId ≠ tx.assetId
  · exfalso
    have h0 : (burn s tx).2 = false := by dsimp only [burn, State.lookup]; rw [if_pos hfail]
    rw [h0] at hsucc; exact Bool.noConfusion hsucc
  · have hge : tx.amount ≤ (s.accounts tx.from_).balance :=
      Nat.not_lt.mp (fun hb => hfail (Or.inl hb))
    have hCfrom : (s.accounts tx.from_).assetId = tx.assetId :=
      Decidable.not_not.mp (fun hc => hfail (Or.inr hc))
    have hacc : ∀ pk, (burn s tx).1.accounts pk =
        if pk = tx.from_ then
          { s.accounts tx.from_ with balance := (s.accounts tx.from_).balance - tx.amount, lastActive := tx.epoch }
        else s.accounts pk := by
      intro pk
      dsimp only [burn, State.lookup]
      rw [if_neg hfail]
      dsimp only [State.update]
      rw [hwk tx.from_]
    have hother : ∀ pk, pk ≠ tx.from_ →
        (if ((burn s tx).1.accounts pk).assetId = tx.assetId then ((burn s tx).1.accounts pk).balance else 0)
      = (if (s.accounts pk).assetId = tx.assetId then (s.accounts pk).balance else 0) := by
      intro pk hpk; rw [hacc pk, if_neg hpk]
    have heq := totalSupply_single_update s (burn s tx).1 tx.assetId tx.from_ live hin hnd hother
    have c_s : (if (s.accounts tx.from_).assetId = tx.assetId then (s.accounts tx.from_).balance else 0)
             = (s.accounts tx.from_).balance := if_pos hCfrom
    have c_s' : (if ((burn s tx).1.accounts tx.from_).assetId = tx.assetId then ((burn s tx).1.accounts tx.from_).balance else 0)
              = (s.accounts tx.from_).balance - tx.amount := by
      rw [hacc tx.from_, if_pos rfl]; dsimp only []; rw [if_pos hCfrom]
    rw [c_s, c_s'] at heq
    -- heq : totalSupply (burn).1 + balance = totalSupply s + (balance - amount)
    -- goal: totalSupply s = totalSupply (burn).1 + amount
    have lhs :
        (totalSupply (burn s tx).1 tx.assetId live + tx.amount)
          + ((s.accounts tx.from_).balance - tx.amount)
        = totalSupply s tx.assetId live + ((s.accounts tx.from_).balance - tx.amount) := by
      rw [Nat.add_assoc, Nat.add_comm tx.amount ((s.accounts tx.from_).balance - tx.amount),
          Nat.sub_add_cancel hge]
      exact heq
    exact (Nat.add_right_cancel lhs).symm

end PSL
