# Runbook — Dispute storm

**Trigger:** `PSLAgentDisputeStorm` alert (dispute rate >5σ above
7-day baseline) OR `PSLMempoolOverflow` driven by disputes.
**Severity:** high. **On-call:** business-hours response acceptable
unless rate continues climbing or sequencer halts.
**Estimated MTTR:** 15-90 min.

## Step 0 — Distinguish legitimate spike from attack

A real dispute spike usually correlates with:

- A new contract version that has a bug → many disputes against
  honest executors of that contract.
- An institutional partner running a load test → disputes are
  part of the test plan, expected.

A dispute storm attack looks like:

- Same contract, mass identical-looking disputes, all from
  **different** disputer pubkeys (sybil pattern).
- Or: same disputer pubkey, very high frequency (griefing
  pattern).

Query:

```
psl-sequencer disputes-recent --window=10m --groupby=disputer
psl-sequencer disputes-recent --window=10m --groupby=contract
```

## Step 1 — Bound the damage

Each dispute resolution is bounded (per
`agent_protocol::dispute::resolve_dispute`); the worst it can do
is fill the mempool with re-execution work.

If `psl_sequencer_mempool_depth > 50000` AND climbing:

```
psl-sequencer set-mempool-cap 100000        # raise temporarily
psl-sequencer dispute-rate-limit --by=disputer --max=10/min
```

The rate limiter is per-disputer-pubkey; honest agents in the
allowed-counterparty list can be exempted via:

```
psl-sequencer dispute-rate-limit --exempt=<pubkey>
```

## Step 2 — If sybil attack

If the storm is from many pubkeys all freshly registered:

1. Check the registration cost — if low, raise it temporarily:
   ```
   psl-sequencer set-registration-bond 10000  # was e.g. 1000
   ```
2. Identify the funding source for the sybil pubkeys
   (`psl-light-client trace-funding <pubkey>`). If a single
   account funded all sybils, freeze that account via
   `freeze_authority` (gate 5 mechanism).

## Step 3 — If griefing (single disputer, high volume)

The rate limiter from step 1 should suffice. If the disputer keeps
hitting the cap, slash their bond per the dispute rules:

```
psl-sequencer slash --pubkey <disputer> --amount <bond_fraction>
```

Slash decisions should be reviewed by the on-call lead before
finalizing — `--dry-run` first.

## Step 4 — If legitimate spike from contract bug

If the storm is many disputes against **one specific contract**
all disputers winning (= executors slashing rate up):

1. The contract's executors may be misimplementing it. Check the
   contract's `program_hash` and re-run a few witnesses through
   the canonical engine to confirm the contract is correct.
2. If the contract IS correct: brief the executors via the
   registry endpoints; they need to fix their implementation.
3. If the contract is BUGGY: pause it via:
   ```
   psl-sequencer pause-contract --hash <program_hash>
   ```
   This prevents new proposals against the buggy contract while
   it's fixed.

## Step 5 — Resume normal operation

Once the storm subsides:

1. Lower the mempool cap back to baseline.
2. Lift the rate limit (or keep if attack is still ongoing).
3. If contract was paused: deploy fix, unpause via
   `psl-sequencer unpause-contract`.

## Step 6 — Post-incident

1. Incident report in `docs/incidents/`.
2. If sybil: review whether registration bond should be
   permanently raised.
3. If griefing: review whether per-disputer rate limit should
   be permanent (vs. emergency-only).
4. If contract bug: file finding in `docs/AUDIT_FINDINGS.md`,
   add regression test, schedule fix.

## Mass dispute-resolution capacity

Per the load test in `OPERATIONAL_READINESS.md § 6`, the sequencer
can resolve ~N disputes/sec (TBD on real hardware). If sustained
load exceeds this, capacity-plan to the next tier.
