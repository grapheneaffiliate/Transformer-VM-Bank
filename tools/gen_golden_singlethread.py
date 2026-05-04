#!/usr/bin/env python3
"""Same as gen_golden.py but pins MKL/OMP to a single thread and CBWR=compatible.
Hypothesis: multi-threaded MKL parallel-reduction order is the source of fp drift
that breaks bit-exact parity vs the Rust runner on long primitives."""

import os
# Must be set before any numpy/torch import.
os.environ.setdefault("OMP_NUM_THREADS", "1")
os.environ.setdefault("MKL_NUM_THREADS", "1")
os.environ.setdefault("MKL_CBWR", "COMPATIBLE")
os.environ.setdefault("MKL_DYNAMIC", "FALSE")

import sys
import time

TVM = os.environ.get("TRANSFORMER_VM_PATH", os.path.expanduser("~/Transformer-VM"))
sys.path.insert(0, TVM)

import torch
torch.set_num_threads(1)
torch.set_num_interop_threads(1)

from transformer_vm.attention import StandardKVCache
from transformer_vm.model.weights import load_weights

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def gen_one(name: str, suffix: str = ".singlethread") -> None:
    weights_path = os.path.join(REPO, "weights", f"{name}.bin")
    spec_path = os.path.join(REPO, "data", f"{name}_spec.txt")
    out_path = os.path.join(REPO, "rust_runner", "tests", "golden", f"{name}.expected{suffix}")

    print(f"[{name}] loading weights {weights_path}")
    model, all_tokens, tok_to_idx_map = load_weights(weights_path)

    with open(spec_path) as f:
        tokens = f.read().split()
    idx_seq = [tok_to_idx_map[t] for t in tokens]
    print(f"[{name}] input prompt: {len(idx_seq)} tokens, threads={torch.get_num_threads()}")

    t0 = time.time()
    result = model.generate_with_cache(
        torch.tensor([idx_seq], dtype=torch.long),
        max_new_tokens=50_000,
        cache_class=StandardKVCache,
    )
    dt = time.time() - t0
    predicted = [all_tokens[i] for i in result[0].tolist()]
    print(f"[{name}] generated {len(predicted)} tokens in {dt:.1f}s ({len(predicted)/dt:.0f} tok/s)")

    line = " ".join(predicted)
    with open(out_path, "w") as f:
        f.write(line + "\n")
    print(f"[{name}] wrote {out_path}")


def main() -> None:
    if len(sys.argv) < 2:
        sys.exit("usage: gen_golden_singlethread.py <primitive> [<primitive> ...]")
    for name in sys.argv[1:]:
        gen_one(name)


if __name__ == "__main__":
    main()
