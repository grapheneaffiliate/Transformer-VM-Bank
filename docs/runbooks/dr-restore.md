# Runbook — Disaster recovery (full restore from backup)

**Trigger:** total loss of sequencer host(s) — disk failure,
ransomware, region outage, accidental `rm -rf`. The chain cannot
make progress because the canonical state is gone.
**Severity:** critical.
**Estimated MTTR:** 30 min (warm standby) to 4 h (cold restore
from object storage).

## Pre-conditions

- Backups exist and are recent (see `tools/backup.sh` and the
  `psl_backup_age_seconds` Prometheus gauge — should be < 24 h).
- Backup integrity has been verified at least once in the last 30
  days via `tools/backup.sh --verify-latest`.
- The trusted-signers set used by light clients is documented and
  recoverable independently of the sequencer host.

If any of these is false, you are in an *uncontrolled* DR — see
the bottom of this runbook ("If backups are gone too").

## Step 1 — Confirm loss is total

Before triggering restore, rule out the recoverable cases:

```
ssh sequencer-primary uptime              # is the host actually down?
ssh sequencer-primary df -h /var/lib/psl  # is the disk gone, or just full?
```

A full disk is fixable in place — see `consensus-halt.md` § "Disk
exhaustion". Restore from backup is only justified if the
**canonical state directory is unrecoverable**.

## Step 2 — Provision a new sequencer host

```
cd infra/
terraform apply -target=module.sequencer_primary \
    -var "instance_role=sequencer" \
    -var "backup_restore=true"
```

The Terraform module spins up a fresh VM with the standard PSL
binaries, an empty state directory, and SSH access for the on-call.

## Step 3 — Pull the latest verified backup

```
ssh sequencer-primary-new
sudo systemctl stop psl-sequencer       # safety: no writes during restore
tools/backup.sh --restore-latest --target /var/lib/psl
tools/backup.sh --verify /var/lib/psl   # checks BLAKE3 of state root
                                        # against the manifest
```

`--verify` recomputes the state root from the restored data and
compares it to the BLAKE3 recorded in the backup manifest. If it
mismatches, **do not start the sequencer**; pull an older backup.

## Step 4 — Restore signing key

The signing key is **not** in the state backup (it is held in a
separate KMS / HSM / sealed file per `sequencer-key-compromise.md`).
Follow the sealed-key recovery procedure in that runbook.

If the signing key is also lost, this is a **chain-halt-and-restart**
event — see "If the signing key is gone" below.

## Step 5 — Cold-start the sequencer

```
sudo systemctl start psl-sequencer
journalctl -u psl-sequencer -f
```

Watch for:
- "Loaded state at height N" — N must equal the height in the
  restored backup manifest.
- "Begin producing block N+1" — sequencer is live again.

## Step 6 — Re-attach followers

Followers will detect the new sequencer via the gossip / RPC
endpoint. Each follower will:
- Notice the height regression (if any) and refuse to follow
  *backwards*. This is correct behavior.
- If the follower is **ahead** of the restored sequencer (because
  the backup was older than the follower's last seen height), the
  follower must be reset:

```
ssh follower-N
sudo systemctl stop psl-follower
rm -rf /var/lib/psl/*           # nuke local state
sudo systemctl start psl-follower
```

The follower will then sync from the new sequencer normally.

## Step 7 — Re-issue light-client trusted-signers if rotated

If recovery required generating a new sequencer signing key
(because the original was lost too), every light client must be
told the new pubkey:

```
psl-admin publish-trusted-signers --new
```

This emits a signed `TrustedSignersV2` artifact to the configured
distribution channel. Light clients pick it up and verify its
signature against the previous trusted set (per
`KeyRotation` mechanism).

## Step 8 — Public communication

If the chain was unavailable for > 5 min, post-incident
communication is required:
- Status page update (cause + restoration time).
- Direct notification to known institutional users.
- Schedule a public post-mortem within 7 days.

## If backups are gone too

This is an **unrecoverable state loss**. The chain cannot continue
on the same chain ID with new state. Options, in order of
preference:
1. Recover backups from a colder tier (off-site tape, air-gapped
   vault). This is the reason `tools/backup.sh` writes to *both*
   hot object storage *and* a cold archive.
2. Reconstruct state from light-client snapshots if any third
   party retained one. The light-client format is sufficient for
   reconstructing balances (not transaction history).
3. Restart on a new chain ID with a published genesis that
   acknowledges the discontinuity. Existing balances become
   non-redeemable on the new chain unless explicitly migrated by a
   signed off-chain process. This is a terminal event for the
   current chain.

## If the signing key is gone

The chain cannot extend with a new key without breaking light-
client trust. Use the documented `KeyRotation` mechanism: a
signed-by-the-old-key message authorizing the new key. If the old
key is unrecoverable, see `sequencer-key-compromise.md` §
"unrecoverable key loss".

## Post-incident

- Review whether the backup cadence (currently every 6 h) was
  appropriate. If state loss happened during the gap, shorten the
  cadence.
- Verify the `tools/backup.sh --verify-latest` cron is actually
  firing weekly. A backup you have never restored from is not a
  backup.
- File an issue for any step in this runbook that turned out to be
  unclear or incorrect; this runbook is the second-most-important
  document in the repo (after the security review).
