# Engagement request — Trail of Bits

**Status:** draft, awaiting human sign-off and send.
**Recipient:** sales@trailofbits.com (verify current address before send).
**Subject:** Audit engagement request — PSL agent execution layer

---

To the Trail of Bits team,

We're requesting a security engagement for **PSL** (Percepta
Settlement Layer) — a deterministic financial ledger with a
first-of-its-kind agent-to-agent execution layer. Phase 2 of the
project shipped an integer-arithmetic execution kernel
(`ternary_vm/`), an 8-contract standard library (`agent_contracts/`),
SLIP-0010 hierarchical key custody (`agent_wallet/`), a 5-message
negotiation protocol with deterministic dispute resolution
(`agent_protocol/`), and a high-level Rust SDK (`agent_sdk/`).

We believe Trail of Bits is a strong fit for this engagement
because of your published work on Rust security review, blockchain
protocol audits, and your team's experience with novel
execution-layer designs. Specifically:

- The execution kernel uses pure-integer arithmetic (no fp64) so
  cross-platform determinism is provable rather than empirical;
  we want a fresh read on the construction's correctness.
- The trace-hash contract is content-addressed
  (`BLAKE3(weights_hash || canonical_input || canonical_output)`)
  — a structural simplification that we want stress-tested for
  unanticipated equivalences or collisions.
- The dispute-resolution mechanism reduces to deterministic
  re-execution; we want adversarial scenarios beyond the seven we
  have tested (`agent_protocol/tests/adversarial_dispute.rs`).
- 35 of 40 production-path `unwrap()` calls are
  `Mutex::lock().unwrap()` — we have a documented audit
  (`docs/UNWRAP_AUDIT.md`) but want your read on whether
  poisoning is the right failure mode.

## What we're proposing

Scope: 5 in-scope crates (~10k LOC), 4 secondary crates (~3k
LOC). Detailed scope in `docs/AUDIT_BRIEF.md` (attached).

Deliverables:
- Read-through report.
- Findings with severity tags (Critical / High / Medium / Low /
  Informational).
- 1 round of remediation re-audit included.
- Public report published in `docs/audits/<date>_trail-of-bits.pdf`
  on acceptance + 30-day partner-courtesy disclosure window.

Estimated duration: 3-5 weeks of auditor time.

Estimated start: as soon as we sign your engagement letter; we are
not blocked on any internal dependency.

## What we'd like from you

- Statement of work for the scope above.
- Quote.
- Earliest start date.
- Confirmation of your team's availability for the engagement
  window.

## Practical details

- **Repository:** github.com/grapheneaffiliate/Transformer-VM-Bank
- **Audit brief:** `docs/AUDIT_BRIEF.md` (attached as PDF — please
  generate from the markdown source via your preferred toolchain).
- **Reproducibility report:** `docs/REPRODUCIBILITY_REPORT.md` —
  every gate's test command + expected timing on a fresh cloud VM.
- **Threat model + invariants:** `docs/SECURITY_REVIEW.md`.
- **Tag for engagement:** `v0.1.0` (will be cut at engagement
  signing).

We're happy to do a 30-minute video call this week to walk through
the architecture and answer any pre-engagement questions. Reply
with your availability.

Thanks for considering us.

— PSL maintainers

---

## Sender checklist before send

- [ ] Replace `sales@trailofbits.com` with the current correct
      address (check trailofbits.com for sales / engagement
      contact).
- [ ] Generate `docs/AUDIT_BRIEF.md` to PDF and attach.
- [ ] Generate `docs/SECURITY_REVIEW.md` to PDF and attach (or
      include as link to commit-pinned URL).
- [ ] Replace "PSL maintainers" with the actual signer's name +
      role.
- [ ] BCC any internal stakeholder who needs to be informed.
- [ ] Diary the response window (5 business days is reasonable for
      a sales-side first response).
