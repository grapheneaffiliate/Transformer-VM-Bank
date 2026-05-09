# Engagement request — Zellic

**Status:** draft, awaiting human sign-off and send.
**Recipient:** hello@zellic.io (verify current address before send).
**Subject:** Audit engagement request — PSL agent execution layer

---

To the Zellic team,

We're requesting a security audit for **PSL** — a deterministic
financial ledger with a Phase 2 agent transaction layer. Detailed
scope in the attached `docs/AUDIT_BRIEF.md`.

Why Zellic for this engagement:

- Your published work on novel execution-layer designs is closest
  to PSL's ternary-integer kernel. We're particularly interested
  in your read on the cross-platform determinism property
  (integer arithmetic + `checked_add`/`checked_sub` everywhere)
  vs. comparable schemes like the various zkVM execution traces.
- Your audit cadence (typically 3-4 weeks for a project of this
  size) fits our v0.1.0 release timeline.
- We've reviewed your past engagement reports — particularly the
  Aleo / Solana DEX audits — and find your level of detail and
  written-finding clarity exemplary.

## Proposed scope

5 in-scope crates (~10k LOC). 4 secondary crates (~3k LOC). Full
scope statement in `docs/AUDIT_BRIEF.md`.

Phases we'd ask you to cover:

1. Read-through of in-scope crates.
2. Fuzz coverage extension — we have 5 cargo-fuzz harnesses;
   please run for at least the audit-recommended 1 CPU-hour each
   and contribute additional targets from your reading.
3. Property test extension — we have 23 proptests; identify
   uncovered branches.
4. Cross-platform determinism — build & exhaustive byte_add on
   x86_64-linux + aarch64-darwin + aarch64-linux. Assert
   byte-identical.
5. Crypto hygiene confirmation.

## Deliverables we're asking for

- Findings report with severity tags.
- Public version published in tree on acceptance.
- 1 round of remediation re-audit.

## Logistics

- Repository: github.com/grapheneaffiliate/Transformer-VM-Bank
- Tag for engagement: `v0.1.0`.
- Maintainer response SLA during engagement: 48 business hours.
- Critical / high finding fix turnaround: 10 business days.
- Public disclosure window: 30 days post-acceptance.

Please reply with a statement of work, quote, earliest start, and
team availability for the engagement window.

Happy to take a call this week to walk through the architecture
before SOW.

Thanks,
— PSL maintainers

---

## Sender checklist before send

- [ ] Verify current Zellic engagement intake address.
- [ ] Attach AUDIT_BRIEF.md → PDF.
- [ ] Attach SECURITY_REVIEW.md → PDF or commit-pinned link.
- [ ] Replace "PSL maintainers" with signer name + role.
- [ ] Diary 5-business-day response window.
