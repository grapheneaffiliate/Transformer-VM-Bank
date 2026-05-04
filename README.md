# Percepta Settlement Layer (PSL)

A deterministic financial ledger where state transitions are **bit-exactly
re-executable as transformer traces** — anyone holding the analytical
transformer weights and a Merkle-Patricia Trie state root can independently
verify any block. Two operating modes share the same execution layer:
**sovereign** (single sequencer, ships first) and **consortium** (BFT-ordered
via ABCI + CometBFT, v2 swap-in). Settlement rails for tokenized USD, CBDC,
gold, and treasuries; mobile light-client; ed25519 + BLAKE3 native crypto.

## Status

| Gate | Description | Status | Result | Commit |
| --- | --- | --- | --- | --- |
| **1** | Primitive bit-exact (10k vectors each) | ✅ | All 7 active primitives 10000/10000 | `9c50e3d` |
| **2** | SMT / crypto determinism | ✅ | 22/22 (`cargo test -p crypto`) | `93bae87` |
| **3** | Lean lake build | ✅ | Compiles against mathlib v4.12.0; 3 sorrys with target dates 2026-06-15 / 2026-07-15 | `113c11b` |
| **4** | Sequencer + 3 followers, 100 blocks mixed traffic | ✅ | All 4 state roots agree at every block; mutation detected | `93bae87` |
| **5** | Compliance enforcement | ✅ | 9/9 — travel-rule × 3, freeze authority × 4, view-key proofs × 2 | `b157f2f` |
| **6** | Light-client cross-verifies 1000 balances | ✅ | 8/8 — 1000-balance + 6 adversarial (tampered proof / sig / root / chain / signer) | `3d4d3e6` |
| **7** | End-to-end pilot (register → mint → transfer → burn → verify) | ✅ | Full flow; light-client verifies merchant=50 against the published 4-block chain | `dfc11e6` |
| **8** | Pure-Rust runner parity (Phase 1.5) | ⚠️ partial | Short primitives (`byte_add` 117 tok, `byte_sub` 402 tok, `mpt_emit` 3678 tok) bit-exact vs Python; gate 8.5 vector sweep 4500/4500 with 0 failures. `freeze_setup` / `freeze_apply` parity at scale is a known limitation — see below. | `b2546e8` |
| **9** | Consortium swap (ABCI + CometBFT) | ⏸ deferred | Vendor audit done (`docs/CONSENSUS_DECISION.md`); awaits federation triggers | — |

Per-gate command, output, and commit hash: `docs/STATUS.md`.

## Results — gate 1 trace lengths

The empirical lesson from gate 1: **trace length is the precision-budget
currency**, not WASM instruction count. Sequential dependencies (carry chains,
multi-step state) need sub-1k token traces; independent ops can fit larger.

| Primitive | WASM instr | Trace tokens | 10k pass |
| --- | --- | --- | --- |
| `byte_add_with_carry` | 26 | 119 | 10000/10000 ✓ |
| `byte_sub_with_borrow` | 142 | 404 | 10000/10000 ✓ |
| `transfer_check` (16-iter MSB-first compare) | 86 | 1,624 | 10000/10000 ✓ |
| `transfer_finalize` (u64 nonce inc) | 142 | 656 | 10000/10000 ✓ |
| `freeze_setup` (parse 65 → emit 2) | — | 17,566 | 10000/10000 ✓ |
| `freeze_apply` (toggle bit on binary form) | — | 7,723 | 10000/10000 ✓ |
| `mpt_emit_record` (64-byte pass-through) | 20 | 3,741 | 10000/10000 ✓ |
| Transfer end-to-end (chained 4-stage) | — | — | 10000/10000 ✓ |

Composition counts at the sequencer: freeze = 2 trace hashes per tx,
transfer = 34, mint = 16, burn = 17, multi-asset transfer = N × 34. Each
follower re-executes every primitive and verifies each output independently.

## Components

- **`primitives/`** — C source for transformer-verifiable state-transition
  primitives. Active set:
  `byte_add_with_carry`, `byte_sub_with_borrow`,
  `transfer_check`, `transfer_finalize`,
  `freeze_setup`, `freeze_apply`,
  `mpt_emit_record`. Style v3 (`docs/STYLE_GUIDE_v3.md`).
  Older monolithic primitives are in `docs/archive/primitives/`.
- **`crypto/`** — native Rust (NOT compiled through Transformer-VM):
  ed25519 signature verification, BLAKE3 hashing, Merkle-Patricia Trie
  (`crypto/src/mpt.rs`) holding the system-wide state root.
- **`sequencer/`** — Rust binary, sovereign-mode block producer. Ingests
  txs, pre-validates sigs and nonces natively, runs the transformer trace
  per primitive composition, applies deltas to the MPT, signs and publishes
  block headers.
