# Percepta Settlement Layer (PSL)

A deterministic financial ledger whose execution layer is bit-exactly
re-expressible as a transformer trace, paired with a Sparse Merkle Tree
state commitment for offline-verifiable balances. Two operating modes share
the same execution layer: **sovereign** (single sequencer, ships first) and
**consortium** (BFT-ordered via ABCI + CometBFT, v2 swap-in).

The execution-layer primitives (transfer, mint, burn, freeze, multi-asset
transfer, record emission) are written in a constrained C dialect, compiled
to WASM, then compiled through the existing Transformer-VM at
`/mnt/c/Users/atchi/Transformer-VM/` to produce analytical transformer
weights. Anyone with the weights and a state commitment can verify any
block's state delta by re-running the transformer.

## Status — gates 1-4 cleared

| Gate | Status | Result |
| --- | --- | --- |
| 1. Primitive bit-exact (10k vectors each) | ✅ | All 7 active primitives at **10000/10000** |
| 2. SMT determinism (cargo test -p crypto) | ✅ | **22/22** crypto tests pass |
| 3. Lean lake build | ✅ | Compiles against mathlib v4.12.0; 3 sorrys with target dates 2026-06-15 / 2026-07-15 |
| 4. Sequencer + 3 followers, 100 blocks mixed traffic | ✅ | All 4 state roots agree at every block; mutation detected |
| 5. Compliance enforcement (view-keys, travel-rule, freeze-authority) | ⏳ | next |
| 6. Light-client cross-verifies 1000 balances | ⏳ | — |
| 7. End-to-end pilot (register → mint → transfer → burn → verify) | ⏳ | — |
| 8. Pure-Rust runner parity (Phase 1.5) | ⏳ | port estimate ~1.5 weeks (`docs/STATUS.md`) |
| 9. Consortium swap (ABCI + CometBFT) | ⏳ | v2 work; vendor decision in `docs/CONSENSUS_DECISION.md` |

See `docs/STATUS.md` for per-gate command, output, and commit hash.

## Results — gate 1 trace lengths

The empirical lesson from gate 1: **trace length is the precision-budget
currency**, not WASM instruction count. Sequential dependencies (carry
chains, multi-step state) need sub-1k token traces; independent ops can
fit larger. See `docs/STYLE_GUIDE_v3.md`.

| Primitive | WASM instr | Trace tokens | 10k pass |
| --- | --- | --- | --- |
| `byte_add_with_carry` | 26 | 119 | 10000/10000 ✓ |
| `byte_sub_with_borrow` | 142 | 404 | 10000/10000 ✓ |
| `transfer_check` (16-iter MSB-first compare) | 86 | 1,624 | 10000/10000 ✓ |
| `transfer_finalize` (u64 nonce inc) | 142 | 656 | 10000/10000 ✓ |
| `freeze_setup` (parse 65 → emit 2) | — | 17,566 | 10000/10000 ✓ |
| `freeze_apply` (toggle bit on binary form) | — | 7,723 | 10000/10000 ✓ |
| `mpt_emit_record` (64-byte pass-through) | 20 | 3,741 | 10000/10000 ✓ |

Composition counts at the sequencer: freeze = 2 trace hashes, transfer = 34,
mint = 16, burn = 17, multi-asset transfer = N × 34. Each follower
re-executes every primitive and verifies each output independently.

## Components

- **`primitives/`** — C source for transformer-verifiable state-transition
  primitives. **Style v3** (`docs/STYLE_GUIDE_v3.md`): trace length is
  the budget; avoid `i32.shr_u` / `<<` patterns that explode under
  `lower.py` expansion; use additive normalization + `select` instead.
  Active set: `byte_add_with_carry`, `byte_sub_with_borrow`,
  `transfer_check`, `transfer_finalize`, `freeze_setup`, `freeze_apply`,
  `mpt_emit_record`. Older monolithic primitives are in
  `docs/archive/primitives/`.
- **`crypto/`** — native Rust (NOT compiled through transformer-VM):
  ed25519 signature verification, BLAKE3 hashing, **Sparse Merkle Tree**
  (`crypto/src/smt.rs`) holding the system-wide state root.
