#!/usr/bin/env bash
# Forwards to the Windows wasi-sdk clang.exe but rewrites any Linux-style
# /mnt/c/... paths to Windows paths first. Required because Python's
# subprocess module bypasses WSL's automatic interop path translation.

set -e

CLANG="${WSL_CLANG_EXE:-/mnt/c/Users/atchi/wasi-sdk/bin/clang.exe}"

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

exec "$CLANG" "${ARGS[@]}"
