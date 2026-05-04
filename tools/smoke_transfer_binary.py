"""Binary-IO transfer_sub smoke. Each value = ONE wire byte (no decimal parsing).
Spec writer encodes witness bytes directly as wire tokens.
Output is also raw bytes; parsed back from C++ engine output bytes.
"""

import os
import random
import subprocess
import sys
import re
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TVM = Path(os.environ.get("TRANSFORMER_VM_PATH", "/mnt/c/Users/atchi/Transformer-VM"))
SPEC_DIR = Path("/tmp/psl_smoke_binary")
SPEC_DIR.mkdir(exist_ok=True)


def render_binary_spec(witness):
    """Each witness byte = one wire token."""
    tokens = ["start"]
    for b in witness:
        if 0x20 < b < 0x7F and chr(b) not in ("{", "}"):
            tokens.append(chr(b))
        else:
            tokens.append(f"{b:02x}")
    tokens.append("00")
    tokens.append("commit(+0,sts=0,bt=0)")
    return " ".join(tokens)


def run_binary(weights, spec_path):
    """Run wasm-run via Python path with -v to get out(...) tokens.
    Returns the list of output byte values."""
    r = subprocess.run(
        ["uv", "run", "wasm-run", "--python", "--model", str(weights),
         "--max-new-tokens", "50000", "-v", spec_path],
        cwd=str(TVM), capture_output=True, text=True, timeout=180,
    )
    if r.returncode != 0:
        return None, r.stderr[-200:]
    out_tokens = re.findall(r'out\(([^)]+)\)', r.stderr + r.stdout)
    bytes_ = []
    for t in out_tokens:
        if len(t) == 1:
            bytes_.append(ord(t))
        else:
            bytes_.append(int(t, 16))
    return bytes_, ""


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 20
    weights = REPO / "weights" / "transfer_sub_binary.bin"

    pass_ct = 0
    fail_ct = 0
    fail_examples = []
    for seed in range(n):
        rng = random.Random(seed)
        success = 1  # always test success path
        from_balance = [rng.randint(20, 250) for _ in range(16)]
        amount = [rng.randint(0, 19) for _ in range(16)]  # smaller so no borrow

        # Native golden
        from_b_int = int.from_bytes(bytes(from_balance), "little")
        amt_int = int.from_bytes(bytes(amount), "little")
        new_b = from_b_int - amt_int
        expected = list(new_b.to_bytes(16, "little"))

        witness = [success] + from_balance + amount
        spec = SPEC_DIR / f"sub_{seed}.txt"
        spec.write_text(render_binary_spec(witness))

        got_bytes, err = run_binary(weights, str(spec))
        if got_bytes is None:
            fail_ct += 1
            print(f"  [{seed}] FAILED: {err}")
            continue

        if got_bytes == expected:
            pass_ct += 1
        else:
            fail_ct += 1
            d = next((i for i in range(min(len(got_bytes), len(expected))) if got_bytes[i] != expected[i]), -1)
            if len(fail_examples) < 5:
                fail_examples.append({
                    "seed": seed, "diff_at": d,
                    "exp": expected[d] if d >= 0 else "?",
                    "got": got_bytes[d] if d >= 0 else "?",
                })

    print(f"\nResult: {pass_ct}/{n} passed")
    if fail_examples:
        for fe in fail_examples:
            print(f"  {fe}")


if __name__ == "__main__":
    main()
