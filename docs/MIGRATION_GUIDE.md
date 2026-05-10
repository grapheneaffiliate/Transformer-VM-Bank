# PSL Migration Guide

**Audience:** SDK users and partners integrating PSL across version
transitions.
**Last refreshed:** 2026-05-10 (post-Phase G phases 1-3 merge).
**Companion docs:** [`STATUS.md`](STATUS.md) for the current gate
table, [`docs/decisions/`](decisions/) for the authoritative ADRs.

This document is the running record of every breaking-change
migration in PSL's public surface, with the action required from
external integrators in each case. ADRs are the *decision* records;
this is the *integrator's* record.

## Migration matrix

| From → To              | Surface affected                  | Action required                  | ADR        |
| ---                    | ---                               | ---                              | ---        |
| Pre-v0.1.0 → v0.1.0    | Legacy fp64 trace contract        | None — legacy crate frozen, not removed | ADR-0001 |
| v0.1.0 → v0.1.x        | trace_hash format v1 → v2         | New: dual versions; default to v2 for new code | ADR-0008 |
| v0.1.0 → v0.1.x        | ed25519 → hybrid signatures       | Optional during v0.1.x; required at v0.2 cutover | ADR-0006 |
| v0.1.x → v0.2 (planned)| `program_hash` → BLAKE3-512       | Recompute on contract redeploy   | ADR-0008 |
| v0.1.x → v0.2 (planned)| State tree → hash-of-pubkey       | Run `tools/migrate_state_to_v2.sh` (forthcoming) | ADR-0007 |
| v0.1.x → v0.2 (planned)| Hybrid required (ed25519 deprec.) | Migrate accounts before deadline | ADR-0006 § Phase 6 |

## 1. Legacy fp64 trace contract → ternary integer kernel

**Status:** done in v0.1.0 (gate 8 closed via retirement per
ADR-0001). No action required for v0.1.0 deployments — the legacy
crate `legacy/rust_runner/` is frozen and remains buildable for
historical block verification. New code must not depend on it; CI
guard `tools/ci/check_legacy_isolation.sh` enforces the boundary.

If you maintain a custom verifier that depends on the legacy fp64
trace contract: keep using `legacy::trace_hash_legacy` for
historical blocks; switch to `ternary_vm::trace_hash::v2::trace_hash_v2`
for new blocks. The trace contracts are mutually exclusive — one
block uses one or the other, never both.

## 2. trace_hash format v1 → v2 (BLAKE3-512 weights_hash)

**Status:** dual versions ship in v0.1.x.
**ADR:** [ADR-0008](decisions/0008-blake3-512-for-long-lived-commitments.md).

### What changed

- `trace_hash_v1` (frozen): commits a 32-byte BLAKE3-256 `weights_hash`.
- `trace_hash_v2` (canonical): commits a 64-byte BLAKE3-512 `weights_hash`.
- Trace-hash output stays 32 bytes in both versions; only the
  `weights_hash` width changes.

### Action for SDK consumers

If you constructed `WeightsHeader` instances directly:

```rust
// Old (v0.1.0):
let header = WeightsHeader {
    weights_hash: digest_v1,
    // ...
};

// New (v0.1.1+):
let (_, digest_v1, digest_v2) = pack_weights_dual(name, in_dim, out_dim, &layers);
let header = WeightsHeader {
    weights_hash: digest_v1,
    weights_hash_v2: digest_v2,
    // ...
};
```

If you call `unpack_weights`, no change required — it now populates
both fields automatically.

If you call the deprecated `trace_hash_ternary` re-export, it still
works and returns v1 bytes (pinned by `deprecated_re_export_is_v1_not_v2`
test). Migrate to `trace_hash::v2::trace_hash_v2` at your convenience;
the re-export will be removed at v0.2.

### Cutover policy for chain operators

There is no live PSL chain at v0.1.x (audit-pending per gate 17/18).
When you operate one:
1. Choose a cutover block height N in your genesis-config addendum.
2. Pre-N blocks: sequencer emits v1 trace_hashes, verifiers verify
   under v1.
3. At-or-after-N blocks: sequencer emits v2, verifiers verify under
   v2.
4. Both verifiers ship in the same binary; selection is per the
   block's `trace_hash_format_version` field (planned for the v0.2
   block header).

## 3. ed25519 → hybrid signatures (ed25519 + ML-DSA-65)

**Status:** hybrid implementation ships in v0.1.x; ed25519 remains
the default for compatibility. **Hybrid becomes required at v0.2
cutover** per ADR-0006 phase 6.
**ADR:** [ADR-0006](decisions/0006-post-quantum-cryptography-strategy.md).

