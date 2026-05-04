#!/usr/bin/env python3
"""Try to reverse-engineer MKL dgemv's reduction order for length-2162 dot.

Strategy: enumerate plausible SIMD-lane summation patterns (2/4/8/16 accs,
various block sizes), find one that bit-exactly matches torch.matmul on a
specific test case.
"""

import os
os.environ.setdefault("OMP_NUM_THREADS", "1")
os.environ.setdefault("MKL_NUM_THREADS", "1")
os.environ.setdefault("MKL_DYNAMIC", "FALSE")
import sys
TVM = os.environ.get("TRANSFORMER_VM_PATH", os.path.expanduser("~/Transformer-VM"))
sys.path.insert(0, TVM)

import numpy as np
import torch

torch.set_num_threads(1)


def lane_dot(W: np.ndarray, x: np.ndarray, lanes: int) -> np.ndarray:
    """Lane-parallel reduction: <lanes> independent accumulators, then horiz sum."""
    rows, cols = W.shape
    y = np.zeros(rows, dtype=np.float64)
    for i in range(rows):
        accs = [0.0] * lanes
        # main loop in lanes-wide chunks
        full = (cols // lanes) * lanes
        for j0 in range(0, full, lanes):
            for k in range(lanes):
                accs[k] += float(W[i, j0 + k]) * float(x[j0 + k])
        # tail
        tail_acc = 0.0
        for j in range(full, cols):
            tail_acc += float(W[i, j]) * float(x[j])
        # horizontal sum: pairwise tree
        cur = list(accs)
        while len(cur) > 1:
            cur = [cur[k] + cur[k + 1] for k in range(0, len(cur), 2)]
        y[i] = cur[0] + tail_acc
    return y


def lane_dot_horiz_left(W: np.ndarray, x: np.ndarray, lanes: int) -> np.ndarray:
    """Lane-parallel, then left-to-right horizontal sum (a + b + c + d)."""
    rows, cols = W.shape
    y = np.zeros(rows, dtype=np.float64)
    for i in range(rows):
        accs = [0.0] * lanes
        full = (cols // lanes) * lanes
        for j0 in range(0, full, lanes):
            for k in range(lanes):
                accs[k] += float(W[i, j0 + k]) * float(x[j0 + k])
        tail_acc = 0.0
        for j in range(full, cols):
            tail_acc += float(W[i, j]) * float(x[j])
        s = 0.0
        for k in range(lanes):
            s += accs[k]
        y[i] = s + tail_acc
    return y


def block_dot(W: np.ndarray, x: np.ndarray, block: int, lanes: int) -> np.ndarray:
    """Process in blocks of <block> elements, each block uses <lanes> accumulators."""
    rows, cols = W.shape
    y = np.zeros(rows, dtype=np.float64)
    for i in range(rows):
        block_total = 0.0
        b0 = 0
        while b0 + block <= cols:
            accs = [0.0] * lanes
            full = (block // lanes) * lanes
            for j0 in range(b0, b0 + full, lanes):
                for k in range(lanes):
                    accs[k] += float(W[i, j0 + k]) * float(x[j0 + k])
            block_acc = 0.0
            for k in range(lanes):
                block_acc += accs[k]
            block_total += block_acc
            b0 += block
        # tail
        tail_acc = 0.0
        for j in range(b0, cols):
            tail_acc += float(W[i, j]) * float(x[j])
        y[i] = block_total + tail_acc
    return y


def main() -> None:
    rng = np.random.default_rng(42)
    rows = 66
    cols = 2162
    W = rng.standard_normal((rows, cols)).astype(np.float64)
    x = rng.standard_normal(cols).astype(np.float64)
    y_torch = (torch.from_numpy(W) @ torch.from_numpy(x)).numpy()

    print(f"matmul {rows}x{cols} @ {cols}")
    candidates = []
    for lanes in (2, 4, 8, 16):
        y = lane_dot(W, x, lanes)
        diff = np.max(np.abs(y - y_torch))
        eq = (y == y_torch).all()
        candidates.append((f"lane_dot(L={lanes}, tree-horiz)", diff, eq))
        y = lane_dot_horiz_left(W, x, lanes)
        diff = np.max(np.abs(y - y_torch))
        eq = (y == y_torch).all()
        candidates.append((f"lane_dot(L={lanes}, left-horiz)", diff, eq))
    # blocked variants — common BLAS strategy
    for block in (32, 64, 128, 256, 512):
        for lanes in (4, 8, 16):
            y = block_dot(W, x, block, lanes)
            diff = np.max(np.abs(y - y_torch))
            eq = (y == y_torch).all()
            candidates.append((f"block_dot(B={block}, L={lanes})", diff, eq))

    print("\nstrategy                           : maxabs vs torch    : exact match")
    for name, diff, eq in sorted(candidates, key=lambda c: c[1]):
        print(f"  {name:35s}: {diff:.3e}        : {eq}")


if __name__ == "__main__":
    main()
