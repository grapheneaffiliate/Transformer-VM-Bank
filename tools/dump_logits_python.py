#!/usr/bin/env python3
"""Per-step logits dumper for freeze_apply, Python (PyTorch MKL) reference.

Writes:
  /tmp/freeze_apply_py.logits.bin    — raw f64 array, one [vocab] row per
                                        generation step (no header).
  /tmp/freeze_apply_py.argmax.txt    — newline-separated argmax tokens, one
                                        per step.

Usage:
  PYBIN="$TRANSFORMER_VM_PATH/.venv/bin/python"
  OMP_NUM_THREADS=1 MKL_NUM_THREADS=1 $PYBIN tools/dump_logits_python.py
"""

import os
import struct
import sys
import time

TVM = os.environ.get("TRANSFORMER_VM_PATH", os.path.expanduser("~/Transformer-VM"))
sys.path.insert(0, TVM)

import torch
import torch.nn.functional as F

from transformer_vm.attention import StandardKVCache
from transformer_vm.model.weights import load_weights
from transformer_vm.model.transformer import add_position_encoding

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NAME = sys.argv[1] if len(sys.argv) > 1 else "freeze_apply"
MAX_GEN = int(sys.argv[2]) if len(sys.argv) > 2 else 200  # cap to keep file small for diff


def main() -> None:
    weights_path = os.path.join(REPO, "weights", f"{NAME}.bin")
    spec_path = os.path.join(REPO, "data", f"{NAME}_spec.txt")

    print(f"[{NAME}] threads={torch.get_num_threads()}")
    model, all_tokens, tok_to_idx_map = load_weights(weights_path)
    with open(spec_path) as f:
        tokens = f.read().split()
    idx_seq = [tok_to_idx_map[t] for t in tokens]
    print(f"[{NAME}] prompt {len(idx_seq)} tok, generating up to {MAX_GEN}")

    out_logits = open(f"/tmp/{NAME}_py.logits.bin", "wb")
    out_argmax = open(f"/tmp/{NAME}_py.argmax.txt", "w")

    n_layers = len(model.attn)
    n_heads = model.attn[0].num_heads
    cache = StandardKVCache(n_layers, n_heads)

    idx_list = list(idx_seq)
    t0 = time.time()
    with torch.no_grad():
        for pos in range(len(idx_list) + MAX_GEN):
            if pos >= len(idx_list):
                break
            x = model.tok.weight[idx_list[pos]].clone()
            add_position_encoding(x, pos)
            for layer_idx, (attn, ff_in, ff_out) in enumerate(
                zip(model.attn, model.ff_in, model.ff_out, strict=True)
            ):
                q, k, v = (attn.in_proj_weight @ x).chunk(3, dim=-1)
                out = cache.layer_step(layer_idx, k, q, v)
                x = x + attn.out_proj(out)
                gate, val = ff_in(x).chunk(2, dim=-1)
                x = x + ff_out(F.relu(gate) * val)

            if pos + 1 == len(idx_list):
                logits = model.head(x)  # [vocab] f64
                arr = logits.detach().cpu().numpy().astype("float64")
                out_logits.write(arr.tobytes())
                next_id = int(logits.argmax().item())
                out_argmax.write(f"{next_id}\t{all_tokens[next_id]}\n")
                idx_list.append(next_id)
                if next_id == model.stop_token_id:
                    print(f"[stop] at pos {pos}")
                    break
    out_logits.close()
    out_argmax.close()
    print(f"[{NAME}] done in {time.time() - t0:.1f}s, generated {len(idx_list) - len(idx_seq)} tokens")


if __name__ == "__main__":
    main()
