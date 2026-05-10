# ADR-0013 — TPS bench maturation: defer perf-CI gate + real-trace measurement to v0.2

**Status:** accepted.
**Date:** 2026-05-10.
**Deciders:** PSL maintainers.

## Context

PR #23 shipped `bench_sequencer_tps_10k_blocks` as an `#[ignore]`d
regression bench (in `sequencer/tests/integration.rs`) producing the
sequencer TPS numbers that v0.1.0's client-facing surfaces (README,
AUDIT_BRIEF, whitepaper §9, launch blog) lead with: ~925 tx/s
4-replica, ~3,990 tx/s single-replica.

PR #25 added the two near-term improvements to that bench: tail-
latency percentiles (p50 / p95 / p99 / p99.9 / max) and pinned
hardware-spec capture (`uname -a` + relevant `lscpu` fields recorded
at run time). These materially improved how partners read the
existing numbers — p99.9 and pinned hardware are exactly what
capacity-planning conversations require.

Two follow-ups remain from the engineer-reviewer's PR #23 sign-off
list:

1. **Automated perf regression gating.** The bench is `#[ignore]`d,
   so a future PR that drops single-replica TPS from 3,990 to 100
   does not fail default CI. The PR descriptions and STATUS.md say
   "meaningful regressions would block a release" — but that's
   manual discipline, not an automated guardrail.
