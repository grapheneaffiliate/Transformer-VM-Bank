/-
  Block-level supply accounting (machine-checked, core Lean only).

  The per-transaction theorems (Conservation, LedgerInvariants) say what each
  operation does to supply in isolation. This file composes them across an
  entire block:

    • `wellKeyed_applyTx` / `wellKeyed_applyBlock` — the `WellKeyed` invariant
      is preserved by every operation, so the per-tx theorems can legitimately
      be chained. (Without preservation the invariant would hold only at the
      first transaction — a real gap in the chained story.)
    • `block_supply_accounting` — over ANY block, for every asset:
        supply_before + total_minted = supply_after + total_burned
      where minted/burned sum exactly the amounts of the *successful* mint and
      burn transactions for that asset. Additive form avoids `Nat` truncation.
    • `block_without_authority_conserves` — corollary: a block containing no
      mint or burn transactions cannot change the supply of any asset.

  This is the regulator-facing statement at the block level: the ledger's
  total supply moves only by the exact authorized mint/burn amounts, no matter
  how transfers and freezes are interleaved.
-/

import PSL.LedgerInvariants

namespace PSL

/-! ### WellKeyed is preserved by every operation -/

theorem wellKeyed_update (s : State) (a : Account) (hwk : WellKeyed s) :
    WellKeyed (s.update a) := by
  intro pk
  show (if pk = a.pubkey then a else s.accounts pk).pubkey = pk
  by_cases h : pk = a.pubkey
  · rw [if_pos h]; exact h.symm
  · rw [if_neg h]; exact hwk pk

theorem wellKeyed_applyTx (s : State) (tx : Tx) (hwk : WellKeyed s) :
    WellKeyed (applyTx s tx).1 := by
  cases tx with
  | transfer t =>
    show WellKeyed (transfer s t).1
    by_cases hfail : (s.accounts t.from_).frozen ∨ (s.accounts t.from_).balance < t.amount
        ∨ (s.accounts t.from_).assetId ≠ t.assetId ∨ (s.accounts t.to).assetId ≠ t.assetId
    · have hs : (transfer s t).1 = s := by
        dsimp only [transfer, State.lookup]; rw [if_pos hfail]
      rw [hs]; exact hwk
    · dsimp only [transfer, State.lookup]
      rw [if_neg hfail]
      exact wellKeyed_update _ _ (wellKeyed_update _ _ hwk)
  | mint t =>
    show WellKeyed (mint s t).1
    by_cases hfail : (s.accounts t.to).assetId ≠ t.assetId
    · have hs : (mint s t).1 = s := by
        dsimp only [mint, State.lookup]; rw [if_pos hfail]
      rw [hs]; exact hwk
    · dsimp only [mint, State.lookup]
      rw [if_neg hfail]
      exact wellKeyed_update _ _ hwk
  | burn t =>
    show WellKeyed (burn s t).1
    by_cases hfail : (s.accounts t.from_).balance < t.amount
        ∨ (s.accounts t.from_).assetId ≠ t.assetId
    · have hs : (burn s t).1 = s := by
        dsimp only [burn, State.lookup]; rw [if_pos hfail]
      rw [hs]; exact hwk
    · dsimp only [burn, State.lookup]
      rw [if_neg hfail]
      exact wellKeyed_update _ _ hwk
  | freeze t =>
    show WellKeyed (freeze s t).1
    exact wellKeyed_update _ _ hwk

theorem wellKeyed_applyBlock (s : State) (txs : List Tx) (hwk : WellKeyed s) :
    WellKeyed (applyBlock s txs) := by
  induction txs generalizing s with
  | nil => exact hwk
  | cons tx _ ih => exact ih (applyTx s tx).1 (wellKeyed_applyTx s tx hwk)

/-! ### Failure no-ops and other-asset conservation for mint/burn -/

theorem mint_fail_state (s : State) (tx : MintTx) (h : (mint s tx).2 = false) :
    (mint s tx).1 = s := by
  by_cases hc : (s.accounts tx.to).assetId ≠ tx.assetId
  · dsimp only [mint, State.lookup]; rw [if_pos hc]
  · exfalso
    have hT : (mint s tx).2 = true := by
      dsimp only [mint, State.lookup]; rw [if_neg hc]
    exact Bool.noConfusion (hT.symm.trans h)

