# PSL Architecture (living doc)

**Last refreshed: 2026-05-09 (v0.1.0 cut + Phase H docs cleanup).**

This is the executable spec for the Percepta Settlement Layer. It captures
the per-component contracts the implementation must satisfy. **The contracts
in this document are normative.** If implementation drifts, this doc is the
authority unless explicitly amended in a tagged commit.

For a top-down map of all PSL documentation, see
[INDEX.md](INDEX.md). For per-gate verification status, see
[STATUS.md](STATUS.md). For the agent execution layer (Phase 2 / gates
10-16) which is described later in this doc as § 8, see also the
[whitepaper draft](whitepaper/PSL.md).

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

### 0.2 Trace hash (canonical, ternary)

As of Phase 2 (gates 10-16 + ADR-0001), the canonical trace contract
is the ternary-integer one. Same input + same `weights_hash` →
bit-identical output on **any** conformant integer-arithmetic
verifier. No fp64 reduction-order surface, no canonical-engine pin.

```
trace_hash_ternary(P, x) := BLAKE3(
    weights_hash(P)
 || canonical_input_encoding(x)
 || canonical_output_encoding(y)
)
```

where `y = TernaryNetwork::forward(x)` for the canonical
construction of primitive `P`. `weights_hash(P)` is the BLAKE3 over
the canonical packed weights payload (`ternary_vm::weights::pack_weights`).
Canonical input/output encodings are 4-byte big-endian length prefix
followed by 8-byte big-endian per `i64`.

Reference implementation: `ternary_vm/src/trace_hash.rs`.

### 0.A Legacy trace hash (frozen, fp64 autoregressive)

Per ADR-0001, the previous fp64 token-sequence contract is frozen
and lives in the `legacy/` subtree. Documented here for verifiers
that must reproduce historical block-headers.

```
trace_hash_legacy(P, x) := BLAKE3(utf8(" ".join(trace(P, x))))
```

Where `trace(P, x)` is the predicted-token sequence (including input
prefix) the Transformer-VM specialized model produces under
greedy-argmax decoding. The canonicalized representation is the
predicted tokens joined by single ASCII spaces, with no leading or
trailing whitespace, encoded as UTF-8, hashed with BLAKE3 to a
32-byte digest. Reference implementations:
`tools/verify_trace.py` (third-party verifier) and
`legacy/rust_runner/src/generate.rs` (frozen production path).

New code MUST NOT depend on the legacy contract. Existing block
headers signed under the legacy contract remain verifiable via the
frozen `legacy/rust_runner` crate; the CI guard
`tools/ci/check_legacy_isolation.sh` enforces no new dependencies.

### 0.3 Canonical engine ordering (pinned 2026-05-04)

For determinism across implementations, PSL pins three engines in a
strict ordering. Trace_hash production for block-header signing MUST
use the canonical engine; the other two are for cross-validation.

1. **Canonical: pure-Rust runner** (`rust_runner/`,
   `target/release/psl-runner`). Reference for trace-hash production —
   every published `trace_hash` value in a block header is produced by
   this engine. Algorithm: greedy argmax decoding via sequential
   `for j: y[i] += W[i,j] * x[j]` matrix-vector reduction, no BLAS, no
   FMA, IEEE-754 fp64 throughout. Deterministic across hosts given the
   same weights and witness. Ratified by re-running the gate-1 vector
   set under this engine — see `docs/STATUS.md` gate 8.5.

2. **Secondary: Transformer-VM C++ engine**
   (`transformer_vm/model/transformer.cpp`, Linux build, `#else` branch
   in `matvec`). Algorithmically identical to the canonical engine
   (same sequential matmul) — produces bit-identical output on every
   primitive tested. Used historically for gate-1 validation; retained
   as an independent cross-check for future regression testing.

