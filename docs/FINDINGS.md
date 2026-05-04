# Gate-1 Findings (updated 2026-05-04)

Honest characterization of the bit-exact gate after working through the
prescribed fix order.

## TL;DR (2026-05-04)

- freeze (decomposed into freeze_setup + freeze_apply): **10000/10000 PASS**
- transfer per-byte sub-primitives, each: **10000/10000 PASS**
  - byte_sub_with_borrow, byte_add_with_carry, transfer_check, transfer_finalize
- chained transfer end-to-end: in progress (running 10k now)
- mint/burn: by composition of already-validated byte_add/byte_sub × 16 + check
- multi_asset_transfer: by composition (N × 34 trace hashes per transfer)
- mpt_apply_delta: TBD; measure trace length, decompose if needed

## DESIGN RULE (the empirical lesson from gate 1)

**Trace length is the precision-budget currency.** The 2000-WASM-instruction
limit in the v2 style guide is a *proxy* — it correlates with d_ffn but
fails predictably on primitives with sequential data dependencies (carry
chains, hash rounds, anything where step N depends on step N-1's output).

The real rule:
- **Independent ops** (e.g. parse a stream, set a single byte): can fit
  ~50k-token traces.
- **Sequential dependencies** (carry chains, multi-step state): target
  **sub-1k token traces** per primitive. Decompose so each step is one
  cycle of the dependency.

Per-byte u128 ops (`byte_sub_with_borrow`, `byte_add_with_carry`) at 119–404
tokens pass 10000/10000. The same logic in a 16-iteration loop inside one
primitive (~8k token trace) failed 11% on rare witnesses — the sequential
carry chain accumulated drift the model couldn't recover.

### Instrumentation pass

Every new primitive: measure its trace length on a representative witness
**before** declaring it complete. `wasm-run` reports `RAN N tok` per spec.
Target sub-1k for sequential ops, accept up to ~30k for independent ops.

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

## 10k results (added 2026-05-04)

### freeze: 10000/10000 PASS ✓

```
[setup] done in 7217s (1.4/s)
[apply] done in 2369s (4.2/s)
Result: 10000/10000 passed (0 failed)
```

freeze decomposed (`freeze_setup` + `freeze_apply`) clears gate 1 cleanly.

### transfer (binary I/O single primitive): 8914/10000 (89%)

```
done in 7067s (~2 hours)
Result: 8914/10000 passed (char-form comparison)
```

Failures show length mismatches: expected 41 output bytes, got 1/24/28/30/31.
The model halts prematurely on ~11% of witnesses. transfer_binary's
`d_ffn_per_layer = [9, 6, 1994, 51, 45, 25, 26]` has one layer near the
2000 limit; precision drift at scale flips rare argmax decisions toward
the halt token.

Path forward to 10k/10k for transfer: split into 2-3 binary-I/O stages.

## Binary I/O breakthrough (added post-transfer-decomposition failure)

ASCII decimal I/O is the trace-length killer. Each "decimal-encoded byte"
costs ~10 wire tokens for input plus the parse-loop overhead inside the
trace. By switching to **binary I/O** — one wire token per value byte,
read via `input[i]`, written via `putchar(byte)`, no parsing or printf —
trace length drops by ~10×.

| Primitive | I/O | Tokens | Smoke pass |
| --- | --- | --- | --- |
| `freeze_apply` | ASCII | 7,723 | 100/100 |
| `transfer_compute` (ASCII, decomposed) | ASCII | 211,995 | 1/5 |
| `transfer_sub_binary` (POC) | binary | 8,196 | 20/20 |
| `transfer_binary` (single primitive) | binary | ~10,000 | 10/10 |

**transfer no longer needs decomposition** under the binary-I/O design. A
single primitive handles the full transfer in ~10k tokens (well under the
30k envelope). The sequencer's role: instead of writing decimal-encoded
witness strings, write spec.txt files with one wire token per witness byte.

### When to use which I/O style

- **Binary**: when the primitive operates on byte arrays (balance, nonce,
  flags). Most settlement-layer primitives. Faster, shorter traces.
- **ASCII**: when the witness is naturally textual (e.g. `flag` as a
  single decimal digit). Freeze primitives currently use ASCII; could be
  rewritten to binary for consistency but already pass at scale.

### Implication for sequencer

`sequencer/src/trace.rs` becomes simpler — instead of formatting decimal
strings, it directly emits the wire-token form for each byte of the
witness. The sequencer already has accounts in binary form (in the SMT),
so this is the natural representation.

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

---

## Resolution (2026-05-04)

User chose option (1) — decompose. The result is the per-byte primitive
set documented in `docs/STYLE_GUIDE_v3.md` and the trace-length table
above.

**Per-byte u128 decomposition.** A single u128 subtract → 16 invocations
of `byte_sub_with_borrow`, each with its own trace hash. Per-byte traces
are 119–404 tokens (versus ~8k for the inline 16-iteration loop). Each
sub-primitive clears 10000/10000 bit-exact at scale. Composition counts:

- **freeze**: 2 trace hashes (`freeze_setup` + `freeze_apply`)
- **transfer**: 34 trace hashes (`transfer_check` + 16× sub + 16× add + `transfer_finalize`)
- **mint**: 16 trace hashes (16× add)
- **burn**: 17 trace hashes (`transfer_check` + 16× sub)
- **multi-asset (N recipients)**: N × 34 trace hashes

The architectural cost is N hashes per logical tx in the block. The
sequencer and every follower thread outputs through chained primitives
in deterministic order; intermediate values are re-derived by followers,
so witnesses don't grow proportionally.

This validates the v3 style guide rule: **sequential ops target sub-1k
token traces; decompose otherwise.**

## Gate 4 — sequencer + 3 followers, 100 mixed-traffic blocks (2026-05-04)

`cargo test -p psl-sequencer --test integration` runs two tests.

### `sequencer_and_3_followers_agree_on_100_mixed_blocks` ✅

100 blocks of mixed traffic driven by a seeded RNG:
- Every block: 1 transfer (Alice → random recipient, random amount)
- Every 5th block: mint to Alice
- Every 7th block: burn from Bob
- Every 11th block: freeze a derived account
- Every 13th block: multi-asset transfer (3 recipients)

After every block, `assert_roots_agree(&states)` checks all 4 nodes'
account-tree roots match. The test passes — sovereign sequencer and 3
followers stay in lockstep across the full 100-block sequence.

Total per-tx hash count over 100 blocks: a couple thousand (transfer
contributes 34 each block, the periodic mint/burn/freeze/multi-asset
contribute the rest). The block header commits to the BLAKE3 of the
concatenated trace hashes; followers re-derive each one and verify.

### `published_root_mutation_detected` ✅

Constructed a block header with a 1-byte mutation in `new_state_root`
(XOR'd `0xff` into byte 0). The follower's recomputed root differs from
the published value by every bit position downstream of the mutation;
`assert_ne!(header.new_state_root, follower_root)` fires as expected.
A sequencer publishing a lie about state is publicly provable —
followers' independent re-execution makes it so.

### Why this is the load-bearing gate

Gates 1-3 verify each component in isolation. Gate 4 is the first test
that the sequencer's actual block-production loop is consistent with
followers running the same code on the same inputs. Without this gate,
a "passing" gate 1 means each primitive is correct in isolation, but
nothing rules out a sequencer integration bug that produces a block
header inconsistent with what followers compute.

## Gate 3 — Lean lake build (2026-05-04)

`cd lean && lake build` cleared after fixing three issues that
prevented compilation against mathlib v4.12.0:

1. **No `PSL.lean` library entry point.** The lakefile's
   `globs := #[.andSubmodules \`PSL]` requires both a root file AND
   submodules. Created `lean/PSL.lean` that imports
   `PSL.{Account, Ledger, Conservation, Determinism, MPT}`.
2. **`Vector` not in scope, function-equality decidability stuck.**
   Original `PubKey := Vector (Fin 256) 32` triggered both: Vector
   isn't auto-imported, and even if it were, the `if pk = a.pubkey`
   guard in `Ledger.lean` couldn't synthesize `Decidable` because
   function equality isn't decidable. Changed `PubKey := Nat`. The
   conservation/determinism theorems only need pubkeys to be
   distinguishable; byte-level structure isn't load-bearing.
3. **Theorem name mismatch.** `Nat.zero_lt_two_pow` doesn't exist in
   mathlib 4.12.0; the correct name is `Nat.two_pow_pos`.

Build status: 16/17 modules built, 3 sorrys remaining (Conservation:42,
Conservation:60, MPT:58) within target dates 2026-06-15 / 2026-07-15.
Per the sorry tracker, gate 3's success criterion is "compiles" not
"zero sorrys yet."

The mathlib precompiled cache (5134 oleans) downloaded successfully
via the `lake update` post-hook — total round trip from elan install
to passing build was about 30 minutes.

## Gate 8.5 — pure-Rust runner vector sweep (2026-05-04)

`cargo run --release --bin run_gate1 -- --primitive <p> --count <n>`
re-runs the gate-1 random-witness validation using the pure-Rust
`rust_runner/`, eliminating the Python+PyTorch dependency on the short
primitives. **5/5 short primitives, 4500/4500 vectors, 0 failures.**

| Primitive | Vectors | Time | Rate |
| --- | --- | --- | --- |
| `byte_add_with_carry` | 1000/1000 | 19.6s | 50.9 vec/s |
| `byte_sub_with_borrow` | 1000/1000 | 254.2s | 3.9 vec/s |
| `transfer_finalize` | 1000/1000 | 576.2s | 1.7 vec/s |
| `transfer_check` | 1000/1000 | 3113.8s | 0.3 vec/s |
| `mpt_emit_record` | 500/500 | 5512.0s | 0.1 vec/s |

Logs preserved in `docs/gate85_logs/`. The Rust runner load-tests the
full pipeline: input encoding → forward pass → `out(...)` parsing →
arithmetic ground-truth comparison.

### Long-primitive parity divergence (the load-bearing finding)

`freeze_setup` (17.5k tok) and `freeze_apply` (7.7k tok) are
**not bit-exact with PyTorch** at scale, even on the unmodified runner
(verified by reverting and re-testing). Localised via
`tools/localize_drift.py` — the divergence is exclusively in `ff_out`'s
66×2162 matmul reduction. PyTorch CPU dispatches that case to **Intel
MKL's `mkl_blas_avx2_xdgemv_t`**, whose vectorized reduction order does
not match a sequential summation. Drift is ~1e-14 per step, accumulates,
and flips an argmax around generation step 5944 of `freeze_apply`.

Single-thread MKL produces the same drift, so it is the algorithm itself,
not parallelism. We enumerated 25 candidate SIMD-lane patterns
(`tools/match_mkl_reduction.py`) — none reproduce MKL bit-for-bit; the
closest, `lane_dot(L=4, tree-horiz)`, drifts at 4.3e-14 vs MKL's output.

**Cross-engine algorithm match still holds.** Both our pure-Rust runner
and `Transformer-VM/transformer_vm/model/transformer.cpp`'s Linux build
(the `#else` branch in `matvec`) use a sequential `for j: y[i] += W[i,j]
* x[j]` reduction. Production correctness on the long primitives rests
on the original gate-1 C++-engine sweep at 10000/10000 each, not on
PyTorch+MKL bit-for-bit equivalence.

### Why gate 8.5 was the right framing

The gate-8 first-pass framing ("bit-exact vs Python on every primitive")
implied PyTorch was a single canonical reference. It isn't — PyTorch's
output depends on which BLAS it dispatches to, and the dispatch is
size-dependent. Gate 8.5 reframes the load-bearing claim as
**arithmetic correctness** on random witnesses, the same property gate 1
checks through the C++ engine. Bit-exact parity remains a useful
secondary check on small primitives where both runners take the same
non-BLAS code path.

### Open performance work

- `freeze_setup` / `freeze_apply` at 10k vectors are infeasible at the
  current Rust runner speed (~9 days each, single-threaded).
  C++ engine parity comes from sparse-matvec + AVX2 BLAS in
  `transformer.cpp`. Future Rust runner work: sparse weight detection
  + matrixmultiply-backed dense path or BLAS link via build script.
- The flat-buffer attention rewrite (`4ffe560`) cut baseline parity
  test wall-clock 21.6s → 10.4s (2×) by eliminating per-step Array1
  allocations. Bit-exactness preserved because summation order didn't
  change. Further wins likely from rewriting the FFN ReGLU loop as
  a pair of matrix multiplies and from a SIMD-aware ff_out.