2. **Direct measurement with the real ternary VM.** The bench uses
   `NativeTraceExecutor` (a deterministic stub). The composed
   estimate of ~1,750 tx/s end-to-end with real ternary trace is
   currently arithmetic (251 µs sequencer baseline + 34 trace-hashes
   × ~9.5 µs each from gate-10's `byte_add` throughput), not
   measurement. Directly wiring the ternary VM into the sequencer's
   trace path and re-running the bench would resolve composition
   uncertainty empirically.

For v0.1.0 we have to decide whether to ship either now.

## Decision

**Defer both to v0.2.0.** PR #25 closed the two near-term
improvements (percentiles + hardware spec). The perf-CI gate and the
real-trace measurement are real work, deliberately deferred with
explicit triggers, with the rationale documented for external
reviewers.

## Rationale

### 1. Perf-CI gate requires hardware-pinned infrastructure, not just code

The credibility of an automated regression threshold (e.g., "-20%
from baseline blocks merge") rests on a runner pool with consistent
hardware. GitHub-hosted runners share infrastructure; cross-runner
variance is on the order of 10-30% from background noise. A naive
threshold over GitHub-hosted runs would fire constantly on noise
without catching real regressions, OR be set so wide that real
regressions slip through.

The right shape is: dedicated perf-CI tier on a runner pool whose
hardware is pinned and known-quiet (no other workloads). That is
**infrastructure work** (runner provisioning, monitoring, cost
budget), not test-harness work. It bundles cleanly with v0.2's
operational maturation alongside ADR-0012's `sled` migration —
both touch deployment infrastructure rather than the v0.1.0
audit-pending core.

### 2. Real-trace measurement is a one-way door touching the sequencer hottest path

Replacing `NativeTraceExecutor` with the real ternary VM in the
sequencer's `apply_to_all` path requires:

- Loading and exposing real `TernaryNetwork` weights (from
  `ternary_vm/`) in the sequencer-test binary.
- Wiring the ternary VM call into the trace_hash computation
  (currently the stub's `execute()` returns a synthetic hash).
- Re-validating that all existing sequencer integration tests still
  pass with the real executor (gate 4 in particular).

That is substantive architectural work in the sequencer crate, not
a test-harness change. Bundling it into v0.1.0 risks introducing
new bugs into a stable codebase right before external review — the
same shape of "one-way door touching hottest path" rationale that
ADR-0012 used to defer the `sled` migration.

### 3. Neither is on the audit-blocking path

Gate 17 (external audit) and gate 19 (cryptographer review) examine
cryptographic constructions, consensus logic, protocol correctness,
and key-handling hygiene. Auditors do not typically rule on:

- Whether your perf-CI gate is automated vs manual (operational
  process discipline, not a security property).
- Whether your TPS number is composed-arithmetic vs direct-measured
  (capacity-planning concern, not a security property).

Deferring both does not delay any audit-track work. The composed
estimate is labeled as composed in every client-facing surface; the
manual-regression-discipline is documented in PR descriptions and
STATUS.md.

### 4. v0.1.0's measurement floor is already credible

The shipped numbers — 925 tx/s 4-replica, p99.9 = 4.20 ms on pinned
i7-7700 hardware — are honest, conservative (consumer-tier CPU from
2017), and reproducible (the bench prints captured hardware so any
re-run self-documents). The composed estimate is explicitly labeled
back-of-envelope. A partner conversation that requires more rigor
than this is the trigger condition for doing the v0.2 work, not a
reason to do it speculatively.

## v0.2 trigger conditions

Both follow-ups begin on the **first** of:

1. **First pilot's TPS-SLA requirement** establishes a number the
   project must defend with a measurement, not a composed estimate.
   Real-trace measurement happens first; perf-CI gating follows
   when there's enough run-history to set a threshold.
2. **Repeated bench runs (≥3 within a release cycle) show
   regressions** that the manual-discipline review missed. Trigger
   the perf-CI gate first; real-trace work follows when scope
   permits.
3. **Audit findings** from gate-17 require sequencer-path changes
   that bundle naturally with the trace-executor wiring work.
4. **v0.2 planning revisit.** Like ADR-0012's `sled` deferral, this
   ADR commits to revisiting the assumption at v0.2 planning time.
   Continuing manual discipline alone is not a sufficient reason to
   defer further if production usage is producing real numbers and
   real regression risk.

## Alternatives considered

For the perf-CI gate:

- **Standalone GitHub-hosted perf runner.** Considered. Rejected
  for the noise-floor reason above — cross-run variance on shared
  GitHub-hosted infrastructure swamps a useful regression
  threshold. The right alternative is a dedicated runner pool, not
  a shared one.
- **Criterion-based microbenchmark suite.** Considered. Covers
  parts (per-primitive throughput, per-component latency) but not
  the block-level integration the existing bench measures. Useful
  as a complement, not a substitute, for the integration bench.
  Could ship in v0.2 alongside the perf-CI gate.
- **Manual discipline only (current state).** Rejected as the v0.2
  end-state because manual discipline doesn't catch silent
  regressions in unrelated PRs. Acceptable for v0.1.0 because the
  release scope and contributor count are small enough that manual
  review reliably catches changes to the sequencer hot path.

For real-trace measurement:

- **Compose post-hoc** (continue current approach). Rejected as the
  v0.2 end-state because composition arithmetic depends on
  parallelization vs serialization vs contention assumptions that
  aren't validated. Acceptable for v0.1.0 because composed numbers
  are explicitly labeled.
- **Use a smaller representative ternary network** (not real
  weights). Considered. Useful for incremental validation but
  doesn't resolve the question partners actually ask ("what's the
  TPS with the production model?"); just defers it. Better to do
  the real measurement once when v0.2 work begins.

## Consequences

- v0.1.0 ships with two documented technical debts in the bench
  posture (this ADR + ADR-0012 for `sled`), both with explicit
  rationale and v0.2 trigger conditions.
- External reviewers reading the v0.1.0 release see "deferred with
  rationale" rather than "missing without explanation" for both
  bench-maturation items.
- v0.2 scope grows by two substantial items, but the ADR makes
  those costs legible at planning time rather than discovered at
  execution time.
- The "deferred to v0.2" phrasing in client-facing surfaces (README,
  AUDIT_BRIEF, whitepaper §9, launch blog, STATUS.md) gets an
  explicit cite — pointers to this ADR — once this PR lands, in a
  follow-up small docs PR.

## Relationship to other ADRs

- **ADR-0012 (sled migration, deferred).** Same shape: real work,
  real reasons to defer, explicit triggers, alternatives noted
  without prejudging the v0.2 choice. Both are operational-tier
  deferrals that bundle naturally for v0.2 planning.
- **ADR-0002 (BFT consensus, deferred).** Same shape, different
  scope (consensus engine vs operational bench maturation).
- **ADR-0006 / ADR-0011 (cryptographer review).** Audit-track work,
  not blocked by this deferral.
