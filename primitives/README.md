# PSL Primitives

These C source files are compiled to WASM, then specialized through the
existing Transformer-VM toolchain at `$TRANSFORMER_VM_PATH` (default
`/mnt/c/Users/atchi/Transformer-VM`) to produce per-primitive transformer
weights. Each primitive's specialized model is bit-exactly equivalent to
running the WASM natively — that equivalence is the entire trust property
PSL relies on.

## v2 style guide (inherited from Transformer-VM `arc_common.h`)

Each primitive must satisfy:

- **`void compute(const char *input)`** — single entry point.
- **Static globals** for state, no malloc, no floats.
- **No `mul_var`** — variable multiplication uses precomputed offset tables
  (`build_account_offsets` for multi-account access, sequential `idx++` in
  loops). The compiler will emit a noinline `mul_var` helper if you write
  `r * cols` with both variables; that costs ~30 instructions and pushes
  `d_ffn` past 4000.
- **`printf` for output**, never `print_int` (always_inline duplicates
  ~20 instructions per call).
- **`sscanf`** for fixed-count header args.
- **Safety counters on every `while` loop**: `while(cond && _s < N) { _s++; }`.
- **Max nesting 2–3 levels.**
- **Under 2000 WASM instructions total per primitive.**

## Wire format

Inputs and outputs are space-separated decimal bytes. An account record is
64 bytes — 64 decimals on the wire. A u128 amount is 16 bytes — 16 decimals.

## Authorization vs. trace

The transformer trace verifies **state-transition arithmetic only**. The
sequencer verifies BEFORE invocation:

- ed25519 signature on the tx
- Nonce monotonicity (`tx.nonce == account.nonce + 1`)
- Issuer-registry authority for mint/burn/freeze
- Asset-id matching across the inputs
- Travel-rule metadata for high-value txs

If any pre-check fails, the tx is rejected from the mempool and never invoked.

## Primitives

| Primitive | Input bytes | Output bytes | ~WASM instr |
| --- | --- | --- | --- |
| `ledger_freeze.c` | 1 + 64 | 64 | ~150 |
| `ledger_transfer.c` | 1 + 64 + 64 + 16 = 145 | 128 | ~600 |
| `ledger_mint.c` | 1 + 64 + 16 = 81 | 64 | ~350 |
| `ledger_burn.c` | 1 + 64 + 16 = 81 | 64 | ~400 |
| `ledger_multi_asset.c` | 2 + N(64+64+16) | N · 128 | ~1500 (N=4) |
| `mpt_apply_delta.c` | 1 + N(1+64) | N(1+64) | ~700 (N=8) |

Instruction estimates are approximate; the bit-exact gate (10k vectors per
primitive) is the actual ground truth. Failure of any vector means the
primitive's compiled instruction count exceeded the precision envelope.
Decompose further or re-style.

## Build

```bash
./tools/compile.sh primitives/ledger_freeze.c
./tools/specialize.sh primitives/ledger_freeze.txt
./tools/build_all_primitives.sh                    # builds and specializes all
uv run pytest tests/test_bit_exact.py -v           # gate 1
```
