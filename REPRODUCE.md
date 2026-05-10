# Reproduce PSL from a clean machine

**Goal**: a third party — auditor, partner, regulator — clones this repo,
follows this doc, and sees green on every gate within one working day.

**Last refreshed: 2026-05-09 (post-v0.1.0 cut + Phase H docs cleanup).**
**Companion:** [`docs/REPRODUCIBILITY_REPORT.md`](docs/REPRODUCIBILITY_REPORT.md) —
pinned toolchain + per-gate command + wall-clock timings on the
reference VM.

There are two reproducibility tiers, depending on how much you want to
verify yourself:

- **Tier 1 (gates 2–7, 10–16, 19)** — pure Rust + (optional) Lean.
  Verifies the consensus, state-commitment, formal-model, light-client,
  compliance, and pilot layers PLUS the entire Phase 2 agent execution
  layer (ternary VM, contracts, wallet, protocol, dispute, SDK) PLUS
  the Phase G phase 1 cryptographic agility crate. Fully reproducible
  on any x86_64 Linux host. **~5 minutes wall-clock total** after Rust
  toolchain is installed; ~15 min more if you also do Lean. The
  headline is `cargo build --workspace --release && cargo test
  --workspace --release` plus the two SDK reference examples.
- **Tier 2 (legacy gate 1)** — adds the `Transformer-VM` analytical-model
  build pipeline. Verifies that every state-transition primitive in
  the **legacy gate-1 era** is bit-exact between native WASM and the
  specialized transformer. **Hours of compute** (~10–20 min per
  primitive × 7 active primitives, plus a one-time ~3 GB torch
  download). The legacy fp64 runner is frozen per ADR-0001 and is no
  longer the canonical engine; this tier exists for the historical
  receipts and for verifiers who want to re-validate the gate-1
  result independently.

Tier 1 alone is enough to convince yourself that PSL's *claimed* design
is implemented correctly, that its proofs build, that an end-to-end
pilot runs to completion, that the agent layer's two reference binaries
(trader_agent + service_agent) execute the happy and dispute paths
correctly, and that the cryptographic agility infrastructure works.
Tier 2 verifies the legacy transformer-trace contract; new verifiers
should default to Tier 1.

---

## Prerequisites (Tier 1)

### Operating system

Tested: Ubuntu 22.04 LTS x86_64. macOS / other Linux distros likely work
but are not in the CI matrix.

### Hardware

