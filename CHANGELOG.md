# Changelog

Human-readable history of PSL milestones. Per-gate entries point at the
load-bearing commit on `origin/main`.

## [Unreleased] — post-v0.1.0 work

### Changed — Formal-verification layer hardened to sorry-free + CI-gated

The Lean formal layer is now **sorry-free** and its trust boundary is
machine-enforced. (Supersedes the "Gate 3 cleared — 3 sorrys remain" entry
below, which records the state at commit `113c11b`.)

- **Conservation theorems corrected and proven.** An audit found the originals
  were unsound or vacuous as stated: `freeze_conserves` needed a `WellKeyed`
  state invariant, `transfer_conserves` needed `live.Nodup` + endpoint
  conditions, and `supply_changes_only_via_authority` was vacuous (its
  conclusion held for the constant `"mint"`). All three now proven under the
  genuinely-required hypotheses, with no-axiom `decide` counterexamples
  proving each hypothesis necessary.
- **MPT `inclusion_proof_sound` proven as value binding.** The original
  conclusion (`value.length ∈ {0,64}`) was ill-posed — no verifier enforces
  it. Replaced by the real soundness property: a committed `(root, key)` pins
  a unique value (forging another that verifies breaks collision-resistance).
  `verifyProof` now mirrors `crypto/src/smt.rs::verify_proof`.
- **New invariants** (`lean/PSL/LedgerInvariants.lean`): mint/burn change
  supply by exactly the authorized amount; frozen senders cannot move funds;
  successful transfers advance the sender nonce.
- **Functional SMT model** (`lean/PSL/SMTModel.lean`): the tree as a pure
  function of the key→value map, faithful to `crypto/src/smt.rs`. Proves
  inclusion-proof **completeness** (honest proofs verify — no
  collision-resistance needed), the capstone **correctness** (any verifying
  proof carries exactly the stored value; non-inclusion soundness falls out
  as the empty-value case), and spec-level root **order-independence**.
- **Block-level supply accounting** (`lean/PSL/BlockAccounting.lean`): over
  any block, `supply_before + minted = supply_after + burned` exactly, for
  every asset — plus the corollary that a block with no mint/burn cannot
  change supply. Includes the `WellKeyed`-preservation lemmas (no axioms)
  that let per-tx theorems legitimately chain across a block.
- **In-build axiom-audit gate** (`lean/PSL/Audit.lean`) + `formal-verification`
  CI job: `lake build` now fails if any of the 13 load-bearing theorems gains a
  disallowed axiom (`sorryAx`/`Lean.ofReduceBool`/unlisted axiom) or goes
  missing. Trust boundary documented in `VERIFICATION.md`.

### Added — Post-quantum cryptography (gate 19 → 🟢, PRs #11-#15 + #18)

The full ADR-0011 5-commit plan plus follow-up tests shipped to main.
Hybrid post-quantum cryptography is now load-bearing in v0.1.0.

- **PR #11** — Spec + skeleton for hybrid X25519 + ML-KEM-768 KEM
  (types, trait surface, no impl yet). Reviewable against ADR-0011
  before cutting impl.
- **PR #12** — `HybridX25519MlKem768Kem` impl. Decapsulation total at
  the type level (implicit rejection per FIPS 203 §6.3); 6 KEM tests +
  4 from-bytes-never-panic proptests.
- **PR #13** — Witness encryption impl per ADR-0011 § "AEAD layer":
  HKDF-SHA-512 transcript-binding combiner, AES-256-GCM, per-witness
  ephemeral hybrid keypairs zeroized on drop. 7 of 8 ADR-0011 blocking
  tests (round-trip, forward secrecy, implicit rejection, component
  swap, wrong-context, zeroization, edge sizes).
- **PR #14** — Agent-layer wire-format cascade. `Propose.program_hash`
  widened from 32B to 64B `ProgramHash`; new `ProposalHash` newtype
  (not a type alias) so the compiler refuses to mix it with other
  32-byte digests. Tag bumped to `b"PSL-PROPOSE-V2"` for cross-version
  isolation.
- **PR #15** — Cross-platform determinism CI matrix. Workflow runs the
  full crypto_agility test suite + a pinned cross-platform fixture
  (BLAKE3-256 of HKDF salt + 3 context strings) on
  `ubuntu-24.04` (x86_64) and `ubuntu-24.04-arm` (aarch64) GitHub-
  hosted runners. Test count after PR #15: 252 workspace tests pass
  on both architectures.
