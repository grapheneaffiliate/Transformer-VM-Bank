"""Batched runner: feeds N spec files to a single wasm-run invocation per
primitive. Amortizes model-load over all witnesses (~3s amortized + 0.25s
per witness vs per-witness 5s subprocess startup).

Pipeline:
  1. Render N freeze_setup spec files
  2. Run all N through wasm-run (one process) → N (flag, byte_47) outputs
  3. Render N freeze_apply spec files from those outputs
  4. Run all N through wasm-run (one process) → N new_byte outputs
  5. Compare to native golden model. Report pass/fail.
"""

import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TVM = Path(os.environ.get("TRANSFORMER_VM_PATH", "/mnt/c/Users/atchi/Transformer-VM"))
SPEC_DIR = Path("/tmp/psl_batch_specs")


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


def run_batch(weights, spec_paths, max_new_tokens=200000, batch_label="batch"):
    """Run a list of spec files in a single wasm-run invocation.
    Returns parallel list of output-byte lists (or None on parse failure)."""
    cmd = [
        "uv", "run", "wasm-run",
        "--model", str(weights),
        "--max-new-tokens", str(max_new_tokens),
    ] + [str(p) for p in spec_paths]
    print(f"  [{batch_label}] launching wasm-run with {len(spec_paths)} spec files...", flush=True)
    t0 = time.time()
    r = subprocess.run(cmd, cwd=str(TVM), capture_output=True, text=True, timeout=14400)
    elapsed = time.time() - t0
    if r.returncode != 0:
        print(f"  [{batch_label}] wasm-run FAILED ({r.returncode}): {r.stderr[-400:]}", flush=True)
        return [None] * len(spec_paths)

    outputs = []
    current_name = None
    current_output = None
    line_re = re.compile(r"^(\S+):\s+(?:RAN|PASS|FAIL)\s")
    for raw in r.stdout.splitlines():
        line = raw.rstrip()
        m = line_re.match(line.lstrip())
        if m:
            if current_name is not None:
                outputs.append((current_name, current_output))
            current_name = m.group(1)
            current_output = None
            continue
        s = line.strip()
        if s.startswith("output:"):
            current_output = [int(x) for x in s[len("output:"):].split()]
    if current_name is not None:
        outputs.append((current_name, current_output))

    by_stem = {os.path.basename(str(p)).replace(".txt", ""): None for p in spec_paths}
    for name, out in outputs:
        if name in by_stem:
            by_stem[name] = out
    result = [by_stem.get(os.path.basename(str(p)).replace(".txt", "")) for p in spec_paths]
    print(f"  [{batch_label}] done in {elapsed:.0f}s ({len(spec_paths)/elapsed:.1f}/s)", flush=True)
    return result


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 100
    setup_w = REPO / "weights" / "freeze_setup.bin"
    apply_w = REPO / "weights" / "freeze_apply.bin"

    vec_path = REPO / "tests" / "vectors" / "ledger_freeze.json"
    with vec_path.open() as f:
        data = json.load(f)
    vectors = data["vectors"][:n]

    SPEC_DIR.mkdir(exist_ok=True)
    # Clear old specs
    for old in SPEC_DIR.glob("*.txt"):
        old.unlink()

    # Step 1: render freeze_setup specs
    print(f"[1/4] rendering {len(vectors)} freeze_setup specs...", flush=True)
    setup_paths = []
    for i, v in enumerate(vectors):
        p = SPEC_DIR / f"setup_{i:06d}.txt"
        p.write_text(render_spec(v["input"]))
        setup_paths.append(p)

    # Step 2: run all freeze_setup in one wasm-run call
    print(f"[2/4] running freeze_setup batch...", flush=True)
    setup_outputs = run_batch(setup_w, setup_paths, max_new_tokens=200000, batch_label="setup")

    # Step 3: render freeze_apply specs
    print(f"[3/4] rendering freeze_apply specs from setup outputs...", flush=True)
    apply_paths = []
    bad_setup = 0
    for i, out in enumerate(setup_outputs):
        if out is None or len(out) != 2:
            bad_setup += 1
            apply_paths.append(None)
            continue
        p = SPEC_DIR / f"apply_{i:06d}.txt"
        p.write_text(render_spec(out))
        apply_paths.append(p)
    print(f"  bad setup outputs: {bad_setup}", flush=True)

    valid_apply_paths = [p for p in apply_paths if p is not None]

    # Step 4: run all freeze_apply in one wasm-run call
    print(f"[4/4] running freeze_apply batch ({len(valid_apply_paths)} valid)...", flush=True)
    apply_outputs_valid = run_batch(apply_w, valid_apply_paths, max_new_tokens=50000, batch_label="apply")
    apply_iter = iter(apply_outputs_valid)
    apply_outputs = [next(apply_iter) if p is not None else None for p in apply_paths]

    # Compare
    pass_count = 0
    fail_count = 0
    fail_examples = []
    for i, (v, ao) in enumerate(zip(vectors, apply_outputs)):
        witness = v["input"]
        flag = witness[0]
        b47 = witness[48]  # witness[0]=flag, witness[1..64]=account
        if flag:
            expected = (b47 & 127) | 128
        else:
            expected = b47 & 127
        if ao is None or len(ao) != 1:
            fail_count += 1
            if len(fail_examples) < 5:
                fail_examples.append({"i": i, "err": "no apply output", "expected": expected})
            continue
        got = ao[0]
        if got == expected:
            pass_count += 1
        else:
            fail_count += 1
            if len(fail_examples) < 5:
                fail_examples.append({
                    "i": i, "flag": flag, "b47": b47,
                    "expected": expected, "got": got,
                    "setup_out": setup_outputs[i],
                })

    print(f"\nResult: {pass_count}/{len(vectors)} passed ({fail_count} failed)", flush=True)
    if fail_examples:
        print(f"\nFirst {len(fail_examples)} failures:", flush=True)
        for fe in fail_examples:
            print(f"  {fe}", flush=True)


if __name__ == "__main__":
    main()