- ≥ 4 GB RAM (Lean's mathlib decompression is the peak)
- ≥ 5 GB free disk (Tier 1; ~10 GB for Tier 2 with torch + weights)
- Network access (for cargo + mathlib cache + clone)

### Toolchains (exact versions — pinned)

| Tool | Version on dev box | Install command |
| --- | --- | --- |
| Rust (rustup) | rustc 1.95.0 / cargo 1.95.0 | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Lean (elan) | lean 4.12.0 | `curl https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh -sSf \| sh -s -- -y` |
| Python | 3.13.5 | distro package |
| uv | 0.11.6 | `curl -LsSf https://astral.sh/uv/install.sh \| sh` |
| Git | any recent | distro package |

After installing rustup and elan, source their envs (or open a new shell):

```bash
source $HOME/.cargo/env
source $HOME/.elan/env
```

The Lean version is pinned by `lean/lean-toolchain` to v4.12.0 — elan
will install that automatically the first time `lake build` runs.

Note: a fresh elan install has no default toolchain set, so a global
`lean --version` errors with `no default toolchain configured`. This is
expected — the toolchain is auto-installed by elan when you `cd lean &&
lake build` because of the in-repo `lean-toolchain` pin. If you prefer a
global default, run `elan default leanprover/lean4:v4.12.0` once.

---

## Step 1 — Clone

```bash
git clone https://github.com/grapheneaffiliate/Transformer-VM-Bank.git
cd Transformer-VM-Bank
```

---

## Step 2 — Gate 2: SMT determinism + crypto suite (~30 s)

```bash
cargo test -p psl-crypto --release
```

Expected tail (warnings vary):

```
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

22 tests cover BLAKE3, ed25519, the Sparse Merkle Tree (incl. a 100k
randomized-put determinism test), inclusion + non-inclusion proof
verification, and account record (de)serialization.

---

## Step 3 — Gate 4: sovereign sequencer + 3 followers (~30 s)

```bash
cargo test -p psl-sequencer --test integration --release
```

Expected:

```
running 2 tests
test sequencer_and_3_followers_agree_on_100_mixed_blocks ... ok
test published_root_mutation_detected ... ok

test result: ok. 2 passed; 0 failed
```

The first test runs 100 blocks of mixed traffic (transfer / mint / burn /
freeze / multi-asset, RNG-seeded) across one sovereign sequencer plus
three followers, and asserts all four nodes' account-tree roots match at
every block. The second test mutates a published `new_state_root` and
verifies a follower detects it.

---

## Step 4 — Gate 5: compliance enforcement (~5 s)

```bash
cargo test -p psl-sequencer --test compliance --release
```

Expected:

```
running 9 tests
test result: ok. 9 passed; 0 failed
```

Three travel-rule cases (high without metadata rejected, high with
metadata accepted, low without metadata accepted), four freeze-authority
cases (non-issuer rejected, no court order rejected, issuer + court order
accepted, frozen account's transfer rejected), and two view-key proof
cases (regulator inclusion proof verifies, tampered-balance proof
rejected).

---

## Step 5 — Gate 6: light client cross-verification (~10 s)

```bash
cargo test -p psl-light-client --release
```

Expected:

```
running 1 test
test tests::verify_balance_round_trip ... ok

running 7 tests
test cross_verify_1000_random_balances ... ok
... 6 adversarial tests ...

test result: ok. 7 passed; 0 failed
```

The 1000-balance test builds an SMT with 1000 random (pubkey, balance)
pairs, publishes a signed block header, and re-verifies every balance via
the light-client API. The six adversarial tests cover tampered-proof
value, tampered-proof siblings, bad signature, tampered header root,
wrong signer, and broken header chain.

---

## Step 6 — Gate 7: end-to-end pilot (~5 s)

```bash
RUST_LOG=info cargo run --bin issuer_demo --release -- --full-flow
```

(`RUST_LOG=info` is required to see the verification trail — without it
the binary runs silently and exits with status 0 if everything passed.)

Expected log lines (timestamps elided):

```
PSL issuer-demo pilot starting
weights/ missing → using NativeTraceExecutor (DEV ONLY, trace_hash is a marker)
registering issuer for asset_id=1
after mint: treasury balance = 1000000
after xfer 100 to customer: treasury=999900 customer=100
after xfer 50 to merchant: customer=50 merchant=50
after burn: treasury balance = 999800
light-client verified: merchant balance = 50
PSL pilot completed all steps.
```

The `weights/ missing` warning is expected for Tier 1 — the pilot
substitutes `NativeTraceExecutor` so the same pilot binary works without
the analytical-model build pipeline. For an end-to-end run that exercises
the actual transformer, complete Tier 2 first.

---

## Step 7 — Gate 3: Lean lake build (~25 min first time, ~30 s subsequent)

```bash
cd lean
lake update     # downloads mathlib + 5134-olean precompiled cache, ~20 min
lake build      # compiles PSL + verifies sorry tracker
cd ..
```

Expected:

```
✔ [11/17] Built PSL.Account
⚠ [12/17] Built PSL.MPT
warning: PSL/MPT.lean:49:8: declaration uses 'sorry'
✔ [13/17] Built PSL.Ledger
✔ [14/17] Built PSL.Determinism
⚠ [15/17] Built PSL.Conservation
warning: PSL/Conservation.lean:30:8: declaration uses 'sorry'
warning: PSL/Conservation.lean:54:8: declaration uses 'sorry'
✔ [16/17] Built PSL
Build completed successfully.
```

(Lean reports the declaration-start line; the literal `sorry` keyword
is a few lines further into each proof body.)

The 3 sorrys are tracked in `docs/STATUS.md` with target close dates
2026-06-15 / 2026-07-15. Per the gate-3 contract ("compiles" is the
success criterion), this is a pass. **Note**: `lake update` takes most
of the time — it downloads ~5000 precompiled mathlib oleans. If you
want to skip the cache and let mathlib compile from scratch, expect
1–3 hours instead.

If you only want to verify Tier 1, you can stop here. Total wall-clock
so far: about 30 minutes (most of which is the mathlib cache).

---

## Tier 2 — Gate 1: bit-exact at scale

Gate 1 is the load-bearing claim of PSL: every active primitive's
specialized-transformer output equals its native-WASM output, byte for
byte, across 10000 random witnesses. This requires:

1. A clone of the upstream Transformer-VM repo.
2. WASI SDK clang (Linux native or Windows-side via WSL).
3. ~3 GB of torch / numpy from PyPI on first `uv sync`.
4. ~6 GB of weights binaries written under `weights/` (gitignored).

### Step 1 — get Transformer-VM

```bash
git clone https://github.com/anthropics/Transformer-VM.git "$HOME/Transformer-VM"
export TRANSFORMER_VM_PATH="$HOME/Transformer-VM"
cd "$TRANSFORMER_VM_PATH"
uv sync       # ~3 GB torch download on first run
cd -          # back to PSL repo
```

(Substitute the actual upstream URL when it is published; PSL's repo
holds no copy of Transformer-VM by design.)

### Step 2 — install WASI SDK

Pick **one** of the two options:

**Option A (recommended, native Linux)**:
```bash
WASI_VER=24
wget https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-${WASI_VER}/wasi-sdk-${WASI_VER}.0-x86_64-linux.tar.gz
sudo tar -xzf wasi-sdk-${WASI_VER}.0-x86_64-linux.tar.gz -C /opt
sudo ln -s /opt/wasi-sdk-${WASI_VER}.0 /opt/wasi-sdk
export WASI_CLANG=/opt/wasi-sdk/bin/clang
```

**Option B (WSL only, using a Windows-side wasi-sdk)**: install wasi-sdk
on the Windows side and set `WASI_CLANG_EXE` to its `clang.exe` path.
The dispatcher at `tools/clang-wsl-wrapper.sh` rewrites Linux paths to
Windows paths automatically.

### Step 3 — build all 7 primitives (~5–15 min)

```bash
./tools/build_all_primitives.sh
```

This compiles each primitive's C source to WASM via `tools/compile.sh`
and runs `wasm-specialize` to produce the analytical transformer weights
under `weights/`. Outputs are gitignored.

### Step 4 — run the 10k harness per primitive

```bash
# byte_add_with_carry, byte_sub_with_borrow, transfer_check, transfer_finalize
python3 tools/run_per_byte_10k.py

# freeze_setup + freeze_apply
python3 tools/run_freeze_decomposed.py

# mpt_emit_record (single primitive)
python3 tools/run_mpt_10k.py
```

Each script runs all its witnesses in sequence through the specialized
transformer's C++ engine (`uv run wasm-run --model …` from
Transformer-VM). Expected per-script runtimes:

| Primitive | Trace tokens | Wall-clock for 10k |
| --- | --- | --- |
| byte_add_with_carry | 119 | ~5 min |
| byte_sub_with_borrow | 404 | ~10 min |
| transfer_check | 1,624 | ~30 min |
| transfer_finalize | 656 | ~15 min |
| freeze_setup | 17,566 | ~2 hours |
| freeze_apply | 7,723 | ~40 min |
| mpt_emit_record | 3,741 | ~30 min |

Pass criterion: each primitive prints `10000/10000 passed` at the end. Any
witness mismatch is dumped to `tests/failures/<primitive>_<hash>.txt`
along with the input bytes, expected output, and observed output.

The full table of results is reproduced in `docs/STATUS.md` — your run
should match.

---

## What if a step fails?

| Symptom | Likely cause | Fix |
| --- | --- | --- |
| `cargo: command not found` | rustup env not sourced | `source ~/.cargo/env` |
| `lake: command not found` | elan env not sourced | `source ~/.elan/env` |
| `lake update` hangs at "Cloning" | network / DNS flake | retry once; mathlib cache is idempotent |
| `lake build` errors on mathlib | toolchain mismatch | confirm `lean/lean-toolchain` says `leanprover/lean4:v4.12.0` |
| `cargo test` errors with `error: linker 'cc' not found` | missing build essentials | `sudo apt install build-essential` |
| `TRANSFORMER_VM_PATH=… does not exist` | not pointing at your TVM checkout | `export TRANSFORMER_VM_PATH=$HOME/Transformer-VM` (or wherever) |
| `no WASI clang found` | WASI_CLANG not set | follow Tier 2 step 2 above |

If a fix isn't here, open an issue at
github.com/grapheneaffiliate/Transformer-VM-Bank/issues with the full
output of the failing command and your toolchain versions
(`rustc --version`, `lean --version`, `python3 --version`).

---

## Verifying a specific commit

Each gate's load-bearing commit hash is in `docs/STATUS.md` and
`CHANGELOG.md`. To reproduce results at the exact state cited:

```bash
git checkout <commit-hash>
# follow steps above
git checkout main  # when done
```

For the audit trail, the cleared-gate commits are:

| Gate | Commit |
| --- | --- |
| 1 | `9c50e3d` |
| 2 | `93bae87` |
| 3 | `113c11b` |
| 4 | `93bae87` |
| 5 | `b157f2f` |
| 6 | `3d4d3e6` |
| 7 | `dfc11e6` |

---

## Wall-clock budget for a full Tier-1 + Tier-2 fresh-machine run

| Phase | Time |
| --- | --- |
| Clone + Rust + Lean install | 5 min |
| Gates 2 / 4 / 5 / 6 / 7 (`cargo test` + `cargo run`) | 5 min |
| Gate 3: `lake update` (mathlib cache) | 20 min |
| Gate 3: `lake build` | 5 min |
| **Tier 1 subtotal** | **~35 min** |
| Tier 2: Transformer-VM clone + `uv sync` + torch download | 15 min |
| Tier 2: WASI SDK install | 5 min |
| Tier 2: `build_all_primitives.sh` | 10 min |
| Tier 2: 7 × 10k harness runs (sequential, one CPU) | ~5–6 hours |
| **Tier 2 subtotal** | **~6 hours** |
| **Total fresh-machine reproduction** | **under one working day** |

Tier 2 parallelizes naturally if you have multiple cores — run the seven
harness scripts in parallel and the wall-clock collapses to the longest
single run (`freeze_setup`, ~2 hours).