theorem burn_fail_state (s : State) (tx : BurnTx) (h : (burn s tx).2 = false) :
    (burn s tx).1 = s := by
  by_cases hc : (s.accounts tx.from_).balance < tx.amount
      ∨ (s.accounts tx.from_).assetId ≠ tx.assetId
  · dsimp only [burn, State.lookup]; rw [if_pos hc]
  · exfalso
    have hT : (burn s tx).2 = true := by
      dsimp only [burn, State.lookup]; rw [if_neg hc]
    exact Bool.noConfusion (hT.symm.trans h)

/-- Mint cannot change the supply of any asset other than its own. -/
theorem mint_conserves_other (s : State) (tx : MintTx) (asset : AssetId)
    (live : List PubKey) (hwk : WellKeyed s) (ha : tx.assetId ≠ asset) :
    totalSupply s asset live = totalSupply (mint s tx).1 asset live := by
  by_cases hc : (s.accounts tx.to).assetId ≠ tx.assetId
  · have hs : (mint s tx).1 = s := by
      dsimp only [mint, State.lookup]; rw [if_pos hc]
    rw [hs]
  · have hCto : (s.accounts tx.to).assetId = tx.assetId := Decidable.not_not.mp hc
    have hacc : ∀ pk, (mint s tx).1.accounts pk =
        if pk = tx.to then
          { s.accounts tx.to with balance := (s.accounts tx.to).balance + tx.amount, lastActive := tx.epoch }
        else s.accounts pk := by
      intro pk
      dsimp only [mint, State.lookup]
      rw [if_neg hc]
      dsimp only [State.update]
      rw [hwk tx.to]
    apply totalSupply_congr
    intro pk
    show (if (s.accounts pk).assetId = asset then (s.accounts pk).balance else 0)
       = (if ((mint s tx).1.accounts pk).assetId = asset then ((mint s tx).1.accounts pk).balance else 0)
    rw [hacc pk]
    by_cases hp : pk = tx.to
    · subst hp
      rw [if_pos rfl]
      dsimp only []
      rw [if_neg (fun h => ha (hCto.symm.trans h)), if_neg (fun h => ha (hCto.symm.trans h))]
    · rw [if_neg hp]

/-- Burn cannot change the supply of any asset other than its own. -/
theorem burn_conserves_other (s : State) (tx : BurnTx) (asset : AssetId)
    (live : List PubKey) (hwk : WellKeyed s) (ha : tx.assetId ≠ asset) :
    totalSupply s asset live = totalSupply (burn s tx).1 asset live := by
  by_cases hc : (s.accounts tx.from_).balance < tx.amount
      ∨ (s.accounts tx.from_).assetId ≠ tx.assetId
  · have hs : (burn s tx).1 = s := by
      dsimp only [burn, State.lookup]; rw [if_pos hc]
    rw [hs]
  · have hCfrom : (s.accounts tx.from_).assetId = tx.assetId :=
      Decidable.not_not.mp (fun h => hc (Or.inr h))
    have hacc : ∀ pk, (burn s tx).1.accounts pk =
        if pk = tx.from_ then
          { s.accounts tx.from_ with balance := (s.accounts tx.from_).balance - tx.amount, lastActive := tx.epoch }
        else s.accounts pk := by
      intro pk
      dsimp only [burn, State.lookup]
      rw [if_neg hc]
      dsimp only [State.update]
      rw [hwk tx.from_]
    apply totalSupply_congr
    intro pk
    show (if (s.accounts pk).assetId = asset then (s.accounts pk).balance else 0)
       = (if ((burn s tx).1.accounts pk).assetId = asset then ((burn s tx).1.accounts pk).balance else 0)
    rw [hacc pk]
    by_cases hp : pk = tx.from_
    · subst hp
      rw [if_pos rfl]
      dsimp only []
      rw [if_neg (fun h => ha (hCfrom.symm.trans h)), if_neg (fun h => ha (hCfrom.symm.trans h))]
    · rw [if_neg hp]

