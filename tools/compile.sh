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
    PSL_ROOT_FOR_WRAPPER="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    WRAPPER="$PSL_ROOT_FOR_WRAPPER/tools/clang-wsl-wrapper.sh"
    if [[ -x "$WRAPPER" ]]; then
        # On WSL, Python's subprocess bypasses interop path translation; the
        # wrapper rewrites /mnt/c/... → C:\... before forwarding to clang.exe.
        export CLANG_PATH="$WRAPPER"
    elif [[ -x "/mnt/c/Users/atchi/wasi-sdk/bin/clang.exe" ]]; then
        export CLANG_PATH="/mnt/c/Users/atchi/wasi-sdk/bin/clang.exe"
    else
        echo "WARNING: CLANG_PATH not set and default not found" >&2
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

# Move generated outputs to PSL data/ for tracking. Transformer-VM writes
# under transformer_vm/data/, not data/.
NAME="$(basename "$SOURCE" .c)"
TVM_DATA_DIRS=("$TRANSFORMER_VM_PATH/transformer_vm/data" "$TRANSFORMER_VM_PATH/data")
for suffix in .txt _spec.txt _ref.txt; do
    for d in "${TVM_DATA_DIRS[@]}"; do
        src="$d/${NAME}${suffix}"
        if [[ -f "$src" ]]; then
            cp "$src" "$PSL_ROOT/data/${NAME}${suffix}"
            break
        fi
    done
done

echo "[compile] OK: $NAME (outputs in $PSL_ROOT/data/)"
