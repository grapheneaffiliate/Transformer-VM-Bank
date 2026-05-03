# Gate-1 Findings (2026-05-03)

Honest post-mortem of the first run of the bit-exact gate. The architecture
holds; specific clang/Transformer-VM lowering bugs prevent the v1 primitive
designs from passing as written.

## Summary

- **Gate 1 status**: ledger_freeze passes 1/3 representative witnesses
  (all-zero account, flag=1). Fails on non-zero accounts (parse loop
  interactions with the freeze write trigger silent miscompilation). Other
  primitives not yet exercised — likely hit similar issues.
- **Architecture verdict**: still sound. The transformer can re-execute WASM
  bit-exactly (universal evaluator and specialized C++ engine agreed
  byte-for-byte on the all-zero trial). The bug is in compilation, not the
  trust model.
- **Implication**: v1 primitive designs need decomposition. The
  parse-then-modify-then-print pattern in a single C source file hits
  clang/Transformer-VM edge cases. Splitting into smaller primitives that
  each do one thing should sidestep these.

## Reproducible bugs

### Bug 1: `cur | 128` elided after read-from-memory

Pattern that fails:
```c
int cur = (int)(unsigned char)g_account[47];   // cur = 0 (fresh from parse loop)
if (flag) cur = cur | 128;                      // expected: cur = 128
g_account[47] = (char)cur;                      // expected: byte 47 = 128
```
Observed: byte 47 = 0 in the universal trace. Greppable for `i32.or` in
`data/ledger_freeze.txt` returns zero hits — clang elided the OR entirely.

Workaround that works:
```c
if (flag) {
    g_account[47] = (char)128;
} else {
    g_account[47] = (char)0;
}
```

This loses the ability to preserve the low 7 bits of byte 47 (acceptable
for v1 because gen_vectors.py caps balances at 2^120, so byte 47 of the
balance field is always 0).

### Bug 2: Parse loop corrupts non-zero account positions

With flag=1 and account = `[(i*7) % 100 for i in range(64)]`, the printed
output is structurally wrong: positions 1, 3, 6, 16, 17, 18, etc. read as 0
or 32 (= 0x20 = ASCII space) instead of the parsed value. Position 47
itself reads as 0 instead of the freeze-flag value 128 — the entire
freeze write fails when the surrounding account has non-zero bytes.

Hypothesis: clang's optimizer interleaves the parse-write with the freeze
write under -O2, producing a WASM stream whose write-through-load sequence
gets mistranslated by Transformer-VM's `lower_hard_ops` pass. Not yet
isolated to a minimal repro; further investigation requires
`wasm2wat` disassembly of the .wasm before it's deleted by the build.

### Bug 3: Universal Python evaluator hard-coded max_steps=50000

For traces longer than ~50k tokens (which the v1 freeze primitive exceeds —
~70k tokens needed for a complete run), `transformer_vm/evaluator.py:317`
hard-codes `max_steps = 50000` and silently truncates. The failure mode is
a partial output that *looks* correct on the prefix but is missing the
later positions where bugs would manifest. Easy to fix by passing
max_steps through the CLI.

## Path forward

1. **Decompose freeze.c**: separate primitives for parse, modify, print.
   The sequencer chains them. Each individual primitive is small enough to
   compile cleanly under Transformer-VM's lowering.

2. **Pre-validate decomposition viability** before re-running gate 1:
   compile and run a 200-instruction "set-byte-47-to-flag" primitive that
   takes a pre-parsed account in a fixed memory layout. If that passes
   bit-exact for randomized inputs, the decomposition is the path.

3. **Investigate Transformer-VM lowering bugs upstream**: file repros of
   bugs 1 and 2 against `transformer_vm/wasm/interpreter.py:lower_hard_ops`
   (the most likely culprit). Even with decomposition shipping v1, these
   bugs will keep biting future primitives unless fixed.

4. **Increase universal evaluator max_steps**: trivial CLI flag fix in
   `evaluator.py`. Required so future bit-exact harnesses can compare
   complete traces, not truncated ones.

## What survives

- The v2 style guide (under 2000 instr, no malloc, no floats, no mul_var,
  safety counters) is necessary but not sufficient; need additional rules
  about parse/modify/print interaction.
- The `clang-wsl-wrapper.sh` path-translation fix is generally useful and
  unaffected.
- The trace-hash contract (BLAKE3 of token sequence) is still valid.
- The Sparse Merkle Tree, sequencer skeleton, light client, Lean models,
  pilot binary — all still valid; they don't depend on any specific
  primitive shape.

## Build commands that worked

```bash
# Sync (one-time, with network)
cd /mnt/c/Users/atchi/Transformer-VM && uv sync

# Compile + specialize freeze (works for the all-zero baseline)
cd /mnt/c/Users/atchi/Transformer_VM_Bank
./tools/compile.sh primitives/ledger_freeze.c --args "1 $(python3 -c 'print(" ".join(["0"]*64))')"
./tools/specialize.sh data/ledger_freeze.txt

# Verify against the universal evaluator (extends max_steps to 200k)
cd /mnt/c/Users/atchi/Transformer-VM && uv run python -c "
import sys; sys.path.insert(0, '.')
from transformer_vm.evaluator import Runtime
with open('/mnt/c/Users/atchi/Transformer_VM_Bank/data/ledger_freeze.txt') as f:
    tokens = f.read().split()
end = tokens.index('}')
rt = Runtime(use_hull=True)
for t in tokens: vals = rt.step(t)
outs = []
for _ in range(200000):
    nxt = rt.predict_next(vals)
    if nxt.startswith('out('): outs.append(nxt)
    if nxt == 'halt': break
    vals = rt.step(nxt)
print(' '.join(outs))
"

# Verify against the specialized C++ engine
cd /mnt/c/Users/atchi/Transformer-VM && uv run wasm-run \\
    --model /mnt/c/Users/atchi/Transformer_VM_Bank/weights/ledger_freeze.bin \\
    --max-new-tokens 100000 \\
    /mnt/c/Users/atchi/Transformer_VM_Bank/data/ledger_freeze_spec.txt
```
