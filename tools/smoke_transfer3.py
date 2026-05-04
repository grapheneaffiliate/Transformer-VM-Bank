"""3-stage transfer smoke test.

Pipeline (sequencer-side parsing, transformer-trace verifies arithmetic):
  1. Sequencer extracts (frozen, from_balance, amount) → transfer_check → success
  2. Sequencer extracts (success, from_balance, to_balance, amount) → transfer_balance
       → (new_from_balance, new_to_balance)
  3. Sequencer extracts (success, from_nonce) → transfer_nonce → new_from_nonce
"""

import json
import os
import random
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TVM = Path("/mnt/c/Users/atchi/Transformer-VM")
SPEC_DIR = Path("/tmp/psl_smoke_specs3")
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


def run_one(weights, spec_path, max_new=200000):
    r = subprocess.run(
        ["uv", "run", "wasm-run", "--model", str(weights),
         "--max-new-tokens", str(max_new), spec_path],
        cwd=str(TVM), capture_output=True, text=True, timeout=180,
    )
    if r.returncode != 0:
        return None, r.stderr[-200:]
    for line in r.stdout.splitlines():
        s = line.strip()
        if s.startswith("output:"):
            return [int(x) for x in s[len("output:"):].split()], ""
    return None, "no output"


def make_witness(seed):
    rng = random.Random(seed)
    epoch = rng.randint(1, 1000)
    from_acc = [rng.randint(0, 255) for _ in range(64)]
    from_acc[47] &= 0x7F  # not frozen
    for i in range(8):
        from_acc[32 + i] = rng.randint(0, 255)
    for i in range(8, 16):
        from_acc[32 + i] = 0
    to_acc = [rng.randint(0, 255) for _ in range(64)]
    amount = [rng.randint(0, 100), rng.randint(0, 100)] + [0] * 14
    return epoch, from_acc, to_acc, amount


def main():
    check_w = REPO / "weights" / "transfer_check.bin"
    balance_w = REPO / "weights" / "transfer_balance.bin"
    nonce_w = REPO / "weights" / "transfer_nonce.bin"

    seeds = [1, 2, 3, 42, 100]
    pass_ct = 0
    for seed in seeds:
        epoch, from_acc, to_acc, amount = make_witness(seed)
        frozen = (from_acc[47] >> 7) & 1
        from_balance = from_acc[32:48]
        to_balance = to_acc[32:48]
        from_nonce = from_acc[48:56]

        # Stage 1: check
        spec1 = SPEC_DIR / f"check_{seed}.txt"
        spec1.write_text(render_spec([frozen] + from_balance + amount))
        out1, err1 = run_one(check_w, str(spec1))
        if out1 is None or len(out1) != 1:
            print(f"seed {seed}: check FAILED: {err1}")
            continue
        success = out1[0]

        # Stage 2: balance
        spec2 = SPEC_DIR / f"balance_{seed}.txt"
        spec2.write_text(render_spec([success] + from_balance + to_balance + amount))
        out2, err2 = run_one(balance_w, str(spec2))
        if out2 is None or len(out2) != 32:
            print(f"seed {seed}: balance FAILED: {err2}")
            continue
        new_from_balance = out2[:16]
        new_to_balance = out2[16:32]

        # Stage 3: nonce
        spec3 = SPEC_DIR / f"nonce_{seed}.txt"
        spec3.write_text(render_spec([success] + from_nonce))
        out3, err3 = run_one(nonce_w, str(spec3))
        if out3 is None or len(out3) != 8:
            print(f"seed {seed}: nonce FAILED: {err3}")
            continue
        new_nonce = out3

        # Native golden
        from_b_int = int.from_bytes(bytes(from_balance), "little")
        to_b_int = int.from_bytes(bytes(to_balance), "little")
        amt_int = int.from_bytes(bytes(amount), "little")
        nonce_int = int.from_bytes(bytes(from_nonce), "little")
        exp_success = 1 if (frozen == 0 and from_b_int >= amt_int) else 0
        if exp_success:
            exp_from_b = list((from_b_int - amt_int).to_bytes(16, "little"))
            exp_to_b = list((to_b_int + amt_int).to_bytes(16, "little"))
            exp_nonce = list((nonce_int + 1).to_bytes(8, "little"))
        else:
            exp_from_b = [0] * 16
            exp_to_b = [0] * 16
            exp_nonce = [0] * 8

        if (success == exp_success and new_from_balance == exp_from_b
            and new_to_balance == exp_to_b and new_nonce == exp_nonce):
            print(f"seed {seed}: PASS (success={success})")
            pass_ct += 1
        else:
            print(f"seed {seed}: MISMATCH")
            if success != exp_success:
                print(f"  success: exp={exp_success} got={success}")
            if new_from_balance != exp_from_b:
                d = next((i for i in range(16) if exp_from_b[i] != new_from_balance[i]), -1)
                print(f"  from_balance diff at {d}: exp={exp_from_b} got={new_from_balance}")
            if new_to_balance != exp_to_b:
                d = next((i for i in range(16) if exp_to_b[i] != new_to_balance[i]), -1)
                print(f"  to_balance diff at {d}: exp={exp_to_b} got={new_to_balance}")
            if new_nonce != exp_nonce:
                d = next((i for i in range(8) if exp_nonce[i] != new_nonce[i]), -1)
                print(f"  nonce diff at {d}: exp={exp_nonce} got={new_nonce}")

    print(f"\n{pass_ct}/{len(seeds)} passed")


if __name__ == "__main__":
    main()
