# Runbook — Sequencer signing-key compromise (suspected)

**Trigger:** sustained `PSLSignatureFailureSpike` alert (possible
attacker probing for nonce reuse), OR insider report, OR HSM
tamper alert, OR multiple signed messages from same nonce
detected by the sequencer's own self-monitoring.
**Severity:** critical (break-glass). **On-call:** page +
notify security lead within 5 min.
**Estimated MTTR:** 1-3 hours to rotate; 24-48 h for full audit.

The sequencer's signing key may be in adversary hands. Treat as
true until forensics says otherwise.

## Step 0 — Acknowledge + isolate

```
systemctl stop psl-sequencer
```

**Stop the sequencer immediately.** No new blocks until rotation
completes. Followers will detect the halt; that's expected.

In the incident channel:
- Page security lead.
- Page legal lead (key compromise may trigger disclosure
  obligations to institutional partners).
- Open a private incident channel; remove anyone not strictly
  needed.

## Step 1 — Assess scope

Before rotating, capture the evidence:

1. Current sequencer signing key fingerprint (from HSM or key
   storage):
   ```
   psl-sequencer key-fingerprint
   ```
2. Last 1000 signed block headers + their tx hashes:
   ```
   psl-sequencer dump-headers --last=1000 > /tmp/incident-headers.json
   ```
3. Any tx in the recent window that came from an unexpected
   source (the SignatureFailureSpike alert may have been the
   attacker probing, not a legitimate spike).
4. Audit log from the HSM (if applicable): who accessed the
   key in the last 30 days.

Save all evidence to `/var/log/psl/incidents/<date>/` with
write-once permissions.

## Step 2 — Rotate via parent key

Per `agent_wallet/src/rotation.rs`, the parent key issues a
signed `KeyRotation` mapping the compromised child pubkey to a
fresh one. The sequencer's parent key MUST be in cold storage
(HSM); access requires the documented break-glass procedure
(typically 2-of-3 quorum).

```
psl-sequencer rotate \
    --parent-key-source hsm://primary \
    --new-child-key-seed <freshly-generated> \
    --reason-hash <blake3-of-incident-doc>
```

This creates a `KeyRotation` tx and a `Revocation` tx for the
compromised key, both signed by the parent. Submit them to the
network via the **backup signing identity** (the parent key acts
as backup signer for this case).

## Step 3 — Confirm landing

Wait for the rotation + revocation txs to land in a block. Per
`docs/SOVEREIGN_MODE_TRUST.md` § 4.4, this is the moment the
compromise is publicly disclosed; brief institutional partners
just before.

```
psl-light-client lookup-revocation <compromised-pubkey>
# expect: revoked=true at_block=N
```

## Step 4 — Re-deploy sequencer

```
psl-sequencer deploy \
    --signing-key <fresh-child-key-handle> \
    --start-from-block N+1
```

Sequencer re-joins the chain with the fresh key. Followers
re-establish connection; light clients automatically detect the
key rotation and update their trusted-signer set.

## Step 5 — Post-incident audit

Within 48 hours of resolution:

1. Review every signed block in the **suspicious window** — from
   the earliest possible compromise time (HSM access logs +
   alert spike timestamps) to the rotation tx — for any
   unauthorized actions.
2. For any unauthorized signed action: file a finding in
   `docs/AUDIT_FINDINGS.md`. Treat any state changes from the
   compromised window as void per the rotation's
   `rotated_at_unix` cutoff (the protocol does not auto-revert;
   the operator may need to issue compensating txs).
3. Notify any institutional pilot whose state was affected via
   the channel agreed in their MOU.
4. File CVE if the compromise root cause was a vulnerability
   (vs. operational failure).
5. Update HSM access policy and break-glass procedure based on
   the root cause.

## Step 6 — Public disclosure

Per agreed window in institutional partner MOUs (typically 30
days post-resolution unless the compromise is public earlier):

1. Public incident report in `docs/incidents/<date>-key-compromise.md`.
2. Update `SECURITY.md` with any policy changes.
3. Brief any external auditor mid-engagement (their report should
   reference the incident).

## Communication checklist

- T+0:   page on-call + security lead + legal lead.
- T+15m: institutional partner notification (per MOU).
- T+30m: rotation tx submitted.
- T+1h:  rotation landed, sequencer back online.
- T+24h: post-incident audit started.
- T+48h: audit complete, findings filed.
- T+30d: public disclosure (per MOU window).

## What this runbook does NOT cover

- **Multi-sig sequencer key compromise:** if gate 9 BFT has shipped,
  the rotation is consensus-mediated rather than parent-key-mediated.
  Different runbook needed (TBD when gate 9 closes).
- **Catastrophic loss of parent key:** if the parent key itself is
  compromised or lost, the network must be fork-recovered. See
  `docs/runbooks/dr-restore.md`.
