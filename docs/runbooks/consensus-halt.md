# Runbook — Consensus halt

**Trigger:** `PSLSequencerHalt` or `PSLStateRootMismatch` alert fired.
**Severity:** critical. **On-call:** page immediately.
**Estimated MTTR:** 15-60 min for sequencer halt; 1-4 h for state-root mismatch.

The chain has stopped producing blocks (sequencer halt) or the
sequencer and at least one follower disagree on a state root
(consensus halt). Both are incident-grade. Follow this runbook.

## Step 0 — Acknowledge

In PagerDuty (or your alerting tool), acknowledge the page within
5 minutes. Open the incident channel. Note the alert that fired and
the timestamp.

## Step 1 — Distinguish the two cases

```
psl-sequencer status                    # is the sequencer process alive?
journalctl -u psl-sequencer -n 100      # what's the last log line?
```

- **If the sequencer process is dead:** go to "Sequencer halt."
- **If the process is alive but blocks have stopped:** go to
  "Sequencer halt."
- **If the alert was `PSLStateRootMismatch`:** go to "State-root
  mismatch."

## Sequencer halt

### Step 2a — Common causes (check in order)

1. **Disk full**
   ```
   df -h /var/lib/psl
   df -h /tmp
   ```
   If <500 MB free: `journalctl --vacuum-size=200M`,
   rotate `/var/lib/psl/blocks/*.log`, restart sequencer.

2. **Mempool stuck on bad tx**
   Check `psl_sequencer_mempool_depth` over the last 10 min. If
   it's been monotonically rising and now flatlined:
   ```
   psl-sequencer drain-mempool --confirm
   ```
   This preserves the canonical tx log and restarts block
   production. **Irreversible** — the drained txs will not be
   re-included automatically; the issuer must re-submit.

3. **Trace runtime exception**
   ```
   journalctl -u psl-sequencer --since "10 min ago" \
     | grep -E 'panic|Overflow|Err'
   ```
   If you see a panic or `TernaryError::Overflow{...}`:
   - Save the witness from the offending tx (look for
     `tx_hash=` in the surrounding log lines).
   - Restart the sequencer with `--skip-tx <tx_hash>` to step
     past the offending tx. **This is a temporary fix.**
   - File an issue with the witness, the panic, and the offending
     primitive name.
   - Roll back to the previous binary if the panic is in
     unmodified code (see "Rollback" in `OPERATIONAL_READINESS.md`
     § 5).

### Step 3a — If you cannot recover within 15 minutes

Trigger DR plan (see `docs/DR_DRILL_PLAN.md`). The on-call
incident commander makes this call.

## State-root mismatch

This is a **hard halt**. Stop producing blocks until resolved.

### Step 2b — Capture diagnostic snapshot

```
psl-sequencer dump-state-roots --last=10 > /tmp/seq-roots.json
ssh follower-1 'psl-light-client dump-state-roots --last=10' > /tmp/f1-roots.json
ssh follower-2 'psl-light-client dump-state-roots --last=10' > /tmp/f2-roots.json
ssh follower-3 'psl-light-client dump-state-roots --last=10' > /tmp/f3-roots.json
```

Compare with `diff /tmp/seq-roots.json /tmp/f1-roots.json` etc.
Identify the **first divergent block**.

### Step 3b — Bisect to the offending tx

For the divergent block, run:

```
psl-sequencer dump-block <height> > /tmp/block.json
```

Pull each tx's witness from `block.json`. For each:

```
psl-runner --primitive transfer --witness <hex> --canonical
# vs
psl-runner --primitive transfer --witness <hex> --legacy   # only if pre-ADR-0001 block
```

Compare against the sequencer's published per-tx output. The first
mismatching tx is the bug.

### Step 4b — Adjudicate

Re-run the offending witness through the canonical
`TernaryProgram` directly (not via the sequencer):

```
cargo run -p psl-agent-contracts --release --example replay -- \
    --contract <name> --witness <hex>
```

The canonical program's output is the truth.

- If sequencer's output != canonical → sequencer has a bug. Roll
  back the sequencer (`OPERATIONAL_READINESS.md § 5`).
- If follower's output != canonical → follower has a bug. Roll
  back the follower; sequencer continues.

### Step 5b — Resume block production

After rolling back the offending side and confirming its state
root matches the canonical re-execution at the divergent height,
restart block production:

```
systemctl start psl-sequencer
```

Followers should rejoin and converge within 1 block interval.

## Step 6 — Post-incident

1. File an incident report in `docs/incidents/<date>-<short>.md`
   following the template.
2. Add a regression test against the offending witness in
   `agent_protocol/tests/regressions/` (if dispute-related) or
   the appropriate primitive's exhaustive suite.
3. Open a finding in `docs/AUDIT_FINDINGS.md` if the bug is in an
   audited crate.
4. Schedule a blameless postmortem within 5 business days.

## Communication

- T+0:   page on-call, incident channel.
- T+5m:  status post to internal Slack: "investigating sequencer
         halt at block N."
- T+30m: status post: cause hypothesis + ETA.
- Resolution: status post: cause + fix + post-incident issue link.
- T+24h: blameless postmortem doc circulated.
