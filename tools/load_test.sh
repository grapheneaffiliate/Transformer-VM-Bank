#!/usr/bin/env bash
# PSL load test scaffold.
#
# Drives synthetic transactions through a sequencer to find:
#   - sustained TPS ceiling
#   - p50 / p95 / p99 inclusion latency
#   - mempool depth and block utilization at saturation
#   - the exact breaking point (where rejections begin)
#
# This is *not* a benchmark we publish — it is a regression guardrail.
# We run this on the same VM class as production and record the result
# in ops/load-results/YYYY-MM-DD.json. Any release whose result is
# meaningfully worse than the previous release blocks the release.
#
# Usage:
#   tools/load_test.sh                   # full sweep, ~10 min
#   tools/load_test.sh --quick           # 60-second smoke test
#   tools/load_test.sh --target-tps 500  # hold a fixed rate
#
# Requires:
#   - psl-load-driver binary in PATH (built by `cargo build -p psl-loadgen`)
#   - sequencer reachable at PSL_SEQUENCER_RPC (default localhost:26657)
#   - prometheus reachable at PSL_PROM_URL (default localhost:9090)

set -euo pipefail

RPC="${PSL_SEQUENCER_RPC:-http://localhost:26657}"
PROM="${PSL_PROM_URL:-http://localhost:9090}"
OUT_DIR="${PSL_LOAD_OUT_DIR:-ops/load-results}"
mkdir -p "${OUT_DIR}"

QUICK=0
FIXED_TPS=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --quick)      QUICK=1; shift ;;
        --target-tps) FIXED_TPS="$2"; shift 2 ;;
        *)            echo "unknown: $1" >&2; exit 2 ;;
    esac
done

log() { printf '[load %s] %s\n' "$(date -u +%H:%M:%S)" "$*" >&2; }

run_phase() {
    local label="$1" rate="$2" duration_s="$3"
    log "phase ${label}: ${rate} TPS for ${duration_s}s"
    psl-load-driver \
        --rpc "${RPC}" \
        --target-tps "${rate}" \
        --duration "${duration_s}s" \
        --tx-mix "transfer:0.7,swap:0.2,policy_check:0.1" \
        --output "${OUT_DIR}/${label}.jsonl"
}

prom_query() {
    local q="$1"
    curl -s --data-urlencode "query=${q}" "${PROM}/api/v1/query" \
        | python3 -c 'import sys,json; r=json.load(sys.stdin)["data"]["result"]; print(r[0]["value"][1] if r else "NaN")'
}

if [[ -n "${FIXED_TPS}" ]]; then
    run_phase "fixed-${FIXED_TPS}tps" "${FIXED_TPS}" 300
    exit 0
fi

if [[ "${QUICK}" -eq 1 ]]; then
    run_phase "smoke" 100 60
    exit 0
fi

# Full sweep: ramp up until we find the saturation point, then dwell on
# it long enough to characterize p99 latency.
log "starting full load sweep"
for rate in 50 100 200 400 800 1200 1600 2000 2400; do
    run_phase "ramp-${rate}" "${rate}" 60
    rejections="$(prom_query 'increase(psl_mempool_rejections_total[1m])')"
    if (( $(echo "${rejections} > 100" | bc -l) )); then
        log "saturation detected at ${rate} TPS (${rejections} rejections/min)"
        ceiling="${rate}"
        break
    fi
    ceiling="${rate}"
done

dwell="$(( ceiling * 80 / 100 ))"
log "dwelling at 80% of ceiling = ${dwell} TPS for 5 min"
run_phase "dwell-${dwell}" "${dwell}" 300

# Capture summary into the dated JSON file the regression check reads.
date_tag="$(date -u +%Y-%m-%d)"
cat > "${OUT_DIR}/${date_tag}.json" <<EOF
{
  "date": "${date_tag}",
  "ceiling_tps": ${ceiling},
  "sustained_tps": ${dwell},
  "p50_inclusion_ms": $(prom_query 'histogram_quantile(0.50, rate(psl_inclusion_latency_seconds_bucket[5m])) * 1000'),
  "p95_inclusion_ms": $(prom_query 'histogram_quantile(0.95, rate(psl_inclusion_latency_seconds_bucket[5m])) * 1000'),
  "p99_inclusion_ms": $(prom_query 'histogram_quantile(0.99, rate(psl_inclusion_latency_seconds_bucket[5m])) * 1000'),
  "max_mempool_depth": $(prom_query 'max_over_time(psl_mempool_depth[10m])'),
  "psl_version": "$(git rev-parse --short HEAD)"
}
EOF

log "summary written to ${OUT_DIR}/${date_tag}.json"
cat "${OUT_DIR}/${date_tag}.json"
