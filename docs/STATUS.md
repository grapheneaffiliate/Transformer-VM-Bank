# PSL Build Status

**Last verified against repository state on 2026-05-10, commit `HEAD` (post v0.1.0-cleanup-pass).** Re-verify weekly per `GOVERNANCE.md`.

**Status as of v0.1.0 cut**: gates 1-8 ✅ (gate 8 closed via retirement per
ADR-0001). Gate 9 ⏸ deferred per ADR-0002 with three concrete trigger
conditions. Gates 10-16 ✅ (Phase 2 agent execution layer shipped — ternary
contract VM, 8-contract standard library, SLIP-0010 wallet, 5-message
protocol with deterministic dispute resolution, SDK with reference
agents). Gates 17 + 18 🟢 (audit hand-off package + production ops stack
shipped; awaits human action — signed engagement letter and first DR
drill on staging respectively). Gate 19 🟢 (full ADR-0011 5-commit plan
shipped: hybrid X25519 + ML-KEM-768 KEM, forward-secret witness
encryption, agent-layer cascade, cross-platform determinism CI on
x86_64 + aarch64; awaits external cryptographer review per ADR-0006 /
ADR-0011 acceptance criteria — only step requiring human-in-the-loop
sign-off). Sled storage migration deferred to v0.2 per ADR-0012.

## Bootstrap requirements

Before any verification gate can run:

1. **Rust toolchain** — install via `rustup`, pinned to 1.95.0 per
   `docs/REPRODUCIBILITY_REPORT.md`. `cargo build --workspace --release`
   then `cargo test --workspace --release` is the headline reproduction
   path; runs in ~2 min on a 4-vCPU cloud VM after toolchains land.
2. **Lean toolchain** (Tier-2, only for gate 3) — `elan` installed at
   `~/.elan/`. `lean-toolchain` pin selects v4.12.0. `lake build` from
   `lean/`. Mathlib pulled via precompiled cache (`lake update`
   triggers `cache get` post-hook).
3. **Transformer-VM `.venv`** (Tier-2, only for the historical gate-1
   10k-vector sweep on the C++ engine) — `uv sync` in `Transformer-VM/`
   requires network access to download `torch==2.10.0` (~3 GB). Default
   reproduction skips this; the canonical engine is now the pure-Rust
   ternary kernel (`ternary_vm/`) per ADR-0001.
4. **Python test deps** (Tier-2) — `uv sync` at PSL repo root, only
   needed for the legacy gate-1 sweep.

The default Tier-1 reproduction (gates 2-7 + 10-16 + 19) requires only
the Rust toolchain and ~5 minutes of build/test time. See `REPRODUCE.md`
for the two-tier breakdown.

## Gate status

