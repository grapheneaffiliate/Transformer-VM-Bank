"""Lean ↔ C drift checker.

The Lean models in `lean/PSL/Ledger.lean` are hand-translations of the C
primitives in `primitives/`. When a primitive's C source changes, the Lean
model MUST be re-checked (and the conservation/determinism theorems
re-proven if the change is semantic).

This tool hashes each primitive's C source and compares against a checked-in
manifest. Mismatches abort with a non-zero exit code; the manifest can be
updated only after a human has reconciled the Lean model with the new C.

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
PRIMS = REPO / "primitives"
MANIFEST = REPO / "lean" / "drift_manifest.json"

PRIMITIVES = [
    "common.h",
    "ledger_freeze.c",
    "ledger_transfer.c",
    "ledger_mint.c",
    "ledger_burn.c",
    "ledger_multi_asset.c",
    "mpt_apply_delta.c",
]


def hash_file(p: Path) -> str:
    return hashlib.blake2b(p.read_bytes(), digest_size=16).hexdigest()


def load_manifest() -> dict:
    if MANIFEST.exists():
        return json.loads(MANIFEST.read_text())
    return {}


def save_manifest(d: dict):
    MANIFEST.parent.mkdir(parents=True, exist_ok=True)
    MANIFEST.write_text(json.dumps(d, indent=2, sort_keys=True))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--update", action="store_true",
                    help="Update manifest after human review of Lean models")
    args = ap.parse_args()

    current = {p: hash_file(PRIMS / p) for p in PRIMITIVES}
    pinned = load_manifest()

    if args.update:
        save_manifest(current)
        print("[drift] manifest updated; re-run lake build to confirm proofs still close")
        return 0

    if not pinned:
        print(f"[drift] no manifest at {MANIFEST}; run --update after manual Lean review")
        return 1

    drift = {p: (pinned.get(p), current[p]) for p in PRIMITIVES if pinned.get(p) != current[p]}
    if drift:
        print("[drift] LEAN MODELS MAY BE STALE — these C primitives changed:")
        for p, (old, new) in drift.items():
            print(f"  {p}:  pinned={old}  current={new}")
        print("Reconcile lean/PSL/Ledger.lean against the new C source, then run --update.")
        return 2

    print("[drift] OK — all primitives match the pinned hashes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
