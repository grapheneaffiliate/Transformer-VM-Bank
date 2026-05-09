# ADR-0001 — Retire the legacy fp64 transformer runner

**Status:** Accepted
**Date:** 2026-05-09
**Author:** PSL maintainers (Phase 2 closure session)
**Supersedes:** none
**Superseded by:** none

## Context

Gate 8 was scoped during Phase 1 to deliver bit-exact parity between
PSL's pure-Rust runner (`rust_runner/`, fp64 autoregressive) and
Transformer-VM's reference engines on every active primitive.

The Phase 1 sweep delivered 50,000 / 50,000 bit-exact passes on the 5
short primitives that fit inside the soft-attention model's precision
envelope. The 2 long primitives (`freeze_setup`, `freeze_apply`) hit
a structural wall:

- `transformer.cpp` defaults to **hard attention** (hull-based,
  O(log n), via `HardAttentionHead` in `hull2d_cht.h`).
- Our `rust_runner` ports Python's `StandardKVCache` (soft attention,
  O(n), softmax over the K/V cache, fp64 sums).

These are different algorithms. Soft attention's fp64 accumulation
drift is large enough at 17.5k tokens that the model never produces a
`halt` token in either Python or Rust without MKL's reduction order
intervening. The original gate-1 10000/10000 results on `freeze_*`
came from `wasm-run`'s default (hard attention), not the Python soft
path.

Two options for closing gate 8:

(a) **Port hard attention (hull cache) to the Rust runner.** ~1 week
of additional work. Closes the gap for a code path that is no longer
the canonical execution layer (gates 10-16 shipped the
ternary-integer engine in `ternary_vm/` as the canonical engine for
Phase 2 onward).

(b) **Retire the legacy fp64 runner.** Acknowledge that gates 10-16
made the soft-attention runner architecturally redundant. Existing
primitives stay runnable for backward-compatibility verification only;
all new primitives ship ternary-integer.

## Decision

We adopt option (b): **retire the legacy fp64 runner in favor of the
ternary-integer engine** (`ternary_vm/`).

Concrete actions:

1. Move `rust_runner/` to `legacy/rust_runner/`. Update workspace
   members in `Cargo.toml`.
2. Mark every public item in the legacy crate `#[deprecated]` with a
   migration message pointing at the ternary equivalent.
3. CI guard (`ci/legacy-runner-frozen`) fails any new code outside
   `legacy/` that imports from `psl_rust_runner`. Implementation:
   `tools/ci/check_legacy_isolation.sh` — a `git grep` rejection
   list.
4. `docs/ARCHITECTURE.md`: promote § 0.8 (ternary trace contract) to
   § 0.2 as the canonical contract; move the existing fp64
   token-sequence trace contract to a new "§ 0.A Legacy Trace
   Contract (Frozen)" subsection.
5. `docs/STATUS.md` gate 8 row updates from ⚠️ partial to ✅ closed
   (retired in favor of ternary-integer per ADR-0001).

## Consequences

**Positive:**
- Eliminates the canonical-engine pin that gate 8 had to maintain. Any
  conformant integer-arithmetic verifier produces bit-identical output
  for the same input + `weights_hash`.
- Removes the multi-week effort to port hull-based hard attention to
  Rust.
- Aligns the codebase with the agent execution layer (gates 10-16) as
  the production substrate.
- Cross-platform determinism becomes a property of integer addition
  rather than fp64 reduction order.

**Negative:**
- Existing primitives compiled through Transformer-VM (`freeze_setup`,
  `freeze_apply`, etc.) cannot be re-validated against the Phase 1
  Python+MKL reference at scale via the Rust runner. They remain
  validated by the original gate-1 sweep through the C++ engine
  (10000/10000 each).
- A future implementor wanting to verify that the *transformer-VM
  weights themselves* still produce expected output cannot use our
  Rust runner past 7k tokens. They must use the C++ engine.

**Risk mitigation:**
- The legacy crate stays in tree, frozen but runnable, so the
  backward-compat verification path is preserved.
- A migration table in `docs/ARCHITECTURE.md § 0.A` maps every legacy
  primitive to its ternary equivalent.
- The CI guard prevents accidental new dependencies on the legacy
  runner.

## Alternatives considered

- **Port hard attention to Rust (option a).** Rejected — the cost
  buys parity for a code path the system no longer relies on.
- **Hybrid: keep both runners as first-class.** Rejected — two
  canonical engines means two trust surfaces, two test matrices, two
  audit scopes. Architectural drag exceeds value.
- **Keep legacy runner but mark it experimental.** Rejected —
  `#[experimental]` is not a Rust attribute and `#[deprecated]` is the
  honest signal. "Frozen" is the policy; deprecation is the
  enforcement.

## References

- `docs/STATUS.md` gate 8 row history (commits `38dbe9f`, `91ed18a`).
- `docs/ARCHITECTURE.md` § 0.3 (canonical engine ordering, soft vs hard
  attention caveat).
- `docs/FINDINGS.md` § Gate 8 follow-up (MKL reduction-order
  investigation).
- `Transformer-VM/transformer_vm/model/transformer.cpp` (`hull2d_cht.h`,
  `HardAttentionHead` — the algorithm we did not port).
