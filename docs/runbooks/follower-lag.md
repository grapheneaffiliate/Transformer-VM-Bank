# Runbook — Follower lag

**Trigger:** `psl_follower_lag_blocks > 10` for 5 min, OR a follower
operator reports lag.
**Severity:** medium (single follower lag), high (multiple followers
or growing lag).
**Estimated MTTR:** 5-30 min.

A follower has fallen behind the sequencer by >10 blocks. Either
the follower can't keep up (resource), the follower is partitioned
(network), or the sequencer is faster than spec (capacity).

## Step 1 — Check follower health

On the lagging follower:

```
psl-light-client status
journalctl -u psl-follower --since "10 min ago" | grep -E 'WARN|ERROR'
top -p $(pgrep psl-follower)
df -h /var/lib/psl
```

Common causes:

1. **Disk full** → free space, restart.
2. **CPU saturated** → check whether the follower is on a smaller
   instance than the sequencer; capacity-plan accordingly.
3. **Network slow** → check `iperf3` to the sequencer.
4. **Stuck on bad block** → the follower hits the same code path
   as a sequencer halt; see `consensus-halt.md`.

## Step 2 — Check sequencer rate

```
rate(psl_sequencer_block_height_total[5m])
```

If the sequencer is producing blocks faster than the follower can
re-execute, the follower needs more cores or the sequencer needs
to throttle.

## Step 3 — Resync

If the follower has fallen too far behind to catch up via streaming:

```
psl-light-client resync --from-snapshot=s3://psl-backups/<latest>
```

This downloads the latest sequencer snapshot, applies it, then
streams blocks from snapshot height forward. Faster than re-
execution from genesis.

## Step 4 — Escalate

If multiple followers are simultaneously lagging:

1. Likely a sequencer-side issue (slow tx, network partition,
   block size spike). Switch to `consensus-halt.md`.
2. If sequencer is healthy and multiple followers are slow:
   capacity issue — the deployment sizing is wrong. File a
   capacity finding and schedule a tier upgrade.

## Post-incident

For sustained lag of any single follower: add a row to the
follower fleet sizing table in `docs/OPERATIONAL_READINESS.md` §
6 documenting the observed lag pattern and the resource it
exhausted first.
