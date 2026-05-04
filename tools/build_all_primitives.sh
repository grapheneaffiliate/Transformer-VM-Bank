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
#
# Active primitive set (post-decomposition; gate 1 cleared 2026-05-04):
#   freeze_setup + freeze_apply  -> 2 trace hashes per freeze
#   transfer_check + 16x byte_sub_with_borrow + 16x byte_add_with_carry
#                  + transfer_finalize -> 34 trace hashes per transfer
#   16x byte_add_with_carry  -> 16 trace hashes per mint
#   16x byte_sub_with_borrow + 1 check  -> 17 trace hashes per burn
#   mpt_emit_record  -> 1 trace hash per emitted account record
#
# Older monolithic primitives (ledger_freeze.c, ledger_transfer.c, etc.) live
# in docs/archive/primitives/ for historical reference; they're superseded by
# the per-byte composition above. See docs/STYLE_GUIDE_v3.md.

PIPELINE_ARGS=("$@")

# byte_sub_with_borrow: minuend, subtrahend, borrow_in
W_BYTE_SUB="200 50 0"
# byte_add_with_carry: augend, addend, carry_in
W_BYTE_ADD="100 200 0"
# transfer_check: 16 from-balance bytes, 16 amount bytes, frozen flag, asset_id_match
W_CHECK="$(python3 -c 'print(" ".join(["255"]*16) + " " + " ".join(["1"]*16) + " 0 1")')"
# transfer_finalize: 8-byte nonce + 8-byte epoch
W_FINALIZE="$(python3 -c 'print(" ".join(["0"]*8) + " " + " ".join(["1","0","0","0","0","0","0","0"]))')"
# freeze_setup: flag + 64 account bytes
W_FREEZE_SETUP="1 $(python3 -c 'print(" ".join(["0"]*64))')"
# freeze_apply: 64 account bytes (binary form from setup) + 1 byte flag
W_FREEZE_APPLY="$(python3 -c 'print(" ".join(["0"]*64) + " 1")')"
# mpt_emit_record: 64 account bytes (pass-through)
W_MPT_EMIT="$(python3 -c 'print(" ".join(["0"]*64))')"

declare -A WITNESSES=(
    [byte_sub_with_borrow]="$W_BYTE_SUB"
    [byte_add_with_carry]="$W_BYTE_ADD"
    [transfer_check]="$W_CHECK"
    [transfer_finalize]="$W_FINALIZE"
    [freeze_setup]="$W_FREEZE_SETUP"
    [freeze_apply]="$W_FREEZE_APPLY"
    [mpt_emit_record]="$W_MPT_EMIT"
)

PRIMITIVES=(byte_sub_with_borrow byte_add_with_carry transfer_check transfer_finalize freeze_setup freeze_apply mpt_emit_record)

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
