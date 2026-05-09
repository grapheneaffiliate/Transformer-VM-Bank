# Engagement request — OtterSec

**Status:** draft, awaiting human sign-off and send.
**Recipient:** contact@osec.io (verify current address before send).
**Subject:** Audit engagement request — PSL agent execution layer

---

OtterSec team —

Requesting a security review for **PSL** (Percepta Settlement Layer)
— a deterministic financial ledger whose Phase 2 work shipped a
first-of-its-kind agent-to-agent transaction layer with
deterministic dispute resolution. Detailed brief in
`docs/AUDIT_BRIEF.md`.

Why OtterSec:

- Your recent work on Solana program audits and the SVM execution
  surface is the closest comparable to PSL's ternary-integer
  execution kernel — both are deterministic, both are
  re-executable, both have novel security surfaces around
  cross-platform agreement.
- We value your reputation for fast turnaround without
  sacrificing depth.
- Your incident history with Cosmos and Move ecosystems gives us
  confidence on the multi-language boundary risk inside the
  agent SDK (Rust core + planned UniFFI bindings).

## Scope (summary; full in AUDIT_BRIEF.md)

In scope (primary):
- `ternary_vm/` — pure-integer execution kernel.
- `agent_contracts/` — 8-contract standard library.
- `agent_wallet/` — SLIP-0010 + spending policies + revocation.
- `agent_protocol/` — 5 wire messages + state machine + dispute.
- `agent_sdk/` — high-level runtime.

In scope (secondary):
- `crypto/`, `consensus/`, `sequencer/`, `light_client/`.

Out of scope:
- `legacy/rust_runner/` (frozen per ADR-0001).
- Network transport (caller-wired).

## Asks

- Statement of work for the scope above.
- Quote.
- Earliest start date.
- Confirmation of team capacity during the engagement window.

We're targeting `v0.1.0` (cut at signing) as the audit tag. Repo:
github.com/grapheneaffiliate/Transformer-VM-Bank.

Open to a call this week to walk through architecture and answer
pre-engagement questions.

Thanks,
— PSL maintainers

---

## Sender checklist before send

- [ ] Verify current OtterSec contact address (osec.io).
- [ ] Attach AUDIT_BRIEF.md → PDF.
- [ ] Confirm OtterSec's typical engagement format (Telegram?
      email? other?) — adjust delivery accordingly.
- [ ] Replace "PSL maintainers" with signer name + role.
- [ ] Diary 5-business-day response window.
