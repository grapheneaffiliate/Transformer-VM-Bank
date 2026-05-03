# PSL Architecture (living doc)

This is the executable spec for the Percepta Settlement Layer. It mirrors the
approved plan at `/home/username/.claude/plans/cheeky-wandering-treehouse.md`
and adds the per-component contracts the implementation must satisfy. **The
contracts in this document are normative.** If implementation drifts, this doc
is the authority unless explicitly amended in a tagged commit.

---

## 0. Trace contract (load-bearing — pinned 2026-05-03)

This section was written *after* reading `Transformer-VM/transformer_vm/runner.py`
and `Transformer-VM/transformer_vm/evaluator.py`. The earlier plan-level
assumption that the trace is "BLAKE3 over the transformer's hidden-state
sequence" was wrong: Transformer-VM does not commit to hidden states at all.

### 0.1 What the trace is

For a given primitive `P` (e.g. `ledger_transfer.c`) compiled to specialized
weights `W_P`, and an input `x` (a witness — pre-validated transaction +
relevant account records, encoded as a token prefix), the trace is:

```
trace(P, x) := the token sequence the specialized model produces
               by greedy argmax-decoding starting from x
               until it emits the `halt` token,
               INCLUDING the input prefix tokens.
```

This matches the contract used today by both `wasm-eval` (graph evaluator at
`evaluator.py:run_program`) and `wasm-run` (transformer runner at
`runner.py:run_model_program`). Bit-exact correctness is whole-sequence
equality of the predicted token list, identical to the assertion at
`evaluator.py:339` and `test_specialize.py:325`.

### 0.2 Trace hash

PSL defines (because Transformer-VM does not):

```
trace_hash(P, x) := BLAKE3(utf8(" ".join(trace(P, x))))
```

Specifically: the canonicalized representation is the predicted tokens joined
by single ASCII spaces, with no leading or trailing whitespace, encoded as
UTF-8, hashed with BLAKE3 to a 32-byte digest. The reference implementation is
`tools/verify_trace.py` (third-party verifier) and `sequencer/src/trace.rs::hash_trace`
(production path).

### 0.3 Why argmax-decoding is deterministic

The specialized models PSL uses are pure-integer-arithmetic in the intended
case (per the existing test_specialize tests with `StandardKVCache`), and
greedy argmax is deterministic given the weights. PSL pins
`cache_class=StandardKVCache` for sequencer + verifier paths to eliminate any
potential nondeterminism from the optional `HullKVCache` (whose float-ish
internals could in principle drift across implementations).

### 0.4 What the trace does NOT cover

Out of scope for the transformer trace; verified separately by native code:

- Ed25519 signature verification on submitted transactions.
- Hash computations: SHA-256, BLAKE3, MPT root recomputation.
- Issuer-registry authority lookups (sequencer asserts `tx.signer_authority`
  is registered for `tx.asset_id` before the trace runs).
- Block-header signing.
- Network ordering / consensus.

A follower verifying a block performs **two** checks — re-runs the trace on
each tx's witness to verify state-transition arithmetic, AND re-verifies sigs,
hashes, and authority lookups natively. Both must pass.

### 0.5 Witness encoding

Each primitive's input format is space-separated decimal bytes, identical to
the encoding used by `arc_*.c` examples. See per-primitive sections below for
exact byte layout.

---

## 1. Architectural decisions (recap)

1. **Crypto outside the trace.** See § 0.4. Single trust surface for state
   transitions; native code carries authorization.
2. **Repo at `/mnt/c/Users/atchi/Transformer_VM_Bank/`**, depending on
   Transformer-VM via `$TRANSFORMER_VM_PATH`.
3. **PyO3 → Rust runner port is Phase 1.5, not deferred.** Sovereign pilot
   ships on PyO3; no production issuer onboards before the Rust runner exists.
4. **Lean toolchain set up from scratch** (Transformer-VM has none).

---

## 2. Pre-flight items

| ID | Item | Status |
| --- | --- | --- |
| P0 | Pin trace-hash contract (this § 0) | ✅ done 2026-05-03 |
| P1 | malachitebft-rs maturity audit | ⏳ in flight |
| P2 | Repo + remote backup | ✅ done 2026-05-03 |
| P3 | Port arc_common.h, write ledger_freeze.c | ⏳ next |

---

## 3. Account model

```c
typedef struct {
    char pubkey[32];        // ed25519 public key
    char balance[16];       // u128 little-endian
    char nonce[8];          // u64
    char last_active[8];    // u64 epoch
    char asset_id[4];       // u32, references issuer_registry
    char flags[4];          // bit 0 = frozen, bits 1-31 reserved
    // 64 bytes total
} account_t;
```

Each transformer-trace primitive operates on a *witness slice* — the affected
accounts only — to stay under the 2000-WASM-instruction precision budget. The
sequencer assembles witnesses from the live Merkle-Patricia trie.

## 4. Primitive contracts

### 4.1 `ledger_freeze.c`

**Input encoding** (space-separated decimal bytes):
```
flag_value account_byte_0 account_byte_1 ... account_byte_63
```
- `flag_value` ∈ {0, 1} — 1 to set freeze, 0 to unset.
- 64 account bytes encode an `account_t`.

**Authorization (native, before trace runs)**: sequencer verifies that the
freeze tx is signed by the asset's issuer authority and includes a valid
`court_order_hash`.

