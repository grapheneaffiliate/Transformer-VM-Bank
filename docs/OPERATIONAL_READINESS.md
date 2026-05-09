# PSL Operational Readiness — Pre-Production Checklist

**Audience:** SRE / on-call rotation, incident commander, deployment
operator. **Status:** prepared 2026-05-09 against commit `91ed18a`.

This document is the gate-18 deliverable: the documented monitoring,
alerting, runbook, rollback, and disaster-recovery plan that any
production deployment must satisfy before going live with real value.

---

## 1. Service inventory + SLOs

| Service | Long-running? | SLO target | Owner |
|---|---|---|---|
| Sequencer (`psl-sequencer`) | yes | 99.9% block production within 2× expected interval | platform |
| Followers (≥3) | yes | 99% follower-vs-sequencer state-root agreement at every block | platform |
| Light client API | yes | p99 inclusion-proof verification < 50 ms | platform |
| Agent SDK runtime (per agent) | depends | per-agent SLO; not platform-owned | agent operator |

## 2. Prometheus metrics

Each long-running component exports `/metrics` over TCP. Names follow
the `psl_<component>_<noun>_<unit>` convention.

### Sequencer

```text
psl_sequencer_block_height_total            counter   blocks produced since boot
psl_sequencer_block_interval_seconds        histogram wall time between consecutive blocks
psl_sequencer_block_size_txs                histogram tx count per block
psl_sequencer_mempool_depth                 gauge     pending tx count
psl_sequencer_mempool_admit_failures_total  counter   {reason="sig_invalid"|"policy"|"revoked"|"overflow"}
psl_sequencer_state_root_mismatches_total   counter   any time follower != sequencer (incident-grade)
psl_sequencer_trace_runs_total              counter   {primitive=…} ternary forward-pass invocations
psl_sequencer_trace_duration_seconds        histogram {primitive=…}
```

### Light client

```text
psl_light_client_proof_verify_seconds       histogram p50/p95/p99 inclusion-proof verification latency
psl_light_client_proof_verify_failures_total counter  bad-proof attempts (incident-grade if sustained)
psl_light_client_block_header_age_seconds   gauge     stale-header detection
```

### Agent SDK (when running)

```text
psl_agent_proposals_total                   counter   {dir="in"|"out"}
psl_agent_proposals_terminal_total          counter   {state="executed"|"rejected"|"expired"}
psl_agent_disputes_opened_total             counter
psl_agent_disputes_resolved_total           counter   {outcome="slash"|"dismiss"}
psl_agent_spending_window_used               gauge     amount used in current rolling window
```

## 3. Alerting thresholds

PagerDuty / on-call:

| Alert | Threshold | Severity |
|---|---|---|
| Sequencer block not produced | `time() - psl_sequencer_block_height_total[5m]` flat for > 2× block interval | critical |
| State-root mismatch | `rate(psl_sequencer_state_root_mismatches_total[1m]) > 0` | critical (consensus halt) |
| Mempool overflow | `psl_sequencer_mempool_depth > 50000` | high |
| Sustained signature-verification failures | `rate(psl_sequencer_mempool_admit_failures_total{reason="sig_invalid"}[5m]) > 100` | high (possible attack) |
| Light-client bad-proof spike | `rate(psl_light_client_proof_verify_failures_total[5m]) > 10` | high |
| Trace runtime regression | `histogram_quantile(0.95, psl_sequencer_trace_duration_seconds_bucket) > 100ms` | medium |

Email / Slack only:

| Alert | Threshold | Severity |
|---|---|---|
| Stale block header on light client | `psl_light_client_block_header_age_seconds > 5 * block_interval` | low |
| Unusual spending-window utilization on hot agent | `psl_agent_spending_window_used / cap > 0.8` | low |

## 4. Runbooks

### 4.1 Sequencer halt (no block in 2× interval)

1. Page on-call.
2. SSH to sequencer host. `systemctl status psl-sequencer`. Read last
   60s of journalctl.
3. Common causes:
   - **Disk full**: `df -h /var/lib/psl`. Free space; restart sequencer.
   - **Mempool stuck on bad tx**: check `psl_sequencer_mempool_depth`
     trend. If frozen, drain mempool with `psl-sequencer drain-mempool
     --confirm` (irreversible — preserves canonical tx log).
   - **Trace runtime exception**: check journalctl for panic /
     `Overflow{...}` errors. Reproduce locally with the offending
     witness. File issue + revert to previous binary (see § 5
     rollback).
4. If unable to recover within 15 minutes: trigger DR plan (§ 6).

### 4.2 State-root mismatch

This is a **consensus halt** — sequencer and at least one follower
disagree on a state root. PSL stops producing blocks until resolved.

