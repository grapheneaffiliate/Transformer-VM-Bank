#!/usr/bin/env bash
# Sweep all gate-1 primitives via the pure-Rust runner. Saturates 8 cores
# per-primitive. Logs to docs/gate85_logs/canonical/<primitive>.log.

set -e
cd "$(dirname "$0")/.."

LOGDIR="docs/gate85_logs/canonical"
mkdir -p "$LOGDIR"

run_one() {
    local prim="$1"
    local count="$2"
    local log="$LOGDIR/${prim}.log"
    # Resume-friendly: skip if a previous run already wrote a "pass: N/N    fail: 0" summary.
    if grep -q "^  pass: ${count}/${count}    fail: 0$" "$log" 2>/dev/null; then
        echo "============================================================"
        echo "$(date +%H:%M:%S)  $prim  count=$count  -- already complete, skipping"
        echo "============================================================"
        return 0
    fi
    echo "============================================================"
    echo "$(date +%H:%M:%S)  $prim  count=$count"
    echo "============================================================"
    ./target/release/run_gate1 --primitive "$prim" --count "$count" \
        --threads 8 --print-failures 5 2>&1 | tee "$log"
}

# Short primitives at 10k each.
run_one byte_add          10000
run_one byte_sub          10000
run_one transfer_finalize 10000
run_one transfer_check    10000
run_one mpt_emit          10000

# freeze_chain at the largest count that fits in remaining wall budget.
# Per measurements ~140 sec/vec at 8 threads ⇒ 1000 vectors ≈ 4.5h.
run_one freeze_chain      1000

echo "============================================================"
echo "$(date +%H:%M:%S)  ALL DONE"
echo "============================================================"
