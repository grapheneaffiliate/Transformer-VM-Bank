/-
  Functional model of the Sparse Merkle Tree, and the two results that were
  missing from the MPT story:

    • `inclusion_proof_complete` — an honestly-generated proof for any key
      verifies against the model root. (Soundness alone never touched the tree
      semantics; this closes the loop from the other side.)
    • `inclusion_proof_correct` — the capstone: ANY proof that verifies against
      a model root carries exactly the stored value `m key`. Follows from
      completeness + the already-proven value-binding soundness. With
      `m key = []` this *is* non-inclusion soundness (absent keys verify only
      with the empty value), so the previously-unstated
      `non_inclusion_proof_sound` falls out as a special case.
    • `smt_root_order_independent` — the state commitment depends only on the
      final key→value map, not the order writes were applied (the spec-level
      form of crypto/src/smt.rs's `put_order_independent_for_independent_keys`
      test). In this functional model the proof is by construction — which is
      the point: the *spec* is a pure function of the map. The imperative
      node-store `put` matching this spec remains an empirical (tested)
      property, per the hand-translation contract.

  Faithfulness to `crypto/src/smt.rs`:
    subtreeHash f p  = the hash of the depth-(256−f) subtree at path-prefix p:
      leaves are `leafHash key (m key)` (empty value ⇒ default empty-leaf hash,
      so an all-absent subtree reproduces the `default_hashes` progression
      D[d] = H(D[d+1] ‖ D[d+1]), D[256] = H("")), internal nodes are
      H(left ‖ right).
    siblingPath      = `compute_path_to_leaf`: sibling subtree hash per depth.
    proofFor         = `proof()`: 256 siblings + stored value (empty if absent).
-/

import PSL.MPT

namespace PSL.MPT

/-- A key→value map (the `leaves` store): 32-byte keys to value bytes;
    absent keys map to `[]` (standard SMT semantics: empty value = absent). -/
abbrev KV := List (Fin 256) → List (Fin 256)

/-! ### Bits ↔ bytes round-trip -/

/-- Reassemble a byte from its 8 MSB-first bits (weight 128 for bit 0). -/
def assembleByte (b0 b1 b2 b3 b4 b5 b6 b7 : Bool) : Nat :=
  (cond b0 128 0) + (cond b1 64 0) + (cond b2 32 0) + (cond b3 16 0) +
  (cond b4 8 0) + (cond b5 4 0) + (cond b6 2 0) + (cond b7 1 0)

private theorem cond_le_left (b : Bool) (c : Nat) : cond b c 0 ≤ c := by
  cases b <;> simp

theorem assembleByte_lt (b0 b1 b2 b3 b4 b5 b6 b7 : Bool) :
    assembleByte b0 b1 b2 b3 b4 b5 b6 b7 < 256 := by
  unfold assembleByte
  have h0 := cond_le_left b0 128; have h1 := cond_le_left b1 64
  have h2 := cond_le_left b2 32;  have h3 := cond_le_left b3 16
  have h4 := cond_le_left b4 8;   have h5 := cond_le_left b5 4
  have h6 := cond_le_left b6 2;   have h7 := cond_le_left b7 1
  omega

set_option maxRecDepth 8192 in
/-- Extracting the 8 MSB-first bits of a byte and reassembling them gives the
    byte back. Pure kernel computation over all 256 bytes. -/
theorem assembleByte_extract : ∀ b : Fin 256,
    assembleByte
      ((b.val >>> (7 - 0)) % 2 == 1) ((b.val >>> (7 - 1)) % 2 == 1)
      ((b.val >>> (7 - 2)) % 2 == 1) ((b.val >>> (7 - 3)) % 2 == 1)
      ((b.val >>> (7 - 4)) % 2 == 1) ((b.val >>> (7 - 5)) % 2 == 1)
      ((b.val >>> (7 - 6)) % 2 == 1) ((b.val >>> (7 - 7)) % 2 == 1) = b.val := by
  decide

/-- Byte `j` of a 256-bit path, mirroring `bit`'s MSB-first layout. -/
def byteOf (p : List Bool) (j : Nat) : Fin 256 :=
  ⟨assembleByte
      (p.getD (8*j + 0) false) (p.getD (8*j + 1) false)
      (p.getD (8*j + 2) false) (p.getD (8*j + 3) false)
      (p.getD (8*j + 4) false) (p.getD (8*j + 5) false)
      (p.getD (8*j + 6) false) (p.getD (8*j + 7) false),
   assembleByte_lt _ _ _ _ _ _ _ _⟩

/-- The 32-byte key whose bit pattern is the 256-bit path `p`. -/
def keyOfBits (p : List Bool) : List (Fin 256) :=
  (List.range 32).map (fun j => byteOf p j)