### What changed

- New scheme `SignatureScheme::HybridEd25519MlDsa65 = 0x02` ships
  alongside `SignatureScheme::Ed25519 = 0x01`.
- Hybrid pubkey: 1984 bytes (32B ed25519 || 1952B ML-DSA-65).
- Hybrid signature: 3373 bytes (64B ed25519 || 3309B ML-DSA-65).
- Concatenation order is locked (ed25519 first); hard-fail on length
  mismatch (no silent truncation).
- Verification accepts iff **both** components verify.

### Action for SDK consumers

To produce hybrid signatures from new code:

```rust
use psl_crypto_agility::{HybridSigner, HybridVerifier, Signer, Verifier, SignatureScheme};

let signer = HybridSigner::generate();           // hybrid keypair
let sig = signer.sign(b"my message")?;           // 3373-byte hybrid sig
let pk  = signer.public_key();                   // 1984-byte hybrid pk

let verifier = HybridVerifier::new();
verifier.verify(SignatureScheme::HybridEd25519MlDsa65, b"my message", &sig, &pk)?;
```

To accept either ed25519 or hybrid (transition window):

```rust
use psl_crypto_agility::{Ed25519Verifier, VerifierPolicy};

let verifier = Ed25519Verifier::with_policy(VerifierPolicy::ed25519_or_hybrid());
```

To accept hybrid only (post-cutover):

```rust
let verifier = HybridVerifier::new();
// or, for explicit policy:
let verifier = Ed25519Verifier::with_policy(VerifierPolicy::hybrid_only());
```

### Determinism note

Per ADR-0006 § Determinism invariant: signing under hybrid is
**randomized** (FIPS 204 §5.4 standard `Sign`); verification is
**fully deterministic**. This is fine for dispute-by-re-execution
because the chain commits a specific signature once and replays
verify against the committed bytes. If your application requires
deterministic-mode signing, see ADR-0006 § "Note on
HybridEd25519MlDsa65 sign-side randomness" — flagged for external
cryptographer review.

### Wire format for partners

Both pubkey and signature blobs carry a varint scheme prefix per
ADR-0007. The full wire-format helpers:

```rust
use psl_crypto_agility::{encode_hybrid_pubkey_blob, encode_hybrid_sig_blob, decode_hybrid_blob};

// Producer side:
let pk_blob = encode_hybrid_pubkey_blob(&hybrid_pk);     // 1 + 1984 = 1985 bytes
let sig_blob = encode_hybrid_sig_blob(&hybrid_sig);      // 1 + 3373 = 3374 bytes

// Consumer side:
let pk_body = decode_hybrid_blob(&pk_blob, HYBRID_PUBKEY_BYTES)?;
let sig_body = decode_hybrid_blob(&sig_blob, HYBRID_SIG_BYTES)?;
```

Decoder hard-fails on:
- Unknown scheme prefix (`UnknownScheme(u32)`)
- Length mismatch — including one-byte-short (signature-malleability
  defense)

## 4. v0.1.x: program_hash → BLAKE3-512 (architectural pieces shipped)

**Status:** types + contracts shipped in v0.1.x. Full agent-layer
wire-format break (Propose/CounterPropose etc.) deferred to v0.2.
**ADR:** [ADR-0008](decisions/0008-blake3-512-for-long-lived-commitments.md).

### What landed in v0.1.x
- `ProgramHash` is a **newtype** (`pub struct ProgramHash(pub [u8;
  64])`), not a type alias. Compiler refuses to silently mix it
  with [`crate::ProposalHash`] or pubkeys.
- `ProgramHashV1` is a **frozen** newtype around `[u8; 32]` for
  historical-block verification.
- New helpers `program::v1::program_hash_v1` and
  `program::v2::program_hash_v2`. New `build_program_hashes()`
  computes both in one call.
- All 8 standard contracts compute and store **both** digests at
  `build()` time. The trait exposes both methods:
  `program_hash() -> [u8; 32]` (legacy compat for the v0.1.x
  agent layer) and `program_hash_v2() -> ProgramHash` (canonical).
- Frozen v1 KAT (benign + adversarial: distinct names, ordering,
  empty-list).

### Action for SDK consumers
- New code committing to a contract identity for long-lived
  on-chain references (e.g., a contract registry, a partner
  whitelist) should use `contract.program_hash_v2()`, not
  `program_hash()`. The 64-byte form is the post-quantum-safe
  commitment per ADR-0008.
