# ADR-0008 — BLAKE3-512 for long-lived commitments

**Status:** accepted (engineering proceeds; final ratification on first external cryptographer review).
**Date:** 2026-05-09.
**Companion:** ADR-0006 (PQ strategy), ADR-0007 (cryptographic agility architecture).

## Context

Grover's algorithm provides a quadratic speedup against preimage
attacks on hash functions. Under quantum, BLAKE3-256's effective
preimage security drops from 256 bits to ~128 bits. 128-bit security
is **still very strong** — not practical to attack at scale even
with a CRQC. So most hashes in PSL do not need an urgent migration.

But some hashes are **irrevocable commitments**:

- `weights_hash` for a ternary network is committed in every trace
  that uses that network. Once committed, those traces cannot be
  retroactively re-hashed without invalidating every signature
  over them.
- Long-lived agent contract hashes (escrow, time-locked release,
  multi-year multisig) are likewise commitments; the contract is
  identified on-chain by its `program_hash` and that identifier
  cannot change.

For these surfaces, the durability requirement is **multi-decade**.
A 128-bit margin under quantum is uncomfortably narrow given the
40-year horizon some institutional commitments imply.

For surfaces that are short-lived — trace hashes for individual
transactions, MPT roots that are superseded every block, block
headers superseded by the next block — the 128-bit quantum margin
is fine, and the cost (doubled hash size everywhere) is not
warranted.

## Decision

Two-tier hash policy:

| Surface                              | Scheme       | Why |
| ---                                  | ---          | --- |
| Trace hashes (per-tx, ephemeral)     | BLAKE3-256   | Short-lived; 128-bit Q margin acceptable. |
| MPT root (superseded every block)    | BLAKE3-256   | Superseded; 128-bit Q margin acceptable. |
| Block header hashes                  | BLAKE3-256   | Superseded by chain progression. |
| `weights_hash` (irrevocable)         | **BLAKE3-512** | Multi-decade commitment; 256-bit Q margin warranted. |
| Long-lived contract `program_hash`   | **BLAKE3-512** | Same. |
| KEM transcript hash (per ADR-0006 hybrid combiner) | BLAKE3-256 | Per-encryption ephemeral, not committed. |

`HashScheme` enum (per ADR-0007) distinguishes them:

```rust
pub enum HashScheme {
    Blake3_256 = 0x01,
    Blake3_512 = 0x02,
}
```

### Migration plan

1. Phase 2 of the PQ migration (per ADR-0006) introduces BLAKE3-512
   as a new variant.
2. Existing weight files are re-hashed with BLAKE3-512; the new
   `weights_hash` is recorded; trace-hash format version is
   bumped from v1 → v2 to indicate the new hash width on the
   `weights_hash` field.
3. v1 traces (with BLAKE3-256 weights_hash) remain readable — old
   data does not become invalid; we only commit v2 going forward.
4. Existing standard-library agent contracts (`transfer`, `swap`,
   `escrow_*`, etc.) get new `program_hash` values under BLAKE3-512.
   The contract identifiers on-chain change; the contract semantics
   do not.

### Format

BLAKE3 supports variable-length output natively. BLAKE3-512 is a
64-byte output from the standard BLAKE3 construction (no truncation,
no special variant — just `blake3::Hasher::finalize_xof` reading 64
bytes).

Wire encoding follows the agility format from ADR-0007:

```
hash_blob := varint(scheme_id) || hash_bytes
```

A 32-byte BLAKE3-256 hash blob is 33 bytes on the wire.
A 64-byte BLAKE3-512 hash blob is 65 bytes on the wire.

### Forward path for BLAKE3-256 surfaces

Should Grover-class quantum attacks become feasible faster than
expected, the BLAKE3-256 surfaces migrate to BLAKE3-512 by:
- Bumping the trace-hash format version (v2 → v3).
- New blocks emit v3 trace hashes.
- v2 traces remain verifiable with the v2 verifier.
- Light clients accept both during transition; reject v2 after a
  documented deadline.

This forward path is documented but **not active in v0.1.x**.
Trigger to activate: external cryptographer review or NIST
recommendation for BLAKE3-256 (or BLAKE3-class hashes generally).

## Consequences

- `weights_hash` field doubles in width (32 B → 64 B). Trace
  serialization grows correspondingly. Per-trace cost is in the
  noise; per-block cost is bounded by number of distinct programs
  used (small).
- Trace-hash format version bump (v1 → v2) is a clean, documented
  break. v1 traces remain readable via the legacy verifier.
- Standard-library contract identifiers change. Existing identifiers
  remain valid for legacy execution; new identifiers are emitted for
  all new contract instantiations. Migration tool documents the
  mapping.
- Cannot reduce `weights_hash` back to 256 bits without an ADR
  superseding this one.

## Alternatives considered

- **All hashes BLAKE3-512.** Rejected; quintuples MPT-leaf hash
  size (32 B → 64 B per leaf) and similarly grows every short-lived
  hash. Cost not warranted by threat model for non-irrevocable
  surfaces.
- **Stay BLAKE3-256 everywhere; defer.** Rejected for irrevocable
  commitments. Once committed, cannot retroactively migrate without
  invalidating signatures.
- **SHA3-512 instead of BLAKE3-512.** Considered. SHA3 is FIPS
  certified, which has procurement value with US federal customers.
  But: PSL already depends on BLAKE3 for the short-lived surfaces;
  introducing SHA3 alongside BLAKE3 means two hash implementations
  to audit and maintain. Stick with one hash family. Re-evaluate if
  a federal customer requires SHA3 specifically.
- **SHAKE-256 (XOF) instead of BLAKE3-512.** Same reasoning as SHA3
  rejection: avoid second hash family.
