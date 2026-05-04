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
| 8 | Pure-Rust runner parity (Phase 1.5) | port runner.py → Rust + bit-exact verify | ⏳ port itself is the work | — |
| 9 | Consortium swap (ABCI + CometBFT) | 4-node cluster liveness + consistency under failure | ⏳ v2 | — |

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
| `PSL/Conservation.lean:42` | `transfer_balance_delta_sums_to_zero` | The conservation result other theorems unfold to. Without this the entire conservation chain is hand-waved. | 2026-06-15 |
| `PSL/Conservation.lean:60` | `transfer_conserves` | Auditor-facing claim that transfers preserve total supply per asset. Closes immediately once the helper above closes. | 2026-06-15 |
| `PSL/Conservation.lean` | `freeze_conserves` | freeze must not change any balance — a regulator would expect this. | 2026-06-22 |
| `PSL/Conservation.lean` | `supply_changes_only_via_authority` | The compliance-facing invariant that mint/burn are the only supply-altering operations. | 2026-07-01 |
| `PSL/MPT.lean:58` | `inclusion_proof_sound` | Phone-side balance verification soundness. Currently conditioned on `hash_collision_resistant` axiom (fine); the verifier-folding step itself is unproven. | 2026-07-15 |

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
