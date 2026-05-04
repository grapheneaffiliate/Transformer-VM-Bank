"""Smoke test: run ledger_freeze through both paths on its compile-time input.

This is a single-witness check, much faster than the 100-vector test_bit_exact.
Replicates the contract of Transformer-VM's `test_specialize::test_specialize`
but for our primitive.

Pass: predicted token sequence from the specialized model equals the
predicted sequence from the universal graph evaluator (modulo `start` /
`{ ... }` prefix).
"""

import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
import os
TVM = Path(os.environ.get("TRANSFORMER_VM_PATH", os.path.expanduser("~/Transformer-VM")))

sys.path.insert(0, str(TVM))


def main():
    import torch
    from transformer_vm.attention.standard_cache import StandardKVCache
    from transformer_vm.evaluator import run_program
    from transformer_vm.model.weights import load_weights

    program = REPO / "data" / "ledger_freeze.txt"
    spec = REPO / "data" / "ledger_freeze_spec.txt"
    weights = REPO / "weights" / "ledger_freeze.bin"

    for p in (program, spec, weights):
        if not p.exists():
            print(f"missing: {p}", file=sys.stderr)
            return 2

    print(f"[smoke] loading weights from {weights}")
    model, all_tokens, tok_to_idx = load_weights(str(weights))
    print(f"[smoke] vocab={len(all_tokens)}")

    print(f"[smoke] running universal evaluator on {program}")
    t0 = time.time()
    # Capture predicted tokens by patching run_program to return them — quick hack
    import transformer_vm.evaluator as ev
    original = ev.run_program

    captured = {"tokens": None}

    def wrapper(program_file, ref_file=None, use_hull=False, verbose=False):
        with open(program_file) as f:
            tokens = f.read().split()
        rt = ev.Runtime(use_hull=use_hull)
        end_idx = tokens.index("}")
        # Feed program prefix
        for i in range(end_idx + 1):
            vals = rt.step(tokens[i])
        # Force-feed input bytes
        input_end = end_idx
        if end_idx + 1 < len(tokens):
            for i in range(end_idx + 1, len(tokens)):
                vals = rt.step(tokens[i])
                input_end = i
        predicted = list(tokens[: input_end + 1])
        for _ in range(50000):
            nxt = rt.predict_next(vals)
            predicted.append(nxt)
            if nxt == "halt":
                break
            vals = rt.step(nxt)
        rt.destroy()
        captured["tokens"] = predicted
        return predicted == [] if ref_file else True

    ev.run_program = wrapper
    try:
        wrapper(str(program), use_hull=True)
    finally:
        ev.run_program = original

    universal_tokens = captured["tokens"]
    t_uni = time.time() - t0
    print(f"[smoke] universal: {len(universal_tokens)} tokens in {t_uni:.1f}s")
    print(f"[smoke] universal last 10: {universal_tokens[-10:]}")

    print(f"[smoke] running specialized model on {spec}")
    t0 = time.time()
    with open(spec) as f:
        seq = f.read().split()
    idx_seq = [tok_to_idx[t] for t in seq]
    result = model.generate_with_cache(
        torch.tensor([idx_seq], dtype=torch.long),
        max_new_tokens=50000,
        cache_class=StandardKVCache,
    )
    specialized_tokens = [all_tokens[i] for i in result[0].tolist()]
    t_spec = time.time() - t0
    print(f"[smoke] specialized: {len(specialized_tokens)} tokens in {t_spec:.1f}s")
    print(f"[smoke] specialized last 10: {specialized_tokens[-10:]}")

    # Strip prefixes
    end_uni = universal_tokens.index("}")
    u_exec = universal_tokens[end_uni + 1 :]
    s_exec = specialized_tokens[1:] if specialized_tokens and specialized_tokens[0] == "start" else specialized_tokens

    if u_exec == s_exec:
        print(f"[smoke] PASS — bit-exact match ({len(u_exec)} tokens)")
        return 0
    else:
        n_match = 0
        for u, s in zip(u_exec, s_exec):
            if u == s:
                n_match += 1
            else:
                break
        print(f"[smoke] FAIL — first mismatch at index {n_match}")
        print(f"  universal  [{n_match-2}:{n_match+5}]: {u_exec[max(0,n_match-2):n_match+5]}")
        print(f"  specialized[{n_match-2}:{n_match+5}]: {s_exec[max(0,n_match-2):n_match+5]}")
        print(f"  universal len={len(u_exec)}  specialized len={len(s_exec)}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
