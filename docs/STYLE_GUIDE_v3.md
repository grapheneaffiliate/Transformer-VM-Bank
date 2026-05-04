# PSL Style Guide v3 — Avoiding shift-chain trace explosion

**Status:** authoritative. Supersedes the v2 advice (in `Transformer-VM/transformer_vm/examples/arc_common.h`) for any primitive that operates on multi-byte values.

## TL;DR

In Transformer-VM's lowering, **`i32.shr_u` and `i32.shr_s` get expanded into byte-extraction chains by `lower.py`'s `_expand_shr_u` / `_expand_shr_s` functions**. A single shift in the C source can blow up the trace by 50–100×. clang -O2 frequently uses shifts for arithmetic that has nothing to do with multi-byte values (e.g., extracting the sign bit of a small integer). The result: a primitive that "should" have a 400-token trace produces a 32k-token trace and fails 10k bit-exact at scale.

**Recipe**: write the C source so clang lowers your boolean / sign tests through **additive normalization + select**, never through `i32.shr_u`.

## The bug we tripped on

`primitives/byte_sub_with_borrow.c`, first attempt, looked like the natural translation of "subtract two bytes with a borrow":

```c
void compute(const char *input) {
    int minuend     = (int)(unsigned char)input[0];
    int subtrahend  = (int)(unsigned char)input[1];
    int borrow_in   = (int)(unsigned char)input[2];

    int diff = minuend - subtrahend - borrow_in;
    int borrow_out = 0;
    if (diff < 0) {
        diff = diff + 256;
        borrow_out = 1;
    }

    putchar(diff);
    putchar(borrow_out);
}
```

**WASM**: 169 instructions. **Trace**: ~32,000 tokens. **10k bit-exact**: catastrophic — fails far worse than longer freeze primitives.

clang at -O2 turned `if (diff < 0)` into a branchless sign-bit extraction:

```
local.tee 0           ; save diff
i32.const 23
i32.shr_u             ; diff >> 23  (extract sign bits into low byte)
i32.const 256
i32.and
local.get 0
i32.add               ; diff + (256 if negative)
call $putchar
local.get 0
i32.const 31
i32.shr_u             ; diff >> 31  (extract sign bit as 0/1)
call $putchar
```

`i32.shr_u 23` and `i32.shr_u 31` are the killers. lower.py's `_expand_shr_u` writes the i32 to scratch memory, loads selected byte slices, then assembles. Each call expands to ~50–100 WASM ops. Two of them in the same primitive multiply the dynamic trace.

The neighboring `byte_add_with_carry.c`, by contrast, naturally compiled to **26 WASM instructions** with a **119-token trace**, because clang chose an additive pattern with `select`:

```
i32.add               ; augend + addend
local.get offset 2    ; carry_in
i32.add               ; + carry_in
local.tee 0
i32.const -256
i32.add               ; sum - 256
local.get 0
local.get 0
i32.const 255
i32.gt_u              ; sum > 255 (boolean)
local.tee 1           ; carry_out
select                ; if carry_out: sum-256, else: sum
call $putchar
local.get 1
call $putchar
```

No shifts. Just `i32.sub` / `i32.add` / `i32.gt_u` / `select` — all single-instruction lowerings.

## The recipe

**Mirror byte_add_with_carry's pattern.** Compute the result in a way that's **always non-negative**, then use a `<` or `>` comparison with `select` to pick between the corrected and uncorrected forms:

```c
void compute(const char *input) {
    int minuend     = (int)(unsigned char)input[0];
    int subtrahend  = (int)(unsigned char)input[1];
    int borrow_in   = (int)(unsigned char)input[2];

    /* Add 256 up front so the result is always non-negative. */
    int diff_plus  = minuend + 256 - subtrahend - borrow_in;
    int borrow_out = (diff_plus < 256) ? 1 : 0;
    int result     = borrow_out ? diff_plus : (diff_plus - 256);

    putchar(result);
    putchar(borrow_out);
}
```

**Result**: 142 WASM instructions, **404-token trace**, 10000/10000 pass. The `(diff_plus < 256)` test compiles to `i32.lt_u` + `select` — no shifts.

## Why this works

clang's optimizer chooses between several patterns for "is x negative?":
1. `x < 0` → `i32.lt_s` (cheap, single op) — what we want
2. `(x >> 31) & 1` → `i32.shr_u 31; i32.and 1` (cheap on real hardware, expensive after lower.py) — what -O2 prefers when it can prove the value is small
3. Branch on `if (x < 0)` → conditional jump (decently cheap)

