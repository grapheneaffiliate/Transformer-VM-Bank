# Runbook — Light client reports state divergence

**Trigger:** `PSLLightClientBadProofSpike` alert, OR an end-user
reports their balance verification failing.
**Severity:** high.
**Estimated MTTR:** 15-90 min.

A light client is unable to verify inclusion proofs against the
published block header. Either the sequencer's published state has
drifted from what the light client expects (sequencer-side bug),
or the light client is being fed bad proofs (DOS), or the light
client itself is on a stale header.

## Step 1 — Identify the divergence

On the affected light client:

```
psl-light-client status
psl-light-client last-verified-block
```

Compare against the sequencer's published current height. Note
whether the light client is just stale (catch-up issue) or whether
it's actively rejecting new headers.

## Step 2 — Localize

Pick a specific account whose proof is failing:

```
psl-light-client verify-account <pubkey> --verbose
```

The output names the failing step (header signature, MPT path,
leaf hash). That tells you what to look at next.

## Step 3 — Common causes

### 3a — Bad header signature

The sequencer's signing key may have rotated and the light client
hasn't picked up the new pubkey:

```
psl-light-client refresh-trusted-signers
```

If the rotation was via the documented `KeyRotation` mechanism,
the light client will accept the new key automatically. If not:
go to `sequencer-key-compromise.md`.

### 3b — MPT path mismatch

The sequencer published a state root inconsistent with the actual
state. This is a **consensus halt** — see
`consensus-halt.md` § "State-root mismatch". A light client
detecting this is doing exactly what it's designed to do.

### 3c — Stale light client

The light client is several blocks behind and the proof it has is
for a different state version:

```
psl-light-client sync
```

Then re-verify.

## Step 4 — If multiple light clients diverge

If several independent light clients report the same divergence,
the bug is on the sequencer side. Escalate to
`consensus-halt.md` even if the sequencer itself is still running.

## Step 5 — DOS check

If a single light client gets many invalid proofs from one source:

```
psl-light-client list-recent-proofs --invalid-only --by-source
```

Block the abusive source upstream of the light client (rate
limiter, firewall).

## Post-incident

If the divergence was sequencer-side: add a regression test
against the affected (account, block-height) pair.

If the divergence was a stale-trusted-signers issue: review
whether the light client's auto-refresh interval should be
shortened.
