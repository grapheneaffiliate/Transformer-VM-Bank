#!/usr/bin/env bash
# Quarterly review of the gate-9 BFT consensus engine deferral
# (per ADR-0002). Outputs a markdown report to
# docs/quarterly_reviews/YYYY-QN-bft-consensus.md.
#
# Run manually each quarter; this is a checklist generator, not a
# decision maker.

set -e
cd "$(dirname "$0")/.."

QUARTER=$(date +"%Y-Q$(( ($(date +%-m) - 1) / 3 + 1 ))")
OUT="docs/quarterly_reviews/${QUARTER}-bft-consensus.md"
mkdir -p "$(dirname "$OUT")"

cat > "$OUT" <<EOF
# BFT consensus engine review — ${QUARTER}

Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
Per ADR-0002 (\`docs/decisions/0002-bft-consensus-engine-selection.md\`).

## Trigger status

For each trigger, fill in current state and whether it has fired.

### Trigger 1 — institutional pilot LOI requires multi-validator consensus

Pilots in active discussion that mention multi-validator
requirements:

- [ ] (fill in)

Fired: NO

### Trigger 2 — Malachite v1.0 + external audit

Latest Malachite tagged release (canonical repo
\`github.com/circlefin/malachite\`):

\`\`\`
$(curl -sf https://api.github.com/repos/circlefin/malachite/releases/latest 2>/dev/null \
    | python3 -c "import json, sys; d = json.load(sys.stdin); print(d.get('tag_name','UNKNOWN'), '—', d.get('published_at','?'))" 2>/dev/null \
    || echo "(could not query GitHub API; check manually)")
\`\`\`

External audits published:

- [ ] (search the project's docs/ or security advisories)

Fired: NO

### Trigger 3 — test net >100 active agents AND any agent >10% volume

Active agents (last 30 days): TBD
Top agent's share of transaction volume: TBD

Fired: NO

## Action

- [ ] None of the triggers fired this quarter. Re-run next quarter.
- [ ] One or more triggers fired. Open implementation issue with the
      60-day SLA; assign technical lead; begin selection between
      Malachite (option A) and CometBFT (option B) per ADR-0002.

## Notes

(Free-form notes for next quarter's reviewer.)
EOF

echo "Wrote $OUT — open it and fill in the (fill in) sections, then commit."
EOF
