# PSL Build Status

Live state of which gates have been *executed* on this machine versus
*written and ready to execute*. The plan is to build the entire system; some
gates need network access (for torch / cargo registry) that hasn't been
exercised in this environment.

## Bootstrap requirements

Before any verification gate can run on this machine:

1. **Transformer-VM `.venv` must be synced.** `uv sync` in
   `/mnt/c/Users/atchi/Transformer-VM/` requires network access to download
   `torch==2.10.0` (~3 GB). At time of writing the local `.venv` does not
   exist; the sync was attempted but blocked on offline network.

2. **WASI clang at `/mnt/c/Users/atchi/wasi-sdk/bin/clang.exe`** — confirmed
   present. `tools/compile.sh` autodetects this path.

3. **Rust toolchain** — `cargo` not yet checked. Each Rust crate has its
   own `Cargo.toml`; standard `cargo build --workspace` from the repo root
   builds everything once toolchain is installed.

4. **Lean toolchain** — install `elan`, then `lean-toolchain` pin in
   `lean/lean-toolchain` selects the version. `lake build` from `lean/`.

5. **Python deps for tests** — `uv sync` at the PSL repo root (separate
   from Transformer-VM's sync) installs the test harness deps.

## Gate status

| Gate | Code written | Ran here | Pass |
| --- | --- | --- | --- |
| P0: trace-hash contract | ✅ docs/ARCHITECTURE.md § 0 | n/a | n/a |
| P1: malachite audit | ✅ docs/CONSENSUS_DECISION.md | ✅ | verdict: defer, ship on tendermint-rs ABCI |
| P2: repo + remote | ✅ | ⏳ initial push pending | — |
| P3: common.h + freeze.c | ✅ | ⏳ blocked on Transformer-VM sync | — |
| 1: bit-exact (10k/primitive) | ✅ harness written | ⏳ blocked on torch | — |
| 2: MPT determinism | ⏳ | — | — |
| 3: Lean proofs | ⏳ | — | — |
| 4: sequencer e2e | ⏳ | — | — |
| 5: compliance | ⏳ | — | — |
| 6: light client | ⏳ | — | — |
| 7: pilot | ⏳ | — | — |
| 8: pure-Rust runner | ⏳ | — | — |
| 9: consortium swap | ⏳ | — | — |

## How to make this real

```bash
# 1. Sync Transformer-VM (needs network)
cd /mnt/c/Users/atchi/Transformer-VM
uv sync

# 2. Build all primitives end-to-end
cd /mnt/c/Users/atchi/Transformer_VM_Bank
./tools/build_all_primitives.sh

# 3. Generate test vectors and run bit-exact gate (gate 1)
python tools/gen_vectors.py --quick     # smoke at 100/primitive first
uv run pytest tests/test_bit_exact.py -v

# Once gate 1 is green:
python tools/gen_vectors.py             # full 10k/primitive
uv run pytest tests/test_bit_exact.py -v --full

# 4. Build Rust workspace (gates 2, 4, 5, 6, 7, 9)
cargo build --workspace
cargo test --workspace

# 5. Build Lean proofs (gate 3)
cd lean && lake build
```