- **PR #18** — Cross-version isolation + pinned-decap byte-identity
  oracles. Two new tests:
  (1) `v1_shaped_propose_rejected_by_v2_verifier` documents the
  cryptographic isolation between v1 and v2 Propose envelopes (the
  tag is in the signed transcript; a v1 signature cannot validate
  against v2 canonical bytes).
  (2) `pinned_decap_byte_identical_across_architectures` locks the
  strongest cross-platform property the KEM can hold by pinning a
  fixed (sk, ct) → ss triple to a hex constant; CI verifies on both
  architectures.
- **ADR-0011** ratifying the hybrid KEM design + 5-commit plan.
- **ADR-0012** deferring sequencer storage migration off `sled` to
  v0.2 with four explicit trigger conditions and two leading backend
  candidates (`rust-rocksdb`, `redb`) listed without prejudging.

Workspace test count is now **254** on both x86_64 and aarch64.

### Added — Documentation (PRs #16, #17, #19, #20, this PR)

- **PR #16** — Whitepaper `docs/whitepaper/PSL.md` gained §7
  "Post-quantum cryptographic readiness" (7 subsections covering
  threat model, strategy, algorithm choices, hybrid composition with
  combiner specifics, forward-secrecy lifecycle, implementation
  status with the per-PR shipping table, and remaining work).
  `docs/AUDIT_BRIEF.md` gained §4 "Post-Quantum Readiness" so
  external auditors see the PQ posture before reaching the test
  artifacts section. Subsequent sections renumbered.
- **PR #17** — Whitepaper §7.6 / §7.7 framing tightening: clarifies
  that NIST audited the C reference implementations in PQClean (the
  Rust binding crates wrap that C code and have not been
  independently audited as Rust crates); replaces an off-target
  ADR-0003 cite with ADR-0006 / ADR-0011.
- **PR #19** — ADR-0012 (sled-migration deferral, see above).
- **PR #20** — `docs/STATUS.md` gained "## Deferred to v0.2"
  section pointing at ADR-0012, mirroring how gate 9 (BFT, deferred
  per ADR-0002) is surfaced.
