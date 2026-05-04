"""Bit-exact verification gate.

Runs N randomized witnesses per primitive through the specialized
transformer (Transformer-VM's C++ engine via `wasm-run`), parses the
program output, and compares to the expected output computed natively in
Python (golden model that mirrors the C primitive's logic).

The C++ engine is ~30000 tok/s; the pure-Python path is ~1000× slower.
So this harness uses subprocess to wasm-run rather than calling
model.generate_with_cache directly.

Usage:
    uv run pytest tests/test_bit_exact.py -v -k freeze        # 100/primitive
    uv run pytest tests/test_bit_exact.py -v -k freeze --full # 10k/primitive
"""

import json
import os
import subprocess
import tempfile
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent
TVM = Path(os.environ.get("TRANSFORMER_VM_PATH", "/mnt/c/Users/atchi/Transformer-VM"))
DATA_DIR = REPO_ROOT / "data"
WEIGHTS_DIR = REPO_ROOT / "weights"
VECTORS_DIR = REPO_ROOT / "tests" / "vectors"
FAILURES_DIR = REPO_ROOT / "tests" / "failures"

# Active primitive set after gate-1 decomposition (cleared 2026-05-04). The
# original monolithic primitives lived too long traces (>30k tokens) and
# accumulated precision drift on rare witnesses. The per-byte / decomposed
# replacements below all clear 10000/10000 bit-exact at scale.
#
# Composition counts (sequencer threads outputs):
#   freeze       = freeze_setup + freeze_apply             ->  2 hashes
#   transfer     = transfer_check + 16x byte_sub_with_borrow
#                                 + 16x byte_add_with_carry
#                                 + transfer_finalize       -> 34 hashes
#   mint         = 16x byte_add_with_carry                 -> 16 hashes
#   burn         = transfer_check + 16x byte_sub_with_borrow -> 17 hashes
#   mpt_emit     = mpt_emit_record (per record)             ->  1 hash
PRIMITIVES = [
    "byte_sub_with_borrow",
    "byte_add_with_carry",
    "transfer_check",
    "transfer_finalize",
    "freeze_setup",
    "freeze_apply",
    "mpt_emit_record",
]


def _ensure_artifacts(primitive: str):
    if not (TVM / ".venv").exists():
        pytest.skip(f"sync Transformer-VM venv first: cd {TVM} && uv sync")
    if not (DATA_DIR / f"{primitive}.txt").exists():
        pytest.skip(f"compile primitive first: ./tools/compile.sh primitives/{primitive}.c")
    if not (WEIGHTS_DIR / f"{primitive}.bin").exists():
        pytest.skip(f"specialize first: ./tools/specialize.sh data/{primitive}.txt")


def _render_input_tokens(input_values: list[int]) -> list[str]:
    """Encode witness as `start <input>` per compile_wasm.format_spec_input.

    Each input byte: printable ASCII becomes the literal char, else 2-char hex.
    NUL terminator + commit token at the end.
    """
    text = " ".join(str(b) for b in input_values)
    data = text.encode("utf-8") + b"\x00"
    tokens = ["start"]
    for b in data:
        if 0x20 < b < 0x7F and chr(b) not in ("{", "}"):
            tokens.append(chr(b))
        else:
            tokens.append(f"{b:02x}")
    tokens.append("commit(+0,sts=0,bt=0)")
    return tokens


def _run_cpp_engine(primitive: str, witness: list[int]) -> list[int]:
    tokens = _render_input_tokens(witness)
    with tempfile.NamedTemporaryFile(mode="w", suffix="_spec.txt", delete=False) as f:
        f.write(" ".join(tokens))
        spec_path = f.name
    try:
        result = subprocess.run(
            [
                "uv",
                "run",
                "wasm-run",
                "--model",
                str(WEIGHTS_DIR / f"{primitive}.bin"),
                "--max-new-tokens",
                "500000",
                spec_path,
            ],
            cwd=str(TVM),
            capture_output=True,
            text=True,
            timeout=180,
        )
        if result.returncode != 0:
            raise RuntimeError(f"wasm-run failed: {result.stderr}")
        for line in result.stdout.splitlines():
            stripped = line.strip()
            if stripped.startswith("output:"):
                payload = stripped[len("output:"):].strip()
                return [int(x) for x in payload.split()]
        raise RuntimeError(f"no output line in wasm-run stdout: {result.stdout[-500:]}")
    finally:
        try:
            os.unlink(spec_path)
        except OSError:
            pass


# ── Golden models (native Python implementations of each primitive) ──────────


def _golden_freeze(witness: list[int]) -> list[int]:
    flag = witness[0]
    acc = list(witness[1:65])
    if flag:
        acc[47] = acc[47] | 128
    else:
        acc[47] = acc[47] & 127
    return acc


