"""Batch runner for the decomposed freeze pipeline.

Tests the chained primitive: witness → freeze_setup → freeze_apply →
new_byte_47. Compares against the native golden model.
"""

import json
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TVM = Path("/mnt/c/Users/atchi/Transformer-VM")
SPEC_DIR = Path("/tmp/psl_specs")
SPEC_DIR.mkdir(exist_ok=True)


def render_spec(values):
    text = " ".join(str(b) for b in values)
    data = text.encode("utf-8") + b"\x00"
    tokens = ["start"]
    for b in data:
        if 0x20 < b < 0x7F and chr(b) not in ("{", "}"):
            tokens.append(chr(b))
        else:
            tokens.append(f"{b:02x}")
    tokens.append("commit(+0,sts=0,bt=0)")
    return " ".join(tokens)


def run(weights, spec_path):
    r = subprocess.run(
        [
            "uv", "run", "wasm-run",
            "--model", str(weights),
            "--max-new-tokens", "200000",
            spec_path,
        ],
        cwd=str(TVM), capture_output=True, text=True, timeout=120,
    )
    if r.returncode != 0:
        return None, r.stderr[:200]
    for line in r.stdout.splitlines():
        s = line.strip()
        if s.startswith("output:"):
            return [int(x) for x in s[len("output:"):].split()], ""
    return None, "no output"


def chained_freeze(witness, setup_w, apply_w):
    # Step 1: freeze_setup
    spec = SPEC_DIR / "setup.txt"
    spec.write_text(render_spec(witness))
    setup_out, err = run(setup_w, str(spec))
    if setup_out is None:
        return None, f"setup err: {err}"
    if len(setup_out) != 2:
        return None, f"setup expected 2 ints, got {len(setup_out)}"
    # Step 2: freeze_apply
    spec.write_text(render_spec(setup_out))
    apply_out, err = run(apply_w, str(spec))
    if apply_out is None:
        return None, f"apply err: {err}"
    if len(apply_out) != 1:
        return None, f"apply expected 1 int, got {len(apply_out)}"
    return apply_out[0], ""


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 100
    setup_w = REPO / "weights" / "freeze_setup.bin"
    apply_w = REPO / "weights" / "freeze_apply.bin"

    vec_path = REPO / "tests" / "vectors" / "ledger_freeze.json"
    with vec_path.open() as f:
        data = json.load(f)
    vectors = data["vectors"][:n]

    pass_count = 0
    fail_count = 0
    fail_examples = []
    import time
    t0 = time.time()
    for i, v in enumerate(vectors):
        witness = v["input"]
        flag = witness[0]
        acc = list(witness[1:65])
        b47 = acc[47]
        if flag:
            expected = (b47 & 127) | 128
        else:
            expected = b47 & 127

        got, err = chained_freeze(witness, setup_w, apply_w)
        if got is None:
            fail_count += 1
            print(f"  [{i:5d}] ERROR: {err}", flush=True)
            continue
        if got == expected:
            pass_count += 1
            if (i + 1) % 10 == 0:
                elapsed = time.time() - t0
                rate = (i + 1) / elapsed
                eta = (len(vectors) - i - 1) / rate
                print(f"  [{i:5d}] {pass_count} passed, {fail_count} failed ({rate:.1f}/s, ETA {eta:.0f}s)", flush=True)
        else:
            fail_count += 1
            if len(fail_examples) < 5:
                fail_examples.append({
                    "i": i, "flag": flag, "b47_in": b47,
                    "expected": expected, "got": got,
                    "witness_first8": witness[:8],
                })
            print(f"  [{i:5d}] MISMATCH: flag={flag}, b47_in={b47}, expected={expected}, got={got}", flush=True)

    elapsed = time.time() - t0
    print(f"\nResult: {pass_count}/{len(vectors)} passed in {elapsed:.0f}s ({fail_count} failed)", flush=True)
    if fail_examples:
        print(f"\nFirst {len(fail_examples)} failures:", flush=True)
        for fe in fail_examples:
            print(f"  {fe}", flush=True)


if __name__ == "__main__":
    main()
