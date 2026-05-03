# Percepta Settlement Layer (PSL)

A deterministic financial ledger whose execution layer is bit-exactly
re-expressible as a transformer trace, paired with a Merkle-Patricia state
commitment for offline-verifiable balances. Two operating modes share the same
execution layer: sovereign (single sequencer, ships first) and consortium
(BFT-ordered, ~50–150 validators, v2 swap-in).

The execution-layer primitives (transfer, mint, burn, freeze, multi-asset
transfer, MPT delta application) are written in a constrained C dialect,
compiled to WASM, then compiled through the existing Transformer-VM at
`/mnt/c/Users/atchi/Transformer-VM/` to produce analytical transformer weights.
Anyone with the weights and a state commitment can verify any block's state
delta by re-running the transformer.

## Components

- **`primitives/`** — C source for transformer-verifiable state-transition
  primitives. Strict v2 style guide (under 2000 WASM instructions, no malloc,
  no floats, no variable multiplication, safety counters on loops).
- **`crypto/`** — native code (NOT compiled through transformer-VM): vendored
  ed25519 / SHA-256 / BLAKE3, plus the Rust Merkle-Patricia trie
  (`crypto/mpt.rs`) that holds the system-wide state root.
- **`sequencer/`** — Rust binary, sovereign-mode block producer. Ingests txs,
  pre-validates sigs and nonces natively, runs the transformer trace per
  primitive, applies deltas, signs and publishes block headers.
- **`consensus/`** — Rust crate, `Consensus` trait with sovereign and
  BFT (HotStuff via `malachitebft-rs`) implementations.
- **`light_client/`** — Rust crate verifying balances against block headers
  via Merkle proofs. Compiles to iOS / Android via UniFFI.
- **`lean/`** — Lean 4 + mathlib formalization of ledger semantics. Theorems:
  conservation, supply changes only via authorized mint/burn, determinism.
- **`tests/`** — bit-exact comparison harness (native WASM vs. specialized
  transformer), MPT randomized tests, sequencer integration, compliance.
- **`pilot/issuer_demo/`** — end-to-end pilot: register issuer, mint an asset,
  transfer through accounts, burn, light-client-verify every balance.

## Trust boundary

Sigs and hashes are verified by **native code**, not the transformer trace.
The transformer trace covers state-transition arithmetic only (debit, credit,
nonce, freeze flag, multi-asset batched transfers). Followers verify both
layers — see `docs/ARCHITECTURE.md` for the trace-hash contract and trust
model.

## Build / test

```bash
# One-time setup
export TRANSFORMER_VM_PATH=/mnt/c/Users/atchi/Transformer-VM
uv sync
cargo build --workspace
(cd lean && lake build)

# Compile and specialize a primitive
./tools/compile.sh primitives/ledger_freeze.c
./tools/specialize.sh primitives/ledger_freeze.txt

# Bit-exact verification (gate 1)
uv run pytest tests/test_bit_exact.py -v

# MPT (gate 2)
cargo test -p crypto

# Lean proofs (gate 3)
(cd lean && lake build)

# Sequencer end-to-end (gate 4)
uv run pytest tests/test_sequencer_rpc.py -v

# Compliance (gate 5)
uv run pytest tests/test_compliance.py -v

# Light-client cross-verification (gate 6)
cargo test -p light_client

# Pilot (gate 7)
cargo run --bin issuer_demo -- --full-flow
```

## Status

Pre-flight items (see `docs/ARCHITECTURE.md` § Pre-flight):

- [x] **P0** — Trace-hash contract pinned (`docs/ARCHITECTURE.md` § Trace contract).
- [ ] **P1** — `malachitebft-rs` maturity audit (`docs/CONSENSUS_DECISION.md`).
- [x] **P2** — Repo + remote backup configured.
- [ ] **P3** — `primitives/common.h` ported, `ledger_freeze.c` written.

Verification gates:

- [ ] 1. Primitive bit-exact (10k vectors per primitive)
- [ ] 2. MPT determinism (100k randomized)
- [ ] 3. Lean proofs build, zero `sorry`
- [ ] 4. Sovereign sequencer + 3 followers, 100 blocks, all roots match
- [ ] 5. Compliance enforcement (view-keys, travel-rule, freeze-authority)
- [ ] 6. Light client cross-verifies 1000 balances
- [ ] 7. Pilot run completes register → mint → transfer → burn → verify
- [ ] 8. Pure-Rust runner parity (Phase 1.5)
- [ ] 9. Consortium-mode swap, 4-node BFT cluster passes

## Plan

The full plan is at `/home/username/.claude/plans/cheeky-wandering-treehouse.md`,
mirrored in `docs/ARCHITECTURE.md` for the repo.
