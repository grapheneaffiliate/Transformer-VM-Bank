"""Smoke test for the decomposed transfer pipeline:
witness → transfer_parse → transfer_compute → (success, new_balances, new_nonce).
"""

import json
import os
import random
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TVM = Path("/mnt/c/Users/atchi/Transformer-VM")
SPEC_DIR = Path("/tmp/psl_smoke_specs")
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


def make_witness(seed, force_success=True):
    rng = random.Random(seed)
    # epoch
    epoch = rng.randint(1, 1000)
    # from account: 64 bytes; balance large enough if force_success
    from_acc = [rng.randint(0, 255) for _ in range(64)]
    from_acc[47] &= 0x7F  # not frozen
    # set from balance bytes [32..48) to a large value
    if force_success:
        for i in range(8):
            from_acc[32 + i] = rng.randint(0, 255)
        for i in range(8, 16):
            from_acc[32 + i] = 0  # high half clear so balance fits in u64
    # to account
    to_acc = [rng.randint(0, 255) for _ in range(64)]
    # amount: small so transfer succeeds
    amount = [rng.randint(0, 100), rng.randint(0, 100)] + [0] * 14
    return [epoch] + from_acc + to_acc + amount


def native_golden(witness):
    epoch = witness[0]
    from_acc = list(witness[1:65])
    to_acc = list(witness[65:129])
    amount_b = list(witness[129:145])
    frozen = (from_acc[47] & 0x80) != 0
    from_balance = int.from_bytes(bytes(from_acc[32:48]), "little")
    to_balance = int.from_bytes(bytes(to_acc[32:48]), "little")
    amount = int.from_bytes(bytes(amount_b), "little")
    success = (not frozen) and (from_balance >= amount)
    if success:
        new_from_b = from_balance - amount
        new_to_b = to_balance + amount
        nonce = int.from_bytes(bytes(from_acc[48:56]), "little") + 1
    else:
        new_from_b = 0
        new_to_b = 0
        nonce = 0
    out = [1 if success else 0]
    out += list(new_from_b.to_bytes(16, "little"))
    out += list(new_to_b.to_bytes(16, "little"))
    out += list(nonce.to_bytes(8, "little"))
    return out


def main():
    parse_w = REPO / "weights" / "transfer_parse.bin"
    compute_w = REPO / "weights" / "transfer_compute.bin"

    seeds = [1, 2, 3, 42, 100]
    pass_ct = 0
    fail_ct = 0
    for seed in seeds:
        witness = make_witness(seed)
        # Step 1
        spec = SPEC_DIR / f"setup_{seed}.txt"
        spec.write_text(render_spec(witness))
        slice_out, err = run_one(parse_w, str(spec))
        if slice_out is None:
            print(f"seed {seed}: parse FAILED: {err}")
            fail_ct += 1
            continue
        if len(slice_out) != 61:
            print(f"seed {seed}: parse output len {len(slice_out)} != 61: {slice_out}")
            fail_ct += 1
            continue
        # Step 2
        spec2 = SPEC_DIR / f"compute_{seed}.txt"
        spec2.write_text(render_spec(slice_out))
        result, err = run_one(compute_w, str(spec2))
        if result is None:
            print(f"seed {seed}: compute FAILED: {err}")
            fail_ct += 1
            continue
        expected = native_golden(witness)
        if result == expected:
            print(f"seed {seed}: PASS (success={result[0]})")
            pass_ct += 1
        else:
            fail_ct += 1
            print(f"seed {seed}: MISMATCH")
            print(f"  expected: {expected}")
            print(f"  got:      {result}")
            for i in range(min(len(expected), len(result))):
                if expected[i] != result[i]:
                    print(f"  first diff at {i}: exp={expected[i]} got={result[i]}")
                    break

    print(f"\n{pass_ct}/{len(seeds)} passed")


if __name__ == "__main__":
    main()