/-- The first `d` bits of `key`'s path (MSB-first, per `bit`). -/
def pathBits (key : List (Fin 256)) : Nat → List Bool
  | 0 => []
  | d + 1 => pathBits key d ++ [bit key d]

theorem pathBits_length (key : List (Fin 256)) : ∀ d, (pathBits key d).length = d
  | 0 => rfl
  | d + 1 => by simp [pathBits, pathBits_length key d]

theorem pathBits_getD (key : List (Fin 256)) :
    ∀ d i, i < d → (pathBits key d).getD i false = bit key i := by
  intro d
  induction d with
  | zero => intro i h; omega
  | succ d ih =>
    intro i h
    show (pathBits key d ++ [bit key d]).getD i false = bit key i
    by_cases hi : i < d
    · rw [show (pathBits key d ++ [bit key d]).getD i false = (pathBits key d).getD i false by
        simp [List.getD, List.getElem?_append_left ((pathBits_length key d).symm ▸ hi)]]
      exact ih i hi
    · have hieq : i = d := by omega
      subst hieq
      have hlen : (pathBits key i).length = i := pathBits_length key i
      simp [List.getD, List.getElem?_append_right (Nat.le_of_eq hlen), hlen]

/-- Round-trip: reconstructing a 32-byte key from its own path bits gives the
    key back. -/
theorem keyOfBits_pathBits (key : List (Fin 256)) (hk : key.length = 32) :
    keyOfBits (pathBits key 256) = key := by
  apply List.ext_getElem
  · simp [keyOfBits, hk]
  · intro j h1 h2
    have hgd : key.getD j 0 = key[j] := by
      simp [List.getD, List.getElem?_eq_getElem h2]
    have hbit : ∀ t, t < 8 →
        (pathBits key 256).getD (8*j + t) false = ((key[j].val >>> (7 - t)) % 2 == 1) := by
      intro t ht
      rw [pathBits_getD key 256 (8*j + t) (by omega)]
      unfold bit
      have hdiv : (8*j + t) / 8 = j := by omega
      have hmod : (8*j + t) % 8 = t := by omega
      rw [hdiv, hmod, hgd]
    simp only [keyOfBits, List.getElem_map, List.getElem_range]
    apply Fin.ext
    show assembleByte _ _ _ _ _ _ _ _ = key[j].val
    rw [hbit 0 (by omega), hbit 1 (by omega), hbit 2 (by omega), hbit 3 (by omega),
        hbit 4 (by omega), hbit 5 (by omega), hbit 6 (by omega), hbit 7 (by omega)]
    exact assembleByte_extract key[j]

/-! ### The functional tree -/

/-- Hash of the subtree at path-prefix `p`, `f` levels above the leaves.
    `subtreeHash m 256 []` is the root. -/
def subtreeHash (m : KV) : Nat → List Bool → Hash
  | 0, p => leafHash (keyOfBits p) (m (keyOfBits p))
  | f + 1, p => hash (subtreeHash m f (p ++ [false]) ++ subtreeHash m f (p ++ [true]))

/-- The model's state commitment: the root of the full 256-level tree. -/
def rootHash (m : KV) : Hash := subtreeHash m 256 []

theorem subtreeHash_length (m : KV) : ∀ f p, (subtreeHash m f p).length = 32
  | 0, _ => leafHash_length _ _
  | _ + 1, _ => hash_length _

/-- The sibling hashes along `key`'s path from depth `d` down (`f` of them),
    mirroring `compute_path_to_leaf`. -/
def siblingPath (m : KV) (key : List (Fin 256)) : Nat → Nat → List Hash
  | _, 0 => []
  | d, f + 1 => subtreeHash m f (pathBits key d ++ [!bit key d]) :: siblingPath m key (d + 1) f

theorem siblingPath_length (m : KV) (key : List (Fin 256)) :
    ∀ f d, (siblingPath m key d f).length = f
  | 0, _ => rfl
  | f + 1, d => by simp [siblingPath, siblingPath_length m key f (d + 1)]

theorem siblingPath_mem_length (m : KV) (key : List (Fin 256)) :
    ∀ f d s, s ∈ siblingPath m key d f → s.length = 32 := by
  intro f
  induction f with
  | zero => intro d s hs; exact absurd hs (List.not_mem_nil s)
  | succ f ih =>
    intro d s hs
    rcases List.mem_cons.mp hs with h | h
    · rw [h]; exact subtreeHash_length m f _
    · exact ih (d + 1) s h

/-- Honest proof generation, mirroring `SparseMerkleTree::proof`. -/
def proofFor (m : KV) (key : List (Fin 256)) : Proof :=
  { siblings := siblingPath m key 0 256, value := m key }