Pattern (2) is the trap. clang picks it when the source code has both `if (x < 0)` (sign test) AND a use of `x` in the not-negative path (so it can't fold the branch away). The "additive normalization" recipe sidesteps it by moving the sign concern to a magnitude comparison: `x + 256 < 256` ⟺ `x < 0`, but `x + 256 < 256` is unsigned (since `x + 256` is always ≥ 0 for `x ∈ [-256, 0)`), and clang lowers it via `i32.lt_u` instead of via shift.

## Forbidden patterns (sequential primitives only)

For primitives with sequential data dependencies (carry chains, hash rounds, anything where output depends on running state):

- **Don't use `>>` on multi-byte intermediate values.** Even shifts by small amounts go through `_expand_shr_u`'s scratch-memory dance.
- **Don't use `<<` on variable amounts.** Variable shift goes through a 32-iteration loop in lower.py.
- **Don't use `if (x < 0)` followed by a path that uses `x` directly.** Triggers the shr_u-31 trap.
- **Don't use `(x & SIGN_MASK)` patterns.** Same trap.
- **Avoid bitwise OR on multi-bit operands** unless the operands are non-overlapping bit ranges and you can use `+` instead. The runtime-form OR lowering in `lower.py:1526` is **boolean** (`a | b → b ? 1 : a`), wrong for any multi-bit OR. (See `docs/UPSTREAM_BUG_lower_py_runtime_or.md`.)

## Required pattern (sequential primitives)

- **Additive normalization + select.** Whenever a value might be negative or out of range:
  1. Add a known offset so it's always in a safe non-negative range.
  2. Compare to the offset's threshold via `i32.lt_u` / `i32.gt_u`.
  3. `select` between the corrected and uncorrected forms.

## Instrumentation: trace length is the budget

Every new primitive: measure trace length on a representative witness **before declaring it complete**.

```bash
echo "<witness bytes>" | python3 tools/witness_to_spec.py > /tmp/m.txt
cd $TRANSFORMER_VM_PATH
uv run wasm-run --model $PSL/weights/<primitive>.bin --max-new-tokens 5000 /tmp/m.txt 2>&1 | grep RAN
```

The `RAN N tok` field is the metric. Targets:

| Op type | Target trace tokens |
| --- | --- |
| Sequential (carry chain, hash round) | < 1,000 |
| Independent (parse stream, byte-emit) | < 30,000 |
| Anything ≥ 100,000 | **decompose now** — will fail at 10k scale |

A primitive that compiles to 169 WASM instructions might trace at 32k tokens (byte_sub before the rewrite) or at 400 tokens (byte_sub after). The static instruction count is a *proxy*. The dynamic trace is the truth.

## Per-byte decomposition is cheap; sequential trace is not

When in doubt for a primitive with carry chains or other sequential dependencies: **decompose into per-byte primitives chained at the sequencer level**. The architectural cost is N trace hashes per logical operation. The sequencer threads outputs through invocations.

PSL's transfer is decomposed this way — 1 check + 16 byte_sub + 16 byte_add + 1 finalize = 34 trace hashes per transfer. Each individual primitive has a sub-2k token trace. The full chain passes 10000/10000.

| Primitive | WASM instrs | Trace tokens | 10k pass |
| --- | --- | --- | --- |
| byte_sub_with_borrow (v1, sign-bit pattern) | 169 | 32,346 | failed at scale |
| byte_sub_with_borrow (v2, additive recipe) | 142 | 404 | 10000/10000 ✓ |
| byte_add_with_carry | 26 | 119 | 10000/10000 ✓ |
| transfer_check (16-iter MSB-first compare) | 86 | 1,624 | 10000/10000 ✓ |
| transfer_finalize (u64 nonce inc) | 142 | 656 | 10000/10000 ✓ |
| mpt_emit_record (64-byte pass-through) | 20 | 3,741 | 10000/10000 ✓ |

## When this rule doesn't apply

Independent operations (parse a 60-byte stream, emit a 64-byte stream, lookups) can fit larger traces. `freeze_setup` parses 65 numbers and outputs 2; its 17,566-token trace passes 10000/10000. The constraint is sequential data dependencies, not raw count of bytes.

## Upstream context

The shift-chain expansion in `lower.py:_expand_shr_u` is correct behavior — it's the only way to simulate a shift in Transformer-VM's reduced ISA. The bug is that clang's optimizer doesn't know about Transformer-VM's lowering costs and routinely picks the shift-based pattern. A future Transformer-VM upgrade could either (a) raise the d_ffn ceiling so longer traces don't drift, or (b) add a Transformer-VM-aware clang plugin that avoids shifts in hot paths. Until then, this style guide is the workaround.
