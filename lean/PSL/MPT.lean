/-
  Sparse Merkle Tree soundness (corrected).

  ── Audit note ──────────────────────────────────────────────────────────────
  The previous `inclusion_proof_sound` was ill-posed and left as `sorry`:
  its conclusion was `proof.value.length = 0 ∨ proof.value.length = 64`, but
  `verifyProof` never inspects `proof.value`'s length (and the real
  `crypto/src/smt.rs::verify_proof` does not either — it hashes whatever value
  bytes it is given). No implementation enforces that, so the statement is
  simply false as a soundness claim.

  This file instead:
    • gives `verifyProof` the *actual* semantics of `verify_proof`: recompute
      the root by folding the 256 sibling hashes (ordered by the key bits) up
      from `leafHash key value`, and accept iff it equals `root`;
    • proves the genuine soundness property — VALUE BINDING: for a fixed
      `root` and `key`, no two proofs can verify with different values. This is
      exactly what the `tampered_value_fails_proof` test asserts, and it is the
      property the settlement layer actually relies on (a committed root pins a
      unique value per key). It is discharged from `hash_collision_resistant`.

  Crypto assumptions are explicit axioms (as before): collision resistance and
  the fixed 32-byte digest length of BLAKE3-256. No `sorry`, no `native_decide`.

  Faithfulness to `crypto/src/smt.rs`:
    leafHash key v = if v = [] then hash ""            -- empty ⇒ default leaf
                     else hash (0x00 :: key ++ hash v) -- hash_three(0x00,key,H(v))
    parent at depth d = hash(left || right), with (sib,cur) ordered by bit(key,d)
    verify folds d = 255..0 from the leaf; accept iff recomputed == root.
-/

import PSL.Account

namespace PSL.MPT

/--
  BLAKE3 digest of length `n` bytes, modeled as a list-of-bytes.
  Parameterized over length so v1 (32 bytes, BLAKE3-256) and v2
  (64 bytes, BLAKE3-512) per ADR-0008 share one definition.
-/
abbrev Digest (_n : Nat) : Type := List (Fin 256)

/-- The MPT-root surface stays BLAKE3-256 per ADR-0008. -/
abbrev Hash : Type := Digest 32

instance : Inhabited (Digest 32) := ⟨List.replicate 32 0⟩
instance : Inhabited (Digest 64) := ⟨List.replicate 64 0⟩
instance : Inhabited Hash        := ⟨List.replicate 32 0⟩

/-- Opaque hash function. Length-parametric so the same opaque primitive
    models both BLAKE3-256 and BLAKE3-512. -/
opaque hashTo (n : Nat) : List (Fin 256) → Digest n

/-- Convenience: 32-byte hash for the MPT layer. -/
def hash : List (Fin 256) → Hash := hashTo 32

/-- Collision resistance of BLAKE3 (standard cryptographic assumption). -/
axiom hash_collision_resistant :
    ∀ (a b : List (Fin 256)), hash a = hash b → a = b

/-- BLAKE3-256 produces a fixed 32-byte digest. -/
axiom hash_length : ∀ (a : List (Fin 256)), (hash a).length = 32

structure Proof where
  siblings : List Hash
  value    : List (Fin 256)

/-- Bit `i` of the key (MSB-first within each byte), mirroring
    `SparseMerkleTree::bit`: `byte = i/8`, `bit = 7 - i%8`. -/
def bit (key : List (Fin 256)) (i : Nat) : Bool :=
  ((key.getD (i / 8) 0).val >>> (7 - i % 8)) % 2 == 1

/-- Hash a parent node from a sibling and the running child, with the two
    ordered by the key bit at this depth (`bit ⇒ sibling is the left child`). -/
def combine (key : List (Fin 256)) (d : Nat) (sib cur : Hash) : Hash :=
  if bit key d then hash (sib ++ cur) else hash (cur ++ sib)

/-- Recompute a root from a leaf and its sibling path. `recompute key 0 leaf
    [s₀,…,s₂₅₅]` applies the deepest sibling `s₂₅₅` to `leaf` first (innermost)
    and `s₀` last (outermost = root), matching `verify_proof`'s `d = 255..0`. -/
def recompute (key : List (Fin 256)) : Nat → Hash → List Hash → Hash
  | _, cur, []          => cur
  | d, cur, sib :: rest => combine key d sib (recompute key (d + 1) cur rest)

/-- Leaf hash, mirroring `SparseMerkleTree::leaf_hash`. -/
def leafHash (key : List (Fin 256)) (value : List (Fin 256)) : Hash :=
  if value = [] then hash [] else hash ([0] ++ key ++ hash value)

/-- Verify a proof: 256 sibling hashes, each a 32-byte digest, recomputing to
    `root`. Mirrors `SparseMerkleTree::verify_proof`. -/
def verifyProof (root : Hash) (key : List (Fin 256)) (proof : Proof) : Bool :=
  decide (proof.siblings.length = 256
        ∧ (∀ s ∈ proof.siblings, s.length = 32)
        ∧ recompute key 0 (leafHash key proof.value) proof.siblings = root)

/-! ### Length and injectivity lemmas -/

/-- Every recomputed hash is a 32-byte digest (a leaf of that length, or a
    `hash` output). -/