3. **Tertiary: PyTorch+MKL** (`wasm-run --python`). Useful for
   development and small-primitive fixture generation, but its CPU
   matmul dispatch goes through Intel MKL's vectorized dgemv on long
   reductions (FFN width ≥ ~1k). MKL's reduction order is
   implementation-specific and not reproducible without linking MKL
   itself. **Disagreement between PyTorch+MKL and the canonical engine
   on long primitives (`freeze_setup` / `freeze_apply`) is expected,
   documented (`docs/FINDINGS.md` § Gate 8.5), and not a correctness
   issue.** A verifier that uses PyTorch as its execution engine MUST
   reproduce the canonical engine's output, not the other way around.

`tests/test_bit_exact.py` defaults to engine `rust`; set
`PSL_VERIFY_ENGINE=cpp` to cross-validate against the secondary engine.

**Important caveat — attention algorithm.** The pure-Rust runner ports
Python's `StandardKVCache` (softmax attention, O(n) per step). The
Transformer-VM C++ engine (`transformer.cpp`) defaults to a **different
algorithm**: hull-based hard attention (O(log n) per step), per
`hull2d_cht.h` and the `HardAttentionHead` class. Hard attention is
deterministic and bit-stable across implementations; soft attention
accumulates fp64 drift that becomes argmax-flipping at long sequences
(empirically: 17.5k-token `freeze_setup` never converges to halt under
either Rust or Python soft-attention paths without MKL's reduction
order intervening). The two algorithms agree on short primitives that
the model was specialized to handle either way. For long primitives,
the canonical reference is currently the C++ engine's hard-attention
path; Rust soft-attention is a secondary algorithm useful for the
short-primitive subset until either hard attention is ported to Rust
or the analytical models are replaced by the ternary single-shot
executor (next-phase plan).

### 0.5 Why argmax-decoding is deterministic

The specialized models PSL uses are pure-integer-arithmetic in the intended
case (per the existing test_specialize tests with `StandardKVCache`), and
greedy argmax is deterministic given the weights. PSL pins
`cache_class=StandardKVCache` for sequencer + verifier paths to eliminate any
potential nondeterminism from the optional `HullKVCache` (whose float-ish
internals could in principle drift across implementations).

### 0.6 What the trace does NOT cover

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

### 0.7 Witness encoding

Each primitive's input format is space-separated decimal bytes, identical to
the encoding used by `arc_*.c` examples. See per-primitive sections below for
exact byte layout.

### 0.8 Ternary trace contract (Phase 2 — pinned 2026-05-09)

For ternary-network primitives (`ternary_vm/`), trace_hash is defined
without any token sequence:

```
trace_hash_ternary(P, x) := BLAKE3(
    weights_hash(P)
 || canonical_input_encoding(x)
 || canonical_output_encoding(y)
)
```

where `y = TernaryNetwork::forward(x)` for the canonical
construction of primitive `P`. `weights_hash(P)` is the BLAKE3 of the
canonical packed weights payload (`ternary_vm/src/weights.rs::pack_weights`).
Canonical input/output encodings are 4-byte big-endian length prefix
followed by 8-byte big-endian per `i64`.

**Why this is structurally simpler than the autoregressive § 0.2 contract:**
ternary-integer arithmetic is associative and overflow-checked. Any
conformant integer-arithmetic verifier (x86_64, aarch64, FPGA, secure
enclave, microcontroller) produces bit-identical `y` for the same `x`
and `weights_hash`. There is no canonical-engine pin, no soft-vs-hard
attention divergence, no fp64 reduction-order surface. The
`PSL_VERIFY_ENGINE=ternary` mode in `tests/test_bit_exact.py` becomes
the production verifier; alternative engines (Python, C++, the legacy
soft-attention Rust runner) are kept only for migration cross-checks.

The two § 0 contracts coexist during the migration window. Per
`docs/STATUS.md`, gate 10 closes when all 7 active primitives are in
the ternary executor; at that point § 0.2 (token-sequence trace_hash)
is marked legacy.

---

## 1. Architectural decisions (recap)

