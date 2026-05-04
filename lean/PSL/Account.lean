/-
  Lean model of a PSL account record. Mirrors `primitives/common.h` and
  `crypto/src/account.rs`. The Lean model is a hand-translation of the C
  semantics; the `tools/check_lean_drift.py` script enforces that the C source
  hash is unchanged since the last manual port.
-/

namespace PSL

/-- 32-byte ed25519 public key. We model it as a `Nat` index since the
    conservation/determinism theorems only need pubkeys to be distinguishable;
    byte-level structure isn't load-bearing for these proofs. -/
abbrev PubKey  := Nat
abbrev Balance := Nat   -- u128, modeled as ℕ ≤ 2^128 - 2^120 (room for frozen flag)
abbrev Nonce   := Nat   -- u64
abbrev Epoch   := Nat   -- u64
abbrev AssetId := Nat   -- u32

structure Account where
  pubkey       : PubKey
  balance      : Balance
  nonce        : Nonce
  lastActive   : Epoch
  assetId      : AssetId
  frozen       : Bool

namespace Account

/-- Empty (default) account for an unknown pubkey. -/
def empty (pk : PubKey) : Account :=
  { pubkey := pk
    balance := 0
    nonce := 0
    lastActive := 0
    assetId := 0
    frozen := false }

/-- An account is well-formed if its balance fits in u128 minus the frozen-flag bit. -/
def wellFormed (a : Account) : Prop :=
  a.balance < 2 ^ 127 ∧ a.nonce < 2 ^ 64

theorem empty_well_formed (pk : PubKey) : (empty pk).wellFormed := by
  refine ⟨?_, ?_⟩
  · exact Nat.two_pow_pos 127
  · exact Nat.two_pow_pos 64

end Account
end PSL
