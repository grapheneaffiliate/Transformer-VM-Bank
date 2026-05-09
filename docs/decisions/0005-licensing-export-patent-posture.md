# ADR-0005 — Licensing, export-control, and patent posture

**Status:** accepted (subject to legal review before any v0.2.0 dependency on these conclusions).
**Date:** 2026-05-09.
**Deciders:** PSL maintainers.

## Context

PSL ships:
- Cryptographic primitives (ed25519, BLAKE3) — these are the
  same NIST/IRTF-standard primitives used by every modern blockchain
  and many TLS stacks.
- A settlement layer with compliance hooks (freeze authority, view
  keys, travel rule).
- A novel agent execution layer with deterministic dispute
  resolution.

Three legal-adjacent decisions need an explicit posture before v0.1.0
goes out:
1. Open-source license.
2. Export-control category.
3. Patent posture (do we file? do we promise not to assert?).

This ADR records the maintainers' position. **It is not legal
advice**, and the v0.2.0 release must not extend any of these
positions without a real lawyer signing off.

## Decision

### 1. License: MIT

The repository is released under the MIT license (`LICENSE`).

Rationale:
- Permissive licensing maximizes adoption for a settlement layer
  (institutional partners can integrate without copyleft pulling in
  their proprietary code).
- MIT specifically (vs. Apache-2.0) keeps the patent terms
  *un*addressed in the license, which is consistent with the
  patent-posture decision below (we don't file, we don't assert).
- All third-party Rust crates currently in `Cargo.lock` are MIT-,
  Apache-2.0-, or BSD-licensed (verified by `cargo deny check
  licenses` in CI; deny.toml encodes the allow-list).

### 2. Export-control category

Position: PSL is **publicly available open-source software using
publicly available cryptographic primitives** and is therefore
covered by the EAR § 742.15(b) "publicly available" carveout in the
United States. No export-control filing is planned for v0.1.0.

Concretely:
- All cryptography is implemented via well-established open-source
  Rust crates (ed25519-dalek, blake3) which are themselves publicly
  available.
- No proprietary or restricted crypto is in the source tree.
- The repository is hosted publicly on GitHub.

What this does **not** authorize:
- Distribution of compiled binaries to embargoed jurisdictions
  (Cuba, Iran, North Korea, Syria, Crimea region) — those are
  out-of-scope and operator's responsibility.
- Sale or service relationships with embargoed parties.

A real lawyer review is required before:
- Adding any cryptography that isn't currently in the standard
  open-source-crypto set (e.g., post-quantum primitives that haven't
  been formally classified).
- Distributing compiled binaries via channels that are not
  publicly accessible (e.g., partner-only download pages).

### 3. Patent posture: defensive non-assertion

Position: PSL maintainers do not currently hold patents on the
material in this repository, do not plan to file, and explicitly
commit (in this ADR) not to assert any patents derived from this
work against any user or implementor of PSL or compatible systems.

Rationale:
- Patent-encumbered open-source projects in the financial-protocol
  space (HyperLedger, R3 Corda) have struggled with adoption
  because partners hesitate to commit to a stack with unclear
  future patent terms.
- A clear "we don't file, we don't assert" posture removes the
  uncertainty.
- If the agent-layer work turns out to have material patentability
  later, this ADR is the public record that we have already
  committed not to assert. Future maintainers cannot retroactively
  file and enforce against existing users.

This commitment is:
- **Non-revocable** for the v0.1.0 source tree.
- **Forward-applicable** to derivatives so long as they remain
  consistent with the MIT license terms.

## Consequences

- Any contribution to the repo carries the MIT license terms by
  default (CONTRIBUTING.md will state this).
- We cannot pivot to a copyleft license without a full
  re-licensing exercise (impossible in practice for open-source
  contributions already accepted under MIT).
- We cannot file patents on the v0.1.0 design later. If we want
  patent-related defensive moves (e.g., joining the OIN), we do so
  without filing.

## Open items requiring real legal review before v0.2.0

- [ ] License MIT vs Apache-2.0 — final call from external counsel.
      MIT is the v0.1.0 default; counsel may upgrade to Apache-2.0
      with explicit patent grant if they prefer (Apache-2.0 is
      compatible-derivable from MIT, the other direction is not).
- [ ] Export-control sign-off in writing for v0.2.0 if any
      post-quantum crypto enters scope.
- [ ] Sanctions screening for any institutional partner before
      contractual relationship.
- [ ] Trademark on "PSL" / "Percepta Settlement Layer" — defensive
      registration in the United States.

## Alternatives considered

- **Apache-2.0 with explicit patent grant.** Considered. Equivalent
  protection to MIT + this ADR. Defer to legal review for v0.2.0;
  switching from MIT to Apache-2.0 is a one-way add (any MIT
  contribution is permissively re-licensable to Apache-2.0).
- **AGPL or BUSL-style copyleft.** Rejected: institutional partners
  cannot integrate; misaligned with goal of being settlement
  infrastructure.
- **File patents and use them defensively (OIN-style).** Considered.
  Adds legal cost and timeline that v0.1.0 cannot absorb. Future
  maintainers may revisit but cannot retroactively cover material
  in this tree.
