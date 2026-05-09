#!/usr/bin/env bash
# Software bill of materials for the PSL workspace.
#
# Emits:
#   1. Workspace package list
#   2. Full dependency tree (cargo tree)
#   3. Direct dependencies per workspace member
#   4. cargo audit summary (if cargo-audit installed)
#
# Usage: ./tools/sbom.sh > sbom.txt
#
# Pre-audit: install cargo-audit and cargo-deny:
#   cargo install cargo-audit cargo-deny

set -e
cd "$(dirname "$0")/.."

echo "# PSL Software Bill of Materials"
echo "# generated $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "# rust: $(rustc --version)"
echo "# cargo: $(cargo --version)"
echo
echo "## Workspace members"
sed -n '/^members = \[/,/^]/p' Cargo.toml | grep -E '^\s+"' | sed -E 's/^\s+"([^"]+)".*/    - \1/'
echo
echo "## Full dependency tree (cargo tree --workspace --edges normal)"
echo '```'
cargo tree --workspace --edges normal 2>&1 | head -200
echo '```'
echo
echo "## Direct dependencies per workspace member"
for member in crypto consensus sequencer light_client rust_runner ternary_vm agent_contracts agent_wallet agent_protocol agent_sdk pilot/issuer_demo; do
    if [[ -f "$member/Cargo.toml" ]]; then
        echo
        echo "### $member"
        echo '```'
        sed -n '/^\[dependencies\]/,/^\[/p' "$member/Cargo.toml" \
            | grep -vE '^\s*$|^\[' \
            | head -40
        echo '```'
    fi
done
echo
echo "## cargo audit"
echo '```'
if command -v cargo-audit >/dev/null 2>&1; then
    cargo audit 2>&1 | head -80
else
    echo "cargo-audit not installed; install with: cargo install cargo-audit"
fi
echo '```'
echo
echo "## cargo deny"
echo '```'
if command -v cargo-deny >/dev/null 2>&1; then
    cargo deny check licenses bans advisories 2>&1 | head -80
else
    echo "cargo-deny not installed; install with: cargo install cargo-deny"
fi
echo '```'
