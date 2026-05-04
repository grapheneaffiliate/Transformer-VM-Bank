# Changelog

Human-readable history of PSL milestones. Per-gate entries point at the
load-bearing commit on `origin/main`.

## 2026-05-04

### Gate 8 short-primitive completion + gate 8.5 — `4ffe560` → `b2546e8`

`bin/run_gate1` (the pure-Rust gate-1 vector runner): 5/5 short
primitives, **4500/4500 random witnesses, 0 failures**.

| Primitive | Vectors | Time | Rate |
| --- | --- | --- | --- |
| `byte_add_with_carry` | 1000/1000 | 19.6s | 50.9 vec/s |
| `byte_sub_with_borrow` | 1000/1000 | 254.2s | 3.9 vec/s |
| `transfer_finalize` | 1000/1000 | 576.2s | 1.7 vec/s |
| `transfer_check` | 1000/1000 | 3113.8s | 0.3 vec/s |
| `mpt_emit_record` | 500/500 | 5512.0s | 0.1 vec/s |

The flat-buffer attention rewrite (`4ffe560`) cut the gate-8 parity test
wall-clock from 21.6s → 10.4s on the 3 baseline primitives — bit-exact
preserved because summation order didn't change.

`freeze_setup` / `freeze_apply` parity at scale ruled out without MKL
linkage. Localized to `ff_out`'s 66×2162 reduction; PyTorch CPU
dispatches it to Intel MKL's `mkl_blas_avx2_xdgemv_t`, whose vectorized
reduction order doesn't match a sequential summation. Cross-engine
algorithm match against `transformer.cpp`'s Linux build still holds.
Full diagnosis: `docs/FINDINGS.md` § Gate 8.5.

### Gate 8 first-pass — pure-Rust runner bit-exact on 3 primitives

`cargo test -p psl-rust-runner --test parity --release -- --ignored`: 3/3.

Ported `Transformer-VM/transformer_vm/{model/transformer.py, model/weights.py,
attention/standard_cache.py, runner.py}` to native Rust. Forward pass is
greedy argmax-decoding with no biases, no LayerNorm, no attention scaling
— matching the analytical-construction Python path exactly. StandardKVCache
(softmax over scores, einsum) implemented as a triple-nested loop; ndarray
v0.15 used without BLAS feature in this first pass.

Bit-exact match against Python (`wasm-run --python --nohull`) on the
gate-1 spec inputs:

| Primitive | Tokens | Rust | Python | Speedup |
| --- | --- | --- | --- | --- |
| byte_add_with_carry | 117 | 50 ms | 470 ms | 9.4× |
| byte_sub_with_borrow | 402 | ~90 ms | ~1 s | ~11× |
| mpt_emit_record | 3,678 | 31 s | 94 s | 3.0× |

The mismatch shrinks at longer traces because attention is O(n²) and the
naive Rust loop saturates without BLAS. Adding `ndarray = { features = ["blas"] }`
+ a backend (openblas-src or accelerate-src) is expected to take the larger
primitives (freeze_setup at 17k tokens, freeze_apply at 7k) from
many-minutes back into seconds territory and recover the ≥10× target. That
is follow-up work — first-pass parity itself, the harder claim, is in.

PSL (this repo) holds no PyTorch or NumPy dependency. The runner is pure
Rust crate `psl-rust-runner` with one ndarray dep.

### Gate 7 cleared — end-to-end pilot

`cargo run --bin issuer_demo -- --full-flow` walks through the full
register → mint → xfer → xfer → burn flow, with the light-client
verifying the merchant balance against the 4-block chain rooted at
the empty-SMT genesis:

```
PSL issuer-demo pilot starting
weights/ missing → using NativeTraceExecutor (DEV ONLY)
registering issuer for asset_id=1
after mint:    treasury = 1_000_000
after xfer 100 → customer: treasury=999_900  customer=100
after xfer 50  → merchant: customer=50      merchant=50
after burn:    treasury = 999_800
light-client verified: merchant balance = 50
PSL pilot completed all steps.
```

Bug fixes during the gate:
- Pilot was passing only the head header to verify_balance; light client
  required full chain from genesis. Pilot now accumulates the full
  Vec<BlockHeader> and threads parent_hash through correctly.
- `psl_sequencer::block::BlockHeader::header_hash` includes the sequencer
  signature in the hashed bytes; `psl_light_client::header::Header::header_hash`
  did not. The two diverged, breaking chain linking. Aligned: light_client
  now exposes `SignedHeader::full_hash` (signing_bytes ∥ signature) used by
  verify_balance for chain linking; the unsigned variant is kept as
  `Header::unsigned_hash` but no longer used by chain logic.
- Pilot's genesis_root: was hardcoded to [0u8; 32] but the empty SMT root
  is `default_hashes()[0]`, not zero. Pilot now snapshots
  `state.accounts_root()` before the first transaction and passes that as
  the trust anchor.

