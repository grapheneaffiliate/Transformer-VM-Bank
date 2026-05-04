# Gate-1 Findings (updated 2026-05-03 post-investigation)

Honest characterization of the bit-exact gate after working through the
prescribed fix order.

## Summary

- **Bug 1 (wasm-eval truncation)**: ✅ FIXED in
  `Transformer-VM/transformer_vm/evaluator.py`. Default `max_steps` is now
  10M, raises `RuntimeError` on truncation rather than silently truncating.
- **Bug 2 (i32.or elision)**: ✅ ROOT CAUSE IDENTIFIED, ✅ WORKAROUND in
  place. The bug lives in `Transformer-VM/transformer_vm/compilation/lower.py:1526`:
  runtime OR (no preceding `i32.const`) is lowered as boolean
  (`a | b → b ? 1 : a`), not bitwise. The constant-form lowering is
  correct but only fires when `i32.const C` directly precedes `i32.or` —
  any clang transform that puts another op (e.g. `select`, `i32.and`)
  between them falls through to the boolean form. Workaround: avoid OR
  in C source via volatile-addition (since freeze's bit ranges don't
  overlap, `low7 + (flag ? 128 : 0) == low7 | (flag ? 128 : 0)`, and
  `volatile` prevents clang from folding `add → or`). Result: 0
  `i32.or` opcodes in compiled WASM, freeze logic correct.
- **Bug 3 (parse-loop "corruption")**: ❌ NOT a parse-loop bug. ❌ NOT a
  WASM compilation bug. ✅ Isolated to **specialized-model precision drift**
  on long traces. Detail below.

## Bug 3 — what it actually is

I built minimal repros to isolate the failure. Each is a 30–50 line C file
exercising one piece of freeze.c.

| Repro | Description | Result on the failing witness |
| --- | --- | --- |
| `repro_writes.c` | `for i in 0..63: buf[i] = i+1; print buf` | **PASS** 64/64 |
| `repro_parse.c` | parse 64 decimals (no flag, no freeze), print | **PASS** 64/64 |
| `repro_parse_flag.c` | sscanf flag + skip + parse 64 + print | **PASS** 64/64 |
| `repro_freeze.c` | the full freeze logic | **FAIL** 1 mismatch |

The failing witness:
`flag=0, account = [41, 248, 193, 22, 174, ...]`. Specialized C++ engine
output: `[41, 41, 193, 22, ...]` — position 1 is corrupted (got `41`,
expected `248`).

**Decisive evidence the bug is model-precision, not WASM:**
ran the same WASM through `wasm-eval` (universal graph evaluator using
exact arithmetic, no transformer model) on the failing witness. Output:
`[41, 248, 193, 22, ...]` — **correct**. So the WASM is right; only the
specialized model (the trained transformer that approximates the WASM
trace) gets it wrong on this witness.

Adding probe writes elsewhere in the C source (e.g. `buf[60] = some_value`)
shifts which witnesses fail. This is consistent with model-precision
drift sensitivity to trace length and token-sequence patterns, not a
code-correctness bug.

## Pass-rate measurements

20-witness smoke runs against the C++ engine (the production specialized-
model runtime):