- Existing code reading `program_hash()` continues to work
  unchanged; the 32-byte form remains valid for the v0.1.x agent
  layer (Propose's wire field, agent_sdk's contract HashMap).

### What's deferred to v0.2
- `agent_protocol::message::Propose.program_hash` field (wire
  format) widens from `[u8; 32]` to `ProgramHash`.
- `agent_sdk::AgentSdk::contracts` HashMap re-keys from `[u8; 32]`
  to `ProgramHash`.
- All Propose::sign call sites cascade.

These are bundled as a follow-up PR for review-surface reasons —
the agent-layer wire-format break deserves dedicated review attention
separate from the type-system / contract-identity work shipped here.

## 5. The hash-width principle (per ADR-0008)

When deciding what hash width a new commitment should use, apply
this principle:

**Long-lived irrevocable commitments → BLAKE3-512 (64-byte newtype).**
A commitment is long-lived if it is referenced for the lifetime of
the chain after being committed. Examples:
- `weights_hash` (committed in every trace; each trace references
  it for the contract's lifetime).
- `program_hash` (the on-chain contract identifier; permanent for
  the lifetime of the contract).
- Long-lived contract identity hashes (e.g., a multi-year escrow
  identifier).

**Ephemeral content hashes → BLAKE3-256 (32 bytes, may be a newtype
for type-safety).** A commitment is ephemeral if it is content-
addressed within a single protocol round and not referenced
afterwards. Examples:
- `proposal_hash` (the BLAKE3 of a `Propose` message; lives only
  for the proposal lifecycle).
- Per-execution `trace_hash` (committed in the block but the
  block's `trace_hash` field itself is the content hash, not the
  commitment-to-program identity).
- Dispute message hashes, transient negotiation state.

**If a new hash doesn't cleanly fit either bucket, that's a signal
it needs an ADR before it ships.** Borderline cases (a hash that's
"medium-lived") are usually a sign that the design intent is
unclear; surface that ambiguity in an ADR rather than picking
silently.

The two buckets exist because Grover's algorithm gives a quadratic
speedup against preimage attacks: BLAKE3-256's effective preimage
security drops to 128 bits under quantum, BLAKE3-512's to 256.
128 bits is still very strong but not multi-decade durable; 256
bits is the conservative margin for irrevocable commitments. For
ephemeral content hashes the 128-bit margin is fine — by the time
quantum attacks become practical, the ephemeral value has long
since been consumed.

## 5. Planned for v0.2: state tree → hash-of-pubkey storage

**Status:** planned. Bumps state-format version.
**ADR:** [ADR-0007](decisions/0007-cryptographic-agility-architecture.md) § Storage.

When this lands:
- Account state-tree leaves store a 32-byte BLAKE3 hash of the
  full hybrid pubkey, not the pubkey itself (Bitcoin-style P2PKH
  pattern).
- Full pubkeys live in a separate registry subtree keyed by hash.
- Migration tool: `tools/migrate_state_to_v2.sh` (forthcoming) walks
  every account and rewrites the tree.

Action for chain operators: schedule a maintenance window to run
the migration tool against your snapshot before upgrading to v0.2
sequencer code.

## 6. Planned for v0.2 cutover: ed25519-only deprecation

**Status:** planned. Suggested transition window: 12 months from
v0.1.0 release; institutional partners may require shorter via
contract.
**ADR:** [ADR-0006 § Migration phase 6](decisions/0006-post-quantum-cryptography-strategy.md).

When the deadline arrives:
- Sequencer / follower configuration switches to
  `VerifierPolicy::hybrid_only()`.
- ed25519-only signatures are rejected with `SchemeNotAccepted`.
- Migration tool: scans for accounts whose registered key is
  ed25519-only, surfaces them, provides a key-rotation transaction
  template using the existing `KeyRotation` mechanism in
  `agent_wallet/`.

Action for accounts: rotate to a hybrid keypair before the
deadline. The `KeyRotation` envelope lets you rotate from an
ed25519 child key to a hybrid child key without changing the
parent identity.

## What this document does NOT cover

- Operational migration (rolling upgrades, blue-green deploys) —
  see [`docs/runbooks/`](runbooks/) and
  [`docs/OPERATIONAL_READINESS.md`](OPERATIONAL_READINESS.md).
- Test-network coordination — there is no public test net at v0.1.0
  per [ADR-0004](decisions/0004-public-test-network-deferred.md);
  private deployments coordinate out-of-band.
- License migration — there is none. PSL is MIT throughout (per
  [ADR-0005](decisions/0005-licensing-export-patent-posture.md));
  forks under MIT do not need to track our migrations.
