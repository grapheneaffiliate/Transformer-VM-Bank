# ADR-0002 — BFT consensus engine selection

**Status:** Accepted (option C: explicit defer with concrete trigger)
**Date:** 2026-05-09
**Author:** PSL maintainers (Phase 2 closure session)
**Supersedes:** none
**Superseded by:** none

## Context

Gate 9 of the Phase 1 plan called for swapping the sovereign-mode
single sequencer for a BFT-ordered validator set when an institutional
pilot requires multi-validator consensus. `docs/CONSENSUS_DECISION.md`
(2026-05-03) recommended deferring the malachitebft-rs commitment and
shipping the MVP on tendermint-rs ABCI driving a Go CometBFT binary,
revisiting on any of:

1. malachite cuts a 1.0 with semver guarantees on `app-channel`,
2. Circle Arc runs on mainnet for 90 days without consensus halt,
3. an external audit of `core-consensus` is published.

This ADR re-evaluates the three options in light of:

- Phase 2 (gates 10-16) shipped the agent execution layer. The
  consensus question now applies to a system where each transaction
  carries an embedded `program_hash` and is independently
  verifiable; the BFT layer's job narrows to ordering + finality.
- ADR-0001 retired the legacy fp64 runner; the canonical execution
  contract is `trace_hash_ternary` (`ARCHITECTURE.md § 0.2`),
  bit-identical across hosts. This means a heterogeneous validator
  set with mixed CPU architectures (x86_64, aarch64) can run the
  same execution and reach the same state root deterministically.
- The current `consensus/` crate exposes a `Consensus` trait with a
  sovereign implementation in tree. The BFT implementation is the
  swap-in.

## Options

### Option A — Integrate Malachite (informalsystems-malachitebft-rs)

Repository: `github.com/circlefin/malachite` (canonical post-Aug-2025
acquisition). Apache-2.0. Pure Rust. Drives Circle's Arc L1.

Status check (2026-05-09):
- Latest tagged release on the canonical repo: pre-1.0 (per the most
  recent versioning we can verify in tree). Not yet at 1.0 with
  semver guarantees.
- One production deployment (Arc L1) is documented; no third-party
  external audit of `core-consensus` published as of this writing.
- API: `informalsystems-malachitebft-app-channel` is the recommended
  integration seam.

Cost to integrate: 2-3 weeks of focused work plus a 4-validator
consortium test under fault scenarios (one validator down, one
byzantine, network partition). The Lean spec for the validator-set
state machine is also new work.

### Option B — Integrate CometBFT via ABCI++

Repository: `github.com/cometbft/cometbft`. Apache-2.0. Go binary
driven from Rust via the standard ABCI++ socket interface
(`tendermint-rs` for the Rust side).

Status check: CometBFT 1.x is mature, audited, with hundreds of live
chains and frozen wire protocol semantics.

Cost to integrate: 3-4 weeks. Two language runtimes complicate
deployment (Rust sequencer + Go consensus daemon). Cross-language
boundary failure modes (socket disconnect, ABCI app panic, timeout
mismatches) need explicit testing.

### Option C — Defer with a concrete trigger

Keep the sovereign single-sequencer in production for the agent
layer rollout. Multi-validator consensus is genuinely not needed
until either:

1. An institutional pilot signs an LOI requiring multi-validator
   consensus (the original CONSENSUS_DECISION trigger), OR
2. Malachite reaches v1.0 with at least one external audit (still
   valid as before), OR
3. The agent layer reaches >100 active agents on the test network
   AND any single agent exceeds 10% of total transaction volume
   (sovereignty risk crosses an objective threshold).

Each trigger is objective and auditable from public information.

## Decision

We adopt **option C: explicit defer with the three triggers above.**

Rationale:

- The agent layer is the headline of Phase 2. Spending 2-4 weeks on
  BFT integration before any institutional pilot has signed an LOI
  delays the go-to-market with no concrete demand pulling for it.
- The ternary execution layer is deterministic across hosts. When we
  do integrate BFT, the consensus layer's job is purely ordering,
  not re-execution agreement (the latter is a free property of
  Phase 2). This makes the eventual integration smaller in scope
  than originally feared.
- Malachite is on the right trajectory but not yet at the bar that
  was set in `docs/CONSENSUS_DECISION.md` (1.0 + external audit).
  Forcing the integration now means accepting more pre-1.0 risk than
  the original trigger contemplated.
- CometBFT is a viable fallback. We commit to selecting between A
  and B at trigger time, not now — because the choice depends on
  what the institutional partner asks for (some prefer pure-Rust
  stacks; others have CometBFT operational expertise).

This ADR commits us to:

1. Implementing whichever engine the trigger picks **within 60 days
   of the trigger firing.**
2. Re-evaluating Malachite quarterly via a dedicated review issue,
   tracking the v1.0 progress and audit publication.
3. Documenting the sovereign-mode trust assumption explicitly to
   any institutional pilot that engages before the trigger fires.

## Consequences

**Positive:**
- Engineering attention stays on the agent layer (gates 10-16
  delivery, audit prep, public release), not on speculative
  consensus integration.
- The commitment to act within 60 days of trigger gives partners a
  bounded timeline.
- Ternary execution determinism makes the eventual integration
  cheaper.

**Negative:**
- We cannot tell partners "we have a BFT consensus story today."
  Sovereign-mode is the answer until trigger fires.
- An adversarial sequencer is detectable by followers but not
  prevented in real time; a compromised sequencer can produce ~1 tx
  worth of damage before followers raise the alarm.

**Risk mitigation:**
- Sovereign-mode trust assumption documented for institutional
  partners (`docs/SOVEREIGN_MODE_TRUST.md` — added in this session).
- Light client + follower detection of state-root mismatch
  (gate 6 cleared) provides public after-the-fact accountability for
  any sequencer dishonesty.
- 60-day SLA from trigger to integration is in this ADR; not a
  vague deferral.

## Quarterly review checklist

Run via `tools/quarterly_consensus_review.sh` (added in this
session) — outputs current Malachite tag, latest published audits,
and Circle Arc uptime data into a markdown report committed under
`docs/quarterly_reviews/`.

## References

- `docs/CONSENSUS_DECISION.md` (the 2026-05-03 vendor audit).
- `docs/decisions/0001-retire-legacy-fp64-runner.md` (related
  architectural cleanup).
- `docs/SOVEREIGN_MODE_TRUST.md` (sovereign-mode trust assumption,
  added in this session).
- `consensus/` crate — current `Consensus` trait surface where the
  BFT implementation will plug in.