1. **Crypto outside the trace.** See § 0.4. Single trust surface for state
   transitions; native code carries authorization.
2. **Repo lives at the user's local checkout.** Self-contained as of v0.1.0;
   no `$TRANSFORMER_VM_PATH` dependency in the Tier-1 reproduction path
   (per `REPRODUCE.md`). The legacy fp64 reference engine is frozen in
   `legacy/rust_runner/` per ADR-0001.
3. **Ternary integer kernel is canonical** for the trace-hash contract
   (gates 10-16, ADR-0001). Phase 1.5 fp64 work is retired; do not extend
   the legacy crate. New work uses `ternary_vm/` and the agent layer
   (`agent_*/` crates).
4. **Lean toolchain set up from scratch** (Transformer-VM has none).
5. **Sovereign mode ships v0.1.x; BFT consensus deferred** to v0.2 with
   three concrete trigger conditions (ADR-0002).
6. **Cryptographic agility is a first-class architectural concern**
   (ADR-0007). Every signature, KEM ciphertext, and hash blob carries an
   explicit varint scheme prefix; verifiers refuse unknown schemes.
   Hybrid post-quantum is the v0.2 default per ADR-0006.

---

## 2. Pre-flight items

| ID | Item | Status |
| --- | --- | --- |
| P0 | Pin trace-hash contract (this § 0) | ✅ done 2026-05-03 |
| P1 | malachitebft-rs maturity audit (`docs/CONSENSUS_DECISION.md`) | ✅ defer; ABCI + CometBFT for MVP |
| P2 | Repo + remote backup | ✅ done 2026-05-03 |
| P3 | Port arc_common.h + active primitive set | ✅ done; gate 1 cleared 2026-05-04 |

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

## 4. Primitive design rule (gate-1 lesson, ratified 2026-05-04)

**Trace length is the precision-budget currency.** The 2000-WASM-instruction
budget in the v2 style guide is a proxy that fails on primitives with
sequential data dependencies. The real rule (full treatment in
`docs/STYLE_GUIDE_v3.md`):

- **Independent ops** (parse, byte-stream emit): can fit ~30k-token traces.
- **Sequential ops** (carry chains, hash rounds, anything where step N
  depends on step N-1): target **sub-1k token traces** per primitive.
  Decompose so each step is one cycle of the dependency.

The dominant trace-length killer is `lower.py`'s `_expand_shr_u` /
`_expand_shr_s` rewrites — clang at -O2 frequently emits `i32.shr_u 31`
to extract a sign bit, and each shift expands to ~50–100 WASM ops in
PSL's reduced ISA. **Avoid `>>` and `<<` on multi-byte intermediate
values; use additive normalization + `select` instead.** See
`docs/STYLE_GUIDE_v3.md` for the rewrite recipe and `docs/UPSTREAM_BUG_lower_py_runtime_or.md`
for the related runtime-OR bug.

### 4.1 Composition counts

The sequencer threads outputs through chained primitives, producing N
trace hashes per logical transaction. Followers re-execute all N primitives
independently and verify each output. Per-tx hash counts (committed in
`sequencer/src/trace.rs::expected_trace_hash_count`):

| Tx kind | Composition | Trace hashes |
| --- | --- | --- |
| `Freeze` | `freeze_setup` + `freeze_apply` | **2** |
| `Transfer` | `transfer_check` + 16× `byte_sub_with_borrow` + 16× `byte_add_with_carry` + `transfer_finalize` | **34** |
| `Mint` | 16× `byte_add_with_carry` | **16** |
| `Burn` | `transfer_check` + 16× `byte_sub_with_borrow` | **17** |
| `MultiAsset` (N recipients) | N × Transfer composition | **N × 34** |

The block header commits to the BLAKE3-of-concatenated trace hashes; each
follower re-derives every intermediate value by chaining the same
primitives in the same order.

### 4.2 Measurement is mandatory

