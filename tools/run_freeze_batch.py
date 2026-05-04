"""Batch runner: feed N witnesses through wasm-run via the C++ engine
sequentially. ~3s per witness (subprocess + load). 100 witnesses = ~5 min.
Reports pass/fail per witness with first-mismatch diff."""

import json
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TVM = Path("/mnt/c/Users/atchi/Transformer-VM")


def render_spec(witness):
    text = " ".join(str(b) for b in witness)
    data = text.encode("utf-8") + b"\x00"
    tokens = ["start"]
    for b in data:
        if 0x20 < b < 0x7F and chr(b) not in ("{", "}"):
            tokens.append(chr(b))
        else:
            tokens.append(f"{b:02x}")
    tokens.append("commit(+0,sts=0,bt=0)")
    return " ".join(tokens)


def golden_freeze(witness):
    flag = witness[0]
    acc = list(witness[1:65])
    if flag:
        acc[47] = acc[47] | 128
    else:
        acc[47] = acc[47] & 127
    return acc


def run_one(spec_path, weights):
    r = subprocess.run(
        [
            "uv", "run", "wasm-run",
            "--model", str(weights),
            "--max-new-tokens", "5000000",
            spec_path,
        ],
        cwd=str(TVM), capture_output=True, text=True, timeout=180,
    )
    if r.returncode != 0:
        return None, r.stderr
    for line in r.stdout.splitlines():
        s = line.strip()
        if s.startswith("output:"):
            return [int(x) for x in s[len("output:"):].split()], ""
    return None, "no output line"


def main():
    primitive = sys.argv[1] if len(sys.argv) > 1 else "ledger_freeze"
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 100
    vec_path = REPO / "tests" / "vectors" / f"{primitive}.json"
    weights = REPO / "weights" / f"{primitive}.bin"
    with vec_path.open() as f:
        data = json.load(f)
    vectors = data["vectors"][:n]

    spec_dir = Path("/tmp/psl_specs")
    spec_dir.mkdir(exist_ok=True)

    pass_count = 0
    fail_count = 0
    fail_examples = []
    for i, v in enumerate(vectors):
        witness = v["input"]
        expected = golden_freeze(witness)
        spec_path = spec_dir / f"{primitive}_{i}.txt"
        with spec_path.open("w") as f:
            f.write(render_spec(witness))
        got, err = run_one(str(spec_path), weights)
        if got is None:
            fail_count += 1
            print(f"  [{i:5d}] ERROR: {err[:120]}", flush=True)
            continue
        if got == expected:
            pass_count += 1
            if (i + 1) % 10 == 0:
                print(f"  [{i:5d}] running ... {pass_count} passed, {fail_count} failed", flush=True)
        else:
            fail_count += 1
            first_diff = next(
                (j for j in range(min(len(got), len(expected))) if got[j] != expected[j]),
                min(len(got), len(expected)),
            )
            if len(fail_examples) < 5:
                fail_examples.append({
                    "witness_idx": i,
                    "witness_first8": witness[:8],
                    "first_diff_at": first_diff,
                    "expected": expected[first_diff] if first_diff < len(expected) else "?",
                    "got": got[first_diff] if first_diff < len(got) else "?",
                })
            print(f"  [{i:5d}] MISMATCH at byte {first_diff}: expected {expected[first_diff]}, got {got[first_diff]}", flush=True)

    print(f"\nResult: {pass_count}/{len(vectors)} passed ({fail_count} failed)", flush=True)
    if fail_examples:
        print(f"\nFirst {len(fail_examples)} failures:", flush=True)
        for fe in fail_examples:
            print(f"  {fe}", flush=True)


if __name__ == "__main__":
    main()