| # | Gate | Command | Result | Commit |
| --- | --- | --- | --- | --- |
| P0 | Trace-hash contract pinned | `docs/ARCHITECTURE.md § 0` | ✅ pinned 2026-05-03 | — |
| P1 | Consensus vendor audit | `docs/CONSENSUS_DECISION.md` | ✅ defer malachite; ABCI + CometBFT for MVP | — |
| P2 | Repo + remote backup | github.com/grapheneaffiliate/Transformer-VM-Bank | ✅ live | — |
| **1** | Bit-exact 10k/primitive | `tools/run_per_byte_10k.py` + `run_freeze_decomposed.py` | ✅ 10000/10000 on all 7 active primitives | `9c50e3d` |
| **2** | SMT determinism | `cargo test -p crypto` | ✅ 22/22 (incl. 100k randomized put + non-inclusion proofs) | `93bae87` |
| **3** | Lean lake build | `cd lean && lake build` | ✅ compiles against mathlib v4.12.0; **formal layer is now sorry-free** — all conservation theorems and MPT soundness proven (see "Lean `sorry` tracker" below; only declared axioms are `hash_collision_resistant` + `hash_length`) | `113c11b` |
| **4** | Sequencer + 3 followers, 100 blocks | `cargo test -p psl-sequencer --test integration` | ✅ 2/2; all 4 state roots match every block; mutation detected. **Sequencer TPS regression bench** (`bench_sequencer_tps_10k_blocks`, `#[ignore]`d, runs 15,106 mixed signed transactions across 10,000 blocks): **4-replica** with cross-replica root agreement ≈ **925 tx/s** (mean 1.08 ms/tx; p50 950 µs, p95 1.95 ms, p99 2.72 ms, p99.9 4.20 ms; single 922 ms max outlier is OS-scheduler noise). **Single-replica** ≈ **3,990 tx/s** (mean 251 µs/tx; p50 201 µs, p95 464 µs, p99 737 µs, p99.9 1.42 ms). Pinned reference hardware: **Intel Core i7-7700 @ 3.60 GHz, 4 cores / 8 threads, x86_64**, WSL2 Ubuntu (Linux 5.15), release build, `NativeTraceExecutor` synthetic trace, in-memory `State`. Run-to-run TPS variance ~5-15% on this WSL2 host from OS scheduler noise; production cloud-CPU deployments should be more stable and likely faster. End-to-end with real ternary trace (~34 trace-hashes per transfer × ~9.5 µs each from gate 10's measured `byte_add` throughput) is back-of-envelope ≈ 1,750 tx/s single-replica — comfortably above the gate-9 trigger threshold of 100 TPS. Hardware spec is captured at run time by the bench (`uname -a` + relevant `lscpu` fields). Run with `cargo test -p psl-sequencer --test integration --release bench_sequencer_tps_10k_blocks -- --ignored --nocapture` (set `PSL_BENCH_REPLICAS=1` for single-replica). v0.2 maturation (perf-CI auto-regression gate, real-trace measurement) deferred per [ADR-0013](decisions/0013-defer-tps-bench-maturation-to-v0.2.md). | `93bae87` |
| **5** | Compliance enforcement | `cargo test -p psl-sequencer --test compliance` | ✅ 9/9 (travel-rule × 3, freeze authority × 4, view-key proofs × 2) | _(this commit)_ |
| **6** | Light client cross-verifies 1000 balances | `cargo test -p psl-light-client` | ✅ 8/8 (1000-balance cross-verify + 6 adversarial: tampered proof value, tampered siblings, bad sig, tampered root, wrong signer, broken chain) | _(this commit)_ |
| **7** | Pilot end-to-end | `cargo run --bin issuer_demo -- --full-flow` | ✅ register issuer → mint 1M → xfer 100 → xfer 50 → burn 100; light-client verifies merchant=50 against the 4-block chain rooted at empty-SMT genesis | _(this commit)_ |
| **8** | Pure-Rust runner | `cargo test -p psl-rust-runner --release` (legacy, frozen) | ✅ closed via retirement per **[ADR-0001](decisions/0001-retire-legacy-fp64-runner.md)**. Phase 1.5 fp64 autoregressive runner is moved to `legacy/rust_runner/`, `#[deprecated]` on its public API, CI guard `tools/ci/check_legacy_isolation.sh` rejects any new code outside `legacy/` that imports `psl_rust_runner`. The 50000/50000 short-primitive bit-exact result is preserved; long-primitive parity is no longer a goal because gates 10-16 shipped the canonical ternary-integer engine (`ternary_vm/`). Migration table in legacy crate's `lib.rs`. | _(this commit)_ |
| **8.5** | Load-bearing arithmetic correctness via Rust runner | `cargo run --release --bin run_gate1 -- --primitive <p> --count <n>` + Runpod 32-vCPU scale-out | ✅ 50000/50000 vectors, 0 failures across 5 short primitives. Runpod sweep on EPYC 7702P 32-vCPU pod completed `mpt_emit_record` 10k/10k in 4961s; the four other short primitives' 10k results were already in tree from the local sweep. Logs in `docs/gate85_logs/canonical/`. | `50000/50000 0 fail` |
| **9** | Consortium swap (BFT consensus engine) | 4-node validator-set test under fault scenarios — ships within 60 days of any trigger | ⏸ deferred per **[ADR-0002](decisions/0002-bft-consensus-engine-selection.md)** with three concrete triggers: (1) institutional pilot LOI requires multi-validator consensus, (2) Malachite v1.0 + external audit publishes, (3) test net >100 agents + any agent >10% volume. Sovereign-mode trust assumption documented in `docs/SOVEREIGN_MODE_TRUST.md`. Quarterly review via `tools/quarterly_consensus_review.sh`. | _(this commit)_ |
| **10** | Ternary execution engine — Phase 2 Layer 1 | `cargo test -p psl-ternary-vm --release` | ✅ all 7 active primitives in the ternary executor, 42/42 tests passing. **Exhaustive verifications (every input combination)**: `byte_add_with_carry` 131072/131072 (1.25s), `byte_sub_with_borrow` 131072/131072 (1.12s), `freeze_apply` 512/512. **Large random sweeps (≥1000 witnesses)**: `freeze_setup` 1000/1000, `transfer_finalize` 1000/1000, `mpt_emit_record` 1000/1000, `transfer_check` 500/500. byte_add throughput **105k vec/s** single-threaded (≥100k plan bar met). Crate `ternary_vm/`: `SparseTernaryLayer`, `TernaryNetwork`, BLAKE3-hashed packed weight format, `trace_hash_ternary` (`docs/ARCHITECTURE.md § 0.8`), checked-arithmetic forward kernel (no production-path panics). Two primitives (`transfer_finalize`, `transfer_check`) are ternary *programs* — they compose `byte_add` / `byte_sub` 8/16 times respectively. | `481d58c` |
| **11** | Contract DSL standard library — Phase 2 Layer 2 | `cargo test -p psl-agent-contracts --release` | ✅ all 8 standard contracts shipped, 20/20 tests passing. `agent_contracts/`: `TernaryProgram` trait + `program_hash` (BLAKE3 over name + sub-network weights_hashes), `guarded` helper module (u128 add/sub chains, u64 ≥, wrapped_transfer). Contracts: `transfer`, `swap`, `escrow_create`, `escrow_release`, `escrow_refund`, `time_locked_release`, `multisig_2of3`, `conditional_payment`. All emit canonical no-op zeros on precondition failure (insufficient balance, recipient overflow, guard not satisfied, out-of-range flags). Parsed-DSL frontend (lexer / parser / typechecker / interpreter / compiler) is the second half of this gate — landing as follow-up. | _(this commit)_ |
| **12** | Identity & wallet — Phase 2 Layer 3 | `cargo test -p psl-agent-wallet --release` | ✅ 20/20 tests. `agent_wallet/`: SLIP-0010 ed25519 hierarchical key derivation (passes spec test vector #1: master + first hardened child), per-key spending policy with parent-signed envelope (cap-per-window + allowed contracts + allowed counterparties + expiry), revocation set with monotonicity invariant + signed `Revocation` records, `KeyRotation` for parent-signed (old → new) child key replacement. Private keys wrapped in `Zeroizing<…>`. | _(this commit)_ |
| **13** | Negotiation protocol — Phase 2 Layer 4 | `cargo test -p psl-agent-protocol --release` | ✅ 18/18 tests. `agent_protocol/`: `AgentRegistration`, the 5 wire message types (`Propose` / `Accept` / `Reject` / `CounterPropose` / `Execute` — each signed, content-addressed via `proposal_hash`), `ProposalLog` state machine (Proposed → Accepted/Rejected/CounterProposed/Expired → Executed) with idempotent replay handling, `ReputationCounters` + `ReputationLog`. Mutual-TLS network transport is SDK responsibility (Layer 5). | _(this commit)_ |
| **14** | Dispute resolution — Phase 2 Layer 4 | `cargo test -p psl-agent-protocol --release dispute` | ✅ 3/3 tests. `agent_protocol::dispute::resolve_dispute` re-executes the contract via the `TernaryProgram` trait (deterministic by construction) and returns `SlashExecutor` if the executor's claimed output differs from the re-execution, or `DismissDispute` if it matches. End-to-end test wires `TransferContract` through `Propose` / `Execute` / `Dispute`. | _(this commit)_ |
| **15** | Reference agents (trader + service) | `cargo run -p psl-agent-sdk --release --example trader_agent` / `--example service_agent` | ✅ Two reference binaries run end-to-end. **trader_agent**: Alice proposes transfer (1000→750 sender, 500→750 recipient, nonce 7→8) → Bob accepts → Alice signs Execute → Bob verifies, outputs agree. **service_agent**: malicious Bob signs Execute claiming all-zero output → Alice opens Dispute → judge agent re-executes deterministically → SlashExecutor outcome with Bob's pubkey. No human arbiter, no oracle. Docker-compose stack with sequencer + 3 followers + 2 agents is the deployment substrate, follow-up. | _(this commit)_ |
| **16** | SDK 0.1.0 — Phase 2 Layer 5 | `cargo test -p psl-agent-sdk --release` | ✅ 2/2 tests + 2 reference examples run. `agent_sdk/`: `AgentIdentity` (parent + child + signed `PolicyEnvelope`), `AgentSdk` (handle_propose / handle_accept / handle_execute / resolve_dispute_for, mempool admit_outgoing, local reputation), `OnChainView` trait + `InMemoryOnChain` adapter, `Transport` trait + `InProcessBus` for tests/demos. UniFFI bindings to Swift/Kotlin/Python/JS architecturally trivial — emit as a separate crate via uniffi-bindgen in a follow-up. | _(this commit)_ |
| **17** | External security audit (Trail of Bits / Zellic / OtterSec) | hand-off package: `docs/AUDIT_BRIEF.md` + `docs/SECURITY_REVIEW.md` + `docs/REPRODUCIBILITY_REPORT.md` + 5 `cargo-fuzz` harnesses + `outreach/audit-engagement-{tob,zellic,ottersec}.md` | 🟢 **Claude-Code-closeable everything done.** Threat model has full adversary-model section (passive, active, byzantine sequencer, byzantine executor / disputer, malicious DSL author), crypto-primitive selection table with audit status, side-channel & memory-zeroing inventories. Reproducibility report pins toolchain + per-gate command + expected timing. AUDIT_BRIEF is auditor's day-1 entry. Three engagement-letter request emails drafted in `outreach/`. **Action required from human:** sign + send one of the engagement letters; this is the only step that requires payment authorization. | _(this commit)_ |
| **18** | Production-readiness review (monitoring, alerting, runbooks, DR) | `docs/OPERATIONAL_READINESS.md` + `docs/DR_DRILL_PLAN.md` + `docs/runbooks/*` + `ops/*` + `tools/backup.sh` + `tools/load_test.sh` + `infra/*` | 🟢 **Claude-Code-closeable everything done.** Six runbooks shipped (`consensus-halt`, `sequencer-key-compromise`, `dispute-storm`, `follower-lag`, `light-client-divergence`, `dr-restore`). Full observability stack in `ops/`: docker-compose with Prometheus + Grafana + Alertmanager + Loki + Promtail + Tempo, datasource & dashboard provisioning, four alert files (sequencer / light_client / agents / backup) covering 11 PromQL alerts mapped 1-to-1 to runbook triggers. `tools/backup.sh` does dual-tier (hot S3 + cold Glacier) backups with BLAKE3-verified manifests; `--verify-latest` cron + alert. `tools/load_test.sh` ramps to saturation and writes regression-comparable JSON. `docs/DR_DRILL_PLAN.md` defines quarterly drill with explicit pass/fail criteria. `infra/` has reference Terraform (network + sequencer + 3× follower + light_client_gw + observability + backup_buckets) so "redeploy this exactly" is `terraform apply`. CI: `.github/workflows/{ci,security,fuzz}.yml` + `dependabot.yml`. **Action required from human:** schedule + execute the first DR drill on staging (`docs/DR_DRILL_PLAN.md` § "Drill execution") and sign off the result; this is the only step that requires real ops time. | _(this commit)_ |
| **19** | Post-quantum cryptographic agility (Phase G phases 1-4 + agent-layer cascade + cross-platform CI) | `cargo test -p psl-crypto-agility --release && cargo test --workspace --release` (254 tests, 0 failures across x86_64 + aarch64 GitHub-hosted runners) | 🟢 **Phase G phases 1-4 shipped**, full ADR-0011 5-commit plan complete, agent-layer wire-format cascade landed, cross-platform determinism CI matrix verifies byte-stable hashes/contexts on x86_64 + aarch64 (`runs-on: ubuntu-24.04 + ubuntu-24.04-arm`). **Awaits external cryptographer review per ADR-0006 + ADR-0011 acceptance criteria** — only step that requires human-in-the-loop sign-off, explicitly identified as not Claude-Code-closeable from start of session. Original phase 1-3 detail follows for traceability: **Phase 1** (agility infrastructure): `crypto_agility/` defines `SignatureScheme` / `KemScheme` / `HashScheme` enums + `Signer` / `Verifier` / `Kem` / `HashScheme_` traits + `VerifierPolicy` + LEB128 varint codec + ed25519 + BLAKE3-256/512 + explicit `UnknownScheme` rejection. **Phase 2** (BLAKE3-512 for long-lived commitments): `WeightsHeader` carries dual `weights_hash` (32B v1 + 64B v2); `pack_weights_dual` populates both; `trace_hash::v1::trace_hash_v1` (frozen per ADR-0008) and `trace_hash::v2::trace_hash_v2` ship side by side with frozen-KAT tests (benign + adversarial: wrong magic / truncated / tampered digest); v1 ↔ v2 disagreement on identical inputs is asserted; deprecated `trace_hash_ternary` re-export pinned to v1 behavior. **Phase 3** (hybrid signatures): `crypto_agility::hybrid::HybridSigner` / `HybridVerifier` implement ed25519 + ML-DSA-65 (via `pqcrypto-mldsa`); concatenation combiner per NIST SP 800-227; locked discriminant `0x02`; locked concatenation order (ed25519 first, ML-DSA-65 second); fixed-length `HYBRID_PUBKEY_BYTES=1984` / `HYBRID_SIG_BYTES=3373`; 11 hybrid tests cover the brief's 4-case combiner correctness + length-extension + one-byte-short hard reject + component swap + cross-message replay + byte-exact wire format round-trip + verification determinism. ADRs **0006** (PQ strategy + per-scheme determinism table), **0007** (agility traits/wire format), **0008** (BLAKE3-512). Lean `Digest n` parameterized over length (per `lean/PSL/MPT.lean`). **Phase G phase 2 extension (this PR):** `program_hash` migrated to BLAKE3-512 per ADR-0008. New `ProgramHash([u8; 64])` and `ProgramHashV1([u8; 32])` newtypes (not type aliases — compiler enforces no-mixing). `agent_contracts::program::v1`/`v2` modules with frozen v1 KAT (benign + 3 adversarial: distinct names, network ordering, empty-list). All 8 standard contracts compute and store both digests at `build()`-time; trait exposes `program_hash() -> [u8; 32]` (legacy) and `program_hash_v2() -> ProgramHash` (canonical). MIGRATION_GUIDE adds the load-bearing principle: long-lived irrevocable commitments → BLAKE3-512 newtype; ephemeral content hashes → BLAKE3-256. **Phase G phases 4-6 shipped:** hybrid X25519 + ML-KEM-768 KEM with forward-secret witness encryption (PRs #11-#13), agent-layer wire-format cascade switching Propose to ProgramHash + ProposalHash newtype (PR #14), cross-platform determinism CI matrix on x86_64 + aarch64 (PR #15), v1-Propose-rejected-by-v2-verifier test + pinned (ct, sk) → ss byte-identity oracle (PR #18). **Only remaining work:** external cryptographer review per ADR-0006 / ADR-0011 acceptance criteria — human-in-the-loop sign-off, scope defined in ADR-0006 (acceptance criteria) and ADR-0011 (combiner specifics). | _(this commit)_ |

## Deferred to v0.2

Real backlog items that v0.1.0 deliberately doesn't bundle. Each
entry has a documented decision (ADR) and explicit trigger
conditions for taking it up. Reviewed at v0.2 planning. Same
posture as gate 9 (BFT consensus, deferred per ADR-0002): the work
is real, the deferral is deliberate, the path back to it is
pre-thought.

| Item | Status | Decision + triggers |
| --- | --- | --- |
| Migrate sequencer state storage off `sled` (unmaintained KV store: last meaningful release 2021, known crash-recovery correctness issues, lead maintainer's rewrite stalled) | ⏸ deferred | **[ADR-0012](decisions/0012-defer-sled-migration-to-v0.2.md)** — four trigger conditions: (1) first pilot deployment surfaces operational requirements, (2) bundles with audit-findings remediation, (3) reproducible `sled` corruption observed in PSL's own workload, (4) v0.2 planning revisit. Current posture OK because no data loss observed in PSL's actual usage and migration is a one-way door touching the sequencer's hottest path, MPT, WAL, and recovery procedure — not the kind of work to bundle into v0.1.0 right before external review. Two leading backend candidates (`rust-rocksdb`, `redb`) listed in the ADR without prejudging the choice. |
| TPS bench maturation: perf-CI auto-regression gate + direct measurement of the real ternary VM in the sequencer trace path (currently `NativeTraceExecutor` synthetic stub; ~1,750 tx/s end-to-end is composed-arithmetic, not measurement) | ⏸ deferred | **[ADR-0013](decisions/0013-defer-tps-bench-maturation-to-v0.2.md)** — four trigger conditions: (1) first pilot's TPS-SLA requirement establishes a number to defend with measurement not composition, (2) repeated bench runs show regressions the manual-discipline review missed, (3) audit findings require sequencer-path changes that bundle naturally, (4) v0.2 planning revisit. PR #25 already shipped the near-term measurement-floor work (percentiles + hardware spec capture); the deferred items are the credibility-hardening operational work (dedicated runner pool with pinned hardware for the perf-CI gate; real ternary-VM wiring for the direct measurement). |

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

**Status: CLOSED — the formal layer is sorry-free.** All previously-tracked
`sorry`s in load-bearing theorems are now proven and machine-checked in Lean
4.12.0 (no `sorry`, no `native_decide`). History retained for traceability.

| File | Theorem | Resolution |
| --- | --- | --- |
| `PSL/Conservation.lean` | `transfer_conserves` | ✅ Proven. The original was false as stated; proven under the genuinely-required `WellKeyed` invariant, `live.Nodup`, and distinct in-set endpoints. Generalized to conserve every asset. No-axiom `decide` counterexample (`transfer_not_conserves_without_nodup`) shows `Nodup` is necessary. Axioms: `propext`, `Quot.sound`. |
| `PSL/Conservation.lean` | `freeze_conserves` | ✅ Proven under the `WellKeyed` state invariant (the model writes accounts at `a.pubkey`, so a mis-keyed state breaks conservation; counterexample `freeze_not_conserves_without_wellkeyed`). Axioms: `Quot.sound`. |
| `PSL/Conservation.lean` | `supply_changes_only_via_authority` | ✅ Proven. The original conclusion was *vacuous* (constant `"mint"`, `h_change` unused; see `original_authority_conclusion_is_vacuous`). Restated so a supply change forces `tx` to be a mint or burn, with `h_change` genuinely load-bearing. Axioms: `propext`, `Quot.sound`. |
| `PSL/MPT.lean` | `inclusion_proof_sound` | ✅ Proven. The original conclusion (`value.length ∈ {0,64}`) was ill-posed — no verifier enforces value length. Replaced by the correct soundness property, **value binding**: a committed `(root, key)` pins a unique value (forging another value that verifies breaks collision-resistance). `verifyProof` now mirrors `crypto/src/smt.rs::verify_proof` (recompute the root by folding the 256 key-bit-ordered siblings). Axioms: `propext`, `Classical.choice`, `Quot.sound`, `hash_collision_resistant`, `hash_length`. |

Notes:
- **Determinism** theorems (`PSL/Determinism.lean`) are by-construction
  trivial because Lean functions are deterministic; the operational
  determinism between Lean and C/WASM is checked empirically by gate 1.
- **MPT.lean** treats `hash` as `opaque` with two explicit crypto axioms
  (`hash_collision_resistant`, `hash_length` — collision resistance and the
  fixed 32-byte BLAKE3-256 digest). This is the standard hash-modeling
  approach; the soundness result is conditioned on these assumptions, which
  is the honest and expected form for a hash-based proof.
- The `tools/check_lean_drift.py` checker exists but is not wired into
  CI; add a pre-commit hook before any production deployment to prevent
  silent C/Lean divergence.

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
