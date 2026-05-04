# Upstream bug for grapheneaffiliate/Transformer-VM

**File against the Transformer-VM repo as an issue.**

## Title

`compilation/lower.py:1526` lowers runtime `i32.or` as boolean
`select(1, a, b)` — silently breaks bitwise OR of multi-bit values

## Severity

High. Compiles produce wrong specialized models for any C primitive whose
generated WASM contains an `i32.or` not preceded by `i32.const`. The bug is
silent — no compile-time warning. The C code looks correct, the WASM is
correct, but the lowered token program (and therefore the specialized
transformer) computes a different result.

## Repro

In PSL repo (https://github.com/grapheneaffiliate/Transformer-VM-Bank):

```bash
cd /mnt/c/Users/atchi/Transformer_VM_Bank
git checkout 4232f3b   # the commit with the four minimal repros
PSL_KEEP_WASM=1 ./tools/compile.sh primitives/repro_freeze.c \
    --args "0 41 248 193 22 174 195 21 52 205 82 199 99 175 48 214 49 168 150 171 135 218 202 223 192 64 221 175 3 167 35 98 244 1 52 28 183 159 210 167 124 225 140 38 138 108 23 26 0 56 157 242 131 13 163 7 92 253 148 157 102 138 59 209 155"

# Show the WASM has i32.or but the lowered program has zero i32.or:
wasm2wat primitives/repro_freeze.wasm | grep -c i32.or
python3 -c "
tokens = open('data/repro_freeze.txt').read().split()
end = tokens.index('}')
print('lowered i32.or count:', tokens[:end].count('i32.or'))
"
```

The four minimal repros isolate the bug:
- `primitives/repro_writes.c` — sequential writes only, **PASSES**
- `primitives/repro_parse.c` — parse loop only, **PASSES**
- `primitives/repro_parse_flag.c` — flag parse + parse loop, **PASSES**
- `primitives/repro_freeze.c` — adds the bitwise-OR-byte-47 step, **FAILS**

`wasm-eval` (universal exact-arithmetic evaluator) on the failing WASM
produces the **correct** byte-47 output. The C++ engine running the
specialized model produces the wrong byte-47. Same WASM, different runtime.

The failure is consistent with `lower.py:1526` lowering `cur | 128` (where
`cur` is read from memory just-written by a prior store) as
`select(1, cur, 128)` = "if 128 != 0 then 1 else cur" = `1`. The freeze
write at byte 47 stores `1` instead of `128`.

## Root cause

`compilation/lower.py:1526–1539`:

```python
# Runtime OR (no preceding const): a b i32.or → select(1, a, b)
if ins.opcode == OP_I32_OR:
    local_a = temp_base
    new_instrs.extend(
        [
            _instr(OP_LOCAL_SET, local_a),  # save b
            _instr(OP_I32_CONST, 1),  # push 1
            _instr(OP_LOCAL_GET, local_a),  # push b (condition)
            _instr(OP_SELECT),  # if b != 0: 1, else: a
        ]
    )
```

This is BOOLEAN OR (`a || b ? 1 : 0` for non-zero values), not bitwise OR.

It works correctly for **single bits** because `a OR b` of bits equals
`b ? 1 : a`. But for multi-bit operands (e.g., `cur | 128` where 128 has
bit 7 set), the result is `1` when `b ≠ 0`, losing all other bit
information.

The same pattern at `lower.py:1511` for `i32.and` has the analogous
problem (`select(a, 0, b)` is boolean AND, not bitwise).

The constant-form lowering at `lower.py:1255` (`_expand_or(const_val, local_a)`)
is correct (calls `_expand_bitop_general` which does proper byte-wise
bitwise ops via SCRATCH memory). But it only fires when an `i32.const`
immediately precedes the `i32.or`. Any clang transformation that puts
another op between them (commonly `select`, `i32.and`, or branch merging)
falls through to the buggy boolean form.

## Suggested fix

Replace the runtime-form OR/AND/XOR lowerings with proper bitwise
expansions. The primitives to compose:

1. Save both operands to scratch memory (e.g., `local_a` at offset 0,
   `local_b` at offset 4 of a 32-bit-aligned scratch slot).
2. For each of 4 bytes, load `byte_a` and `byte_b`, compute `byte_a OR
   byte_b` bit-by-bit (8 iterations), store result.
3. Reload as i32 and push.

Estimated cost: ~200 instructions per runtime OR call (vs ~4 for the
boolean form). Acceptable: most primitives have few runtime ORs.

Alternatively, raise an error at lowering time if the lowering would lose
correctness (i.e., refuse to silently emit boolean lowering when the
operands cannot be proven 0/1). This catches the bug at compile time
instead of in test results.

## Impact

Any primitive that uses bitwise OR/AND/XOR on multi-bit values where clang
doesn't keep an `i32.const` immediately before the binop will silently
miscompile. PSL's `ledger_freeze.c` is the load-bearing example —
`cur | 128` for setting the freeze flag was producing byte 47 = 1 instead
of 128.

Workaround used in PSL: avoid bitwise OR entirely via volatile-addition
when bit ranges don't overlap. Acceptable for freeze; not always feasible
for primitives that need full bitwise ops.

## Tags

`compiler`, `wasm-lowering`, `correctness`, `silent-failure`
