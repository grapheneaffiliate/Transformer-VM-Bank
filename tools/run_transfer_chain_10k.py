"""End-to-end chained transfer 10k test.

For each witness:
  1. transfer_check on (from_balance[16], amount[16]) → ok
  2. byte_sub_with_borrow × 16 (LSB→MSB, threading borrow) → new_from_balance
  3. byte_add_with_carry × 16 (LSB→MSB, threading carry)   → new_to_balance
  4. transfer_finalize on (from_nonce[8])                   → new_from_nonce

Each per-byte step is independent given correct chained inputs (the
sequencer would thread these). We render N×16 byte_sub specs + N×16
byte_add specs + N transfer_check + N transfer_finalize, run each as a
single big batch, then verify each step's output matches its golden.
"""

import json
import os
import random
import re
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TVM = Path(os.environ.get("TRANSFORMER_VM_PATH", os.path.expanduser("~/Transformer-VM")))
SPEC_DIR = Path("/tmp/psl_chain")
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
    out = []
    for b in expected_bytes:
        if (0x20 <= b < 0x7F) or b == ord('\n') or b == ord('\t'):
            out.append(chr(b))
        else:
            out.append('.')
    return "".join(out)


def _run_one_chunk(weights, chunk_paths, max_new):
    cmd = ["uv", "run", "wasm-run", "--model", str(weights),
           "--max-new-tokens", str(max_new)] + [str(p) for p in chunk_paths]
    r = subprocess.run(cmd, cwd=str(TVM), capture_output=True, text=True, timeout=21600)
    if r.returncode != 0:
        return None, r.stderr
    by_name = {}
    text = r.stdout
    summary_marker = "\n\n0 passed,"
    if summary_marker in text:
        text = text[: text.index(summary_marker)]
    name_re = re.compile(r"^(\S+):\s+(?:RAN|PASS|FAIL)\b.*$", re.MULTILINE)
    matches = list(name_re.finditer(text))
    for i, m in enumerate(matches):
        name = m.group(1)
        block_start = m.end()
        block_end = matches[i + 1].start() if i + 1 < len(matches) else len(text)
        block = text[block_start:block_end]
        out_re = re.compile(r"\n  output:\s?(.*)", re.DOTALL)
        om = out_re.search(block)
        if om:
            content = om.group(1)
            if content.endswith("\n"):
                content = content[:-1]
            by_name[name] = content
    return by_name, ""


def run_batch(weights, spec_paths, label, max_new=5000, chunk=4000):
    """Chunk to avoid kernel ARG_MAX limit (~2MB on Linux). Each chunk is one
    wasm-run process; model weights are loaded per chunk (~3s amortized)."""
    print(f"  [{label}] launching {len(spec_paths)} specs in chunks of {chunk}...", flush=True)
    t0 = time.time()
    by_name = {}
    n_chunks = (len(spec_paths) + chunk - 1) // chunk
    for ci in range(n_chunks):
        chunk_paths = spec_paths[ci * chunk : (ci + 1) * chunk]
        out, err = _run_one_chunk(weights, chunk_paths, max_new)
        if out is None:
            print(f"  [{label}] chunk {ci} FAILED: {err[-300:]}", flush=True)
            continue
        by_name.update(out)
        if (ci + 1) % 5 == 0 or ci + 1 == n_chunks:
            elapsed = time.time() - t0
            done = (ci + 1) * chunk
            rate = done / elapsed if elapsed > 0 else 0
            print(f"  [{label}] chunk {ci+1}/{n_chunks} ({rate:.1f}/s overall)", flush=True)
    elapsed = time.time() - t0
    print(f"  [{label}] done in {elapsed:.0f}s ({len(spec_paths)/elapsed:.1f}/s)", flush=True)
    return by_name, elapsed


