# Security Policy

## Reporting a vulnerability

**Do not file a public GitHub issue for a security vulnerability in PSL.**

Instead, email **security@psl.invalid** (or, until that address is
operational, the maintainer addresses listed in `MAINTAINERS.md`)
with:

- A description of the issue.
- Steps to reproduce, ideally a minimal proof-of-concept.
- The component / commit hash where you observed it.
- Your assessment of severity (blast radius, exploitability).
- How you'd like to be credited if at all.

We will acknowledge receipt within **3 business days**. We aim to
either confirm or refute the report within **14 days**.

## Supported versions

Only the latest released minor version receives security updates.
At v0.1.0, the supported version is `0.1.x`.

## Disclosure timeline

We follow a coordinated disclosure pattern:

| Day | Event |
| --- | --- |
| 0   | Report received, acknowledged within 3 days. |
| 1–14 | Triage, reproduction, severity assessment. |
| 14–60 | Fix developed, reviewed, tested. |
| 60+ | Fix released; advisory published 30 days after release. |

If the issue is being **actively exploited**, we may compress the
timeline aggressively (release within days; advisory immediately).

If after 90 days from initial report we have not made meaningful
progress on a fix, the reporter is free to disclose publicly. We
prefer to avoid this; reach out if we appear to be sitting on a
report and we'll give an honest status.

## Scope

In-scope (please report):

- Cryptographic flaws in `crypto/`, `agent_wallet/`,
  `agent_protocol/`.
- Determinism violations on the verifier path (`ternary_vm/`,
  `agent_contracts/`). Determinism is a security property here, not
  just a correctness one.
- Sequencer- or follower-level consensus bugs that allow state-root
  divergence.
- Light-client bugs that allow accepting invalid proofs.
- Authentication, signature-verification, or replay-protection bugs
  in the agent protocol.
- Any unwrap/expect/panic on production paths reachable from
  attacker-controlled input that wasn't documented in
  `docs/UNWRAP_AUDIT.md`.

Out-of-scope (please don't report as security):

- Code in `legacy/` (frozen per ADR-0001).
- Issues that require root on the host running the sequencer.
- Issues in third-party dependencies — report those upstream first.
  If they affect PSL specifically, mention us in the upstream report.
- Theoretical issues with no demonstrable impact path.

## Recognition

Reporters of valid issues are credited in the release notes for the
fixed release, unless they request otherwise. PSL does not currently
run a paid bug bounty program; this may change post-audit.

## What we ask

- Test on your own deployment, not someone else's.
- Don't access data that isn't yours.
- Don't degrade service.
- Give us a reasonable window to fix before disclosing.

In return:
- We will treat your report seriously and respond on the timeline
  above.
- We will credit you on disclosure.
- We will not threaten legal action against good-faith research
  conducted within these guidelines.
