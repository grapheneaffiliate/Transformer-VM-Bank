#!/usr/bin/env python3
"""Localize the matmul where Python+MKL drifts from sequential summation.

Loads freeze_apply weights and a representative x vector. For each Linear
layer (in_proj, out_proj, ff_in, ff_out, head), compute:
  (a) torch.matmul(W, x)            → uses MKL
  (b) hand-rolled sequential sum    → matches our Rust runner

Reports the largest abs diff per layer. The layer with non-zero diff is
the bit-exactness culprit.
"""

import os
os.environ.setdefault("OMP_NUM_THREADS", "1")
os.environ.setdefault("MKL_NUM_THREADS", "1")
import sys
sys.path.insert(0, "/mnt/c/Users/atchi/Transformer-VM")

import numpy as np
import torch
torch.set_num_threads(1)
from transformer_vm.model.weights import load_weights

REPO = "/mnt/c/Users/atchi/Transformer_VM_Bank"
NAME = "freeze_apply"


def manual_matvec(W: np.ndarray, x: np.ndarray) -> np.ndarray:
    """Sequential f64 sum, identical to Rust's `for j: s += W[i,j]*x[j]`."""
    rows, cols = W.shape
    y = np.zeros(rows, dtype=np.float64)
    for i in range(rows):
        s = 0.0
        for j in range(cols):
            s += float(W[i, j]) * float(x[j])
        y[i] = s
    return y


def main() -> None:
    model, all_tokens, tok_to_idx_map = load_weights(os.path.join(REPO, "weights", f"{NAME}.bin"))
    # Build a representative x: just take embedding[0], add position 23 encoding.
    x = model.tok.weight[0].detach().clone().numpy().astype(np.float64)

    # Layer 2 of freeze_apply has the biggest matrices (FFN width 2162).
    li = 2
    layer_attn = model.attn[li]
    layer_ff_in = model.ff_in[li]
    layer_ff_out = model.ff_out[li]

    # in_proj: [3*d_model, d_model]
    W = layer_attn.in_proj_weight.detach().numpy().astype(np.float64)
    y_torch = (torch.from_numpy(W) @ torch.from_numpy(x)).numpy()
    y_seq = manual_matvec(W, x)
    print(f"in_proj  W{W.shape}: maxabs(torch - seq) = {np.max(np.abs(y_torch - y_seq)):.3e}")

    # ff_in: [2*width=4324, d_model=66]
    W = layer_ff_in.weight.detach().numpy().astype(np.float64)
    y_torch = (torch.from_numpy(W) @ torch.from_numpy(x)).numpy()
    y_seq = manual_matvec(W, x)
    print(f"ff_in    W{W.shape}: maxabs(torch - seq) = {np.max(np.abs(y_torch - y_seq)):.3e}")

    # ff_out: [d_model=66, width=2162]; need a length-2162 input
    width = W.shape[0] // 2
    rng = np.random.default_rng(42)
    act = rng.standard_normal(width).astype(np.float64)
    W = layer_ff_out.weight.detach().numpy().astype(np.float64)
    y_torch = (torch.from_numpy(W) @ torch.from_numpy(act)).numpy()
    y_seq = manual_matvec(W, act)
    print(f"ff_out   W{W.shape}: maxabs(torch - seq) = {np.max(np.abs(y_torch - y_seq)):.3e}")

    # head: [vocab=874, d_model=66]
    W = model.head.weight.detach().numpy().astype(np.float64)
    y_torch = (torch.from_numpy(W) @ torch.from_numpy(x)).numpy()
    y_seq = manual_matvec(W, x)
    print(f"head     W{W.shape}: maxabs(torch - seq) = {np.max(np.abs(y_torch - y_seq)):.3e}")


if __name__ == "__main__":
    main()
