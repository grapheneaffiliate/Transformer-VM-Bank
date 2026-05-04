# Changelog

Human-readable history of PSL milestones. Per-gate entries point at the
load-bearing commit on `origin/main`.

## 2026-05-04

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
