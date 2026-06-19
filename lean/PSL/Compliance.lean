/-
  Compliance policy (machine-checked, core Lean only).

  Faithful model of the mempool admission policy in `sequencer/src/mempool.rs`
  (`validate`) — the pure decision function the gate-5 compliance suite (9/9)
  exercises. A transaction is admissible iff every gate passes; the Rust
  `validate` short-circuits with `Err` on the first failing gate, which is
  exactly "the conjunction of the gates".

  Modeling notes (honest boundary):
    • Signature verification is abstracted as an opaque proposition `SigValid`
      (same treatment as `hash` in the MPT layer) — the policy theorems are
      about the gates `validate` applies *after* the signature check.
    • `court_order_hash.is_none()` / `originator_metadata.is_none()` are modeled
      as the boolean flags `hasCourtOrder` / `hasOriginatorMetadata`.
    • The model assumes the asset is registered (the Rust `unknown asset_id`
      early-return is the "no issuer" case, out of scope here).
    • mint/freeze nonces are registry-tracked, not account-tracked — modeled by
      `NonceFreshOk` being vacuously satisfied for those kinds, matching Rust.

  These theorems pin the regulator-facing guarantees: high-value transfers
  without travel-rule metadata are rejected, freezes need issuer authority AND
  a court order, mint/burn need issuer authority, frozen senders cannot send,
  and a fully-compliant transaction is admitted.

  Watched source: `sequencer/src/mempool.rs` (tools/check_lean_drift.py).
-/

import PSL.Account

namespace PSL.Compliance

/-- Transaction kinds, mirroring `sequencer/src/tx.rs::TxKind`. -/
inductive CKind
  | transfer
  | mint
  | burn
  | freeze
  | multiAsset
  deriving DecidableEq

/-- The fields of a transaction the admission policy reads. -/
structure CTx where
  kind                  : CKind
  signer                : PubKey
  nonce                 : Nonce
  assetId               : AssetId
  amount                : Balance
  hasCourtOrder         : Bool
  hasOriginatorMetadata : Bool

/-- Issuer registry record, mirroring the fields `validate` reads. -/
structure CIssuer where
  authority       : PubKey
  mintEnabled     : Bool
  burnEnabled     : Bool
  freezeEnabled   : Bool
  travelThreshold : Balance

/-! ### The gates (each mirrors one `return Err(..)` site in `validate`) -/

/-- Nonce + frozen-sender gate. Transfer/burn require `nonce = acct.nonce + 1`
    and a non-frozen sender; multi-asset requires the nonce; mint/freeze are
    registry-tracked (no account gate). -/
def NonceFreshOk (tx : CTx) (acct : Account) : Prop :=
  match tx.kind with
  | .transfer | .burn => tx.nonce = acct.nonce + 1 ∧ acct.frozen = false
  | .multiAsset => tx.nonce = acct.nonce + 1
  | _ => True

/-- Issuer-authority gate for mint/burn/freeze. -/
def AuthorityOk (tx : CTx) (iss : CIssuer) : Prop :=
  match tx.kind with
  | .mint => tx.signer = iss.authority ∧ iss.mintEnabled = true
  | .burn => tx.signer = iss.authority ∧ iss.burnEnabled = true
  | .freeze => tx.signer = iss.authority ∧ iss.freezeEnabled = true
  | _ => True

/-- Court-order gate: a freeze must carry a court order. -/
def CourtOk (tx : CTx) : Prop :=
  match tx.kind with
  | .freeze => tx.hasCourtOrder = true
  | _ => True

/-- Travel-rule gate: a transfer/multi-asset that is high-value
    (`threshold = 0` ∨ `amount > threshold`) must carry originator metadata. -/
def TravelOk (tx : CTx) (iss : CIssuer) : Prop :=
  match tx.kind with
  | .transfer | .multiAsset =>
      ¬((iss.travelThreshold = 0 ∨ iss.travelThreshold < tx.amount)
          ∧ tx.hasOriginatorMetadata = false)
  | _ => True

/-- A transaction is admissible iff the signature is valid and every gate
    passes — the conjunction the short-circuiting `validate` computes. -/
def Admissible (SigValid : Prop) (tx : CTx) (acct : Account) (iss : CIssuer) : Prop :=
  SigValid ∧ NonceFreshOk tx acct ∧ AuthorityOk tx iss ∧ CourtOk tx ∧ TravelOk tx iss

/-! ### Rejection theorems (each gate is load-bearing) -/

/-- Invalid signature ⇒ rejected. -/
theorem invalid_signature_rejected (SigValid : Prop) (tx : CTx) (acct : Account)
    (iss : CIssuer) (h : ¬SigValid) : ¬Admissible SigValid tx acct iss :=
  fun ha => h ha.1

/-- A high-value transfer with no originator metadata is rejected (travel rule).
    `high` is the Rust condition `threshold == 0 || amount > threshold`. -/
theorem travel_rule_high_value_rejected (SigValid : Prop) (tx : CTx) (acct : Account)
    (iss : CIssuer) (hk : tx.kind = .transfer)
    (high : iss.travelThreshold = 0 ∨ iss.travelThreshold < tx.amount)
    (hmeta : tx.hasOriginatorMetadata = false) :
    ¬Admissible SigValid tx acct iss := by
  intro ha
  have htravel : TravelOk tx iss := ha.2.2.2.2
  rw [TravelOk, hk] at htravel
  exact htravel ⟨high, hmeta⟩

