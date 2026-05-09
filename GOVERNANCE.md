# PSL — Governance

This document describes how decisions about the PSL repository are made
and recorded. It is intentionally short; PSL is in early-stage
development with a small maintainer set, and elaborate governance is
premature.

## Scope

This governance covers:
- Code in this repository (`github.com/grapheneaffiliate/Transformer-VM-Bank`).
- The set of architectural decisions in `docs/decisions/`.
- The release-tagging process for `vX.Y.Z`.

It does **not** cover:
- Operations of any production deployment of PSL. Each operator runs
  their own deployment and bears responsibility for it.
- Trademark / corporate matters (these live outside the repository).

## Maintainer set

Current maintainers are listed in `MAINTAINERS.md` (one file, names +
GitHub handles). Maintainers have:
- Merge rights on the `main` branch.
- Authority to cut release tags.
- Authority to accept ADRs (per the ADR process below).

A maintainer is added by **unanimous consent of existing maintainers**,
recorded in a `docs/decisions/000N-maintainer-{name}.md` ADR. A
maintainer is removed by majority of remaining maintainers, also
recorded as an ADR. The intent is that adding maintainers is easy,
removing them is rare and explicit.

## Decision process

Day-to-day code changes go through a normal pull-request flow:
- One maintainer review required to merge.
- All CI gates green (`.github/workflows/ci.yml`,
  `security.yml`, `fuzz.yml`).
- Pre-merge `cargo test --workspace --release` clean.

**Architectural decisions** (anything that changes a load-bearing
invariant, retires or adds a major component, or changes the trust
model) require an ADR in `docs/decisions/000N-*.md` that:
- States context, decision, rationale, consequences, alternatives.
- Is reviewed by **all** active maintainers (not just one).
- Lands as a separate commit before the implementing change.

The current ADR set (as of v0.1.0):
- ADR-0001 — retire legacy fp64 runner.
- ADR-0002 — BFT consensus engine selection (deferred).
- ADR-0003 — publication strategy for v0.1.0.
- ADR-0004 — public test network deferred to v0.2.
- ADR-0005 — licensing, export-control, and patent posture.

## Release process

A release is cut when the maintainers agree the milestone is met.
The release is a git tag `vX.Y.Z` with:
- An entry in `CHANGELOG.md` summarizing changes.
- Verified clean reproduction per `REPRODUCE.md` on a fresh VM.
- All claimed-✅ gates verified by re-running their listed commands
  on the tag commit.
- A signed GitHub release page with the binaries (Linux x86_64;
  macOS/aarch64 if signed-build infra exists at release time).

We follow semver:
- `0.x.y`: pre-1.0 — minor versions may break compatibility, patch
  versions do not.
- `1.x.y`: stable; minor versions are additive, patch is bug-fix
  only, major bumps break compatibility.

PSL is at `0.1.0` at v0.1.0 cut and will remain `0.x` until the audit
report (gate 17) lands and at least one DR drill (gate 18) succeeds.

## Security disclosure

Security issues do **not** go through public GitHub issues. See
`SECURITY.md` for the disclosure channel and timeline commitments.

## Conflict resolution

Disagreements between maintainers about technical direction:
1. First, write an ADR with the question framed and both positions
   represented honestly.
2. Discuss async (PR thread on the ADR).
3. If still deadlocked, the maintainers vote. Tie goes to the
   conservative option (retain status quo until the case is clearer).

Disagreements about conduct go through `CODE_OF_CONDUCT.md`.

## What is explicitly NOT in this governance (yet)

- A formal foundation, council, or DAO. The maintainer set is
  small and informal; the moment that stops working we'll write the
  next ADR.
- A token-weighted vote. Settlement infrastructure should not be
  governed by speculative-asset holders, in our view.
- A pinned funding source. PSL is volunteer development at v0.1.0;
  if external funding becomes a question we'll record it as an ADR.

## Amendments

This document changes via ADR.
