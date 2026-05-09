# PSL Build Status

**Honest framing**: gates 1-4 cleared. Compliance / light-client / pilot
gates (5-7) are next. Phase 1.5 work (gate 8: pure-Rust runner) and v2
consortium-mode swap (gate 9) are scoped but not started.

## Bootstrap requirements

Before any verification gate can run:

1. **Transformer-VM `.venv` synced.** `uv sync` in
   `/mnt/c/Users/atchi/Transformer-VM/` requires network access to download
   `torch==2.10.0` (~3 GB).
2. **WASI clang** at `/mnt/c/Users/atchi/wasi-sdk/bin/clang.exe` — confirmed
   present.
3. **Rust toolchain** — install via `rustup`. `cargo build --workspace`
   then `cargo test --workspace`.
4. **Lean toolchain** — `elan` installed at `~/.elan/`. `lean-toolchain`
   pin selects v4.12.0. `lake build` from `lean/`. Mathlib pulled via
   precompiled cache (`lake update` triggers `cache get` post-hook).
5. **Python test deps** — `uv sync` at PSL repo root.

## Gate status

| # | Gate | Command | Result | Commit |
| --- | --- | --- | --- | --- |
| P0 | Trace-hash contract pinned | `docs/ARCHITECTURE.md § 0` | ✅ pinned 2026-05-03 | — |
| P1 | Consensus vendor audit | `docs/CONSENSUS_DECISION.md` | ✅ defer malachite; ABCI + CometBFT for MVP | — |
| P2 | Repo + remote backup | github.com/grapheneaffiliate/Transformer-VM-Bank | ✅ live | — |
| **1** | Bit-exact 10k/primitive | `tools/run_per_byte_10k.py` + `run_freeze_decomposed.py` | ✅ 10000/10000 on all 7 active primitives | `9c50e3d` |
| **2** | SMT determinism | `cargo test -p crypto` | ✅ 22/22 (incl. 100k randomized put + non-inclusion proofs) | `93bae87` |
| **3** | Lean lake build | `cd lean && lake build` | ✅ compiles against mathlib v4.12.0; 3 sorrys remain (target dates below) | `113c11b` |
| **4** | Sequencer + 3 followers, 100 blocks | `cargo test -p psl-sequencer --test integration` | ✅ 2/2; all 4 state roots match every block; mutation detected | `93bae87` |
| **5** | Compliance enforcement | `cargo test -p psl-sequencer --test compliance` | ✅ 9/9 (travel-rule × 3, freeze authority × 4, view-key proofs × 2) | _(this commit)_ |
| **6** | Light client cross-verifies 1000 balances | `cargo test -p psl-light-client` | ✅ 8/8 (1000-balance cross-verify + 6 adversarial: tampered proof value, tampered siblings, bad sig, tampered root, wrong signer, broken chain) | _(this commit)_ |
| **7** | Pilot end-to-end | `cargo run --bin issuer_demo -- --full-flow` | ✅ register issuer → mint 1M → xfer 100 → xfer 50 → burn 100; light-client verifies merchant=50 against the 4-block chain rooted at empty-SMT genesis | _(this commit)_ |
| **8** | Pure-Rust runner — canonical engine | `cargo test -p psl-rust-runner --test parity --release -- --ignored` + `tools/run_canonical_gate1.sh` | ⚠️ partial — Rust runner is the canonical reference for **soft-attention trace-hash production** (`docs/ARCHITECTURE.md § 0.3`). Validated against gate-1 vectors at 10k each on the 5 short primitives: `byte_add` 10000/10000 (37.9s), `byte_sub` 10000/10000 (470s), `transfer_finalize` 10000/10000 (1184s), `transfer_check` 10000/10000 (3114s), `mpt_emit_record` 10000/10000 (4961s) — **50000/50000, 0 failures**. `freeze_setup` and `freeze_apply` are not re-validatable under the current Rust runner: `transformer.cpp` defaults to **hard attention** (hull-based, O(log n)), our Rust ports Python's `StandardKVCache` (soft attention, O(n)). On 17.5k-token sequences soft attention's fp64 accumulation drift is large enough that the model never produces a halt token in either Python or Rust without MKL — confirmed locally (canonical input runs to max_new without halt). The original gate-1 10000/10000 results on `freeze_*` came from `wasm-run` default (hard attention). Closing this row at 100% requires either (a) porting hard attention to Rust, or (b) the ternary single-shot executor (per next-phase plan). | _(this commit)_ |
| **8.5** | Load-bearing arithmetic correctness via Rust runner | `cargo run --release --bin run_gate1 -- --primitive <p> --count <n>` + Runpod 32-vCPU scale-out | ✅ 50000/50000 vectors, 0 failures across 5 short primitives. Runpod sweep on EPYC 7702P 32-vCPU pod completed `mpt_emit_record` 10k/10k in 4961s; the four other short primitives' 10k results were already in tree from the local sweep. Logs in `docs/gate85_logs/canonical/`. | `50000/50000 0 fail` |
| 9 | Consortium swap (ABCI + CometBFT) | 4-node cluster liveness + consistency under failure | ⏳ v2 | — |
| **10** | Ternary execution engine — Phase 2 Layer 1 | `cargo test -p psl-ternary-vm --release` | ✅ all 7 active primitives in the ternary executor, 42/42 tests passing. **Exhaustive verifications (every input combination)**: `byte_add_with_carry` 131072/131072 (1.25s), `byte_sub_with_borrow` 131072/131072 (1.12s), `freeze_apply` 512/512. **Large random sweeps (≥1000 witnesses)**: `freeze_setup` 1000/1000, `transfer_finalize` 1000/1000, `mpt_emit_record` 1000/1000, `transfer_check` 500/500. byte_add throughput **105k vec/s** single-threaded (≥100k plan bar met). Crate `ternary_vm/`: `SparseTernaryLayer`, `TernaryNetwork`, BLAKE3-hashed packed weight format, `trace_hash_ternary` (`docs/ARCHITECTURE.md § 0.8`), checked-arithmetic forward kernel (no production-path panics). Two primitives (`transfer_finalize`, `transfer_check`) are ternary *programs* — they compose `byte_add` / `byte_sub` 8/16 times respectively. | `481d58c` |
| **11** | Contract DSL standard library — Phase 2 Layer 2 | `cargo test -p psl-agent-contracts --release` | 🟡 in progress — `agent_contracts/` crate scaffolded with `TernaryProgram` trait + `program_hash` (BLAKE3 over name + sub-network weights_hashes). **First standard contract `transfer` shipped**: composes `transfer_check`, `byte_sub` ×16, `byte_add` ×16, `transfer_finalize`. 5/5 tests pass including 100 random u128 witnesses (insufficient balance → canonical no-op zeros; recipient overflow → zeros; otherwise correct arithmetic + nonce++). Remaining 7 contracts (swap, escrow_create / release / refund, time_locked_release, multisig_2of3, conditional_payment) follow the same composition pattern — landing in subsequent commits. Parsed-DSL frontend (typechecker / interpreter / compiler) is the second half of this gate. | _(this commit)_ |