For each new primitive: **measure trace length on a representative witness
before declaring it complete.** `wasm-run`'s `RAN N tok` field is the
metric. Target sub-1k for sequential ops, accept up to ~30k for
independent ops. Anything ≥100k tokens means "decompose now."

## 5. Primitive contracts

> Subsections are numbered 5.x (numbering bug present in pre-v0.1.0
> revisions of this file is fixed as of the Phase H docs cleanup).

### 5.1 `ledger_freeze.c`

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

### 5.2 `ledger_transfer.c`

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

### 5.3 `ledger_mint.c`

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

### 5.4 `ledger_burn.c`

Symmetric to mint. Debits `from`. Asserts `from.balance >= amount`.

**Instruction budget**: ≤ 400 instructions.

### 5.5 `ledger_multi_asset.c`

Batched transfer of N (default N=4 for v1; can grow if budget allows) transfer
triples in a single primitive invocation. Loops bounded by safety counter per
v2 style guide.

**Input encoding**: `n_transfers` followed by N transfer payloads.

**Instruction budget**: ≤ 1500 instructions for N=4.

### 5.6 `mpt_apply_delta.c`

Takes a list of `(account_index, account_record)` pairs and emits a structured
byte stream the native MPT layer can hash and apply. This primitive does NOT
hash; it serializes and orders. Decouples transformer-verifiable arithmetic
from crypto-heavy hashing.

**Instruction budget**: ≤ 400 instructions.

---

## 6. Block format

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

## 7. Verification gates (in order)

This is the architecture-doc summary. **`docs/STATUS.md` is the
authoritative ground-truth table** — re-verify there before relying on
any state below.

| #  | Gate                                                 | State at v0.1.0 cut |
| -- | ---                                                  | --- |
| 1  | Primitive bit-exact (10k/primitive)                  | ✅ all 7 active primitives 10000/10000 |
| 2  | SMT / crypto determinism                             | ✅ 22/22 |
| 3  | Lean lake build                                      | ✅ compiles; 3 sorrys with target close dates |
| 4  | Sequencer + 3 followers, 100 blocks                  | ✅ all roots match every block; mutation detected |
| 5  | Compliance enforcement                               | ✅ 9/9 |
| 6  | Light client cross-verifies                          | ✅ 8/8 |
| 7  | End-to-end pilot                                     | ✅ |
| 8  | Pure-Rust runner — canonical (legacy fp64 retired)   | ✅ closed via retirement per ADR-0001 |
| 9  | Consortium / BFT consensus                           | ⏸ deferred per ADR-0002 (3 trigger conditions, 60-day SLA) |
| 10 | Ternary execution engine (Phase 2 Layer 1)           | ✅ 42 baseline + 11 proptest tests |
| 11 | Contract DSL standard library (8 contracts)          | ✅ 20 tests |
| 12 | Identity & wallet (SLIP-0010 + spending policies)    | ✅ 25 tests |
| 13 | Negotiation protocol (5 messages, idempotent)        | ✅ 25 tests |
| 14 | Dispute resolution by re-execution                   | ✅ 7 adversarial scenarios |
| 15 | Reference agents (trader + service)                  | ✅ 2 binaries run end-to-end |
| 16 | SDK 0.1.0                                            | ✅ Rust canonical + Python/TypeScript bindings |
| 17 | External security audit                              | 🟢 hand-off package ready (awaits engagement) |
| 18 | Production-readiness (runbooks + DR drill)           | 🟢 stack shipped (awaits first staging drill) |
| 19 | Post-quantum cryptographic agility                   | 🟡 phase-1 infrastructure shipped (per Phase G); phases 2-6 pending |

Per-gate command, output, and commit hash: `docs/STATUS.md`.

---

## 8. Agent execution layer (Phase 2)

Gates 10-16 shipped a deterministic agent transaction layer on top of the
settlement layer. The novel property is **dispute resolution by
deterministic re-execution** — there is no human arbiter and no off-chain
oracle. Bytes match the executor's claim → dismiss the dispute. Bytes
differ → slash the executor. The mechanism is a function of `(contract
code, input)`.