### Gate 6 cleared — light client cross-verifies 1000 balances

`cargo test -p psl-light-client` 8/8 (1 unit + 7 in `tests/gate6.rs`):

- 1000-balance cross-verify: build random state with 1000 accounts,
  publish a signed header committing to the SMT root, light client
  re-verifies every (account, balance) pair via `verify_balance`.
- Tampered proof value rejected (`ProofFailed`).
- Tampered proof siblings rejected (`ProofFailed`).
- Tampered header signature rejected (`InvalidSignature`).
- Tampered header `new_state_root` rejected (sig mismatch).
- Wrong-signer expectation rejected (`InvalidSignature`).
- Out-of-order header chain rejected (`HeaderChainBroken`).

### Gate 5 cleared — compliance enforcement

`cargo test -p psl-sequencer --test compliance` 9/9. Three areas exercised
against `mempool::validate` and `state::account_proof`:

- **Travel rule**: high-value transfer without `originator_metadata`
  rejected; with metadata accepted; low-value passes without metadata.
- **Freeze authority**: non-issuer freeze rejected; freeze without
  `court_order_hash` rejected; issuer freeze with court order accepted;
  frozen account's subsequent transfer rejected.
- **View-key proofs**: regulator's SMT inclusion proof verifies against
  published root; tampered-balance proof rejected.

### Gate 3 cleared — Lean lake build (`113c11b`)

`cd lean && lake build` succeeds against mathlib v4.12.0 cached oleans.
Three sorrys remain on load-bearing theorems with target dates 2026-06-15
(Conservation:42, Conservation:60) and 2026-07-15 (MPT:58). Per the sorry
tracker, gate 3's success criterion is "compiles" not "zero sorrys yet."

### Gates 2 + 4 cleared — crypto suite + sequencer integration (`93bae87`)

- Gate 2: `cargo test -p crypto` 22/22 (incl. 100k-randomized SMT put,
  inclusion / non-inclusion proofs, signature round-trips).
- Gate 4: `cargo test -p psl-sequencer --test integration` 2/2 — sovereign
  sequencer + 3 followers agree on state root across 100 blocks of mixed
  traffic; published-root mutation is detected by every follower.
- Total workspace test count: **28/28** passing.

### Gate 1 cleared — bit-exact, 10000/10000 across active primitives (`9c50e3d`)

After per-byte decomposition: all 7 active primitives clear 10k randomized
witnesses with byte-for-byte equality between the native WASM output and
the specialized transformer's output:

| Primitive | Trace tokens | Pass |
| --- | --- | --- |
| `byte_add_with_carry` | 119 | 10000/10000 |
| `byte_sub_with_borrow` | 404 | 10000/10000 |
| `transfer_check` | 1,624 | 10000/10000 |
| `transfer_finalize` | 656 | 10000/10000 |
| `freeze_setup` | 17,566 | 10000/10000 |
| `freeze_apply` | 7,723 | 10000/10000 |
| `mpt_emit_record` | 3,741 | 10000/10000 |

Composition: freeze = 2 trace hashes, transfer = 34, mint = 16, burn = 17,
multi-asset = N × 34. See `docs/STYLE_GUIDE_v3.md` for the trace-length
design rule and the additive-normalization recipe that replaced
`i32.shr_u`-heavy patterns.

### Per-byte u128 decomposition (`9a6111b`)

Resolved the gate-1 wall (transfer at 89% pass on the monolithic
single-primitive design) by splitting into per-byte sub-operations
chained at the sequencer level. Empirical: a single 16-iteration loop's
trace accumulates precision drift at scale; per-byte primitives at 119-404
tokens each clear 10k cleanly. Documented in `docs/FINDINGS.md` and
`docs/STYLE_GUIDE_v3.md`.

## 2026-05-03

### Trace-length design rule + 10k results (`1aef4f4`)

`docs/STYLE_GUIDE_v3.md` written: trace length is the precision-budget
currency; sequential ops target sub-1k tokens; avoid `i32.shr_u` /
`i32.shr_s` patterns that explode under `lower.py`'s expansion.

### Pre-flight items P0–P2 cleared

- **P0** — Trace-hash contract pinned in `docs/ARCHITECTURE.md § 0`
  after reading `Transformer-VM/transformer_vm/runner.py` end-to-end.
  The trace is the greedy-argmax-decoded token sequence (including input
  prefix); `trace_hash` is BLAKE3 of UTF-8 of space-joined tokens.
- **P1** — `docs/CONSENSUS_DECISION.md`: defer `malachitebft-rs` (alpha,
  ownership transition Informal→Circle, no GA in 6 months); MVP rides
  ABCI + CometBFT. Reconsider on any of: 1.0 release, 90-day Circle Arc
  mainnet, third-party audit.
- **P2** — Repo bootstrapped at github.com/grapheneaffiliate/Transformer-VM-Bank.
