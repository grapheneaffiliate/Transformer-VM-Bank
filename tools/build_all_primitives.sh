#!/usr/bin/env bash
# Build all PSL primitives end-to-end (compile → specialize).
#
# For each primitive, uses a deterministic representative witness as the
# --args input. The witness shape doesn't affect the specialized weights;
# it just lets wasm-compile produce a sample input prefix.
#
# Usage: ./tools/build_all_primitives.sh [--milp-gap 0.05]

set -euo pipefail

PSL_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PSL_ROOT"

# Sample witnesses: pick small concrete values so wasm-compile sees a valid
# parse without ambiguity. These don't affect the specialized weights — only
# the bit-exact verification harness does.

PIPELINE_ARGS=("$@")

# ledger_freeze: flag=1, then 64 zero bytes
W_FREEZE="1 $(python3 -c 'print(" ".join(["0"]*64))')"

# ledger_transfer: epoch=1, from(64), to(64), amount(16)
W_TRANSFER="1 $(python3 -c 'print(" ".join(["0"]*64) + " " + " ".join(["0"]*64) + " " + " ".join(["0"]*16))')"

# ledger_mint: epoch=1, to(64), amount(16)
W_MINT="1 $(python3 -c 'print(" ".join(["0"]*64) + " " + " ".join(["0"]*16))')"

# ledger_burn: epoch=1, from(64), amount(16)
W_BURN="1 $(python3 -c 'print(" ".join(["0"]*64) + " " + " ".join(["0"]*16))')"

# ledger_multi_asset: epoch=1, n=1, then one transfer payload
W_MULTI="1 1 $(python3 -c 'print(" ".join(["0"]*64) + " " + " ".join(["0"]*64) + " " + " ".join(["0"]*16))')"

# mpt_apply_delta: n=1, idx=0, then 64 bytes
W_MPT="1 0 $(python3 -c 'print(" ".join(["0"]*64))')"

declare -A WITNESSES=(
    [ledger_freeze]="$W_FREEZE"
    [ledger_transfer]="$W_TRANSFER"
    [ledger_mint]="$W_MINT"
    [ledger_burn]="$W_BURN"
    [ledger_multi_asset]="$W_MULTI"
    [mpt_apply_delta]="$W_MPT"
)

PRIMITIVES=(ledger_freeze ledger_transfer ledger_mint ledger_burn ledger_multi_asset mpt_apply_delta)

for name in "${PRIMITIVES[@]}"; do
    echo
    echo "═══════════════════════════════════════════════════════════════"
    echo "  $name"
    echo "═══════════════════════════════════════════════════════════════"
    ./tools/compile.sh "primitives/${name}.c" --args "${WITNESSES[$name]}"
    ./tools/specialize.sh "data/${name}.txt" "${PIPELINE_ARGS[@]}"
done

echo
echo "[build_all] OK: all primitives compiled and specialized"
