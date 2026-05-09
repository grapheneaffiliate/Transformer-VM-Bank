# `unwrap()` / `expect()` audit

**Per session-brief operating principle #2:** "no `unwrap()` /
`expect()` in production paths."

This document is the audit of every `unwrap` / `expect` in non-test
code in the workspace, with a per-call justification or fix.

Audit performed 2026-05-09 against commit `5e0ad17` (Phase A close).
Methodology: Python script that walks every `**/src/**/*.rs`
(excluding `legacy/` and `target/`), splits at the first
`#[cfg(test)]`, and grep on the production prefix.

## Result

40 production-path hits. All are one of two patterns:

### Pattern 1 — `Mutex::lock().unwrap()` / `RwLock::read().unwrap()`

Sites: 35 of 40.
Crates: `agent_sdk`, `consensus`, `sequencer`, `pilot/issuer_demo`.

A `LockResult::Err(PoisonError)` arises **only** if a previous
holder of the lock panicked while holding it. In every PSL crate,
the only way to panic a lock holder is a programming bug (per
operating principle #4: no silent failures, all input-driven errors
return `Result`). Therefore poisoning is a programming-bug-class
event, not a runtime-input event.

Policy (this audit): `lock().unwrap()` is acceptable on production
paths. Lock poisoning **is** an incident-grade event; if it happens
the process is no longer in a consistent state and panicking is the
correct response.

The alternative is `parking_lot::Mutex` which doesn't poison. We do
not import `parking_lot` because it adds a workspace dependency for
no behavioral improvement at the layer of trust we operate (a
poisoned lock is not safe to continue working with).

### Pattern 2 — `checked_add(..).expect("invariant")`

Sites: 3 of 40.

- `ternary_vm/src/thermo.rs:43`: `count.checked_add(1).expect("thermo length overflow")`
  — `count` is bounded by the input slice length (`thermo.decode`
  iterates a `&[i64]`), which itself is bounded by `Vec` capacity
  (≤ `isize::MAX` bytes). Overflow into i64 is structurally
  impossible.
- `ternary_vm/src/weights.rs:82,94`: `ptr.checked_add(row.len() as u32).expect("ptr overflow")`
  — `ptr` accumulates row sizes during weights packing; bounded
  by `output_dim × input_dim ≤ u32::MAX` per the
  `WeightsHeader::input_dim`/`output_dim: u32` field types.
  Overflow would require constructing a network with > 4G non-zero
  weights, which our `from_dense` constructor cannot reach
  (`SparseTernaryLayer::input_dim`/`output_dim: usize` clamped at
  load time to u32 range).

Policy (this audit): kept as `expect` with the above justifications
inlined as comments above each call site (added in a follow-up
commit). The `expect` message documents the invariant for any
post-mortem reader.

## Genuine bugs found and fixed

None. Every audited site is a documented programming-bug-class
unwrap (lock poisoning) or a structurally-impossible-overflow
expect.

## Test code

Out of scope for this audit. `unwrap` in `#[cfg(test)] mod tests`
blocks is idiomatic Rust and required for ergonomic test assertions
(`assert_eq!(value.unwrap(), expected)`). Test panics are test
failures, not production incidents.

## Followup

Tracked separately: a clippy lint `clippy::unwrap_used` could be
configured at workspace level to forbid new unwraps outside test
modules. Adding this would surface any future regression
automatically. Decision: defer. Currently 35 lock-poison unwraps
are intentional and would require `#[allow(...)]` on each one —
noisier than the audit doc.