/-! ### Block-level definitions -/

/-- Amount this transaction mints into `asset` (0 unless it is a *successful*
    mint of that asset). -/
def txMintDelta (s : State) (asset : AssetId) : Tx → Nat
  | .mint t => if (mint s t).2 = true ∧ t.assetId = asset then t.amount else 0
  | _ => 0

/-- Amount this transaction burns from `asset` (0 unless it is a *successful*
    burn of that asset). -/
def txBurnDelta (s : State) (asset : AssetId) : Tx → Nat
  | .burn t => if (burn s t).2 = true ∧ t.assetId = asset then t.amount else 0
  | _ => 0

/-- Total successfully-minted amount of `asset` across a block (success is
    evaluated against the evolving state, exactly as `applyBlock` does). -/
def blockMinted (s : State) (asset : AssetId) : List Tx → Nat
  | [] => 0
  | tx :: rest => txMintDelta s asset tx + blockMinted (applyTx s tx).1 asset rest

/-- Total successfully-burned amount of `asset` across a block. -/
def blockBurned (s : State) (asset : AssetId) : List Tx → Nat
  | [] => 0
  | tx :: rest => txBurnDelta s asset tx + blockBurned (applyTx s tx).1 asset rest

/-- Endpoint well-formedness of a transaction w.r.t. the working set: the
    conditions under which the per-tx theorems apply (the sequencer guarantees
    these for every admitted transaction). -/
def TxWellFormed (live : List PubKey) : Tx → Prop
  | .transfer t => t.from_ ≠ t.to ∧ t.from_ ∈ live ∧ t.to ∈ live
  | .mint t => t.to ∈ live
  | .burn t => t.from_ ∈ live
  | .freeze _ => True

/-! ### Per-transaction accounting, then the block theorem -/

/-- One transaction's exact effect on supply, in additive form:
    `supply_before + minted = supply_after + burned`. -/
theorem applyTx_supply_accounting (s : State) (tx : Tx) (asset : AssetId)
    (live : List PubKey) (hwk : WellKeyed s) (hnd : live.Nodup)
    (hwf : TxWellFormed live tx) :
    totalSupply s asset live + txMintDelta s asset tx
      = totalSupply (applyTx s tx).1 asset live + txBurnDelta s asset tx := by
  cases tx with
  | transfer t =>
    obtain ⟨hd, hf, ht⟩ := hwf
    show totalSupply s asset live + 0 = totalSupply (transfer s t).1 asset live + 0
    rw [Nat.add_zero, Nat.add_zero]
    exact transfer_conserves s t asset live hwk hnd hd hf ht
  | freeze t =>
    show totalSupply s asset live + 0 = totalSupply (freeze s t).1 asset live + 0
    rw [Nat.add_zero, Nat.add_zero]
    exact freeze_conserves s t asset live hwk
  | mint t =>
    show totalSupply s asset live
           + (if (mint s t).2 = true ∧ t.assetId = asset then t.amount else 0)
       = totalSupply (mint s t).1 asset live + 0
    rw [Nat.add_zero]
    by_cases hs : (mint s t).2 = true
    · by_cases ha : t.assetId = asset
      · rw [if_pos ⟨hs, ha⟩, ← ha]
        exact (mint_increases_supply s t live hwk hnd hwf hs).symm
      · rw [if_neg (fun h => ha h.2), Nat.add_zero]
        exact mint_conserves_other s t asset live hwk ha
    · have hfb : (mint s t).2 = false := Bool.eq_false_iff.mpr hs
      rw [if_neg (fun h => hs h.1), Nat.add_zero, mint_fail_state s t hfb]
  | burn t =>
    show totalSupply s asset live + 0
       = totalSupply (burn s t).1 asset live
           + (if (burn s t).2 = true ∧ t.assetId = asset then t.amount else 0)
    rw [Nat.add_zero]
    by_cases hs : (burn s t).2 = true
    · by_cases ha : t.assetId = asset
      · rw [if_pos ⟨hs, ha⟩, ← ha]
        exact burn_decreases_supply s t live hwk hnd hwf hs
      · rw [if_neg (fun h => ha h.2), Nat.add_zero]
        exact burn_conserves_other s t asset live hwk ha
    · have hfb : (burn s t).2 = false := Bool.eq_false_iff.mpr hs
      rw [if_neg (fun h => hs h.1), Nat.add_zero, burn_fail_state s t hfb]