## Gate 1 — primitive trace-length results

| Primitive | WASM instr | Trace tokens | 10k pass |
| --- | --- | --- | --- |
| `byte_add_with_carry` | 26 | 119 | 10000/10000 ✓ |
| `byte_sub_with_borrow` | 142 | 404 | 10000/10000 ✓ |
| `transfer_check` | 86 | 1,624 | 10000/10000 ✓ |
| `transfer_finalize` | 142 | 656 | 10000/10000 ✓ |
| `freeze_setup` | — | 17,566 | 10000/10000 ✓ |
| `freeze_apply` | — | 7,723 | 10000/10000 ✓ |
| `mpt_emit_record` | 20 | 3,741 | 10000/10000 ✓ |

Composition: freeze = 2 trace hashes per tx, transfer = 34, mint = 16,
burn = 17, multi-asset = N × 34.

## Lean `sorry` tracker

Each open `sorry` in a load-bearing theorem with a target close date.
"Load-bearing" = a theorem we'd cite to prove safety or compliance.

| File | Theorem | Why it's load-bearing | Target close |
| --- | --- | --- | --- |
| `PSL/Conservation.lean:30` (sorry at :42) | `transfer_balance_delta_sums_to_zero` | The conservation result other theorems unfold to. Without this the entire conservation chain is hand-waved. | 2026-06-15 |
| `PSL/Conservation.lean:54` (sorry at :60) | `transfer_conserves` | Auditor-facing claim that transfers preserve total supply per asset. Closes immediately once the helper above closes. | 2026-06-15 |
| `PSL/Conservation.lean` | `freeze_conserves` | freeze must not change any balance — a regulator would expect this. | 2026-06-22 |
| `PSL/Conservation.lean` | `supply_changes_only_via_authority` | The compliance-facing invariant that mint/burn are the only supply-altering operations. | 2026-07-01 |
| `PSL/MPT.lean:49` (sorry at :58) | `inclusion_proof_sound` | Phone-side balance verification soundness. Currently conditioned on `hash_collision_resistant` axiom (fine); the verifier-folding step itself is unproven. | 2026-07-15 |