theorem recompute_length (key : List (Fin 256)) (d : Nat) (leaf : Hash)
    (sibs : List Hash) (hleaf : leaf.length = 32) :
    (recompute key d leaf sibs).length = 32 := by
  cases sibs with
  | nil => exact hleaf
  | cons s r =>
    show (combine key d s (recompute key (d + 1) leaf r)).length = 32
    unfold combine
    by_cases hb : bit key d
    · rw [if_pos hb]; exact hash_length _
    · rw [if_neg hb]; exact hash_length _

/-- `leafHash` is always a 32-byte digest. -/
theorem leafHash_length (key value : List (Fin 256)) :
    (leafHash key value).length = 32 := by
  unfold leafHash
  by_cases hv : value = []
  · rw [if_pos hv]; exact hash_length _
  · rw [if_neg hv]; exact hash_length _

/-- Equal recomputed roots over equal-length, 32-byte sibling paths force equal
    leaves — the core of binding, by collision resistance at each level. -/
theorem recompute_inj (key : List (Fin 256)) (leaf1 leaf2 : Hash)
    (hleaf1 : leaf1.length = 32) (hleaf2 : leaf2.length = 32) :
    ∀ (sibs1 sibs2 : List Hash) (d : Nat),
      (∀ s ∈ sibs1, s.length = 32) → (∀ s ∈ sibs2, s.length = 32) →
      sibs1.length = sibs2.length →
      recompute key d leaf1 sibs1 = recompute key d leaf2 sibs2 → leaf1 = leaf2 := by
  intro sibs1
  induction sibs1 with
  | nil =>
    intro sibs2 d _ _ hlen h
    cases sibs2 with
    | nil => simpa [recompute] using h
    | cons s2 r2 => simp at hlen
  | cons s1 r1 ih =>
    intro sibs2 d hl1 hl2 hlen h
    cases sibs2 with
    | nil => simp at hlen
    | cons s2 r2 =>
      simp only [recompute] at h
      have hs1 : s1.length = 32 := hl1 s1 (List.mem_cons_self s1 r1)
      have hs2 : s2.length = 32 := hl2 s2 (List.mem_cons_self s2 r2)
      have hR1len := recompute_length key (d + 1) leaf1 r1 hleaf1
      have hR2len := recompute_length key (d + 1) leaf2 r2 hleaf2
      have hRR : recompute key (d + 1) leaf1 r1 = recompute key (d + 1) leaf2 r2 := by
        unfold combine at h
        by_cases hb : bit key d
        · rw [if_pos hb, if_pos hb] at h
          exact (List.append_inj (hash_collision_resistant _ _ h) (by rw [hs1, hs2])).2
        · rw [if_neg hb, if_neg hb] at h
          exact (List.append_inj (hash_collision_resistant _ _ h) (by rw [hR1len, hR2len])).1
      exact ih r2 (d + 1)
        (fun s hs => hl1 s (List.mem_cons_of_mem s1 hs))
        (fun s hs => hl2 s (List.mem_cons_of_mem s2 hs))
        (by simpa using hlen) hRR

/-- `leafHash` is injective in the value (for a fixed key): distinct values give
    distinct leaves, by collision resistance. -/
theorem leafHash_inj (key value1 value2 : List (Fin 256))
    (h : leafHash key value1 = leafHash key value2) : value1 = value2 := by
  unfold leafHash at h
  by_cases h1 : value1 = [] <;> by_cases h2 : value2 = []
  · rw [h1, h2]
  · rw [if_pos h1, if_neg h2] at h
    exfalso
    have hlen := congrArg List.length (hash_collision_resistant _ _ h)
    simp only [List.length_nil, List.length_append, List.length_cons] at hlen
    omega
  · rw [if_neg h1, if_pos h2] at h
    exfalso
    have hlen := congrArg List.length (hash_collision_resistant _ _ h)
    simp only [List.length_nil, List.length_append, List.length_cons] at hlen
    omega
  · rw [if_neg h1, if_neg h2] at h
    have hpre := hash_collision_resistant _ _ h
    -- hpre : [0] ++ key ++ hash value1 = [0] ++ key ++ hash value2
    have hv : hash value1 = hash value2 := List.append_cancel_left hpre
    exact hash_collision_resistant _ _ hv

/-! ### Soundness: value binding -/

/-- **Inclusion-proof soundness (value binding).** For a fixed committed `root`
    and `key`, two proofs that both verify must carry the same `value`. Hence a
    root pins a unique value per key: forging an alternative value that still
    verifies would break `hash_collision_resistant`. (This replaces the previous
    vacuous/ill-posed `value.length ∈ {0,64}` statement.) -/
theorem inclusion_proof_sound
    (root : Hash) (key : List (Fin 256)) (p1 p2 : Proof)
    (h1 : verifyProof root key p1 = true)
    (h2 : verifyProof root key p2 = true)
    : p1.value = p2.value := by
  simp only [verifyProof, decide_eq_true_eq] at h1 h2
  obtain ⟨hlen1, hsib1, hr1⟩ := h1
  obtain ⟨hlen2, hsib2, hr2⟩ := h2
  have hleaf : leafHash key p1.value = leafHash key p2.value :=
    recompute_inj key (leafHash key p1.value) (leafHash key p2.value)
      (leafHash_length key p1.value) (leafHash_length key p2.value)
      p1.siblings p2.siblings 0 hsib1 hsib2 (by rw [hlen1, hlen2])
      (by rw [hr1, hr2])
  exact leafHash_inj key p1.value p2.value hleaf

end PSL.MPT
