"""mpt_emit_record 10k smoke."""
import os
import random
import re
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TVM = Path(os.environ.get("TRANSFORMER_VM_PATH", os.path.expanduser("~/Transformer-VM")))
SPEC_DIR = Path("/tmp/psl_mpt")
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
        return None, r.stderr[-300:]
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


def run_batch(weights, spec_paths, label, max_new=10000, chunk=4000):
    print(f"  [{label}] {len(spec_paths)} specs in chunks of {chunk}", flush=True)
    t0 = time.time()
    by_name = {}
    n_chunks = (len(spec_paths) + chunk - 1) // chunk
    for ci in range(n_chunks):
        ch = spec_paths[ci * chunk : (ci + 1) * chunk]
        out, err = _run_one_chunk(weights, ch, max_new)
        if out is None:
            print(f"  [{label}] chunk {ci} FAILED: {err}", flush=True)
            continue
        by_name.update(out)
    elapsed = time.time() - t0
    print(f"  [{label}] done in {elapsed:.0f}s ({len(spec_paths)/elapsed:.1f}/s)", flush=True)
    return by_name


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 10000
    weights = REPO / "weights" / "mpt_emit_record.bin"
    print(f"\n=== mpt_emit_record @ {n} witnesses ===", flush=True)
    for old in SPEC_DIR.glob("*.txt"):
        old.unlink()
    paths = []
    expected = []
    rng = random.Random(55)
    for i in range(n):
        record = [rng.randint(0, 255) for _ in range(64)]
        p = SPEC_DIR / f"mpt_{i:06d}.txt"
        p.write_text(render_binary_spec(record))
        paths.append(p)
        expected.append(record)
    out = run_batch(weights, paths, "mpt", max_new=10000)
    pass_ct = 0
    fail_examples = []
    for i, p in enumerate(paths):
        got = out.get(p.stem, "")
        want = expected_chars(expected[i])
        if got == want:
            pass_ct += 1
        elif len(fail_examples) < 5:
            fail_examples.append((i, len(want), len(got)))
    print("\n=== mpt_emit_record summary ===", flush=True)
    print(f"  Result: {pass_ct}/{n}", flush=True)
    for ex in fail_examples:
        print(f"  FAIL #{ex[0]}: exp_len={ex[1]} got_len={ex[2]}", flush=True)


if __name__ == "__main__":
    main()
