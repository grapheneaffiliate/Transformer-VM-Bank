# Sovereign-mode trust assumption

**Audience:** institutional pilot operators, regulators, partners
considering integration before gate-9 BFT is in place.
**Status:** current as of ADR-0002 (2026-05-09).

## What sovereign mode is

PSL ships in two operating modes:

- **Sovereign** — single block-producing sequencer, multiple followers
  that re-execute every block independently. **In production today.**
- **Consortium** — BFT-ordered validator set producing blocks via
  Malachite or CometBFT. **Deferred per ADR-0002, with three explicit
  triggers and a 60-day SLA from any trigger firing to integration.**

This document tells you exactly what trust the sovereign-mode operator
asks of you, and what cryptographic guarantees you keep regardless.

## What you must trust the sequencer for

In sovereign mode, you trust the sequencer to:

1. **Order transactions** — pick the order in which submitted
   transactions land in blocks. The sequencer's signing key is the
   final arbiter of block sequence.
2. **Be live** — produce blocks within the documented interval. If
   the sequencer halts, no new blocks land. (Light client + followers
   detect this; you can detect it too.)

That is the entire trust surface.

## What you do **not** have to trust the sequencer for

The sequencer **cannot**:

1. **Forge state transitions.** Every primitive runs through the
   canonical `TernaryProgram` (`docs/ARCHITECTURE.md § 0.2`). Same
   input + same `weights_hash` → bit-identical output on any
   conformant integer-arithmetic verifier. A sequencer publishing a
   block whose state root is inconsistent with the trace is publicly
   provable wrong by any follower or any holder of the weights.
2. **Forge signatures.** Ed25519 signature verification happens in
   native code (audited library), not in the trace. A sequencer
   that admits an unsigned tx fails follower validation.
3. **Forge balances.** The MPT inclusion-proof check the light
   client runs is independent of the sequencer's claim. You can
   verify your own balance against the published block header
   without trusting the sequencer.
4. **Hide a fork.** Block headers are signed and published; a
   sequencer producing two contradictory blocks at the same height
   is detectable by any follower comparing headers.

## Detection of sequencer dishonesty

If the sovereign sequencer **does** misbehave:

- A follower's recomputed state root will not match the published
  one. The follower raises a `state_root_mismatch` event. (Gate 4
  cleared 2/2 on this; mutation in any single field is detected.)
- A light client running balance verification will see proofs that
  don't validate. Operationally surfaced as
  `proof_verify_failures`.
- The misbehaving block + its signed header are public artifacts
  the operator cannot retract.

What sovereign mode does **not** give you that BFT will:

- **Real-time prevention.** A sovereign sequencer can produce one
  bad block before followers raise the alarm. BFT consensus
  prevents this by requiring quorum agreement before block
  finalization.
- **Liveness under sequencer failure.** A single sequencer halt
  halts the chain until restored. BFT continues with f failed
  validators out of 3f+1.

## When to consider waiting for BFT

Wait for the gate-9 BFT integration if:

- You require real-time prevention of sequencer dishonesty (e.g.,
  for funds movements above a regulatory threshold that must be
  held against the sequencer operator's credit risk).
- You require >99.9% chain liveness with no single point of
  failure.

Sovereign mode is appropriate if:

- Your use case can tolerate the ~1 block of detection latency
  before sequencer dishonesty is publicly proven.
- You can establish operational trust in the sequencer operator
  through legal / contractual / insurance mechanisms.
- Your institutional risk team can sign off on the documented
  detection mechanisms as adequate.

## The path to BFT

The triggers in ADR-0002 fire when:

1. An institutional pilot signs an LOI requiring multi-validator
   consensus, OR
2. Malachite ships v1.0 with at least one external audit, OR
3. Test network grows to >100 active agents AND any single agent
   exceeds 10% of total transaction volume.

When any trigger fires, the BFT integration ships **within 60 days**.

## How to evaluate sovereign-mode operator integrity

If you are an institutional pilot considering sovereign-mode
deployment, ask the operator for:

- Sequencer signing key custody arrangement (HSM model + access
  control + rotation procedure).
- Operator's internal incident response runbook for sequencer-key
  compromise (PSL provides a template at
  `docs/runbooks/sequencer-key-compromise.md`).
- Insurance against operational loss attributable to sequencer
  dishonesty (PSL does not provide insurance; this is a
  commercial layer).
- Legal jurisdiction and recourse path for tort claims arising
  from sequencer behavior.

## Documentation update commitment

This document updates in the same commit as ADR-0002 changes. If
either trigger fires and BFT integration begins, this document gets
a "deprecated" header pointing to the new BFT operating model. Until
then, it is the authoritative description of what sovereign-mode
trust looks like.
