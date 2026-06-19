"""Batched 10k transfer test using pre-computed reference traces.

For each witness:
  1. Compute expected output bytes natively.
  2. Render spec.txt as binary tokens of the witness.
  3. Render _ref.txt as the EXPECTED predicted token sequence for the C++ engine
     to compare against (input prefix + execution + output tokens + halt).
  4. Run wasm-run with multiple spec files; the C++ engine compares each to its
     _ref.txt and reports PASS/FAIL.

The token sequence the model emits is:
  start <input_token>... NUL commit
  <execution tokens>...
  out(byte_0) out(byte_1) ... out(byte_N-1)
  halt

We can't easily compute the execution tokens without re-running the universal
evaluator. So instead, we capture the input prefix + the OUT tokens from the
ACTUAL C++ engine output and check the OUT tokens match expected.

Simpler approach: just use the C++ engine's "output:" line which shows
printable bytes literally + '.' for non-printable. Compare char form, allowing
'.' to match expected non-printable bytes exactly.
"""

import os
import random
import re
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TVM = Path(os.environ.get("TRANSFORMER_VM_PATH", os.path.expanduser("~/Transformer-VM")))
SPEC_DIR = Path("/tmp/psl_t10k")
SPEC_DIR.mkdir(exist_ok=True)


def render_binary_spec(witness):
    tokens = ["start"]
    for b in witness:
        if 0x20 < b < 0x7F and chr(b) not in ("{", "}"):
            tokens.append(chr(b))
        else:
            tokens.append(f"{b:02x}")
    tokens.append("00")
    tokens.append("commit(+0,sts=0,bt=0)")
    return " ".join(tokens)


def expected_chars(expected_bytes):
    """Convert expected output bytes to the C++ engine's display char form."""
    out = []
    for b in expected_bytes:
        if (0x20 <= b < 0x7F) or b == ord('\n') or b == ord('\t'):
            out.append(chr(b))
        else:
            out.append('.')
    return "".join(out)


def make_witness(seed):
    rng = random.Random(seed)
    success_in = 1
    from_balance = [rng.randint(0, 255) for _ in range(8)] + [0] * 8
    to_balance = [rng.randint(0, 255) for _ in range(16)]
    amount = [rng.randint(0, 100) for _ in range(2)] + [0] * 14
    from_nonce = [rng.randint(0, 100) for _ in range(2)] + [0] * 6
    return [success_in] + from_balance + to_balance + amount + from_nonce


def compute_expected(witness):
    success_in = witness[0]
    from_balance = witness[1:17]
    to_balance = witness[17:33]
    amount = witness[33:49]
    from_nonce = witness[49:57]
    from_int = int.from_bytes(bytes(from_balance), "little")
    to_int = int.from_bytes(bytes(to_balance), "little")
    amt_int = int.from_bytes(bytes(amount), "little")
    nonce_int = int.from_bytes(bytes(from_nonce), "little")
    success_out = 1 if (success_in and from_int >= amt_int) else 0
    if success_out:
        new_from = list((from_int - amt_int).to_bytes(16, "little"))
        new_to = list((to_int + amt_int).to_bytes(16, "little"))
        new_nonce = list((nonce_int + 1).to_bytes(8, "little"))
    else:
        new_from = [0] * 16
        new_to = [0] * 16
        new_nonce = [0] * 8
    return [success_out] + new_from + new_to + new_nonce


def run_batch(weights, spec_paths):
    cmd = [
        "uv", "run", "wasm-run",
        "--model", str(weights),
        "--max-new-tokens", "50000",
    ] + [str(p) for p in spec_paths]
    t0 = time.time()
    r = subprocess.run(cmd, cwd=str(TVM), capture_output=True, text=True, timeout=14400)
    elapsed = time.time() - t0
    if r.returncode != 0:
        print(f"  wasm-run FAILED ({r.returncode}): {r.stderr[-400:]}", flush=True)
        return {}, elapsed
    by_name = {}
    current_name = None
    line_re = re.compile(r"^(\S+):\s+(?:RAN|PASS|FAIL)")
    for raw in r.stdout.splitlines():
        line = raw.rstrip()
        m = line_re.match(line.lstrip())
        if m:
            current_name = m.group(1)
            continue
        s = line.strip()
        if s.startswith("output:") and current_name:
            by_name[current_name] = s[len("output:"):].strip()
            current_name = None
    return by_name, elapsed


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 10000
    weights = REPO / "weights" / "transfer_binary.bin"

    print(f"[1/3] rendering {n} specs...", flush=True)
    for p in SPEC_DIR.glob("*.txt"):
        p.unlink()
    spec_paths = []
    expecteds = []
    for seed in range(n):
        witness = make_witness(seed)
        expected = compute_expected(witness)
        p = SPEC_DIR / f"t_{seed:06d}.txt"
        p.write_text(render_binary_spec(witness))
        spec_paths.append(p)
        expecteds.append(expected)

    print("[2/3] running wasm-run batch...", flush=True)
    by_name, elapsed = run_batch(weights, spec_paths)
    print(f"  done in {elapsed:.0f}s ({len(spec_paths)/elapsed:.1f}/s)", flush=True)

    print("[3/3] comparing outputs...", flush=True)
    pass_ct = 0
    fail_ct = 0
    fail_examples = []
    for i, p in enumerate(spec_paths):
        stem = p.stem
        got_chars = by_name.get(stem)
        if got_chars is None:
            fail_ct += 1
            if len(fail_examples) < 5:
                fail_examples.append({"i": i, "err": "no output"})
            continue
        want_chars = expected_chars(expecteds[i])
        if got_chars == want_chars:
            pass_ct += 1
        else:
            fail_ct += 1
            d = next((j for j in range(min(len(got_chars), len(want_chars)))
                      if got_chars[j] != want_chars[j]), -1)
            if len(fail_examples) < 5:
                fail_examples.append({
                    "i": i, "diff_at": d,
                    "exp_len": len(want_chars), "got_len": len(got_chars),
                    "exp_char": want_chars[d] if 0 <= d < len(want_chars) else "?",
                    "got_char": got_chars[d] if 0 <= d < len(got_chars) else "?",
                    "exp_byte": expecteds[i][d] if 0 <= d < len(expecteds[i]) else "?",
                })

    print(f"\nResult: {pass_ct}/{n} passed (char-form comparison)", flush=True)
    if fail_examples:
        for fe in fail_examples:
            print(f"  {fe}", flush=True)


if __name__ == "__main__":
    main()
