# Documentation index

Canonical entry point to the PSL documentation surface. Every
non-third-party Markdown file in the repository is listed here, grouped
semantically. Whenever a doc is added, moved, or removed, this file
updates in the same commit (per `GOVERNANCE.md`).

**Last refresh: 2026-05-09 (v0.1.0 cut + Phase H docs cleanup).**

## Start here

- [README](../README.md) — Repository overview. Leads with the agent
  layer and dispute-by-re-execution.
- [STATUS.md](STATUS.md) — Gate-by-gate ground truth. Carries the
  "last verified" line; re-verified weekly.
- [REPRODUCE.md](../REPRODUCE.md) — Two-tier reproduction guide
  (Tier 1: ~5 min Rust-only; Tier 2: longer, adds Lean + legacy gate-1
  sweep).

## Architecture & design

- [ARCHITECTURE.md](ARCHITECTURE.md) — System architecture across all
  layers. Starts with the trust boundary and trace-hash contract.
  See § 0 first.
- [SOVEREIGN_MODE_TRUST.md](SOVEREIGN_MODE_TRUST.md) — Documented
  trust assumption for v0.1.x sovereign-mode operation. Companion to
  ADR-0002 (BFT consensus deferral).
- [STYLE_GUIDE_v3.md](STYLE_GUIDE_v3.md) — *Historical.* Style guide
  for the original WASM-primitive workflow (gate 1 era). Superseded
  for new work by the ternary VM kernel; preserved because some
  primitives in `primitives/` are still maintained under v3 rules and
  CHANGELOG references the migration.

## Status, reproducibility, and history

- [STATUS.md](STATUS.md) — see above.
- [REPRODUCIBILITY_REPORT.md](REPRODUCIBILITY_REPORT.md) — Pinned
  toolchain, per-gate command, expected timing on the Phase 2 closure
  reference VM.
- [SESSION_SUMMARY.md](SESSION_SUMMARY.md) — Outside-reviewer-readable
  summary of the v0.1.0 closure session (Phases A-F).
- [CHANGELOG.md](../CHANGELOG.md) — Per-release history.
- [FINDINGS.md](FINDINGS.md) — Empirical findings from gate
  verification. *Some sections superseded by the ternary VM pivot;
  superseded sections are marked inline rather than removed (per docs
  refresh policy: history is preserved).*

## Security & compliance

- [SECURITY.md (top-level)](../SECURITY.md) — How to report
  vulnerabilities. Disclosure timeline. Scope.
- [SECURITY.md (docs)](SECURITY.md) — Threat model summary.
- [SECURITY_REVIEW.md](SECURITY_REVIEW.md) — Audit-package security
  review. Adversary inventory, crypto primitive selection table, side-
  channel + memory-zeroing inventories.
- [UNWRAP_AUDIT.md](UNWRAP_AUDIT.md) — Audit of every `unwrap()` /
  `expect()` in production paths (40 hits, 0 bugs, 35 lock-poison + 3
  documented checked-add + 2 covered by other audits).
- [SAFETY.md](SAFETY.md) — Memory-safety posture. **Zero `unsafe`
  blocks on PSL production paths.** Per-crate inventory + transitive-
  dep `unsafe` posture + lint-hardening roadmap.
- [LICENSE_REVIEW.md](LICENSE_REVIEW.md) — Dependency license audit
  (217 transitive deps, all permissive, zero unknowns, zero GPL/AGPL).
  Companion to `cargo-deny check licenses`.
- [COMPLIANCE.md](COMPLIANCE.md) — Compliance / regulatory posture
  (travel rule, freeze authority, view keys).
- [FUZZING.md](FUZZING.md) — Five fuzz harness inventory + how to run
  campaigns offline + how CI schedules them.
