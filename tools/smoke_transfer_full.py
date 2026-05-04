"""Single-primitive transfer_binary smoke. Tests the full transfer in one call."""

import os
import random
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TVM = Path(os.environ.get("TRANSFORMER_VM_PATH", "/mnt/c/Users/atchi/Transformer-VM"))
SPEC_DIR = Path("/tmp/psl_full_smoke")
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


def run_python(weights, spec_path):
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
    weights = REPO / "weights" / "transfer_binary.bin"

    pass_ct = 0
    fail_examples = []
    for seed in range(n):
        rng = random.Random(seed)
        success_in = 1
        from_balance = [rng.randint(0, 255) for _ in range(8)] + [0] * 8
        to_balance = [rng.randint(0, 255) for _ in range(16)]
        # smaller amount so balance >= amount usually
        amount = [rng.randint(0, 100) for _ in range(2)] + [0] * 14
        from_nonce = [rng.randint(0, 100) for _ in range(2)] + [0] * 6

        # Native golden
        from_int = int.from_bytes(bytes(from_balance), "little")
        to_int = int.from_bytes(bytes(to_balance), "little")
        amt_int = int.from_bytes(bytes(amount), "little")
        nonce_int = int.from_bytes(bytes(from_nonce), "little")
        success_out = 1 if from_int >= amt_int else 0
        if success_out:
            new_from = list((from_int - amt_int).to_bytes(16, "little"))
            new_to = list((to_int + amt_int).to_bytes(16, "little"))
            new_nonce = list((nonce_int + 1).to_bytes(8, "little"))
        else:
            new_from = [0] * 16
            new_to = [0] * 16
            new_nonce = [0] * 8
        expected = [success_out] + new_from + new_to + new_nonce

        witness = [success_in] + from_balance + to_balance + amount + from_nonce
        spec = SPEC_DIR / f"transfer_{seed}.txt"
        spec.write_text(render_binary_spec(witness))

        got, err = run_python(weights, str(spec))
        if got is None:
            if len(fail_examples) < 5:
                fail_examples.append({"seed": seed, "err": err})
            continue
        if got == expected:
            pass_ct += 1
        else:
            d = next((i for i in range(min(len(got), len(expected))) if got[i] != expected[i]), -1)
            if len(fail_examples) < 5:
                fail_examples.append({
                    "seed": seed, "diff_at": d,
                    "exp_len": len(expected), "got_len": len(got),
                    "exp": expected[d] if 0 <= d < len(expected) else "?",
                    "got": got[d] if 0 <= d < len(got) else "?",
                })

    print(f"\n{pass_ct}/{n} passed")
    for fe in fail_examples:
        print(f"  {fe}")


if __name__ == "__main__":
    main()
