#!/usr/bin/env bash
# Compile a PSL primitive C source to WASM token files via Transformer-VM.
#
# Usage: ./tools/compile.sh primitives/ledger_freeze.c [--args "1 0 0 ..."]
#
# Output (in $TRANSFORMER_VM_PATH/data/, then symlinked to PSL data/):
#   <name>.txt        — universal-model input (program + sample input)
#   <name>_spec.txt   — specialized-model input (start + sample input)
#   <name>_ref.txt    — reference trace (only if --reference passed)

set -euo pipefail

if [[ -z "${TRANSFORMER_VM_PATH:-}" ]]; then
    TRANSFORMER_VM_PATH="/mnt/c/Users/atchi/Transformer-VM"
fi

if [[ ! -d "$TRANSFORMER_VM_PATH" ]]; then
    echo "ERROR: TRANSFORMER_VM_PATH=$TRANSFORMER_VM_PATH does not exist" >&2
    exit 1
fi

if [[ -z "${CLANG_PATH:-}" ]]; then
    if [[ -x "/mnt/c/Users/atchi/wasi-sdk/bin/clang.exe" ]]; then
        export CLANG_PATH="/mnt/c/Users/atchi/wasi-sdk/bin/clang.exe"
    else
        echo "WARNING: CLANG_PATH not set and default not found; relying on Transformer-VM defaults" >&2
    fi
fi

PSL_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="$1"
shift || true

if [[ ! -f "$SOURCE" ]]; then
    echo "ERROR: source file not found: $SOURCE" >&2
    exit 1
fi

# Symlink primitives/common.h into Transformer-VM examples temporarily so
# clang can resolve `#include "common.h"`. Cleanup in trap.
mkdir -p "$PSL_ROOT/data"

cd "$TRANSFORMER_VM_PATH"
uv run wasm-compile "$PSL_ROOT/$SOURCE" "$@"

# Move generated outputs to PSL data/ for tracking
NAME="$(basename "$SOURCE" .c)"
for suffix in .txt _spec.txt _ref.txt; do
    src="$TRANSFORMER_VM_PATH/data/${NAME}${suffix}"
    if [[ -f "$src" ]]; then
        cp "$src" "$PSL_ROOT/data/${NAME}${suffix}"
    fi
done

echo "[compile] OK: $NAME (outputs in $PSL_ROOT/data/)"