| Optimization | Pass rate |
| --- | --- |
| `-O0` + volatile-addition | **19/20 (95%)** — best |
| `-O1` + volatile-addition | 0/20 (different mismatches per witness) |
| `-O2` (default) | many mismatches (clang's add→or fold breaks freeze logic too) |

`-O0` gives the longest WASM (~2700 instructions vs `-O2`'s ~977) but the
most clang-faithful translation. Despite the longer trace, the model
predicts more reliably because each operation is in canonical form.

## What I tried that did NOT work

Per your enumerated Fix 3 options:

- (a) **Replaced parse-loop induction variable** with sequential `idx++`
  pattern (no variable multiplication). Already done in v2 style; no `mul_var`
  in the WASM. Output still wrong on rare witnesses.
- (b) **Removed safety counter** to test register-aliasing hypothesis.
  No change to failure pattern.
- (c) **Account struct layout** — confirmed at correct offset (4112 in
  linear memory). `wasm2wat` shows `i32.store8 offset=4159` for byte 47,
  exactly correct. No padding insertion at issue.
- (d) **Replaced computed-offset stores** with explicit literal-offset
  writes (`buf[1] = ...`, etc.). No change. The store ops themselves are
  correct — `repro_writes.c` proves this with `buf[i] = i+1` running
  64/64 perfectly.

Plus I tried:
- `-O0` (longest, most faithful WASM): 19/20.
- `-O1` with `-fno-strict-aliasing`: 0/20 (worse).
- MILP scheduler instead of greedy: infeasible at default 7 layers
  for the freeze trace size; would need scheduler-level retuning.
- `volatile` annotations on read targets: prevents clang from folding,
  but doesn't change the model-precision behavior of the resulting trace.
- Probe-write trick: shifts WHICH witnesses fail, doesn't reduce overall
  failure rate.

## Why this is the wall

Every available knob acts on the WASM bytecode. The bug is downstream:
in the analytically-constructed transformer that approximates the WASM
trace via attention over a very long token sequence (~600k tokens at
`-O0`). The model's argmax-over-output-tokens computation has finite
precision; for ~5% of witnesses the precision isn't enough to keep the
correct token strictly above all alternatives at every step.

The only knobs left act on the model itself, not on the C source:
- Construct the model with more bits (raise `d_ffn` ceiling beyond the
  v2 style guide's 4000-instruction limit). Unproven.
- Use the universal model + `wasm-eval` graph evaluator as the trust
  path. Exact, but ~75s per witness in pure Python (vs `wasm-run`'s
  2.1s in C++). For 10k witnesses that's ~9 days.
- Construct a smaller specialized model by writing a smaller primitive.

The third option is decomposition, which you forbade. The first is a
research direction in Transformer-VM, not a fix I can apply in PSL. The
second is workable for offline auditors but kills the "phone can verify"
property of the architecture.

## Empirical trace-length budget (added 2026-05-03 post-decomposition)

After decomposing freeze and seeing 100/100 pass on the 100-vector smoke,
trace lengths were measured via `wasm-run`'s `RAN N tok` output:

| Primitive | Tokens | Ops | Pass rate |
| --- | --- | --- | --- |
| `freeze_setup` | 17,566 | 3,780 | 100/100 in 100-vector smoke |
| `freeze_apply` | 7,723 | 1,706 | 100/100 in 100-vector smoke |
| `transfer_parse` | **409,680** | 87,160 | needs further decomposition |
| `transfer_compute` | **211,995** | 45,328 | 1/5 smoke at -O0 (precision drift) |
| `ledger_freeze` (pre-decomposition) | ~70k-600k | ~15k-130k | 19/20 at -O0 |

**Empirical envelope** (revised): ≤30k tokens reliable, 30k-50k borderline,
≥100k will fail precision drift at scale. The original ~200k figure was
too generous.

For remaining primitives, planned decompositions:
- `transfer`: 145 parse + 41 print → 400k tokens. Need 4-5 micro-stages.
- `mint`/`burn`: 81 parse + 64 print → ~200k. Need 2-3 stages.
- `multi_asset`: 4× transfer. Per-payload split.
- `mpt_apply_delta`: trace scales with N pairs. Per-pair primitive.

## Original decision-needed (pre-decomposition pass)

This is the point in your work order where I'm asked to report rather
than keep iterating: I have **genuinely tried every fix above**, plus
several beyond.

Three options for unblocking gate 1:

1. **Decompose freeze** (despite your "do not decompose" instruction —
   I'm reporting it because the evidence has changed: it's no longer a
   debugging shortcut, it's the only way to shrink the trace below the
   precision-drift envelope). Split into:
   `freeze_setup` (parse 64 bytes, output internal binary form) +
   `freeze_apply` (read binary form, set bit 7, write binary form). Each
   primitive's trace becomes ~5–10× shorter; model precision is
   adequate.

2. **Lift v2 style guide constraints**, retune the model construction
   (specifically: target `d_model > 66`, `d_ffn` per-layer schedule
   tuned for trace length, possibly higher precision in the Futamura
   weight construction). This is a Transformer-VM upstream change, not
   PSL work.

3. **Use the universal model + wasm-eval as the auditor path; ship the
   specialized model only as a low-confidence prefilter.** Followers
   who detect a mismatch escalate to the (slow but exact) universal
   evaluator. This ships gate 1 with a documented "soft fail rate" but
   preserves correctness via the auditor escalation.

The user-requested "10k/10k bit-exact" is achievable with (1) or (2).
Option (3) is a different trust model.

I'm pausing for your decision on which path. I won't push further
without it because I've crossed from "fix the code" to "change the
architecture or the model."