- **`consensus/`** — Rust crate, `Consensus` trait with sovereign and
  ABCI + CometBFT implementations (per `docs/CONSENSUS_DECISION.md`).
- **`light_client/`** — Rust crate verifying balances against block
  headers via MPT inclusion proofs. Compiles to iOS / Android via UniFFI.
- **`rust_runner/`** — pure-Rust port of the Transformer-VM specialized-model
  runner. Bit-exact with the Python reference on short primitives; 2× faster
  than baseline first pass after the flat-buffer attention rewrite.
- **`lean/`** — Lean 4 + mathlib formalization of ledger semantics.
- **`pilot/issuer_demo/`** — end-to-end pilot binary.

## Architecture highlights

- **Per-byte decomposition.** A single 16-byte u128 subtract is 16
  `byte_sub_with_borrow` primitive invocations chained at the sequencer
  level, each with its own trace-hash. The monolithic in-line equivalent
  produces an ~8k-token trace and fails ~11% of randomized witnesses at
  scale; the decomposed version clears 10000/10000 across all 7 primitives.
- **Trace length as precision budget.** Empirically: <1k tokens for
  sequential dependencies; larger only for parallelisable ops. The v3
  style guide encodes the rules (avoid `i32.shr_u` / `<<` patterns that
  explode under `lower.py` expansion; use additive normalization +
  `select`).
- **Pure-Rust runner with zero PyTorch dependency on short primitives.**
  `rust_runner/` reads the same `.bin` weight format produced by
  `transformer_vm.model.weights::save_weights`, runs an identical sequential
  matmul to `transformer.cpp`'s Linux build, and matches Python+MKL bit-for-bit
  on every primitive whose `ff_out` reduction stays small.

Full design: `docs/ARCHITECTURE.md`. Style rules: `docs/STYLE_GUIDE_v3.md`.

## Build / reproduce

The canonical reproduction guide is **`REPRODUCE.md`**, with a two-tier
structure:

- **Tier 1 (~35 minutes)**: gates 2-7. Pure Rust + Lean toolchains, no
  Transformer-VM dependency. Validates SMT/crypto, Lean proofs, sequencer
  100-block run, compliance, light-client, end-to-end pilot.
- **Tier 2 (~6 hours)**: adds gate 1 bit-exact 10k-vector sweep across all
  7 primitives via the C++ engine, plus gate 8 short-primitive parity for
  the pure-Rust runner.

Pin the Transformer-VM checkout via `TRANSFORMER_VM_PATH` (the codebase no
longer hard-codes a user-specific filesystem path); the rest of the build is
portable across machines.

## Trust boundary

Sigs and hashes are verified by **native code**, not the transformer trace.
The transformer trace covers state-transition arithmetic only (debit, credit,
nonce, freeze flag, multi-asset batched transfers). Followers verify both
layers — see `docs/ARCHITECTURE.md` § 0 for the trace-hash contract and
trust model.

## Known limitations

- **Gate 8 long-primitive parity at scale.** `freeze_setup` (17k tokens)
  and `freeze_apply` (7.7k) drift from PyTorch's reference output by
  ~1e-14 starting at step 23, localized to `ff_out`'s 66×2162 matmul.
  The cause is MKL's vectorized reduction order in PyTorch's matmul
  dispatch; none of 25 SIMD-lane patterns we tried reproduce it bit-for-bit
  without linking MKL itself. Cross-engine algorithm match still holds
  against `transformer.cpp`'s Linux build (sequential matmul, same
  algorithm). Arithmetic correctness on those primitives rests on the
  original gate 1 C++ sweep at 10000/10000 each. Deferred to a v2 perf
  milestone (sparse matvec + AVX2 BLAS).
- **Three Lean `sorry`s** remain in load-bearing theorems
  (`Conservation.lean:42`, `Conservation.lean:60`, `MPT.lean:58`) within
  target close dates 2026-06-15 and 2026-07-15. Tracker:
  `docs/STATUS.md`.
- **Phase 1.5 Rust runner.** Short primitives are 2× faster than Python
  via flat-buffer attention; longer primitives (≥7k tokens) are still
  gated on attention SIMD optimization (matrixmultiply or BLAS linkage).
- **Consortium mode (gate 9).** Deferred pending production triggers;
  vendor decision documented.

## Plan

Architecture and design rules live in `docs/ARCHITECTURE.md` and
`docs/STYLE_GUIDE_v3.md`. Per-gate history, sorry tracker, and command
recipes in `docs/STATUS.md`. Empirical findings and case studies in
`docs/FINDINGS.md`.
