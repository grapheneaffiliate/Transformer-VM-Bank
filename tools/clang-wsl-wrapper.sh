#!/usr/bin/env bash
# WASI clang dispatcher.
#
# On native Linux: forwards directly to a Linux-native WASI clang binary.
#   Resolution order:
#     1. $WASI_CLANG  (full path to clang)
#     2. /opt/wasi-sdk/bin/clang
#     3. /usr/local/wasi-sdk/bin/clang
#     4. clang in PATH (only if it knows the wasm32-wasi target)
#
# On WSL where only the Windows-side wasi-sdk is installed: forwards to
# clang.exe and rewrites /mnt/c/... → C:\... path arguments first
# (Python's subprocess module bypasses WSL's automatic interop translation).
#   Resolution order:
#     1. $WSL_CLANG_EXE  (legacy)
#     2. $WASI_CLANG_EXE
#     3. error out — set $WASI_CLANG_EXE to your wasi-sdk Windows binary
#        (e.g. /mnt/c/<user>/wasi-sdk/bin/clang.exe on WSL).
#
# Also injects PSL-specific clang flags that disable optimizations known to
# trigger Transformer-VM WASM-lowering bugs (see docs/FINDINGS.md):
#   -fno-strict-aliasing : prevents the aliasing-based DSE that drops
#                          read-then-OR sequences when clang can prove
#                          the source memory was just written.
#
# Override via PSL_CLANG_EXTRA_FLAGS env var (e.g. PSL_CLANG_EXTRA_FLAGS="-O0").

set -e

# Decide which clang to call: prefer Linux-native if found, fall back to
# the Windows wrapper on WSL.
NATIVE_CLANG=""
for candidate in "${WASI_CLANG:-}" /opt/wasi-sdk/bin/clang /usr/local/wasi-sdk/bin/clang; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
        NATIVE_CLANG="$candidate"
        break
    fi
done

EXTRA_FLAGS=("-fno-strict-aliasing")
if [[ -n "${PSL_CLANG_EXTRA_FLAGS:-}" ]]; then
    read -r -a USER_FLAGS <<< "$PSL_CLANG_EXTRA_FLAGS"
    EXTRA_FLAGS+=("${USER_FLAGS[@]}")
fi

if [[ -n "$NATIVE_CLANG" ]]; then
    # Linux native: pass arguments through unchanged.
    exec "$NATIVE_CLANG" "$@" "${EXTRA_FLAGS[@]}"
fi

# WSL fallback: rewrite /mnt/c/... paths and call clang.exe.
WIN_CLANG="${WSL_CLANG_EXE:-${WASI_CLANG_EXE:-}}"
if [[ ! -x "$WIN_CLANG" ]]; then
    echo "ERROR: no WASI clang found." >&2
    echo "  Set WASI_CLANG to a Linux-native wasi-sdk clang, e.g." >&2
    echo "    export WASI_CLANG=/opt/wasi-sdk/bin/clang" >&2
    echo "  Or on WSL, install wasi-sdk on the Windows side and set" >&2
    echo "    export WASI_CLANG_EXE=/mnt/c/path/to/wasi-sdk/bin/clang.exe" >&2
    exit 1
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

exec "$WIN_CLANG" "${ARGS[@]}" "${EXTRA_FLAGS[@]}"