Five layers, each its own crate:

| Layer                          | Crate                | What it provides |
| ---                            | ---                  | --- |
| Ternary integer execution VM   | `ternary_vm/`        | Forward kernel for ternary networks (weights ∈ {-1, 0, +1}, integer biases, ReLU). Bit-exact across machines. The trust-critical inner loop. |
| Contract DSL standard library  | `agent_contracts/`   | 8 standard contracts as `TernaryProgram` instances: transfer, swap, escrow_create/release/refund, time_locked_release, multisig_2of3, conditional_payment. |
| Identity & wallet              | `agent_wallet/`      | SLIP-0010 ed25519 hierarchical derivation, spending policies (cap-per-window + allowed contracts + allowed counterparties + expiry), revocation set with monotonicity invariant, key rotation. |
| Negotiation protocol           | `agent_protocol/`    | 5 wire messages (`Propose / Accept / Reject / CounterPropose / Execute`), `ProposalLog` state machine with idempotent replay, `resolve_dispute` re-executor. |
| SDK                            | `agent_sdk/`         | High-level `AgentSdk` runtime, in-process bus for tests, `OnChainView` trait. UniFFI / napi-rs bindings to Python and TypeScript (in `sdk-examples/`). |

Demo:

```bash
cargo run -p psl-agent-sdk --release --example trader_agent     # happy path
cargo run -p psl-agent-sdk --release --example service_agent    # dispute path
```

Full design: [whitepaper/PSL.md](whitepaper/PSL.md) and the per-layer crate
documentation. The dispute mechanism in particular is in
`agent_protocol/src/dispute.rs::resolve_dispute<P: TernaryProgram +
?Sized>`.

---

## 9. Cryptographic agility layer (Phase G)

Per ADR-0007: every signature, KEM ciphertext, and hash blob in PSL
carries an explicit varint scheme prefix. Verifiers refuse unknown
schemes with a typed error; never silent fallback. The architecture
allows new schemes to be added without hard forks.

`crypto_agility/` defines:
- `SignatureScheme` enum with reserved discriminants for `Ed25519`
  (implemented), `HybridEd25519MlDsa65` (reserved), `SlhDsa128s`
  (reserved).
- `KemSchemeId` enum for `X25519` and `HybridX25519MlKem768`.
- `HashScheme` enum for `Blake3_256` and `Blake3_512` (per ADR-0008,
  `Blake3_512` is for long-lived irrevocable commitments only).
- `Signer` / `Verifier` / `Kem` / `HashScheme_` traits.
- `VerifierPolicy` presets for transition windows.
- LEB128 varint codec.

Phase G phase 1 ships ed25519 + BLAKE3-256/512; phases 2-6 (hybrid
ML-DSA-65 / ML-KEM-768 integration, agent-layer hybrid migration) are
queued and require pulling in `pqcrypto-mldsa` / `pqcrypto-mlkem` plus an
external cryptographer review per ADR-0006 acceptance criteria.

---

## 10. Open contracts to be filled in

- `lean/PSL/MPT.lean` — proof depends on whether we use a verified
  BLAKE3 in Lean or treat hashing as an opaque collision-resistant
  function.
- `lean/PSL/Conservation.lean` — 2 of 3 outstanding sorrys; target
  close dates 2026-06-15 / 2026-07-15.
- Hybrid signature/KEM implementations per ADR-0006 phases 2-6.
- BFT consensus engine selection on first ADR-0002 trigger fire (60-day
  SLA from trigger).

(Items previously listed here that are now complete: `crypto/ed25519/`
ships via `ed25519-dalek` with SBOM in `docs/SECURITY.md`; `consensus/`
trait shipped with sovereign-mode impl + ABCI deferred per ADR-0002;
`light_client/` ships, UniFFI bindings emit per `agent_sdk/uniffi.toml`
follow-up.)
