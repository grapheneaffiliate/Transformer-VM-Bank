# ADR-0007 — Cryptographic agility architecture

**Status:** accepted (engineering proceeds; final ratification on first external cryptographer review).
**Date:** 2026-05-09.
**Companion:** ADR-0006 (PQ strategy), ADR-0008 (BLAKE3-512 long-lived commitments).

## Context

The hard part of the post-quantum migration (ADR-0006) is not
adopting any one scheme. It is making the architecture **agile
enough that future migrations are not hard forks**.

PQC standards are new (finalized 2024). Cryptanalysis on lattice-
based schemes will continue for years; any of ML-DSA, ML-KEM,
SLH-DSA, FN-DSA could surface a flaw that requires migration to a
successor scheme. The same is true of any signature or KEM we
choose: history shows everything migrates eventually (RSA → ECDSA →
ed25519 happened over decades).

Without an agility layer, every migration is a hard fork. With one,
adding a new scheme is a one-byte prefix change.

## Decision

### Trait shape

A new workspace crate `crypto_agility/` defines five traits and
five enums. All cryptography in PSL goes through these traits.
**No call site reaches into a primitive crate (`ed25519-dalek`,
`pqcrypto-mldsa`, etc.) directly.**

```rust
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignatureScheme {
    Ed25519              = 0x01,
    HybridEd25519MlDsa65 = 0x02,
    SlhDsa128s           = 0x03,   // reserved, not yet implemented
    // future schemes get their own discriminant; never reuse retired ones
}

pub trait Signer {
    fn scheme(&self) -> SignatureScheme;
    fn public_key(&self) -> Vec<u8>;
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, SignerError>;
}

pub trait Verifier {
    fn scheme(&self) -> SignatureScheme;
    fn verify(&self, message: &[u8], signature: &[u8], public_key: &[u8]) -> Result<(), VerifierError>;
}

#[repr(u32)]
pub enum KemScheme {
    X25519                = 0x01,
    HybridX25519MlKem768  = 0x02,
}

pub trait Kem {
    fn scheme(&self) -> KemScheme;
    fn encapsulate(&self, recipient_pk: &[u8]) -> Result<(Vec<u8>, SharedSecret), KemError>;
    fn decapsulate(&self, ciphertext: &[u8], my_sk: &[u8]) -> Result<SharedSecret, KemError>;
}

#[repr(u32)]
pub enum HashScheme {
    Blake3_256 = 0x01,
    Blake3_512 = 0x02,
}

pub trait HashScheme_ {
    fn scheme(&self) -> HashScheme;
    fn hash(&self, data: &[u8]) -> Vec<u8>;
}
```

### Wire format — varint scheme prefix

Every key, signature, and encapsulation blob carries an explicit
scheme identifier as a leading **varint**. Verifiers parse the
varint, dispatch by scheme, and **refuse unknown schemes with a
typed error** (never silent acceptance, never best-effort fallback).

```
sig_blob   := varint(scheme_id) || scheme_specific_signature_bytes
pubkey     := varint(scheme_id) || scheme_specific_public_key_bytes
kem_ct     := varint(scheme_id) || scheme_specific_ciphertext_bytes
encrypted  := varint(kem_scheme) || kem_ct ||
              varint(symmetric_scheme) || nonce || aead_ciphertext
```

Varint encoding is **unsigned LEB128** (the same encoding used by
WebAssembly and by IPFS multicodec). Encoded length: 1 byte for
schemes ≤ 127, 2 bytes for ≤ 16383, etc. We do not anticipate
needing more than 1 byte per prefix in the foreseeable future.

### State-tree storage — hash-of-pubkey

Variable-length hybrid public keys are too large for fixed-width
state-tree leaves. Two options were considered:

- **A. Hash-of-pubkey.** State tree stores a 32-byte BLAKE3 hash of
  the full hybrid pubkey. Full key is in a separate registry
  subtree keyed by hash.
- **B. Variable-length pubkey field.** State tree leaves grow.
  Simpler, but state-tree size grows ~50× for hybrid pubkeys.

**Decision: A (hash-of-pubkey).** This is the same pattern Bitcoin
uses for P2PKH. It generalizes cleanly to future scheme migrations
without state-tree format changes.

State-tree change is a state-format-version bump. Migration tool:
`tools/migrate_state_to_v2.sh` walks every account, computes the
hash, populates the registry subtree, rewrites the state tree.
Documented in `docs/MIGRATION_GUIDE.md`.

### Registration of new schemes

Adding a new signature/KEM/hash scheme is:

1. New variant on the relevant `Scheme` enum, with a new unique
   discriminant (never reuse a retired one).
2. New `Signer` / `Verifier` (or `Kem`, `HashScheme_`) impl in
   `crypto_agility/`.
3. Round-trip tests, wire-format tests, cross-platform determinism
   tests added.
4. ADR documenting why the scheme is being added.

That's it. No state-tree changes, no protocol changes, no breaking
wire-format changes for existing schemes.

### Refusing unknown schemes

A `Verifier` faced with a scheme prefix it doesn't know returns
`VerifierError::UnknownScheme(scheme_id)`. **Not** "best-effort
verify with the known schemes" or "silently accept." This is the
same discipline as TLS cipher-suite handling — silent fallback is a
known-bad pattern.

### Compatibility window

When introducing a new scheme:
- A `Verifier` accepting both old and new (transition window) is
  configured via `VerifierPolicy { accepted: HashSet<SignatureScheme> }`.
- After a deadline (recorded in the relevant migration ADR), the
  policy is restricted to the new scheme only.
- Reading historical data signed under retired schemes uses a
  separate `LegacyVerifier` interface, audit-logged on every use.

## Consequences

- Every existing call site that touches `ed25519-dalek` or
  `x25519-dalek` directly must be refactored to go through the
  trait. This is the bulk of Phase 1 work in ADR-0006.
- New schemes are a small change. Migrations are not hard forks.
- Wire format gains 1 byte per cryptographic blob for the prefix.
  Negligible.
- Verifiers must explicitly declare what schemes they accept.
  This is more code than "verify this signature" but the trade-off
  is correct — it is what makes migration possible.
- Cannot ship a new scheme without an ADR. This is a feature.

## Alternatives considered

- **No agility layer; introduce hybrid in-place.** Rejected. Every
  future migration becomes a hard fork. We are building this once.
- **TLS-style cipher-suite negotiation (server proposes, client
  accepts).** Rejected for on-chain data. There is no negotiation
  on a block — the producer chooses, the verifier checks. The
  policy mechanism above is sufficient.
- **Type-level scheme selection (each scheme a distinct type).**
  Considered. Adds compile-time safety but loses the flexibility of
  a runtime registry. Verifiers need to handle multiple schemes
  at once; that requires runtime dispatch. Use traits + enum
  discriminants.
- **Multicodec from the IPFS ecosystem directly** (importing the
  full multicodec table). Rejected; pulls in a large registry of
  codecs we will never use. Use the same encoding (LEB128 varint)
  but our own table of scheme IDs.
