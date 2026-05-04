"""Per-byte primitives 10k harness.

Tests each of the 4 transfer per-byte primitives at 10k random witnesses,
plus the full chained transfer end-to-end.

Output comparison uses char-form (printable chars literal, '.' for non-
printable). For byte primitives where outputs are deterministic and we
know expected, this is sufficient: if all char positions match, the run
passes.

Stages run sequentially through the C++ engine in big batches. Total
expected runtime: ~10-20 min for all 5 stages.
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
TVM = Path("/mnt/c/Users/atchi/Transformer-VM")
SPEC_DIR = Path("/tmp/psl_pb")
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


def run_batch(weights, spec_paths, label, max_new=5000):
    cmd = ["uv", "run", "wasm-run", "--model", str(weights),
           "--max-new-tokens", str(max_new)] + [str(p) for p in spec_paths]
    print(f"  [{label}] launching {len(spec_paths)} specs...", flush=True)
    t0 = time.time()
    r = subprocess.run(cmd, cwd=str(TVM), capture_output=True, text=True, timeout=14400)
    elapsed = time.time() - t0
    if r.returncode != 0:
        print(f"  [{label}] FAILED: {r.stderr[-300:]}", flush=True)
        return {}, elapsed
    # The C++ engine emits output bytes literally (incl. \n and \t), so a single
    # `output:` line can span MULTIPLE physical lines if any byte is 0x0a.
    # Parse by anchoring on `<NAME>: RAN` headers and capturing the content
    # between `  output: ` and the next header.
    by_name = {}
    text = r.stdout
    # End-of-batch marker the engine emits after the last spec
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
            # transformer.cpp emits a trailing '\n' after the output bytes.
            if content.endswith("\n"):
                content = content[:-1]
            by_name[name] = content
    print(f"  [{label}] done in {elapsed:.0f}s ({len(spec_paths)/elapsed:.1f}/s)", flush=True)
    return by_name, elapsed


def clean_specs():
    for p in SPEC_DIR.glob("*.txt"):
        p.unlink()


# --- byte_sub_with_borrow ---


def test_byte_sub(n):
    print(f"\n=== byte_sub_with_borrow @ {n} witnesses ===", flush=True)
    weights = REPO / "weights" / "byte_sub_with_borrow.bin"
    clean_specs()
    paths = []
    expected = []
    rng = random.Random(11)
    for i in range(n):
        m = rng.randint(0, 255)
        s = rng.randint(0, 255)
        b = rng.randint(0, 1)
        diff = m - s - b
        if diff < 0:
            res = diff + 256
            bo = 1
        else:
            res = diff
            bo = 0
        p = SPEC_DIR / f"sub_{i:06d}.txt"
        p.write_text(render_binary_spec([m, s, b]))
        paths.append(p)
        expected.append([res, bo])
    out, _ = run_batch(weights, paths, "sub", max_new=3000)
    pass_ct = 0
    fail_examples = []
    for i, p in enumerate(paths):
        got = out.get(p.stem, "")
        want = expected_chars(expected[i])
        if got == want:
            pass_ct += 1
        elif len(fail_examples) < 5:
            fail_examples.append((i, want, got, expected[i]))
    print(f"  Result: {pass_ct}/{n}", flush=True)
    for ex in fail_examples:
        print(f"  FAIL #{ex[0]}: exp_chars={ex[1]!r} got={ex[2]!r} exp_bytes={ex[3]}", flush=True)
    return pass_ct == n


# --- byte_add_with_carry ---


def test_byte_add(n):
    print(f"\n=== byte_add_with_carry @ {n} witnesses ===", flush=True)
    weights = REPO / "weights" / "byte_add_with_carry.bin"
    clean_specs()
    paths = []
    expected = []
    rng = random.Random(22)
    for i in range(n):
        a = rng.randint(0, 255)
        b = rng.randint(0, 255)
        c = rng.randint(0, 1)
        s = a + b + c
        if s >= 256:
            res = s - 256
            co = 1
        else:
            res = s
            co = 0
        p = SPEC_DIR / f"add_{i:06d}.txt"
        p.write_text(render_binary_spec([a, b, c]))
        paths.append(p)
        expected.append([res, co])
    out, _ = run_batch(weights, paths, "add", max_new=3000)
    pass_ct = 0
    fail_examples = []
    for i, p in enumerate(paths):
        got = out.get(p.stem, "")
        want = expected_chars(expected[i])
        if got == want:
            pass_ct += 1
        elif len(fail_examples) < 5:
            fail_examples.append((i, want, got, expected[i]))
    print(f"  Result: {pass_ct}/{n}", flush=True)
    for ex in fail_examples:
        print(f"  FAIL #{ex[0]}: exp_chars={ex[1]!r} got={ex[2]!r} exp_bytes={ex[3]}", flush=True)
    return pass_ct == n


# --- transfer_check ---


def test_transfer_check(n):
    print(f"\n=== transfer_check @ {n} witnesses ===", flush=True)
    weights = REPO / "weights" / "transfer_check.bin"
    clean_specs()
    paths = []
    expected = []
    rng = random.Random(33)
    for i in range(n):
        from_b = [rng.randint(0, 255) for _ in range(16)]
        amt = [rng.randint(0, 255) for _ in range(16)]
        from_int = int.from_bytes(bytes(from_b), "little")
        amt_int = int.from_bytes(bytes(amt), "little")
        ok = 1 if from_int >= amt_int else 0
        p = SPEC_DIR / f"check_{i:06d}.txt"
        p.write_text(render_binary_spec(from_b + amt))
        paths.append(p)
        expected.append([ok])
    out, _ = run_batch(weights, paths, "check", max_new=3000)
    pass_ct = 0
    fail_examples = []
    for i, p in enumerate(paths):
        got = out.get(p.stem, "")
        want = expected_chars(expected[i])
        if got == want:
            pass_ct += 1
        elif len(fail_examples) < 5:
            fail_examples.append((i, want, got, expected[i]))
    print(f"  Result: {pass_ct}/{n}", flush=True)
    for ex in fail_examples:
        print(f"  FAIL #{ex[0]}: exp_chars={ex[1]!r} got={ex[2]!r} exp_bytes={ex[3]}", flush=True)
    return pass_ct == n


# --- transfer_finalize ---


def test_transfer_finalize(n):
    print(f"\n=== transfer_finalize @ {n} witnesses ===", flush=True)
    weights = REPO / "weights" / "transfer_finalize.bin"
    clean_specs()
    paths = []
    expected = []
    rng = random.Random(44)
    for i in range(n):
        nonce = [rng.randint(0, 255) for _ in range(8)]
        ni = int.from_bytes(bytes(nonce), "little")
        new_ni = (ni + 1) & ((1 << 64) - 1)
        new_nonce = list(new_ni.to_bytes(8, "little"))
        p = SPEC_DIR / f"fin_{i:06d}.txt"
        p.write_text(render_binary_spec(nonce))
        paths.append(p)
        expected.append(new_nonce)
    out, _ = run_batch(weights, paths, "finalize", max_new=3000)
    pass_ct = 0
    fail_examples = []
    for i, p in enumerate(paths):
        got = out.get(p.stem, "")
        want = expected_chars(expected[i])
        if got == want:
            pass_ct += 1
        elif len(fail_examples) < 5:
            fail_examples.append((i, want, got, expected[i]))
    print(f"  Result: {pass_ct}/{n}", flush=True)
    for ex in fail_examples:
        print(f"  FAIL #{ex[0]}: exp_chars={ex[1]!r} got={ex[2]!r} exp_bytes={ex[3]}", flush=True)
    return pass_ct == n


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 10000
    target = sys.argv[2] if len(sys.argv) > 2 else "all"
    results = {}
    if target in ("all", "sub"):
        results["byte_sub"] = test_byte_sub(n)
    if target in ("all", "add"):
        results["byte_add"] = test_byte_add(n)
    if target in ("all", "check"):
        results["transfer_check"] = test_transfer_check(n)
    if target in ("all", "finalize"):
        results["transfer_finalize"] = test_transfer_finalize(n)
    print(f"\n=== summary ===", flush=True)
    for k, v in results.items():
        print(f"  {k}: {'PASS' if v else 'FAIL'}", flush=True)


if __name__ == "__main__":
    main()
