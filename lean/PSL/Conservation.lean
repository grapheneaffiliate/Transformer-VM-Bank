/-
  Conservation theorems (corrected).

  ── Audit note (soundness fixes) ────────────────────────────────────────────
  The previous version of this file left the two conservation theorems as
  `sorry` and stated `supply_changes_only_via_authority` in a VACUOUS form
  (its conclusion `∃ kind, kind = "mint" ∨ kind = "burn"` is satisfied by the
  constant `"mint"` regardless of input — the proof never used `h_change`).

  Three defects, all reproduced as machine-checked counterexamples below:

    1. `freeze` only conserves supply when the state is *well-keyed*: the
       ledger model writes an account at index `a.pubkey` (State.update), so a
       mis-keyed account (whose `.pubkey` field disagrees with the slot it is
       read from) lets `freeze` clobber a different account's balance. The
       invariant `WellKeyed` pins down the property the Rust/C ledger actually
       maintains but the Lean spec never stated.

    2. `transfer` only conserves supply when the working set `live` has no
       duplicate keys. A duplicated key double-counts the moved delta. The
       fix adds `live.Nodup` plus the genuinely-necessary endpoint conditions
       (distinct endpoints, both in `live`).

    3. The authority theorem is restated so its conclusion actually identifies
       the transaction as a mint or a burn, and `h_change` is genuinely load
       bearing (it derives contradictions in the transfer/freeze cases via the
       two conservation theorems).

  Everything here is core Lean 4 only (no mathlib): the proofs depend on
  nothing but `propext`/`Quot.sound` (Lean's standard foundations); the
  counterexamples depend on no axioms at all (`decide`, pure kernel
  computation). No `sorry`, no `native_decide`.
-/

import PSL.Ledger

namespace PSL

/-- Total balance of all accounts holding `assetId`, restricted to a finite
    set of pubkeys (the live working set; this is what the sequencer sees
    in any block). -/
def totalSupply (s : State) (assetId : AssetId) (live : List PubKey) : Nat :=
  (live.map (fun pk =>
    let a := s.accounts pk
    if a.assetId = assetId then a.balance else 0)).foldl (· + ·) 0

/-- A state is *well-keyed* when every account is stored under its own pubkey.
    `State.update` writes an account at index `a.pubkey`, so without this
    invariant a lookup can return an account belonging to a different slot.
    The Rust/C ledger maintains this by construction (the SMT keys on pubkey);
    the Lean model has to assume it explicitly. -/
def WellKeyed (s : State) : Prop := ∀ pk, (s.accounts pk).pubkey = pk

/-! ### List-sum machinery (core only)

    We need: if two contribution functions `f`, `g` agree everywhere on a
    `Nodup` list except at two distinct points `a`, `b`, and the *combined*
    contribution at those points is preserved (`f a + f b = g a + g b`), then
    the total sums agree. Truncated `Nat` subtraction forces an additive
    formulation throughout (no `f a - g a`). -/

/-- `foldl (+)` from accumulator `z` is `z` plus the `foldr (+)` sum. -/
theorem foldl_add_acc :
    ∀ (l : List Nat) (z : Nat), l.foldl (· + ·) z = z + l.foldr (· + ·) 0 := by
  intro l
  induction l with
  | nil => intro z; simp
  | cons x xs ih => intro z; simp only [List.foldl_cons, List.foldr_cons]; rw [ih]; omega

/-- Pointwise-equal contribution functions give equal sums. -/
private theorem foldr_congr (f g : PubKey → Nat) :
    ∀ (l : List PubKey), (∀ x ∈ l, f x = g x) →
      (l.map f).foldr (· + ·) 0 = (l.map g).foldr (· + ·) 0 := by
  intro l
  induction l with
  | nil => intro _; rfl
  | cons x xs ih =>
    intro h
    simp only [List.map_cons, List.foldr_cons]
    rw [h x (List.mem_cons_self x xs), ih (fun y hy => h y (List.mem_cons.mpr (Or.inr hy)))]

/-- Single-point update lemma (additive form to dodge `Nat` subtraction):
    if `f`, `g` agree on a `Nodup` list except possibly at `b ∈ l`, then
    `Σf + g b = Σg + f b`. -/
theorem foldr_one (f g : PubKey → Nat) (b : PubKey) :
    ∀ (l : List PubKey), b ∈ l → l.Nodup → (∀ x ∈ l, x ≠ b → f x = g x) →
      (l.map f).foldr (· + ·) 0 + g b = (l.map g).foldr (· + ·) 0 + f b := by
  intro l
  induction l with
  | nil => intro hb _ _; exact absurd hb (List.not_mem_nil b)
  | cons x xs ih =>
    intro hb hnd hother
    have hxnd := List.nodup_cons.mp hnd
    by_cases hxb : x = b
    · subst hxb
      have hfg : ∀ y ∈ xs, f y = g y := fun y hy =>
        hother y (List.mem_cons.mpr (Or.inr hy)) (fun h => hxnd.1 (h ▸ hy))
      have hF := foldr_congr f g xs hfg
      simp only [List.map_cons, List.foldr_cons]
      omega
    · have hbxs : b ∈ xs := by
        rcases List.mem_cons.mp hb with h | h
        · exact absurd h.symm hxb
        · exact h
      have hfgx : f x = g x := hother x (List.mem_cons_self x xs) hxb
      have hrec := ih hbxs hxnd.2
        (fun y hy hyb => hother y (List.mem_cons.mpr (Or.inr hy)) hyb)
      simp only [List.map_cons, List.foldr_cons]
      omega

/-- Two-point swap lemma: agree off `{a,b}`, combined endpoints preserved
    ⇒ equal sums. -/
private theorem foldr_two (f g : PubKey → Nat) (a b : PubKey) (hab : a ≠ b) :
    ∀ (l : List PubKey), a ∈ l → b ∈ l → l.Nodup →
      (∀ x ∈ l, x ≠ a → x ≠ b → f x = g x) → f a + f b = g a + g b →
      (l.map f).foldr (· + ·) 0 = (l.map g).foldr (· + ·) 0 := by
  intro l
  induction l with
  | nil => intro ha _ _ _ _; exact absurd ha (List.not_mem_nil a)
  | cons x xs ih =>
    intro ha hb hnd hother hsum
    have hxnd := List.nodup_cons.mp hnd
    by_cases hxa : x = a
    · subst hxa
      have hbxs : b ∈ xs := by
        rcases List.mem_cons.mp hb with h | h
        · exact absurd h.symm hab
        · exact h
      have hother' : ∀ y ∈ xs, y ≠ b → f y = g y := fun y hy hyb =>
        hother y (List.mem_cons.mpr (Or.inr hy)) (fun h => hxnd.1 (h ▸ hy)) hyb
      have hone := foldr_one f g b xs hbxs hxnd.2 hother'
      simp only [List.map_cons, List.foldr_cons]
      omega
    · by_cases hxb : x = b
      · subst hxb
        have haxs : a ∈ xs := by
          rcases List.mem_cons.mp ha with h | h
          · exact absurd h hab
          · exact h
        have hother' : ∀ y ∈ xs, y ≠ a → f y = g y := fun y hy hya =>
          hother y (List.mem_cons.mpr (Or.inr hy)) hya (fun h => hxnd.1 (h ▸ hy))
        have hone := foldr_one f g a xs haxs hxnd.2 hother'
        simp only [List.map_cons, List.foldr_cons]
        omega
      · have haxs : a ∈ xs := by
          rcases List.mem_cons.mp ha with h | h
          · exact absurd h.symm hxa
          · exact h
        have hbxs : b ∈ xs := by
          rcases List.mem_cons.mp hb with h | h
          · exact absurd h.symm hxb
          · exact h
        have hfgx : f x = g x := hother x (List.mem_cons_self x xs) hxa hxb
        have hrec := ih haxs hbxs hxnd.2
          (fun y hy => hother y (List.mem_cons.mpr (Or.inr hy))) hsum
        simp only [List.map_cons, List.foldr_cons]
        omega

/-- Lift the two-point swap to `totalSupply`'s `foldl` form. -/
private theorem totalSupply_swap_two
    (f g : PubKey → Nat) (l : List PubKey) (a b : PubKey)
    (hab : a ≠ b) (ha : a ∈ l) (hb : b ∈ l) (hnd : l.Nodup)
    (hother : ∀ x ∈ l, x ≠ a → x ≠ b → f x = g x)
    (hsum : f a + f b = g a + g b)
    : (l.map f).foldl (· + ·) 0 = (l.map g).foldl (· + ·) 0 := by
  rw [foldl_add_acc (l.map f) 0, foldl_add_acc (l.map g) 0,
      foldr_two f g a b hab l ha hb hnd hother hsum]

/-- If two states give equal per-pubkey contributions, their supplies agree. -/
theorem totalSupply_congr (s s' : State) (asset : AssetId) (live : List PubKey)
    (h : ∀ pk, (let a := s.accounts pk; if a.assetId = asset then a.balance else 0)
             = (let a := s'.accounts pk; if a.assetId = asset then a.balance else 0))
    : totalSupply s asset live = totalSupply s' asset live := by
  unfold totalSupply
  rw [show (fun pk => let a := s.accounts pk; if a.assetId = asset then a.balance else 0)
        = (fun pk => let a := s'.accounts pk; if a.assetId = asset then a.balance else 0)
      from funext h]

/-! ### freeze conservation -/

/-- `freeze` never changes any balance, **given `WellKeyed s`**. Without the
    invariant the theorem is false (see `freeze_not_conserves_without_wellkeyed`). -/
theorem freeze_conserves
    (s : State) (tx : FreezeTx) (asset : AssetId) (live : List PubKey)
    (hwk : WellKeyed s)
    : totalSupply s asset live = totalSupply (freeze s tx).1 asset live := by
  apply totalSupply_congr
  intro pk
  have hacc : (freeze s tx).1.accounts pk
      = if pk = tx.account then { s.accounts tx.account with frozen := tx.flag }
        else s.accounts pk := by
    dsimp only [freeze, State.lookup, State.update]
    rw [hwk tx.account]
  show (let a := s.accounts pk; if a.assetId = asset then a.balance else 0)
     = (let a := (freeze s tx).1.accounts pk; if a.assetId = asset then a.balance else 0)
  rw [hacc]
  by_cases h : pk = tx.account
  · subst h; rw [if_pos rfl]
  · rw [if_neg h]

/-! ### transfer conservation -/

/-- `transfer` never changes the total supply of **any** asset, given a
    well-keyed state, a `Nodup` working set, distinct endpoints both present in
    `live`. (Generalized from the original `tx.assetId`-only statement: a
    transfer touches only `tx.assetId` accounts, so every asset's supply is
    conserved.) -/
theorem transfer_conserves
    (s : State) (tx : TransferTx) (asset : AssetId) (live : List PubKey)
    (hwk : WellKeyed s) (hnd : live.Nodup)
    (h_distinct : tx.from_ ≠ tx.to)
    (h_in_from : tx.from_ ∈ live) (h_in_to : tx.to ∈ live)
    : totalSupply s asset live = totalSupply (transfer s tx).1 asset live := by
  by_cases hfail :
      (s.accounts tx.from_).frozen ∨ (s.accounts tx.from_).balance < tx.amount
      ∨ (s.accounts tx.from_).assetId ≠ tx.assetId ∨ (s.accounts tx.to).assetId ≠ tx.assetId
  · -- failure: state unchanged
    have hs : (transfer s tx).1 = s := by
      dsimp only [transfer, State.lookup]; rw [if_pos hfail]
    rw [hs]
  · -- success: extract the negated conditions
    -- omega does not unfold the `Balance`/`AssetId` abbrevs, so extract the
    -- negated conditions with core term-mode lemmas instead.
    have hnB : ¬((s.accounts tx.from_).balance < tx.amount) :=
      fun hb => hfail (Or.inr (Or.inl hb))
    have hge : tx.amount ≤ (s.accounts tx.from_).balance := Nat.not_lt.mp hnB
    have hnCf : ¬((s.accounts tx.from_).assetId ≠ tx.assetId) :=
      fun hc => hfail (Or.inr (Or.inr (Or.inl hc)))
    have hCfrom : (s.accounts tx.from_).assetId = tx.assetId := Decidable.not_not.mp hnCf
    have hnCt : ¬((s.accounts tx.to).assetId ≠ tx.assetId) :=
      fun hc => hfail (Or.inr (Or.inr (Or.inr hc)))
    have hCto : (s.accounts tx.to).assetId = tx.assetId := Decidable.not_not.mp hnCt
    have hacc : ∀ pk, (transfer s tx).1.accounts pk =
        if pk = tx.to then
          { s.accounts tx.to with
            balance := (s.accounts tx.to).balance + tx.amount, lastActive := tx.epoch }
        else if pk = tx.from_ then
          { s.accounts tx.from_ with
            balance := (s.accounts tx.from_).balance - tx.amount,
            nonce := (s.accounts tx.from_).nonce + 1, lastActive := tx.epoch }
        else s.accounts pk := by
      intro pk
      dsimp only [transfer, State.lookup]
      rw [if_neg hfail]
      dsimp only [State.update]
      rw [hwk tx.to, hwk tx.from_]
    unfold totalSupply
    apply totalSupply_swap_two _ _ live tx.from_ tx.to h_distinct h_in_from h_in_to hnd
    · -- agree away from the two endpoints
      intro pk _ hpf hpt
      show (let a := s.accounts pk; if a.assetId = asset then a.balance else 0)
         = (let a := (transfer s tx).1.accounts pk; if a.assetId = asset then a.balance else 0)
      rw [hacc pk, if_neg hpt, if_neg hpf]
    · -- combined endpoint contributions preserved
      show (let a := s.accounts tx.from_; if a.assetId = asset then a.balance else 0)
         + (let a := s.accounts tx.to; if a.assetId = asset then a.balance else 0)
         = (let a := (transfer s tx).1.accounts tx.from_; if a.assetId = asset then a.balance else 0)
         + (let a := (transfer s tx).1.accounts tx.to; if a.assetId = asset then a.balance else 0)
      rw [hacc tx.from_, hacc tx.to, if_neg h_distinct, if_pos rfl, if_pos rfl]
      dsimp only []
      rw [hCfrom, hCto]
      by_cases hasset : tx.assetId = asset
      · -- `bal_f + bal_t = (bal_f - amt) + (bal_t + amt)`, using `amt ≤ bal_f`.
        -- omega is unavailable on `Balance`-typed atoms; rewrite with core
        -- `Nat` lemmas instead.
        simp only [if_pos hasset]
        rw [Nat.add_comm (s.accounts tx.to).balance tx.amount, ← Nat.add_assoc,
            Nat.sub_add_cancel hge]
      · simp only [if_neg hasset]

/-! ### authority: the only supply-changing transactions are mint/burn -/

/-- Honest restatement (the original was vacuous). A change in total supply
    forces `tx` to be a mint or a burn. The transfer/freeze cases are ruled out
    by the conservation theorems, so `h_change` is genuinely used. For the
    transfer case we must assume the transfer is well-formed (distinct endpoints
    both in a `Nodup` working set) — exactly the hypotheses `transfer_conserves`
    needs; without them a transfer *can* change supply (see the counterexamples). -/
theorem supply_changes_only_via_authority
    (s : State) (tx : Tx) (asset : AssetId) (live : List PubKey)
    (hwk : WellKeyed s) (hnd : live.Nodup)
    (hwf : ∀ t, tx = Tx.transfer t → t.from_ ≠ t.to ∧ t.from_ ∈ live ∧ t.to ∈ live)
    (h_change : totalSupply s asset live ≠ totalSupply (applyTx s tx).1 asset live)
    : (∃ t, tx = Tx.mint t) ∨ (∃ t, tx = Tx.burn t) := by
  cases tx with
  | transfer t =>
    refine absurd ?_ h_change
    have hw := hwf t rfl
    exact transfer_conserves s t asset live hwk hnd hw.1 hw.2.1 hw.2.2
  | mint t => exact Or.inl ⟨t, rfl⟩
  | burn t => exact Or.inr ⟨t, rfl⟩
  | freeze t =>
    refine absurd ?_ h_change
    exact freeze_conserves s t asset live hwk

/-! ### Counterexamples: the original (unguarded) statements were false/vacuous.

    These depend on **no axioms** — pure kernel computation via `decide`. -/

/-- A mis-keyed state: the account stored at slot `0` carries `pubkey = 5`. -/
private def cexFreeze : State where
  accounts := fun pk =>
    if pk = 0 then
      { pubkey := 5, balance := 10, nonce := 0, lastActive := 0, assetId := 1, frozen := false }
    else if pk = 5 then
      { pubkey := 5, balance := 99, nonce := 0, lastActive := 0, assetId := 1, frozen := false }
    else Account.empty pk

/-- Finding 1: without `WellKeyed`, freezing slot `0` clobbers slot `5`'s
    balance and total supply changes (109 → 20). -/
theorem freeze_not_conserves_without_wellkeyed :
    totalSupply cexFreeze 1 [0, 5]
      ≠ totalSupply (freeze cexFreeze { account := 0, flag := true }).1 1 [0, 5] := by
  decide

/-- A well-keyed state with two asset-`1` accounts. -/
private def cexTransfer : State where
  accounts := fun pk =>
    if pk = 1 then
      { pubkey := 1, balance := 10, nonce := 0, lastActive := 0, assetId := 1, frozen := false }
    else if pk = 2 then
      { pubkey := 2, balance := 0, nonce := 0, lastActive := 0, assetId := 1, frozen := false }
    else Account.empty pk

/-- Finding 2: with a duplicated key in `live` the moved delta is double-counted
    and supply changes (20 → 15), so `Nodup` is necessary. -/
theorem transfer_not_conserves_without_nodup :
    totalSupply cexTransfer 1 [1, 1, 2]
      ≠ totalSupply (transfer cexTransfer
          { from_ := 1, to := 2, amount := 5, assetId := 1, epoch := 0 }).1 1 [1, 1, 2] := by
  decide

/-- Finding 3: the *original* conclusion is provable with no hypotheses at all,
    i.e. it constrained nothing — the supply change `h_change` was never used. -/
theorem original_authority_conclusion_is_vacuous :
    ∃ kind : String, kind = "mint" ∨ kind = "burn" :=
  ⟨"mint", Or.inl rfl⟩

end PSL