1. Page on-call (critical).
2. Capture diagnostic snapshot: `psl-sequencer dump-state-roots
   --last=10`. Compare against follower's `psl-light-client
   dump-state-roots --last=10`.
3. Identify the first divergent block. The witness for the offending
   tx in that block is the load-bearing artifact.
4. Re-run the witness through the canonical `TernaryProgram` (Layer 1
   ternary engine). If sequencer's output != ternary engine's output,
   the sequencer has a bug — roll back. If follower's output !=
   ternary engine's output, the follower has a bug — roll back the
   follower.
5. Post-incident: file a finding in `docs/AUDIT_FINDINGS.md` and a
   regression test against the offending witness.

### 4.3 Mempool overflow

1. Drop oldest unsigned/invalid txs first (existing eviction policy).
2. If overflow persists: temporarily raise mempool cap via
   `psl-sequencer set-mempool-cap <N>` and investigate the source.
3. If a single agent is the source: consider adding their pubkey to
   the rate-limit allowlist; coordinate with the agent operator.

### 4.4 Sequencer signing-key compromise (suspected)

This is a **break-glass** scenario — the sequencer's private key may
be in adversary hands.

1. Immediate: stop the sequencer (`systemctl stop psl-sequencer`).
2. Issue a parent-signed `KeyRotation` for the sequencer's child key.
   Wait for inclusion in the next block via the backup signing
   identity.
3. Re-deploy the sequencer with the new key. Verify the rotation
   landed via the light client.
4. Audit recent blocks for any unauthorized signed messages from the
   compromised key. Treat any found as void per the rotation's
   `rotated_at_unix` cutoff.

## 5. Rollback

Every release has a rollback path. The current PSL release pipeline:

1. Tag in git: `git tag -a v0.X.Y -m "release X.Y"`.
2. Build artifacts: `cargo build --workspace --release` →
   `target/release/{psl-sequencer,psl-runner,psl-issuer-demo}`.
3. Deploy via blue-green: previous binary kept at
   `/usr/local/bin/psl-sequencer.v0.X-1.Y`.
4. Rollback command: `systemctl stop psl-sequencer; ln -sf
   /usr/local/bin/psl-sequencer.v0.X-1.Y /usr/local/bin/psl-sequencer;
   systemctl start psl-sequencer`.

State migrations:

- **Reversible**: covered above (binary swap).
- **Irreversible** (e.g. MPT schema change): flagged in
  `docs/ARCHITECTURE.md` with a one-way notice. Take a full snapshot
  before applying; rollback path = restore from snapshot.

## 6. Disaster recovery

### 6.1 Backup schedule

- Sequencer state (MPT root + journal): hourly snapshot to S3-
  compatible store. Retention 7 days hot + 90 days cold.
- Block headers: continuous append to a separate object-store
  bucket; never overwritten.
- Sequencer signing keys: cold storage (HSM); rotation procedure
  documented in § 4.4.

### 6.2 Restore drill (run quarterly)

1. Spin up a fresh sequencer host from the same image.
2. Restore the latest snapshot.
3. Replay the block-header journal from the snapshot's height
   forward.
4. Verify state root at the latest height matches what the previous
   sequencer published.
5. Light client should converge within 1 block interval.

### 6.3 Sequencer-host total loss

1. Spin up replacement from image + snapshot per § 6.2.
2. Followers automatically reconnect on the sequencer's stable
   listening address (DNS-pinned).
3. Coordinate any agent operators whose long-poll connections drop.

## 7. Logging

Structured via the `tracing` crate. Production aggregation: any
ELK-equivalent stack via the `tracing-subscriber` JSON exporter.

Log levels:

- `error`: incident-grade. Always retained.
- `warn`: non-fatal but actionable.
- `info`: operational events (block produced, snapshot taken).
- `debug`: tx-flow detail. Off by default in production.
- `trace`: per-step ternary forward pass. Off by default; enable via
  `RUST_LOG=psl_ternary_vm=trace` for debugging only — emits ~kB per
  primitive invocation.

PII / sensitive data:
- Witness contents are logged at `trace` level only.
- Private keys never logged at any level (verified by `Zeroizing`
  wrapper — see `psl_agent_wallet::slip10`).

## 8. Pre-deployment checklist

Before any production deployment carrying real value:

- [ ] All gates 1-16 ✅ in `docs/STATUS.md`.
- [ ] Gate 17 external audit report published in
      `docs/audits/<date>_<vendor>.pdf`. All critical / high
      findings remediated. Audit re-sign-off recorded in
      `docs/AUDIT_FINDINGS.md`.
- [ ] Three Lean `sorry`s closed (`Conservation:42`, `:60`,
      `MPT:58`).
- [ ] CI pipeline runs:
      - `cargo test --workspace --release` (all green)
      - `cargo deny check licenses bans advisories`
      - `cargo audit`
      - `tools/sbom.sh > /tmp/sbom.txt` (committed per release)
- [ ] Monitoring dashboards live and verified against a synthetic
      block-production drill.
- [ ] On-call rotation acknowledged.
- [ ] DR restore drill (§ 6.2) completed within the last 90 days.
- [ ] Rollback path verified end-to-end against the previous release
      binary.

When all rows above are checked and signed off by platform lead +
security lead + on-call lead, the deployment is gate-18 cleared.