Notes:
- **Determinism** theorems (`PSL/Determinism.lean`) are by-construction
  trivial because Lean functions are deterministic; the operational
  determinism between Lean and C/WASM is checked empirically by gate 1.
- **MPT.lean** uses `opaque` modeling for hash; that's the standard
  approach and not a `sorry` problem. The unfinished work is the
  `verifyProof` body and the soundness theorem's step-folding.
- The `tools/check_lean_drift.py` checker exists but is not wired into
  CI; add a pre-commit hook before any production deployment to prevent
  silent C/Lean divergence.

If any of these dates slip, document the slip *in this table*, not in a
side conversation. Permanent `sorry`s are silent technical debt.

---

## Phase 1.5: rust_runner port estimate

Read of `Transformer-VM/transformer_vm/runner.py`,
`model/transformer.py`, `attention/standard_cache.py`, plus the surface
area of `model/weights.py` (31 KB) and `model/transformer.cpp` (28 KB).

### Surface to port

| File | Lines | Complexity | Rust port |
| --- | --- | --- | --- |
| `model/transformer.py` | 86 | Low — straightforward forward pass with multi-head attention, ReGLU FFN, position encoding | `rust_runner/src/transformer.rs` |
| `attention/standard_cache.py` | 34 | Low — softmax KV cache with stacked tensors | `rust_runner/src/attention.rs` |
| `model/weights.py` | ~700 lines (31 KB) | Medium — packed binary format with per-layer FFN width variability | `rust_runner/src/weights.rs` |
| `runner.py` (run_model_program) | ~60 | Low — file I/O + comparison loop | `rust_runner/src/generate.rs` |

NOT ported: `model/transformer.cpp` (production C++ engine — for the v1
sovereign pilot, the Python generate path is acceptable; the C++ engine is
a perf optimization that's out of scope for the bit-exact contract).
NOT ported: `attention/hull_cache.py` and `hull_ext.cpp` — PSL pins
StandardKVCache only (see ARCHITECTURE.md § 0.3).

### Effort estimate

| Task | Days |
| --- | --- |
| Reverse `weights.py` save/load format → Rust struct + serde reader | 1.5 |
| Port forward pass (RMSNorm-free, ReGLU, multi-head attn, head argmax) | 2 |
| Port StandardKVCache (softmax + stacked-tensor reduction) | 0.5 |
| Port `generate_with_cache` greedy-argmax loop | 0.5 |
| Bit-exact / argmax-stable verification on Transformer-VM hello/collatz fixtures | 1 |
| Bit-exact / argmax-stable verification on PSL gate-1 vectors | 0.5 |
| **Total** | **~6 working days = 1.5 weeks for one engineer** |

Confidence: **medium-high**. Main risk identified during the read:
**softmax float-determinism**. PyTorch's softmax (StandardKVCache uses
`F.softmax`) goes through BLAS and may reorder fp64 summation; a
hand-rolled Rust softmax with explicit summation order may produce slightly
different floats. The argmax over the head logits *should* be stable in
practice (the analytically-constructed weights produce well-separated
logits) but this is empirical, not guaranteed. Mitigation: pin Rust
summation order to match Python's reference implementation byte-for-byte
where possible; gate 8 (`tests/test_runner_parity.rs`) is the empirical
check.

### Trigger for actually doing the port

Per the plan, the trigger is "any pilot pushing throughput ≥100 TPS." A
sovereign pilot at <50 TPS can ride PyO3/subprocess to wasm-run. The
moment a real issuer wants to onboard production traffic, this 1.5-week
estimate becomes the gate between "we have the design" and "we can ship."
Surface this to the pilot operator at the time the issuer commitment
is signed — do not wait for the throughput problem to manifest.
