# ADR-0003 — Publication strategy for v0.1.0

**Status:** accepted.
**Date:** 2026-05-09.
**Deciders:** PSL maintainers (closure-session sign-off).
**Supersedes:** —.

## Context

PSL's Phase 2 work (gates 10–16) shipped a deterministic agent
execution layer with re-execution-based dispute resolution. The
system has technical novelty (ternary integer contract VM, no fp64
on verifier path, deterministic dispute) and policy implications
(programmable settlement on tokenized assets without floating-point
arbitration risk). Both audiences — researchers/engineers and
finance/policy — benefit from learning about it; neither has been
told yet.

We need a coherent publication plan tied to the v0.1.0 release tag.

## Decision

Three artifacts go out at v0.1.0 cut:

1. **Repository announcement** (CHANGELOG.md v0.1.0, GitHub release notes,
   one short blog post under `docs/blog/agent-layer-launch.md`).
   Positioned at engineers; opens with what `cargo run --example
   trader_agent` produces and why dispute-by-re-execution is novel.

2. **Whitepaper draft** under `docs/whitepaper/PSL.md`. Single
   document, ~25–30 pages once expanded, covering: motivation, threat
   model, ternary VM design rationale, contract DSL, negotiation
   protocol, dispute mechanism, comparison to Solana-program /
   EVM / Move equivalents. Submitted to arXiv (cs.CR or cs.DC) within
   30 days of v0.1.0 once the audit (gate 17) returns and any
   findings are folded in.

3. **Targeted partner brief** is `docs/AUDIT_BRIEF.md` itself — it
   doubles as the technical due-diligence document for institutional
   pilot conversations. We do **not** publish a separate "investor
   deck" in this repo; that lives outside.

We do NOT publish to:
- Hacker News on launch day. The agent layer is novel enough that
  it needs the whitepaper to land first; otherwise the discussion is
  driven by a 20-line README rather than a 30-page paper. Wait for
  arXiv ID, then post.
- Twitter/X with a thread before the whitepaper. Same reason.
- A "press release". Not the right register for a developer-tools
  repository.

## Rationale

The ordering (repo announcement → whitepaper → social) mirrors the
norms of cryptography/systems research and avoids the pattern where
a sound piece of work gets discussed primarily on the basis of a
tweet thread. PSL has external-validity concerns (audit not done,
DR drill not done) at the moment of v0.1.0 cut — staging the
external attention until those land is appropriate caution.

The whitepaper is the load-bearing artifact for the long term. The
blog post and announcement are catalysts.

## Consequences

- The first 30 days after v0.1.0 cut are explicitly low-tempo on
  social. This is a feature; engineering work continues.
- Audit findings folded into the whitepaper before submission means
  we publish what the audit found, including any open issues.
- We commit to a v0.1.1 within ~90 days that addresses any HIGH-
  severity audit findings, and the whitepaper revision is paired to
  that release.

## Alternatives considered

- **Big bang launch** — release + whitepaper + HN + Twitter on the
  same day. Rejected because audit results aren't in yet, and a
  high-attention launch with unaddressed audit findings is worse
  than a low-tempo launch with the audit already folded in.
- **Hold v0.1.0 until audit completes** — would push tag back
  ~3 months. Rejected because v0.1.0 is the *audit hand-off* tag;
  cutting the tag is what triggers the audit. Not cutting means not
  starting.