- [AUDIT_BRIEF.md](AUDIT_BRIEF.md) — Auditor's day-1 entry document.
- [AUDIT_FINDINGS.md](AUDIT_FINDINGS.md) — Canonical tracker for
  audit + incident findings. Empty as of v0.1.0; populated on first
  finding from gate-17 audit or runbook-driven incident.
  In-scope crate list, threat model summary, where the trust boundaries
  are.

## Operations

- [OPERATIONAL_READINESS.md](OPERATIONAL_READINESS.md) — Service
  inventory, SLOs, alert thresholds, deployment pre-flight checklist.
- [DR_DRILL_PLAN.md](DR_DRILL_PLAN.md) — Pre-committed quarterly
  disaster-recovery drill protocol with explicit pass/fail criteria.
- [runbooks/](runbooks/) — Six per-incident-class runbooks:
  - [consensus-halt.md](runbooks/consensus-halt.md)
  - [sequencer-key-compromise.md](runbooks/sequencer-key-compromise.md)
  - [dispute-storm.md](runbooks/dispute-storm.md)
  - [follower-lag.md](runbooks/follower-lag.md)
  - [light-client-divergence.md](runbooks/light-client-divergence.md)
  - [dr-restore.md](runbooks/dr-restore.md)

## Migration & integration

- [MIGRATION_GUIDE.md](MIGRATION_GUIDE.md) — Cross-version migration
  path for SDK consumers and chain operators. Covers v0.1.0 → v0.1.x
  (trace_hash v1 → v2, ed25519 → hybrid signatures) and the planned
  v0.1.x → v0.2 migrations (program_hash bump, state-tree
  hash-of-pubkey, hybrid required).

## Architectural decisions (ADRs)

ADRs are immutable once Accepted. To change a decision, write a new ADR
that supersedes the old. See [GOVERNANCE.md](../GOVERNANCE.md) § ADR
process.

- [ADR-0001](decisions/0001-retire-legacy-fp64-runner.md) — Retire
  legacy fp64 Rust runner; ternary VM is canonical.
- [ADR-0002](decisions/0002-bft-consensus-engine-selection.md) — BFT
  consensus engine selection deferred to v0.2 with three concrete
  trigger conditions and 60-day SLA from any trigger fire.
- [ADR-0003](decisions/0003-publication-strategy-v0.1.0.md) —
  Publication strategy for v0.1.0 (repo announce → whitepaper → social).
- [ADR-0004](decisions/0004-public-test-network-deferred.md) — Public
  test network deferred to v0.2 (cannot operate one under audit-pending
  + DR-drill-pending posture).
- [ADR-0005](decisions/0005-licensing-export-patent-posture.md) —
  Licensing (MIT), export-control (EAR § 742.15(b) publicly-available
  carveout), patent posture (defensive non-assertion).
- [ADR-0006](decisions/0006-post-quantum-cryptography-strategy.md) —
  Post-quantum hybrid strategy (ed25519 + ML-DSA-65 / X25519 +
  ML-KEM-768; FN-DSA excluded for fp incompatibility).
- [ADR-0007](decisions/0007-cryptographic-agility-architecture.md) —
  Cryptographic agility: Scheme/Signer/Verifier/Kem/HashScheme traits +
  varint scheme prefixes + hash-of-pubkey state-tree storage.
- [ADR-0008](decisions/0008-blake3-512-for-long-lived-commitments.md)
  — BLAKE3-512 only for long-lived irrevocable commitments
  (`weights_hash`, long-lived `program_hash`).
- [ADR-0011](decisions/0011-hybrid-kem-x25519-mlkem768.md) —
  Hybrid X25519 + ML-KEM-768 KEM with HKDF-SHA-512 transcript
  combiner, forward-secret per-witness ephemeral keypairs,
  decapsulation total at the type level (implicit rejection per
  FIPS 203 §6.3).
- [ADR-0012](decisions/0012-defer-sled-migration-to-v0.2.md) —
  Defer sequencer storage migration off `sled` to v0.2 with four
  trigger conditions; two leading backend candidates
  (`rust-rocksdb`, `redb`) listed without prejudging.
