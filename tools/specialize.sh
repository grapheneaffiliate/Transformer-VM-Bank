#!/usr/bin/env bash
# Specialize a compiled PSL primitive to transformer weights.
#
# Usage: ./tools/specialize.sh data/ledger_freeze.txt [--milp-gap 0.05] [--time-limit 3600]
#
# Output:
#   weights/<name>.bin — specialized transformer weights binary

set -euo pipefail

if [[ -z "${TRANSFORMER_VM_PATH:-}" ]]; then
    TRANSFORMER_VM_PATH="/mnt/c/Users/atchi/Transformer-VM"
fi

PSL_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="$1"
shift || true

if [[ ! -f "$SOURCE" ]]; then
    echo "ERROR: program file not found: $SOURCE" >&2
    exit 1
fi

NAME="$(basename "$SOURCE" .txt)"
mkdir -p "$PSL_ROOT/weights"
WEIGHTS="$PSL_ROOT/weights/${NAME}.bin"

# Default to 5% MILP gap unless user overrides
MILP_GAP="--milp-gap"
if [[ ! " $* " =~ " --milp-gap " ]] && [[ ! " $* " =~ " --milp-gap " ]]; then
    set -- --milp-gap 0.05 "$@"
fi

cd "$TRANSFORMER_VM_PATH"
uv run wasm-specialize "$PSL_ROOT/$SOURCE" --save-weights "$WEIGHTS" "$@"

echo "[specialize] OK: $NAME → $WEIGHTS"
