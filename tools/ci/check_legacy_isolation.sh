#!/usr/bin/env bash
# CI guard: no code outside `legacy/` may import from the legacy
# fp64 runner. Per ADR-0001 (`docs/decisions/0001-retire-legacy-fp64-runner.md`).
#
# Usage: ./tools/ci/check_legacy_isolation.sh
# Exit 0 = clean. Exit 1 = a non-legacy file imported the legacy runner.

set -e
cd "$(dirname "$0")/../.."

# Patterns that indicate a legacy import:
#   use psl_rust_runner            (Rust use statement)
#   psl-rust-runner = …            (Cargo dependency)
#   psl_rust_runner::              (qualified path)
#
# Excludes:
#   legacy/                        (the crate itself + its examples)
#   docs/                          (documentation may reference it)
#   docs/decisions/                (ADRs reference it)
#   tools/ci/check_legacy_isolation.sh  (this file)

RESULT=0

while IFS= read -r file; do
    case "$file" in
        # Allowed referencers:
        legacy/*)                                ;;  # the crate itself
        docs/*)                                  ;;  # docs, ADRs, status, history
        CHANGELOG.md)                            ;;  # release history
        Cargo.lock)                              ;;  # generated workspace lockfile
        tools/ci/check_legacy_isolation.sh)      ;;  # this file
        # Cross-engine verification harness — explicit opt-in via env var,
        # documented as a backward-compat path in `tests/test_bit_exact.py`.
        tests/test_bit_exact.py)                 ;;
        *)
            echo "FAIL: $file imports from the legacy fp64 runner."
            grep -nE "psl_rust_runner|psl-rust-runner" "$file" || true
            RESULT=1
            ;;
    esac
done < <(git ls-files | xargs grep -lE "psl_rust_runner|psl-rust-runner" 2>/dev/null || true)

if [[ $RESULT -eq 0 ]]; then
    echo "ok: no legacy-runner imports outside legacy/"
fi
exit $RESULT
