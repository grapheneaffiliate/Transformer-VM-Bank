#!/usr/bin/env python3
"""Generate rust_runner/tests/golden/<primitive>.expected from the
Transformer-VM Python runner (StandardKVCache, no hull cache).

Usage:
    /mnt/c/Users/atchi/Transformer-VM/.venv/bin/python tools/gen_golden.py freeze_setup freeze_apply

Writes one line of space-joined predicted tokens per primitive.
"""

import os
import sys
import time

sys.path.insert(0, "/mnt/c/Users/atchi/Transformer-VM")

import torch
from transformer_vm.attention import StandardKVCache
from transformer_vm.model.weights import load_weights

REPO = "/mnt/c/Users/atchi/Transformer_VM_Bank"


def gen_one(name: str) -> None:
    weights_path = os.path.join(REPO, "weights", f"{name}.bin")
    spec_path = os.path.join(REPO, "data", f"{name}_spec.txt")
    out_path = os.path.join(REPO, "rust_runner", "tests", "golden", f"{name}.expected")

    if not os.path.exists(weights_path):
        sys.exit(f"missing {weights_path}")
    if not os.path.exists(spec_path):
        sys.exit(f"missing {spec_path}")

    print(f"[{name}] loading weights {weights_path}")
    model, all_tokens, tok_to_idx_map = load_weights(weights_path)

    with open(spec_path) as f:
        tokens = f.read().split()
    idx_seq = [tok_to_idx_map[t] for t in tokens]
    print(f"[{name}] input prompt: {len(idx_seq)} tokens")

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
        sys.exit("usage: gen_golden.py <primitive> [<primitive> ...]")
    for name in sys.argv[1:]:
        gen_one(name)


if __name__ == "__main__":
    main()