- **`sequencer/`** — Rust binary, sovereign-mode block producer. Ingests
  txs, pre-validates sigs and nonces natively, runs the transformer
  trace per primitive composition, applies deltas to the SMT, signs and
  publishes block headers. Integration test (`sequencer/tests/integration.rs`)
  drives sequencer + 3 followers through 100 blocks of mixed traffic.
- **`consensus/`** — Rust crate, `Consensus` trait with sovereign and
  ABCI/CometBFT (per `docs/CONSENSUS_DECISION.md`) implementations.
- **`light_client/`** — Rust crate verifying balances against block
  headers via SMT inclusion proofs. Compiles to iOS / Android via UniFFI.
- **`lean/`** — Lean 4 + mathlib formalization of ledger semantics.
  Theorems: conservation, supply changes only via authorized mint/burn,
  determinism, MPT inclusion-proof soundness. Build cleared gate 3;
  three `sorry`s remain on load-bearing theorems (Conservation:42,
  Conservation:60, MPT:58) with target close dates 2026-06-15 / 2026-07-15.
- **`tests/`** — bit-exact comparison harness, SMT randomized tests,
  sequencer integration, compliance.
- **`pilot/issuer_demo/`** — end-to-end pilot: register issuer, mint an
  asset, transfer through accounts, burn, light-client-verify every
  balance.

## Trust boundary

Sigs and hashes are verified by **native code**, not the transformer
trace. The transformer trace covers state-transition arithmetic only
(debit, credit, nonce, freeze flag, multi-asset batched transfers).
Followers verify both layers — see `docs/ARCHITECTURE.md` § 0 for the
trace-hash contract and trust model.

## Architecture: per-byte decomposition

PSL's primitives are not the natural-language operations a banker would
recognize ("transfer A→B amount X"). They are **per-byte sub-operations**
chained at the sequencer level. A single 16-byte u128 subtract becomes
16 `byte_sub_with_borrow` primitive invocations, each with its own
trace-hash, each individually verified by every follower.

The reason: a single primitive that does u128 subtraction inline produces
an ~8k-token trace under the constrained C → WASM → transformer pipeline,
and accumulates enough precision drift to fail ~11% of randomized
witnesses at scale. The same logic decomposed into per-byte primitives
gives 119–404 token traces each, all of which clear 10000/10000.

This is documented in `docs/STYLE_GUIDE_v3.md` (the v3 style guide
supersedes the v2 advice in `Transformer-VM/transformer_vm/examples/arc_common.h`
for any primitive operating on multi-byte values), and the empirical
case study with measurements is in `docs/FINDINGS.md`.

## Build / test

```bash
# One-time setup
export TRANSFORMER_VM_PATH=/mnt/c/Users/atchi/Transformer-VM
uv sync
cargo build --workspace
(cd lean && lake build)

# Compile and specialize a primitive (example: freeze decomposition)
./tools/compile.sh primitives/freeze_setup.c
./tools/specialize.sh data/freeze_setup.txt
./tools/compile.sh primitives/freeze_apply.c
./tools/specialize.sh data/freeze_apply.txt

# Or build everything
./tools/build_all_primitives.sh

# Bit-exact verification (gate 1)
uv run pytest tests/test_bit_exact.py -v

# SMT determinism + crypto suite (gate 2)
cargo test -p crypto

# Lean proofs (gate 3)
(cd lean && lake build)

# Sequencer end-to-end (gate 4)
cargo test -p psl-sequencer --test integration

# Compliance (gate 5)
uv run pytest tests/test_compliance.py -v

# Light-client cross-verification (gate 6)
cargo test -p light_client

# Pilot (gate 7)
cargo run --bin issuer_demo -- --full-flow
```

## Plan / architecture

Architecture lives in `docs/ARCHITECTURE.md` (§ 0 trace contract,
§ 4 decomposition rule, § 5 primitive contracts, § 6 gate status).
History: `CHANGELOG.md`. Sub-docs: `docs/STATUS.md` (gate-by-gate
results), `docs/FINDINGS.md` (empirical lessons), `docs/STYLE_GUIDE_v3.md`
(v3 trace-length style guide), `docs/CONSENSUS_DECISION.md` (vendor
audit), `docs/SECURITY.md`, `docs/COMPLIANCE.md`.