def make_witness(seed):
    rng = random.Random(seed)
    from_balance = [rng.randint(0, 255) for _ in range(8)] + [0] * 8
    amount = [rng.randint(0, 100) for _ in range(2)] + [0] * 14
    to_balance = [rng.randint(0, 255) for _ in range(16)]
    from_nonce = [rng.randint(0, 100) for _ in range(2)] + [0] * 6
    return from_balance, amount, to_balance, from_nonce


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 10000
    check_w = REPO / "weights" / "transfer_check.bin"
    sub_w = REPO / "weights" / "byte_sub_with_borrow.bin"
    add_w = REPO / "weights" / "byte_add_with_carry.bin"
    finalize_w = REPO / "weights" / "transfer_finalize.bin"

    print(f"\n[1/8] generating {n} witnesses + native chain values...", flush=True)
    for old in SPEC_DIR.glob("*.txt"):
        old.unlink()
    witnesses = []
    expected_check = []
    expected_subs = []   # list of N lists of 16 expected (r, b_out)
    expected_adds = []   # list of N lists of 16 expected (r, c_out)
    expected_finalize = []
    sub_inputs = []      # for spec rendering: list of (m, s, b_in) per byte
    add_inputs = []
    for seed in range(n):
        from_b, amt, to_b, fn = make_witness(seed)
        witnesses.append((from_b, amt, to_b, fn))
        # check
        from_int = int.from_bytes(bytes(from_b), "little")
        amt_int = int.from_bytes(bytes(amt), "little")
        ok = 1 if from_int >= amt_int else 0
        expected_check.append([ok])
        # subs
        sub_steps = []
        sub_specs = []
        b_in = 0
        for k in range(16):
            m_byte = from_b[k]
            s_byte = amt[k]
            diff = m_byte - s_byte - b_in
            if diff < 0:
                r = diff + 256
                b_out = 1
            else:
                r = diff
                b_out = 0
            sub_steps.append((r, b_out))
            sub_specs.append((m_byte, s_byte, b_in))
            b_in = b_out
        expected_subs.append(sub_steps)
        sub_inputs.append(sub_specs)
        # adds
        add_steps = []
        add_specs = []
        c_in = 0
        for k in range(16):
            a_byte = to_b[k]
            b_byte = amt[k]
            s = a_byte + b_byte + c_in
            if s >= 256:
                r = s - 256
                c_out = 1
            else:
                r = s
                c_out = 0
            add_steps.append((r, c_out))
            add_specs.append((a_byte, b_byte, c_in))
            c_in = c_out
        expected_adds.append(add_steps)
        add_inputs.append(add_specs)
        # finalize
        nonce_int = int.from_bytes(bytes(fn), "little")
        new_nonce = list(((nonce_int + 1) & ((1 << 64) - 1)).to_bytes(8, "little"))
        expected_finalize.append(new_nonce)

    # ---- Stage 1: check ----
    print(f"\n[2/8] rendering check specs...", flush=True)
    check_paths = []
    for i, (from_b, amt, to_b, fn) in enumerate(witnesses):
        p = SPEC_DIR / f"check_{i:06d}.txt"
        p.write_text(render_binary_spec(from_b + amt))
        check_paths.append(p)
    print(f"[3/8] running check batch...", flush=True)
    check_out, _ = run_batch(check_w, check_paths, "check", max_new=3000)

    # ---- Stage 2: sub × 16 ----
    print(f"\n[4/8] rendering {n*16} byte_sub specs...", flush=True)
    sub_paths = []
    for i in range(n):
        for k in range(16):
            m, s, b_in = sub_inputs[i][k]
            p = SPEC_DIR / f"sub_{i:06d}_{k:02d}.txt"
            p.write_text(render_binary_spec([m, s, b_in]))
            sub_paths.append(p)
    print(f"[5/8] running sub batch...", flush=True)
    sub_out, _ = run_batch(sub_w, sub_paths, "sub", max_new=600)

    # ---- Stage 3: add × 16 ----
    print(f"\n[6/8] rendering {n*16} byte_add specs...", flush=True)
    add_paths = []
    for i in range(n):
        for k in range(16):
            a, b, c_in = add_inputs[i][k]
            p = SPEC_DIR / f"add_{i:06d}_{k:02d}.txt"
            p.write_text(render_binary_spec([a, b, c_in]))
            add_paths.append(p)
    print(f"[7/8] running add batch...", flush=True)
    add_out, _ = run_batch(add_w, add_paths, "add", max_new=300)

    # ---- Stage 4: finalize ----
    print(f"\n  rendering finalize specs...", flush=True)
    fin_paths = []
    for i, (from_b, amt, to_b, fn) in enumerate(witnesses):
        p = SPEC_DIR / f"fin_{i:06d}.txt"
        p.write_text(render_binary_spec(fn))
        fin_paths.append(p)
    print(f"  running finalize batch...", flush=True)
    fin_out, _ = run_batch(finalize_w, fin_paths, "finalize", max_new=1000)

    # ---- Compare ----
    print(f"\n[8/8] comparing all stages...", flush=True)
    pass_ct = 0
    fail_ct = 0
    fail_examples = []
    for i in range(n):
        # check
        check_ok = check_out.get(f"check_{i:06d}", "") == expected_chars(expected_check[i])
        # subs
        sub_ok = True
        for k in range(16):
            want = expected_chars(list(expected_subs[i][k]))
            got = sub_out.get(f"sub_{i:06d}_{k:02d}", "")
            if got != want:
                sub_ok = False
                break
        # adds
        add_ok = True
        for k in range(16):
            want = expected_chars(list(expected_adds[i][k]))
            got = add_out.get(f"add_{i:06d}_{k:02d}", "")
            if got != want:
                add_ok = False
                break
        # finalize
        fin_ok = fin_out.get(f"fin_{i:06d}", "") == expected_chars(expected_finalize[i])

        if check_ok and sub_ok and add_ok and fin_ok:
            pass_ct += 1
        else:
            fail_ct += 1
            if len(fail_examples) < 5:
                fail_examples.append({
                    "i": i, "check": check_ok, "sub": sub_ok, "add": add_ok, "fin": fin_ok,
                })

    print(f"\n=== chained transfer summary ===", flush=True)
    print(f"  Result: {pass_ct}/{n} chained transfers passed", flush=True)
    for fe in fail_examples:
        print(f"  {fe}", flush=True)


if __name__ == "__main__":
    main()
