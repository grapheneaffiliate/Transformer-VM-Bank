# Disaster Recovery Drill Plan

**Status:** ready to execute. Awaits scheduled drill window with operations team.
**Owner:** PSL operations lead (signs off after drill).
**Cadence:** quarterly, plus once before any major release.

This document defines how PSL validates that its disaster-recovery
mechanisms (`docs/runbooks/dr-restore.md`, `tools/backup.sh`) actually
work end-to-end. The runbook is a script; this drill is the test.

A drill that did not actually destroy state is not a drill.

## Scope

The drill covers:
- Full sequencer host loss + restore from hot-tier backup.
- Sequencer host loss + hot tier unavailable, restore from cold tier.
- Follower divergence after a restore that regressed height.
- Light-client trusted-signers re-publication after key recovery.

Out of scope (separate drills):
- Multi-region failover (requires multi-region deployment, not yet
  in scope for v0.1.0).
- Data-center loss (requires same).

## Pre-drill checklist (T-7 days)

- [ ] Schedule a 4-hour window with operations + on-call engineer.
- [ ] Confirm a *non-production* PSL deployment exists for the drill
      (do **not** drill on production until the third successful
      drill on the staging environment).
- [ ] Confirm the staging deployment has had real workload (synthetic
      load via `tools/load_test.sh --quick`) for at least 24 h
      preceding the drill, so the state is non-trivial.
- [ ] Verify backups exist: `tools/backup.sh --list` shows recent
      entries in both hot and cold tiers.
- [ ] Verify the runbook is current: read `docs/runbooks/dr-restore.md`
      end-to-end, file an issue against any step that looks stale.
- [ ] Designate a **scribe** who is not the on-call engineer; the
      scribe records timings and any deviations from the runbook.

## Drill execution (T-0)

### Phase 1 — Snapshot ground truth

Before destroying anything, capture what "correct" looks like:

```bash
ssh staging-sequencer
psl-admin current-height          > /tmp/pre-height.txt
psl-admin state-root              > /tmp/pre-root.txt
psl-admin balances-summary > /tmp/pre-balances.json
```

These three values are the success criterion. After restore the
sequencer must report the same height and the same state root
(allowing for the 0–6 h backup gap, the post-restore height may be
*older* but must produce the same root for that older height).

### Phase 2 — Destroy

Pick **one** scenario per drill (rotate quarterly):

**Scenario A — host loss with hot backups available:**
```bash
ssh staging-sequencer
sudo systemctl stop psl-sequencer
sudo rm -rf /var/lib/psl/*
# leave the host running; this simulates disk failure on a working VM
```

**Scenario B — host loss + hot tier unavailable:**
```bash
# in addition to scenario A, simulate hot tier outage:
aws s3api put-bucket-policy --bucket psl-backups-hot \
    --policy '{"Version":"2012-10-17","Statement":[{"Effect":"Deny","Principal":"*","Action":"s3:*","Resource":"arn:aws:s3:::psl-backups-hot/*"}]}'
# the runbook must succeed via the cold tier
```

**Scenario C — host loss + signing key loss:**
```bash
# scenario A + remove the sealed-key file:
sudo rm /etc/psl/signing-key.sealed
# requires KMS / HSM recovery path
```

### Phase 3 — Restore (the drill itself)

The on-call engineer follows `docs/runbooks/dr-restore.md` step by
step, reading from the rendered Markdown — **not** from memory or a
wiki summary. The scribe times each step.

The scribe also notes:
- Any step that requires improvisation (the runbook is wrong/missing
  something).
- Any tool that does not behave as the runbook claims.
- Total time elapsed from "destroy" to "first new block produced".

### Phase 4 — Verify

```bash
ssh staging-sequencer
psl-admin current-height          # should be ≥ pre-height (modulo backup gap)
psl-admin state-root              # for that height, must match pre-root
psl-admin balances-summary        # diff against /tmp/pre-balances.json
```

Light clients independently:
```bash
psl-light-client verify-account <known-pubkey>   # for several known accounts
```

## Acceptance criteria

The drill **passes** if all of the following are true:

1. The runbook was followed without major improvisation. Minor
   typos / clarifications are filed as issues but do not fail the
   drill. *Major* gaps (a step that doesn't work, a step that's
   missing) **fail** the drill.
2. Total restore time is within the runbook's stated MTTR
   (currently 30 min warm / 4 h cold).
3. State root matches pre-drill snapshot for the restored height.
4. Light clients successfully verify accounts against the new
   sequencer.
5. Followers re-attach without manual reset *unless* the runbook
   explicitly required reset (height regression case).

## Post-drill

Within 5 business days:
- [ ] Scribe writes a 1–2 page report: what happened, timings,
      deviations from the runbook, what surprised us.
- [ ] File issues against the runbook for every clarification or
      correction.
- [ ] If the drill failed, schedule the next attempt within 30 days.
      Do not advance to a new scenario until the failed scenario
      passes.
- [ ] Update this document's "drill log" below.

## Drill log

| Date       | Scenario | Outcome | MTTR     | Notes                                      |
| ---        | ---      | ---     | ---      | ---                                        |
| (pending)  | A        | -       | -        | first scheduled drill — operations to plan |

## Why a documented drill plan matters for v0.1.0

For an audit and for institutional partners, "we have a runbook" is
not the same as "we have a tested runbook." This document is the
pre-commitment: we will execute, the criteria are stated in advance,
and the log records whether we kept the commitment. A green status
on gate 18 requires the first row of the drill log to be filled in
with a passing result.

Per `CLAUDE_CODE_FINAL_CLOSURE.md`: "Gate 18 cannot move to ✅
without a live DR drill." This plan is what the drill executes from.
