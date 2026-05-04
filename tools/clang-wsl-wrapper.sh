#!/usr/bin/env bash
# Forwards to the Windows wasi-sdk clang.exe but rewrites any Linux-style
# /mnt/c/... paths to Windows paths first. Required because Python's
# subprocess module bypasses WSL's automatic interop path translation.
#
# Also injects PSL-specific clang flags that disable optimizations known to
# trigger Transformer-VM WASM-lowering bugs (see docs/FINDINGS.md):
#   -fno-strict-aliasing : prevents the aliasing-based DSE that drops
#                          read-then-OR sequences when clang can prove
#                          the source memory was just written.
#   -fno-tree-vrp        : disables value-range propagation that constant-
#                          folds `cur | 128` to a known constant.
#   -fno-tree-ccp        : disables conditional constant propagation.
#
# Override via PSL_CLANG_EXTRA_FLAGS env var. Override level via
# PSL_CLANG_O_LEVEL (default empty = inherit -O2 from compile_wasm.py).

set -e

CLANG="${WSL_CLANG_EXE:-/mnt/c/Users/atchi/wasi-sdk/bin/clang.exe}"

EXTRA_FLAGS=(
    "-fno-strict-aliasing"
)
if [[ -n "${PSL_CLANG_EXTRA_FLAGS:-}" ]]; then
    # space-separated additional flags (e.g. PSL_CLANG_EXTRA_FLAGS="-O0")
    read -r -a USER_FLAGS <<< "$PSL_CLANG_EXTRA_FLAGS"
    EXTRA_FLAGS+=("${USER_FLAGS[@]}")
fi

ARGS=()
for arg in "$@"; do
    case "$arg" in
        /mnt/*)
            ARGS+=("$(wslpath -w "$arg")")
            ;;
        -include/mnt/*)
            path="${arg#-include}"
            ARGS+=("-include$(wslpath -w "$path")")
            ;;
        -I/mnt/*)
            path="${arg#-I}"
            ARGS+=("-I$(wslpath -w "$path")")
            ;;
        -o)
            ARGS+=("$arg")
            ;;
        -Wl,*)
            ARGS+=("$arg")
            ;;
        *)
            ARGS+=("$arg")
            ;;
    esac
done

# Append our flags AFTER the originals so late flags (e.g. PSL_CLANG_EXTRA_FLAGS=-O0)
# override the -O2 in compile_wasm.py.
exec "$CLANG" "${ARGS[@]}" "${EXTRA_FLAGS[@]}"