/-- The verifier's fold over the honest sibling path reconstructs exactly the
    subtree hash on `key`'s path — the structural heart of completeness. -/
theorem recompute_siblingPath (m : KV) (key : List (Fin 256)) (hk : key.length = 32) :
    ∀ f d, d + f = 256 →
      recompute key d (leafHash key (m key)) (siblingPath m key d f)
        = subtreeHash m f (pathBits key d) := by
  intro f
  induction f with
  | zero =>
    intro d hd
    have hd256 : d = 256 := by omega
    subst hd256
    show leafHash key (m key) = subtreeHash m 0 (pathBits key 256)
    show leafHash key (m key)
       = leafHash (keyOfBits (pathBits key 256)) (m (keyOfBits (pathBits key 256)))
    rw [keyOfBits_pathBits key hk]
  | succ f ih =>
    intro d hd
    show combine key d (subtreeHash m f (pathBits key d ++ [!bit key d]))
           (recompute key (d + 1) (leafHash key (m key)) (siblingPath m key (d + 1) f))
       = subtreeHash m (f + 1) (pathBits key d)
    rw [ih (d + 1) (by omega)]
    have hpath : pathBits key (d + 1) = pathBits key d ++ [bit key d] := rfl
    rw [hpath]
    unfold combine
    by_cases hb : bit key d = true
    · rw [if_pos hb, hb]
      rfl
    · have hbf : bit key d = false := Bool.eq_false_iff.mpr hb
      rw [if_neg hb, hbf]
      rfl

/-! ### Completeness and the capstone -/

/-- **Completeness:** the honestly-generated proof for any (32-byte) key
    verifies against the model root. Needs no collision-resistance — it is a
    purely structural property of the verifier's fold. -/
theorem inclusion_proof_complete (m : KV) (key : List (Fin 256)) (hk : key.length = 32) :
    verifyProof (rootHash m) key (proofFor m key) = true := by
  simp only [verifyProof, decide_eq_true_eq]
  refine ⟨siblingPath_length m key 256 0,
          fun s hs => siblingPath_mem_length m key 256 0 s hs, ?_⟩
  show recompute key 0 (leafHash key (m key)) (siblingPath m key 0 256) = rootHash m
  rw [recompute_siblingPath m key hk 256 0 (by omega)]
  rfl

/-- **Capstone — inclusion-proof correctness.** Any proof that verifies against
    the model root for `key` carries exactly the stored value `m key`. This is
    soundness (value binding) + completeness combined: the committed root pins
    down precisely the map's value at every key. Instantiated with an absent
    key (`m key = []`) this is non-inclusion soundness: only the empty value
    can verify. -/
theorem inclusion_proof_correct (m : KV) (key : List (Fin 256)) (hk : key.length = 32)
    (p : Proof) (hv : verifyProof (rootHash m) key p = true) :
    p.value = m key :=
  inclusion_proof_sound (rootHash m) key p (proofFor m key) hv
    (inclusion_proof_complete m key hk)

/-! ### Order independence (spec level) -/

/-- Point update of the key→value map. -/
def updateMap (m : KV) (k : List (Fin 256)) (v : List (Fin 256)) : KV :=
  fun k' => if k' = k then v else m k'

theorem updateMap_comm (m : KV) {k1 k2 : List (Fin 256)} (h : k1 ≠ k2)
    (v1 v2 : List (Fin 256)) :
    updateMap (updateMap m k1 v1) k2 v2 = updateMap (updateMap m k2 v2) k1 v1 := by
  funext k'
  unfold updateMap
  by_cases h1 : k' = k1
  · by_cases h2 : k' = k2
    · exact absurd (h1.symm.trans h2) h
    · rw [if_neg h2, if_pos h1, if_pos h1]
  · by_cases h2 : k' = k2
    · rw [if_pos h2, if_neg h1, if_pos h2]
    · rw [if_neg h2, if_neg h1, if_neg h1, if_neg h2]

/-- The state commitment depends only on the final map: writing two distinct
    keys in either order yields the same root. (Spec-level form of the Rust
    `put_order_independent_for_independent_keys` test; in this functional
    model the root is a pure function of the map, which is the point of the
    spec. The imperative node-store `put` agreeing with this spec remains an
    empirically-tested property.) -/
theorem smt_root_order_independent (m : KV) {k1 k2 : List (Fin 256)} (h : k1 ≠ k2)
    (v1 v2 : List (Fin 256)) :
    rootHash (updateMap (updateMap m k1 v1) k2 v2)
      = rootHash (updateMap (updateMap m k2 v2) k1 v1) :=
  congrArg rootHash (updateMap_comm m h v1 v2)

end PSL.MPT