- [ADR-0013](decisions/0013-defer-tps-bench-maturation-to-v0.2.md)
  — Defer TPS bench maturation (perf-CI auto-regression gate +
  direct measurement of the real ternary VM in the sequencer trace
  path) to v0.2 with four trigger conditions. PR #25's percentile +
  hardware-spec improvements shipped first as the near-term
  measurement-floor work.

ADR numbers 0009 and 0010 are unused; the sequence skips to 0011.
Future ADRs continue from 0014.

## External-facing artifacts

- [blog/agent-layer-launch.md](blog/agent-layer-launch.md) — Launch
  blog post draft for v0.1.0.
- [whitepaper/PSL.md](whitepaper/PSL.md) — Whitepaper draft for
  arXiv submission per ADR-0003.

## Outreach (audit engagement)

- [outreach/](../outreach/) — Three drafted engagement-letter
  request emails for Trail of Bits / Zellic / OtterSec. Awaits human
  signature + send.

## SDK examples

- [Rust (canonical)](../agent_sdk/examples/) — `trader_agent.rs`
  (happy path) + `service_agent.rs` (dispute path).
- [Python via UniFFI](../sdk-examples/python/) — same scenarios.
- [TypeScript via napi-rs](../sdk-examples/typescript/) — same
  scenarios.
- [SDK examples README](../sdk-examples/README.md) — Cross-language
  overview.

## Component-local READMEs

- [primitives/README.md](../primitives/README.md) — Original
  C-primitive sources (gate 1 / Phase 1 era).
- [pilot/issuer_demo/README.md](../pilot/issuer_demo/README.md) —
  End-to-end pilot binary.
- [legacy/rust_runner/README.md](../legacy/rust_runner/README.md) —
  Frozen per ADR-0001; do not extend.
- [lean/README.md](../lean/README.md) — Lean 4 + mathlib
  formalization.
- [infra/README.md](../infra/README.md) — Reference Terraform deploy.

## Top-level governance & meta

- [LICENSE](../LICENSE) — MIT (per ADR-0005).
- [GOVERNANCE.md](../GOVERNANCE.md) — Decision-making, ADR process,
  release process.
- [MAINTAINERS.md](../MAINTAINERS.md) — Current maintainer set.
- [CONTRIBUTING.md](../CONTRIBUTING.md) — How to contribute.
- [CODE_OF_CONDUCT.md](../CODE_OF_CONDUCT.md) — Expected and
  unacceptable behavior.

## Historical / superseded

These exist for git-history-cross-referencing purposes. They describe
work or analysis that is no longer current. They are kept (not deleted)
because CHANGELOG and ADR text reference them; deleting would create
broken links into the historical record.

- [CONSENSUS_DECISION.md](CONSENSUS_DECISION.md) — Original gate-9
  consensus vendor evaluation. **Superseded by ADR-0002** which is
  the current authoritative decision; this file is the supporting
  evaluation.
- [STYLE_GUIDE_v3.md](STYLE_GUIDE_v3.md) — Original WASM-primitive
  style guide. **Superseded for new work** by the ternary VM kernel;
  retained for the primitives still maintained under v3 rules.
- [UPSTREAM_BUG_lower_py_runtime_or.md](UPSTREAM_BUG_lower_py_runtime_or.md) —
  Upstream-bug report against `Transformer-VM` for `i32.or` lowering.
  **No longer load-bearing**: PSL no longer depends on the affected
  code path (gate 8 retirement per ADR-0001 means the WASM lowering
  pipeline is no longer in the canonical execution path). Retained as
  historical record of the issue we found.

## What's NOT in this index

- Everything under `lean/.lake/packages/` — third-party Lean/mathlib
  dependencies.
- `.pytest_cache/README.md` — auto-generated by pytest, not authored
  by us.
- Inline rustdoc — not Markdown; browse via `cargo doc --workspace
  --open`.