**Output encoding**:
```
account_byte_0 account_byte_1 ... account_byte_63
```
The output `account_t` is identical to input except byte 60 (low byte of
`flags`) has bit 0 set or cleared per `flag_value`.

**Instruction budget**: ≤ 200 WASM instructions.

### 4.2 `ledger_transfer.c`

**Input encoding**:
```
from_byte_0 ... from_byte_63 to_byte_0 ... to_byte_63 amount_byte_0 ... amount_byte_15 asset_id_byte_0 ... asset_id_byte_3
```
148 bytes total.

**Asserted invariants** (primitive halts with error tokens if violated):
- `from.asset_id == to.asset_id == asset_id_arg`
- `from.flags & 0x01 == 0` (not frozen)
- `from.balance >= amount` (u128 comparison)

**Output encoding**:
```
from_byte_0' ... from_byte_63' to_byte_0' ... to_byte_63'
```
Two 64-byte account records. `from'`: balance debited by amount, nonce
incremented, last_active updated. `to'`: balance credited by amount,
last_active updated.

**Instruction budget**: ≤ 600 WASM instructions (u128 arithmetic dominates).

### 4.3 `ledger_mint.c`

**Input encoding**:
```
to_byte_0 ... to_byte_63 amount_byte_0 ... amount_byte_15 asset_id_byte_0 ... asset_id_byte_3
```
84 bytes total.

**Authorization (native)**: sequencer verifies signer's authority via
`issuer_registry[asset_id].authority_pubkey == signer.pubkey` and
`issuer_registry[asset_id].mint_enabled == true`.

**Output encoding**:
```
to_byte_0' ... to_byte_63'
```
64-byte updated account record.

**Instruction budget**: ≤ 400 instructions.

### 4.4 `ledger_burn.c`

Symmetric to mint. Debits `from`. Asserts `from.balance >= amount`.

**Instruction budget**: ≤ 400 instructions.

### 4.5 `ledger_multi_asset.c`

Batched transfer of N (default N=4 for v1; can grow if budget allows) transfer
triples in a single primitive invocation. Loops bounded by safety counter per
v2 style guide.

**Input encoding**: `n_transfers` followed by N transfer payloads.

**Instruction budget**: ≤ 1500 instructions for N=4.

### 4.6 `mpt_apply_delta.c`

Takes a list of `(account_index, account_record)` pairs and emits a structured
byte stream the native MPT layer can hash and apply. This primitive does NOT
hash; it serializes and orders. Decouples transformer-verifiable arithmetic
from crypto-heavy hashing.

**Instruction budget**: ≤ 400 instructions.

---

## 5. Block format

```rust
struct BlockHeader {
    block_n: u64,
    parent_hash: [u8; 32],          // BLAKE3 of parent header
    prev_state_root: [u8; 32],      // MPT root before this block
    tx_list_hash: [u8; 32],         // BLAKE3 of canonical-encoded tx list
    trace_hash: [u8; 32],           // BLAKE3 of concatenated per-tx trace_hashes
    new_state_root: [u8; 32],       // MPT root after applying this block
    issuer_registry_root: [u8; 32], // MPT root of the registry subtree
    timestamp: u64,                  // milliseconds since epoch
    sequencer_pubkey: [u8; 32],     // (sovereign mode) or set of validators (consortium)
    sequencer_sig: [u8; 64],        // ed25519 signature over all preceding fields
}
```

Block body: list of `(SignedTx, witness_pre, witness_post)`. Followers
re-derive `witness_post` from `witness_pre + tx` via the trace and assert
`witness_post` matches the published value.

---

## 6. Verification gates (in order)

1. **Primitive bit-exact** — 10k vectors per primitive, native vs. specialized. Pass = 10k/10k.
2. **MPT determinism** — 100k randomized put operations; identical roots regardless of insertion order of independent keys.
3. **Lean proofs build** — `lake build` succeeds, zero `sorry` in conservation, supply, determinism, MPT theorems.
4. **Sovereign sequencer end-to-end** — sequencer + 3 followers, 100 blocks of mixed traffic, all state roots agree. Adversarial mutation detected.
5. **Compliance enforcement** — view-keys, travel-rule, freeze-authority all behave per spec.
6. **Light client cross-verifies** — 1000 random balance proofs verified; tampered proofs/headers rejected.
7. **End-to-end pilot** — `pilot/issuer_demo --full-flow` completes register → mint → transfer → burn → verify with light-client confirmation.
8. **Pure-Rust runner parity (Phase 1.5)** — Rust runner bit-exact-identical to Python runner on the gate-1 vectors; ≥10× throughput.
9. **Consortium swap** — replace sovereign block production with BFT (per the P1 audit), 4-node test cluster passes liveness + consistency under one-node failure.

---

## 7. Open contracts to be filled in

- `crypto/ed25519/` — vendor source, pin upstream commit, document SBOM in `docs/SECURITY.md`.
- `consensus/src/bft.rs` — concrete adapter selected per P1 audit verdict.
- `light_client/uniffi/` — UniFFI bindings finalized after Rust API stabilizes.
- `lean/PSL/MPT.lean` — proof depends on whether we use a verified BLAKE3 in Lean or treat hashing as an opaque collision-resistant function.
