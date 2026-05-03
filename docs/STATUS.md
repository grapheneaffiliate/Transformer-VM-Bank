# PSL Build Status

**Honest framing**: scaffolding is complete; verification phase begins. Code
exists for every layer in the plan, but until gate 1 (10k/10k bit-exact per
primitive) passes against the real Transformer-VM, every layer above it is
built on an unverified assumption.

## Bootstrap requirements

Before any verification gate can run:

1. **Transformer-VM `.venv` must be synced.** `uv sync` in
   `/mnt/c/Users/atchi/Transformer-VM/` requires network access to download
   `torch==2.10.0` (~3 GB).
2. **WASI clang** at `/mnt/c/Users/atchi/wasi-sdk/bin/clang.exe` — confirmed
   present.
3. **Rust toolchain** — install via `rustup`. `cargo build --workspace`
   then `cargo test --workspace`.
4. **Lean toolchain** — install `elan`; `lean-toolchain` pin selects v4.12.0.
   `lake build` from `lean/`.
5. **Python test deps** — `uv sync` at PSL repo root.

## Gate status

| Gate | Scaffolded | Run? | Pass? |
| --- | --- | --- | --- |
| P0: trace-hash contract pinned | ✅ ARCHITECTURE.md § 0 | ✅ | pinned |
| P1: malachite audit | ✅ CONSENSUS_DECISION.md | ✅ | defer; ABCI + CometBFT |
| P2: repo + remote backup | ✅ | ✅ | live on origin/main |
| **1: bit-exact (10k/primitive)** | ✅ harness | ⚠️ partial: 1/3 freeze witnesses pass | **see docs/FINDINGS.md — decomposition needed** |
| 2: MPT determinism | ✅ crypto/tests/randomized.rs | ⏳ blocked on cargo | — |
| 3: Lean proofs (no `sorry`) | ✅ skeleton | ⏳ blocked on elan | **partial — see Lean tracker below** |
| 4: sequencer + 3 followers, 100 blocks | ✅ sequencer/tests/integration.rs | ⏳ | — |
| 5: compliance enforcement | ✅ mempool + compliance.rs | ⏳ | — |
| 6: light client cross-verifies | ✅ light_client/src/lib.rs test | ⏳ | — |
| 7: pilot e2e | ✅ pilot/issuer_demo | ⏳ | — |
| 8: pure-Rust runner parity | ✅ skeleton; **see runner port estimate below** | ⏳ port itself is the work | — |
| 9: consortium swap | ✅ scaffold per P1 verdict | ⏳ ABCI integration is v2 work | — |

## Priority order (per architectural review)

1. **Gate 1 first.** Run `./tools/build_all_primitives.sh && uv run pytest
   tests/test_bit_exact.py` after `uv sync` completes. Order:
   `ledger_freeze.c` (smallest, ~150 instr) → `ledger_transfer.c`
   (representative, ~600 instr) → others. If freeze and transfer pass clean,
   the architecture is validated; if either fails the precision envelope
   (>2000 WASM instructions for the C source), decompose before continuing.

2. **Lean `sorry` work** — the deepest theorems, not the skeletal models.
   See tracker below.

3. **rust_runner port estimate** — read `runner.py` + `transformer.py` +
   `standard_cache.py`, count surface area, set the trigger criteria. See
   below.

---

## Lean `sorry` tracker

Each open `sorry` in a load-bearing theorem with a target close date.
"Load-bearing" = a theorem we'd cite to prove safety or compliance.

| File | Theorem | Why it's load-bearing | Target close |
| --- | --- | --- | --- |
| `PSL/Conservation.lean` | `transfer_balance_delta_sums_to_zero` | The conservation result other theorems unfold to. Without this the entire conservation chain is hand-waved. | 2026-06-15 |
| `PSL/Conservation.lean` | `transfer_conserves` | Auditor-facing claim that transfers preserve total supply per asset. | 2026-06-15 (closes immediately once the helper above closes) |
| `PSL/Conservation.lean` | `freeze_conserves` | freeze must not change any balance — a regulator would expect this. | 2026-06-22 |
| `PSL/Conservation.lean` | `supply_changes_only_via_authority` | The compliance-facing invariant that mint/burn are the only supply-altering operations. Currently has placeholder returns that satisfy the type but don't derive contradictions. | 2026-07-01 |
| `PSL/MPT.lean` | `inclusion_proof_sound` | Phone-side balance verification soundness. Currently conditioned on `hash_collision_resistant` axiom, which is fine, but the verifier-folding step itself is unproven. | 2026-07-15 |

Notes:
- **Determinism** theorems (`PSL/Determinism.lean`) are by-construction
  trivial because Lean functions are deterministic; the operational
  determinism between Lean and C/WASM is checked empirically by gate 1.
- **MPT.lean** uses `opaque` modeling for hash; that's the standard approach
  and not a `sorry` problem. The unfinished work is the `verifyProof` body
  and the soundness theorem's step-folding.
- The `tools/check_lean_drift.py` checker exists but is not wired into CI;
  add a pre-commit hook before any production deployment to prevent silent
  C/Lean divergence.

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
