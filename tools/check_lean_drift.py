"""Lean ↔ implementation drift checker.

The Lean models in `lean/PSL/` are hand-translations of implementation
sources. When a watched source changes, the Lean model MUST be re-checked
(and the theorems re-proven if the change is semantic).

Watched correspondence (see VERIFICATION.md "Modeling assumptions"):
  - lean/PSL/Ledger.lean   ↔ sequencer/src/trace.rs (NativeTraceExecutor
                             transfer/mint/burn/freeze semantics) composed
                             from primitives/*.c micro-ops
  - lean/PSL/Account.lean  ↔ crypto/src/account.rs + primitives/common.h
  - lean/PSL/MPT.lean,
    lean/PSL/SMTModel.lean ↔ crypto/src/smt.rs
  - lean/PSL/Compliance.lean ↔ sequencer/src/mempool.rs (validate)

This tool hashes each watched source and compares against a checked-in
manifest. Mismatches (or missing files) abort with a non-zero exit code; the
manifest can be updated only after a human has reconciled the Lean models
with the new sources. Runs in CI (`formal-verification` job).

History note: the original version of this tool watched `ledger_*.c` files
that never existed in this tree, so it had never run successfully and the
hand-translation contract was unenforced. The 2026-06 correspondence audit
fixed the watch list and recorded the known model/implementation deltas in
VERIFICATION.md (model-only `assetId`, ℕ vs u128 `wrapping_add`).

Usage:
    python tools/check_lean_drift.py            # check
    python tools/check_lean_drift.py --update   # update manifest after manual review
"""

import argparse
import hashlib
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
MANIFEST = REPO / "lean" / "drift_manifest.json"

# Paths relative to the repo root. Every file here is load-bearing for the
# faithfulness of some Lean model; see the module docstring for the mapping.
WATCHED = [
    "primitives/common.h",
    "primitives/byte_add_with_carry.c",
    "primitives/byte_sub_with_borrow.c",
    "primitives/freeze_apply.c",
    "primitives/freeze_setup.c",
    "primitives/mpt_emit_record.c",
    "primitives/transfer_check.c",
    "primitives/transfer_finalize.c",
    "sequencer/src/trace.rs",
    "sequencer/src/mempool.rs",
    "crypto/src/account.rs",
    "crypto/src/smt.rs",
]


def hash_file(p: Path) -> str:
    return hashlib.blake2b(p.read_bytes(), digest_size=16).hexdigest()


def load_manifest() -> dict:
    if MANIFEST.exists():
        return json.loads(MANIFEST.read_text())
    return {}


def save_manifest(d: dict):
    MANIFEST.parent.mkdir(parents=True, exist_ok=True)
    MANIFEST.write_text(json.dumps(d, indent=2, sort_keys=True) + "\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--update", action="store_true",
                    help="Update manifest after human review of Lean models")
    args = ap.parse_args()

    missing = [p for p in WATCHED if not (REPO / p).exists()]
    if missing:
        print("[drift] watched sources are MISSING (renamed or deleted?):")
        for p in missing:
            print(f"  {p}")
        print("Fix the watch list in tools/check_lean_drift.py after reconciling "
              "the Lean models, then run --update.")
        return 3

    current = {p: hash_file(REPO / p) for p in WATCHED}
    pinned = load_manifest()

    if args.update:
        save_manifest(current)
        print("[drift] manifest updated; re-run `lake build` to confirm proofs still close")
        return 0

    if not pinned:
        print(f"[drift] no manifest at {MANIFEST}; run --update after manual Lean review")
        return 1

    drift = {p: (pinned.get(p), current[p])
             for p in WATCHED if pinned.get(p) != current[p]}
    stale = [p for p in pinned if p not in current]
    if drift or stale:
        if drift:
            print("[drift] LEAN MODELS MAY BE STALE — these watched sources changed:")
            for p, (old, new) in drift.items():
                print(f"  {p}:  pinned={old}  current={new}")
        if stale:
            print("[drift] manifest entries no longer watched (update the manifest):")
            for p in stale:
                print(f"  {p}")
        print("Reconcile lean/PSL/ against the changed sources, then run --update.")
        return 2

    print(f"[drift] OK — all {len(WATCHED)} watched sources match the pinned hashes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