/-- A freeze by a non-issuer-authority signer is rejected. -/
theorem freeze_non_authority_rejected (SigValid : Prop) (tx : CTx) (acct : Account)
    (iss : CIssuer) (hk : tx.kind = .freeze) (hsig : tx.signer ≠ iss.authority) :
    ¬Admissible SigValid tx acct iss := by
  intro ha
  have hauth : AuthorityOk tx iss := ha.2.2.1
  rw [AuthorityOk, hk] at hauth
  exact hsig hauth.1

/-- A freeze without a court order is rejected. -/
theorem freeze_without_court_order_rejected (SigValid : Prop) (tx : CTx) (acct : Account)
    (iss : CIssuer) (hk : tx.kind = .freeze) (hco : tx.hasCourtOrder = false) :
    ¬Admissible SigValid tx acct iss := by
  intro ha
  have hcourt : CourtOk tx := ha.2.2.2.1
  rw [CourtOk, hk] at hcourt
  rw [hco] at hcourt
  exact Bool.noConfusion hcourt

/-- A mint not signed by the (mint-enabled) issuer authority is rejected. -/
theorem mint_non_authority_rejected (SigValid : Prop) (tx : CTx) (acct : Account)
    (iss : CIssuer) (hk : tx.kind = .mint)
    (hbad : tx.signer ≠ iss.authority ∨ iss.mintEnabled = false) :
    ¬Admissible SigValid tx acct iss := by
  intro ha
  have hauth : AuthorityOk tx iss := ha.2.2.1
  rw [AuthorityOk, hk] at hauth
  rcases hbad with h | h
  · exact h hauth.1
  · rw [h] at hauth; exact Bool.noConfusion hauth.2

/-- A burn not signed by the (burn-enabled) issuer authority is rejected. -/
theorem burn_non_authority_rejected (SigValid : Prop) (tx : CTx) (acct : Account)
    (iss : CIssuer) (hk : tx.kind = .burn)
    (hbad : tx.signer ≠ iss.authority ∨ iss.burnEnabled = false) :
    ¬Admissible SigValid tx acct iss := by
  intro ha
  have hauth : AuthorityOk tx iss := ha.2.2.1
  rw [AuthorityOk, hk] at hauth
  rcases hbad with h | h
  · exact h hauth.1
  · rw [h] at hauth; exact Bool.noConfusion hauth.2

/-- A transfer or burn from a frozen account is rejected. -/
theorem frozen_sender_rejected (SigValid : Prop) (tx : CTx) (acct : Account)
    (iss : CIssuer) (hk : tx.kind = .transfer ∨ tx.kind = .burn)
    (hfrozen : acct.frozen = true) :
    ¬Admissible SigValid tx acct iss := by
  intro ha
  have hnf : NonceFreshOk tx acct := ha.2.1
  rcases hk with h | h <;>
    · rw [NonceFreshOk, h] at hnf
      rw [hfrozen] at hnf
      exact Bool.noConfusion hnf.2

/-- A transfer or burn with the wrong nonce is rejected (replay protection at
    admission time). -/
theorem nonce_mismatch_rejected (SigValid : Prop) (tx : CTx) (acct : Account)
    (iss : CIssuer) (hk : tx.kind = .transfer ∨ tx.kind = .burn)
    (hnonce : tx.nonce ≠ acct.nonce + 1) :
    ¬Admissible SigValid tx acct iss := by
  intro ha
  have hnf : NonceFreshOk tx acct := ha.2.1
  rcases hk with h | h <;>
    · rw [NonceFreshOk, h] at hnf
      exact hnonce hnf.1

/-! ### Admission theorem (the gates are also sufficient) -/

/-- A fully-compliant transfer is admitted: valid signature, correct nonce,
    unfrozen sender, and either low-value or metadata present. This is the
    positive direction the gate-5 "accepted" tests check. -/
theorem compliant_transfer_admitted (SigValid : Prop) (tx : CTx) (acct : Account)
    (iss : CIssuer) (hsig : SigValid) (hk : tx.kind = .transfer)
    (hnonce : tx.nonce = acct.nonce + 1) (hunfrozen : acct.frozen = false)
    (htravel : tx.hasOriginatorMetadata = true
               ∨ (iss.travelThreshold ≠ 0 ∧ tx.amount ≤ iss.travelThreshold)) :
    Admissible SigValid tx acct iss := by
  refine ⟨hsig, ?_, ?_, ?_, ?_⟩
  · rw [NonceFreshOk, hk]; exact ⟨hnonce, hunfrozen⟩
  · rw [AuthorityOk, hk]; trivial
  · rw [CourtOk, hk]; trivial
  · rw [TravelOk, hk]
    rintro ⟨hhigh, hmeta⟩
    rcases htravel with hm | ⟨hthr, hle⟩
    · rw [hm] at hmeta; exact Bool.noConfusion hmeta
    · rcases hhigh with h0 | hlt
      · exact hthr h0
      · exact absurd hle (Nat.not_le.mpr hlt)

end PSL.Compliance