/-- **Block-level supply accounting.** Over any block, for every asset:
    `supply_before + total_minted = supply_after + total_burned`, where the
    totals sum exactly the successful mint/burn amounts for that asset. The
    supply of every asset moves only by its authorized mint/burn amounts, no
    matter how transfers and freezes are interleaved. -/
theorem block_supply_accounting (asset : AssetId) (live : List PubKey)
    (hnd : live.Nodup) :
    ∀ (txs : List Tx) (s : State), WellKeyed s → (∀ tx ∈ txs, TxWellFormed live tx) →
      totalSupply s asset live + blockMinted s asset txs
        = totalSupply (applyBlock s txs) asset live + blockBurned s asset txs := by
  intro txs
  induction txs with
  | nil =>
    intro s _ _
    show totalSupply s asset live + 0 = totalSupply s asset live + 0
    rfl
  | cons tx rest ih =>
    intro s hwk hwf
    have htx := applyTx_supply_accounting s tx asset live hwk hnd
      (hwf tx (List.mem_cons_self tx rest))
    have hrec := ih (applyTx s tx).1 (wellKeyed_applyTx s tx hwk)
      (fun t ht => hwf t (List.mem_cons.mpr (Or.inr ht)))
    show totalSupply s asset live + (txMintDelta s asset tx + blockMinted (applyTx s tx).1 asset rest)
       = totalSupply (applyBlock (applyTx s tx).1 rest) asset live
           + (txBurnDelta s asset tx + blockBurned (applyTx s tx).1 asset rest)
    omega

/-- Corollary: a block containing no mint or burn transactions cannot change
    the supply of any asset. -/
theorem block_without_authority_conserves (asset : AssetId) (live : List PubKey)
    (hnd : live.Nodup) (txs : List Tx) (s : State) (hwk : WellKeyed s)
    (hwf : ∀ tx ∈ txs, TxWellFormed live tx)
    (hnoauth : ∀ tx ∈ txs, (∀ t, tx ≠ Tx.mint t) ∧ (∀ t, tx ≠ Tx.burn t)) :
    totalSupply s asset live = totalSupply (applyBlock s txs) asset live := by
  have hM : ∀ (l : List Tx) (s' : State), (∀ tx ∈ l, (∀ t, tx ≠ Tx.mint t) ∧ (∀ t, tx ≠ Tx.burn t)) →
      blockMinted s' asset l = 0 ∧ blockBurned s' asset l = 0 := by
    intro l
    induction l with
    | nil => intro s' _; exact ⟨rfl, rfl⟩
    | cons tx rest ih =>
      intro s' hno
      have hhd := hno tx (List.mem_cons_self tx rest)
      have htl := ih (applyTx s' tx).1 (fun t ht => hno t (List.mem_cons.mpr (Or.inr ht)))
      have hdm : txMintDelta s' asset tx = 0 := by
        cases tx with
        | mint t => exact absurd rfl (hhd.1 t)
        | transfer _ => rfl
        | burn _ => rfl
        | freeze _ => rfl
      have hdb : txBurnDelta s' asset tx = 0 := by
        cases tx with
        | burn t => exact absurd rfl (hhd.2 t)
        | transfer _ => rfl
        | mint _ => rfl
        | freeze _ => rfl
      constructor
      · show txMintDelta s' asset tx + blockMinted (applyTx s' tx).1 asset rest = 0
        rw [hdm, htl.1]
      · show txBurnDelta s' asset tx + blockBurned (applyTx s' tx).1 asset rest = 0
        rw [hdb, htl.2]
  have hacc := block_supply_accounting asset live hnd txs s hwk hwf
  have hz := hM txs s hnoauth
  omega

end PSL
