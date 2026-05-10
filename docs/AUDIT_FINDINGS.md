# PSL Audit Findings

**Status:** placeholder. v0.1.0 external audit pending.

This file is the canonical tracker for findings produced by external
audits and operational incidents. It is referenced by:

- `docs/AUDIT_BRIEF.md` § 8 — auditor remediation tracking.
- `docs/SECURITY_REVIEW.md` § 8 — finding-row procedure.
- `docs/OPERATIONAL_READINESS.md` — post-incident finding capture.
- `docs/runbooks/{consensus-halt,dispute-storm,sequencer-key-compromise}.md`
  — file-a-finding step.

It is intentionally empty as of v0.1.0. The first row will be added
on either:

1. The first finding from the gate-17 external security audit (per
   `docs/AUDIT_BRIEF.md` § 8 engagement-letter procedure).
2. The first incident-derived finding produced by a runbook
   execution post-launch.

## Format (when populated)

Each finding gets a row in the table below:

| ID | Severity | Title | Source | Reported | Status | Fix commit | Re-audit |
| --- | --- | --- | --- | --- | --- | --- | --- |

Where:
- **ID** — `PSL-AF-NNNN` zero-padded sequential.
- **Severity** — Critical / High / Medium / Low / Informational, per
  the auditor's classification or, for incident-derived findings,
  per `docs/SECURITY_REVIEW.md` § 8.
- **Source** — auditor name (Trail of Bits / Zellic / OtterSec /
  cryptographer-review-engagement / etc.) or runbook name.
- **Status** — Open / In progress / Fixed-pending-re-audit / Closed.
- **Fix commit** — short hash + link.
- **Re-audit** — sign-off reference if the finding required re-audit
  per the engagement contract.

## Triage SLA

Per `docs/AUDIT_BRIEF.md` § 9 engagement logistics:
- Critical / High — remediated within 10 business days of report
  delivery.
- Medium / Low — remediated within 30 business days.

Re-audit is bundled into the original engagement scope.

## Public disclosure

Audit reports are published in tree at `docs/audits/<date>_<vendor>.pdf`
on acceptance + 30-day partner-courtesy window per
`docs/AUDIT_BRIEF.md` § 9. CVEs are filed for any pre-disclosure
vulnerabilities discovered.