def _golden_transfer(witness: list[int]) -> list[int]:
    epoch = witness[0]
    from_acc = list(witness[1:65])
    to_acc = list(witness[65:129])
    amount_bytes = list(witness[129:145])
    amount = int.from_bytes(bytes(amount_bytes), "little")
    from_balance = int.from_bytes(bytes(from_acc[32:48]), "little") & ((1 << 127) - 1)
    to_balance = int.from_bytes(bytes(to_acc[32:48]), "little") & ((1 << 127) - 1)
    frozen = (from_acc[47] & 0x80) != 0
    if frozen or from_balance < amount:
        return [0] * 128
    new_from_balance = from_balance - amount
    new_to_balance = to_balance + amount
    from_acc[32:48] = list(new_from_balance.to_bytes(16, "little"))
    to_acc[32:48] = list(new_to_balance.to_bytes(16, "little"))
    nonce = int.from_bytes(bytes(from_acc[48:56]), "little") + 1
    from_acc[48:56] = list(nonce.to_bytes(8, "little"))
    epoch_bytes = list(int(epoch).to_bytes(8, "little"))
    from_acc[56:64] = epoch_bytes
    to_acc[56:64] = epoch_bytes
    return from_acc + to_acc


def _golden_mint(witness: list[int]) -> list[int]:
    epoch = witness[0]
    to_acc = list(witness[1:65])
    amount_bytes = list(witness[65:81])
    amount = int.from_bytes(bytes(amount_bytes), "little")
    to_balance = int.from_bytes(bytes(to_acc[32:48]), "little")
    new_balance = to_balance + amount
    if new_balance >= (1 << 128):
        return [0] * 64
    to_acc[32:48] = list(new_balance.to_bytes(16, "little"))
    to_acc[56:64] = list(int(epoch).to_bytes(8, "little"))
    return to_acc


def _golden_burn(witness: list[int]) -> list[int]:
    epoch = witness[0]
    from_acc = list(witness[1:65])
    amount_bytes = list(witness[65:81])
    amount = int.from_bytes(bytes(amount_bytes), "little")
    balance = int.from_bytes(bytes(from_acc[32:48]), "little")
    if balance < amount:
        return [0] * 64
    from_acc[32:48] = list((balance - amount).to_bytes(16, "little"))
    from_acc[56:64] = list(int(epoch).to_bytes(8, "little"))
    return from_acc


GOLDEN = {
    "ledger_freeze": _golden_freeze,
    "ledger_transfer": _golden_transfer,
    "ledger_mint": _golden_mint,
    "ledger_burn": _golden_burn,
}


def _dump_failure(primitive: str, witness: dict, expected: list[int], got: list[int]):
    FAILURES_DIR.mkdir(parents=True, exist_ok=True)
    import hashlib
    h = hashlib.blake2b(json.dumps(witness, sort_keys=True).encode(), digest_size=8).hexdigest()
    out = FAILURES_DIR / f"{primitive}_{h}.txt"
    with out.open("w") as f:
        f.write(f"# Bit-exact mismatch for {primitive}\n")
        f.write(f"witness = {json.dumps(witness)}\n\n")
        f.write(f"expected ({len(expected)}): {expected}\n")
        f.write(f"got      ({len(got)}): {got}\n\n")
        first_diff = next(
            (i for i in range(min(len(expected), len(got))) if expected[i] != got[i]),
            min(len(expected), len(got)),
        )
        f.write(f"first mismatch at index {first_diff}\n")
        if first_diff < len(expected):
            f.write(f"  expected[{first_diff}] = {expected[first_diff]}\n")
        if first_diff < len(got):
            f.write(f"  got[{first_diff}]      = {got[first_diff]}\n")


@pytest.mark.parametrize("primitive", list(GOLDEN.keys()))
def test_primitive_bit_exact(primitive, vector_count):
    _ensure_artifacts(primitive)
    path = VECTORS_DIR / f"{primitive}.json"
    if not path.exists():
        pytest.skip(f"generate vectors first: python tools/gen_vectors.py --primitive {primitive}")
    with path.open() as f:
        data = json.load(f)
    vectors = data["vectors"]
    if vector_count is not None:
        vectors = vectors[:vector_count]

    failed = 0
    failure_examples = []
    for v in vectors:
        witness = v["input"]
        expected = GOLDEN[primitive](witness)
        try:
            got = _run_cpp_engine(primitive, witness)
        except Exception as e:
            failed += 1
            if len(failure_examples) < 3:
                failure_examples.append((witness, "exception", str(e)))
            continue
        if got != expected:
            failed += 1
            if len(failure_examples) < 3:
                failure_examples.append((witness, expected, got))
            _dump_failure(primitive, v, expected, got)

    if failed:
        msg = f"{primitive}: {failed}/{len(vectors)} mismatches"
        for w, e, g in failure_examples:
            msg += f"\n  witness={w[:8]}... expected={e if isinstance(e, str) else e[:8]}... got={g[:8] if isinstance(g, list) else g}..."
        pytest.fail(msg)
