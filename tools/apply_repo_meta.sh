#!/usr/bin/env bash
# Apply the GitHub repo description + topics in one shot.
#
# Usage:  ./tools/apply_repo_meta.sh <github_pat>
#
# The PAT needs the `repo` scope. It's only used inline by curl — never
# committed or logged.

set -eu

if [[ $# -ne 1 ]]; then
    echo "Usage: $0 <github_pat>" >&2
    exit 1
fi

TOKEN="$1"
OWNER="grapheneaffiliate"
REPO="Transformer-VM-Bank"
API="https://api.github.com/repos/${OWNER}/${REPO}"

DESCRIPTION="Deterministic financial ledger with transformer-verifiable state transitions. Sovereign single-sequencer + consortium BFT modes; ed25519 + BLAKE3 + Sparse Merkle Tree commitments; offline-verifiable balances via mobile light client; ABCI + CometBFT for v2 consensus. Settlement rails for tokenized USD, CBDC, gold, treasuries."

TOPICS_JSON='{"names":["blockchain","settlement-layer","transformer","cryptography","cbdc","stablecoin","tokenization","ed25519","blake3","rust","lean4","wasm","ledger","consensus","light-client","compliance","sparse-merkle-tree","distributed-systems","abci","financial-infrastructure"]}'

# Build the description PATCH body via python json (avoids shell-escaping pitfalls)
DESC_BODY=$(DESCRIPTION="$DESCRIPTION" python3 -c 'import json,os; print(json.dumps({"description": os.environ["DESCRIPTION"]}))')

echo "[1/2] PATCH description..."
HTTP_CODE=$(curl -sS -o /tmp/psl_meta_resp.json -w '%{http_code}' \
    -X PATCH \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    "${API}" \
    -d "$DESC_BODY")
echo "  HTTP $HTTP_CODE"
if [[ "$HTTP_CODE" != "200" ]]; then
    cat /tmp/psl_meta_resp.json
    exit 2
fi
python3 -c 'import json; d=json.load(open("/tmp/psl_meta_resp.json")); print("  description set:", repr((d.get("description") or "")[:100]))'

echo "[2/2] PUT topics..."
HTTP_CODE=$(curl -sS -o /tmp/psl_meta_resp.json -w '%{http_code}' \
    -X PUT \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    "${API}/topics" \
    -d "$TOPICS_JSON")
echo "  HTTP $HTTP_CODE"
if [[ "$HTTP_CODE" != "200" ]]; then
    cat /tmp/psl_meta_resp.json
    exit 3
fi
python3 -c 'import json; d=json.load(open("/tmp/psl_meta_resp.json")); print("  topics set:", d.get("names"))'

rm -f /tmp/psl_meta_resp.json
echo
echo "Done. Verify at https://github.com/${OWNER}/${REPO}"