- **This PR** (v0.1.0 cleanup pass) — repo-wide staleness sweep:
  README crypto framing now mentions hybrid PQ from v0.1.0; gate 19
  status reflects 🟢 in README + STATUS header; whitepaper §7.6
  shipping table extended with PR #18 row + total bumped 252 → 254;
  STATUS.md gate-19 row reflects shipped-not-pending state;
  REPRODUCIBILITY_REPORT gains `Last verified` line; INDEX.md gains
  ADR-0011 + ADR-0012 + AUDIT_FINDINGS entries + 0009/0010-gap note;
  new `docs/AUDIT_FINDINGS.md` placeholder created (was referenced
  by 7 docs but didn't exist).

### Added — Original Phase G phase 1 (gate 19 → 🟡 phase, now superseded by PRs #11-#15)

The first cryptographic-agility infrastructure work, kept here for
historical traceability. Superseded by the full PQ migration above.

- **Cryptographic agility infrastructure**. New crate
  `crypto_agility/` with `Scheme`/`Signer`/`Verifier`/`Kem`/
  `HashScheme_` traits, varint scheme prefixes (LEB128),
  `VerifierPolicy` presets (ed25519-only / ed25519_or_hybrid /
  hybrid_only), `Ed25519Signer` + `Ed25519Verifier` impls,
  `Blake3_256` + `Blake3_512` impls. 23 unit tests + 6 proptest
  invariants. Ratifying ADRs:
  - **ADR-0006** post-quantum strategy (hybrid ed25519+ML-DSA-65
    sigs, hybrid X25519+ML-KEM-768 KEM, FN-DSA excluded for fp
    incompat).
  - **ADR-0007** cryptographic agility architecture (varint
    prefixes, hash-of-pubkey state-tree storage, explicit
    UnknownScheme rejection).
  - **ADR-0008** BLAKE3-512 only for long-lived irrevocable
    commitments (`weights_hash`, long-lived `program_hash`).
- **`docs/INDEX.md`** — canonical entry point listing every non-
  third-party markdown file in the repo, grouped semantically.
  Updates in the same commit as any doc add/move/remove (per
  `GOVERNANCE.md`).

### Changed
- **Workspace license** corrected from `Apache-2.0` to `MIT` per ADR-0005
  (inconsistency caught during Phase G).
- **Documentation refresh (Phase H)**: load-bearing docs gained explicit
  status notes pointing to current authoritative sources where the
  original framing is superseded:
  - `docs/STATUS.md` — header rewritten from gate-4-era framing
    ("gates 1-4 cleared, 5-7 next") to v0.1.0 reality (gates 1-16 ✅,
    17-18 🟢, 19 🟡). Added required "last verified" line.
  - `docs/ARCHITECTURE.md` — added "last refreshed" line; updated § 1
    decisions recap to drop retired Phase 1.5 PyO3 framing; renumbered
    duplicate `## 5` and `### 4.x` subsections; updated § 7 verification
    gates table to all 19 gates with ADR cross-references; added new
    § 8 "Agent execution layer (Phase 2)" and § 9 "Cryptographic
    agility layer (Phase G)"; trimmed § 10 "Open contracts" to what's
    actually still open.
  - `docs/FINDINGS.md`, `docs/SECURITY.md`, `docs/CONSENSUS_DECISION.md`,
    `primitives/README.md` — top-level status notes pointing to current
    authoritative docs (ternary-canonical, ADR-0002, ADR-0006, etc.).
    Original content preserved as historical record.

### Documentation policy
- Stale references are **flagged with authoritative pointers**, not
  deleted. Git history preserves what was true; INDEX.md and inline
  status notes make current-vs-historical explicit.
- Going forward (per `GOVERNANCE.md`): every PR that changes code
  includes the doc updates required by the change. CI (in design)
  enforces for at least README, CHANGELOG, and any directly-affected
  doc.

---

## v0.1.0 — 2026-05-09 — Audit hand-off release

The first release tag for PSL. The core Phase 1 (settlement layer, gates
1-9) and Phase 2 (agent execution layer, gates 10-16) work is closed.
Gates 17 (external audit) and 18 (DR drill) are at 🟢 — the material
is shipped and reviewable, awaiting human action (signed engagement
letter; first scheduled drill on staging). Per ADR-0003, this tag
triggers the audit and the publication sequence.

### Added
- **Phase 2 agent execution layer** (gates 10-16). Ternary integer
  contract VM (`ternary_vm/`); 8-contract standard library
  (`agent_contracts/`); SLIP-0010 wallet + spending policies +
  revocation (`agent_wallet/`); 5-message negotiation protocol +
  dispute-by-re-execution (`agent_protocol/`); SDK with reference
  agents in Rust + Python + TypeScript bindings (`agent_sdk/`,
  `sdk-examples/`).
- **Property and adversarial test corpus** — proptest invariants for
  wallet (5 properties) and ternary VM (11 properties); 7 adversarial
  dispute scenarios on the protocol layer (replay, malformed witness,
  stale, sybil, griefing, cross-proposal, illegal-transition state
  preservation).
- **Five fuzz harnesses** (`docs/FUZZING.md`): unpack_weights,
  byte_add_run, decode_protocol_message, transfer_run, swap_run.
  CI-scheduled per `.github/workflows/fuzz.yml`.
- **Audit hand-off package**: `docs/AUDIT_BRIEF.md`,
  `docs/SECURITY_REVIEW.md` (extended with adversary inventory,
  cryptographic primitive selection, side-channel resistance, memory
  zeroing), `docs/REPRODUCIBILITY_REPORT.md`, `docs/UNWRAP_AUDIT.md`,
  `outreach/audit-engagement-{trail-of-bits,zellic,ottersec}.md`.
- **Production operations stack** (gate 18): six runbooks
  (`docs/runbooks/`), full observability stack (`ops/` —
  Prometheus/Grafana/Alertmanager/Loki/Promtail/Tempo + 11 PromQL
  alerts), backup automation with dual-tier hot/cold storage and
  BLAKE3-verified manifests (`tools/backup.sh`), load-test scaffold
  (`tools/load_test.sh`), pre-committed DR drill protocol
  (`docs/DR_DRILL_PLAN.md`), reference Terraform infra (`infra/`).
- **CI/CD**: `.github/workflows/{ci,security,fuzz}.yml`,
  `.github/dependabot.yml`. Three categories of CI lint:
  build/test/clippy/fmt; cargo-audit/cargo-deny/SBOM; nightly fuzz
  campaigns.
- **Governance scaffolding**: `MAINTAINERS.md`, `GOVERNANCE.md`,
  `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, top-level `SECURITY.md`.
- **External-facing artifacts**: README rewritten to lead with the
  agent layer; launch blog post (`docs/blog/agent-layer-launch.md`);
  whitepaper draft (`docs/whitepaper/PSL.md`) for arXiv submission
  per ADR-0003.

### Changed
- **Gate 8 closed via retirement** (ADR-0001). The legacy fp64
  `rust_runner/` was moved to `legacy/rust_runner/` with `#[deprecated]`
  markers; the pure-Rust ternary kernel (`ternary_vm/`) is the
  canonical execution engine; PyTorch+MKL parity is architecturally
  out-of-scope (MKL's reduction-order opacity is incompatible with
  bit-exact verification). CI guard `tools/ci/check_legacy_isolation.sh`
  prevents new dependencies on the retired code.
- **README rewritten** to lead with the agent transaction layer rather
  than the original transformer-trace narrative. Status table
  consolidated; per-gate detail stays in `docs/STATUS.md`.

### Architectural decisions
- **ADR-0001** — Retire legacy fp64 Rust runner.
- **ADR-0002** — BFT consensus engine selection: defer to v0.2 with
  three concrete trigger conditions and 60-day SLA from any trigger
  fire.
- **ADR-0003** — Publication strategy for v0.1.0 (repo announce →
  whitepaper → social, in that order).
- **ADR-0004** — Public test network deferred to v0.2 (cannot operate
  one under audit-pending + DR-drill-pending posture).
- **ADR-0005** — Licensing (MIT), export-control (EAR § 742.15(b)
  publicly-available carveout), patent posture (defensive
  non-assertion). Subject to legal review before any v0.2 dependencies.

### Status at v0.1.0 cut

| Gate | Description                               | Status |
| ---  | ---                                       | ---    |
| 1    | Primitive bit-exact (10k vectors each)    | ✅ |
| 2    | Crypto + SMT determinism                  | ✅ |
| 3    | Lean lake build                           | ✅ |
| 4    | Sequencer + 3 followers, 100 blocks       | ✅ |
| 5    | Compliance enforcement                    | ✅ |
| 6    | Light client cross-verifies               | ✅ |
| 7    | End-to-end pilot                          | ✅ |
| 8    | Pure-Rust runner canonical (ADR-0001)     | ✅ |
| 9    | BFT consensus (deferred per ADR-0002)     | ⏸ |
| 10   | Ternary execution engine                  | ✅ |
| 11   | Contract DSL standard library             | ✅ |
| 12   | Identity & wallet                         | ✅ |
| 13   | Negotiation protocol                      | ✅ |
| 14   | Dispute resolution                        | ✅ |
| 15   | Reference agents                          | ✅ |
| 16   | SDK 0.1.0                                 | ✅ |
| 17   | External security audit                   | 🟢 awaits engagement letter |
| 18   | Production-readiness                      | 🟢 awaits first DR drill |

Per `docs/STATUS.md` for command + output + commit-hash detail.

---

## 2026-05-04

### Gate 8 — Rust runner ratified canonical for trace-hash production

`docs/ARCHITECTURE.md § 0.3` pins three engines in canonical / secondary /
tertiary ordering: pure-Rust runner (canonical) → C++ engine (secondary,
algorithmically identical) → PyTorch+MKL (tertiary, may diverge on long
matmuls due to MKL's reduction-order opacity). Trace-hash production
must use the canonical engine; tertiary engines must match it, not the
other way around.

`tools/run_canonical_gate1.sh` re-runs the gate-1 vector set under the
canonical engine: 10k vectors × 5 short primitives = 50000/50000 (0 fail)
plus the chained `freeze_setup → freeze_apply` pipeline at the largest
count that fits in wall budget. `tests/test_bit_exact.py` defaults to
the Rust engine (`PSL_VERIFY_ENGINE=rust`); set `cpp` for cross-validation
against the secondary engine.

`bin/run_gate1` gains rayon-based witness-level parallelism (`--threads N`)
and a `freeze_chain` primitive that does setup → apply end-to-end.

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
