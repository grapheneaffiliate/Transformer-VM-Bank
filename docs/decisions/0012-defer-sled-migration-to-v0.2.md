# ADR-0012 — Sled storage migration: deferred to v0.2

**Status:** accepted.
**Date:** 2026-05-10.
**Deciders:** PSL maintainers.

## Context

PSL's sequencer persists state — account balances, MPT nodes,
block headers, WAL — through `sled` (the `sled = "0.34"` workspace
dependency, used in `sequencer/`). `sled` is a Rust-native embedded
KV store, comparable in role to RocksDB or LMDB.

`sled` has been in beta for years and the project is effectively
unmaintained:
- Last meaningful release was in 2021.
- The lead maintainer publicly stated they were rewriting the engine
  but the rewrite stalled.
- The crate carries known-but-unfixed correctness issues under
  crash recovery.

For a settlement layer that must durably persist state across
crashes, depending on an unmaintained backing store is a real
concern that deserves a deliberate decision rather than implicit
accumulation.

## Decision

**Defer migration off `sled` to v0.2.0.** v0.1.0 ships on `sled`
with this deferral documented and the v0.2 trigger conditions
explicit (below).

## Rationale

### 1. No data loss observed in PSL's actual usage

The pilot has run on `sled` through the full gate sequence (gates
1-7 sequencer + light client + agent layer + cross-platform
determinism CI) with zero corruption. The risk profile is
**theoretical and crash-recovery-shaped**, not currently manifest.
We have not reproduced a `sled` correctness failure under PSL's
workload.

### 2. Migration is a one-way door at significant scope

Storage-layer changes touch:
- The sequencer's hottest path (write batch on every block).
- The MPT (read-modify-write on every state transition).
- The WAL and the recovery procedure (the part of the codebase
  whose correctness is hardest to test offline).

This deserves its own ADR sequence, its own PR cascade, and ideally
its own audit pass — **not** a "while we're here" addition to
v0.1.0. Bundling a storage migration into v0.1.0 risks introducing
new bugs into a stable codebase right before external review, which
is the inverse of what a pre-audit posture wants.

### 3. Not on the audit-blocking path

Gate 17 (external audit) and gate 19 (cryptographer review) examine
cryptographic constructions, consensus logic, protocol correctness,
and key-handling hygiene. Auditors do not typically rule on "is your
KV store maintained enough." The `sled` risk surfaces in
**operational reliability**, not security. Deferring it does not
block audit engagement.

## v0.2 trigger conditions

Migration off `sled` begins on the **first** of the following:

1. **First pilot deployment surfaces operational requirements** that
   make the storage backend a load-bearing variable (e.g., a partner
   needs hot backup, point-in-time recovery, or higher write
   throughput than `sled` delivers under their workload).
2. **Concurrent with audit-findings remediation** — if gate 17
   produces findings that require non-trivial sequencer-path work,
   the storage migration bundles into the same v0.2 release rather
   than shipping as point releases.
3. **A reproducible `sled` corruption is observed** in PSL's own
   workload (unforced trigger).
4. **Deferred indefinitely otherwise.** "Unmaintained dependency"
   alone is not a sufficient trigger if PSL's usage continues to
   succeed against it; we revisit this assumption at v0.2 planning.

## Alternatives considered for v0.2

The chosen alternative is left open until v0.2 planning, but the
two leading candidates today are:

- **`rust-rocksdb`** — mature wrapper around RocksDB. Used by
  Solana, CometBFT, and many other production chains. Battle-tested
  under high write load. Heavier dependency (C++ underneath, longer
  build times, larger artifact). Industry standard for this slot.
- **`redb`** — Rust-native, well-maintained, simpler than RocksDB.
  Newer than `sled` was at `sled`'s peak, but the maintainer is
  active and the project ships releases. Smaller surface, smaller
  risk profile, but less battle-tested under settlement-layer
  workloads.

The migration design (whichever backend wins) is:
1. Define a `Storage` abstraction trait covering the operations
   `sequencer/` actually uses (a small subset of `sled`'s API).
2. Implement against the new backend.
3. Write a one-time migration tool that reads `sled` and writes to
   the new store; validate byte-equality on a copy of pilot data
   before cutting over.
4. Cut over in a single release with the migration tool documented
   in `REPRODUCE.md`.

This work happens under its own v0.2-track ADR (the choice between
`rust-rocksdb` vs `redb`) when the v0.2 work begins.

## Consequences

- v0.1.0 ships with a documented technical debt (this ADR), not an
  unexplained `◻` in the task tracker.
- External reviewers reading the v0.1.0 release notes see "deferred
  with rationale" rather than "missing without explanation."
- v0.2 scope grows by one substantial item, but the ADR makes the
  cost legible at planning time.
- If a `sled` correctness bug surfaces between v0.1.0 release and
  v0.2 work beginning, trigger #3 fires and the migration moves up
  the priority list immediately — the path is pre-thought.

## Relationship to other ADRs

- **ADR-0002 (BFT consensus, deferred).** Same shape: a real piece
  of work, deliberately deferred with explicit triggers, not
  pretended-to-be-done.
- **ADR-0004 (public testnet, deferred).** Same shape.
- **ADR-0006 / ADR-0011 (cryptographer review).** Audit-track work
  that is not blocked by this storage deferral.
