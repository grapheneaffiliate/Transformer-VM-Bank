# ADR-0006 — Post-quantum cryptography strategy

**Status:** accepted (engineering proceeds; final ratification on first external cryptographer review).
**Date:** 2026-05-09.
**Deciders:** PSL maintainers.
**Companion ADRs:** ADR-0007 (cryptographic agility architecture), ADR-0008 (BLAKE3-512 for long-lived commitments).

## Context

NIST finalized the post-quantum cryptography standards in 2024 (FIPS
203 ML-KEM, FIPS 204 ML-DSA, FIPS 205 SLH-DSA, FIPS 206 FN-DSA).
Reference implementations exist as audited Rust crates. US federal
agencies have migration deadlines around 2030. SWIFT, BIS, and
major financial-infrastructure consortia are piloting hybrid
schemes today.

PSL targets institutional pilots — banks, central banks, RWA
issuers, regulated AI labs — for which quantum-resistance will be a
procurement requirement within 2-3 years. A settlement chain that
ships v0.1.0 with pure ed25519 has a known-bad posture for the
target buyer audience.

Two categories of quantum threat matter for PSL:

- **Shor's algorithm against discrete-log signatures.** A
  cryptographically-relevant quantum computer (CRQC) breaks ed25519
  in polynomial time. Every signature ever made becomes forgeable.
  Every public key on chain reveals its private key. Without
  mitigation, the chain becomes unrecoverable.
- **Grover's algorithm against hashes.** Quadratic speedup on
  preimage attacks. BLAKE3-256 effectively becomes 128-bit secure
  under quantum. Still not practical to attack at that level, but
  the margin is gone. For multi-decade durability, hash output sizes
  need to double for **long-lived commitments only**.

Exposure surface:

| Surface                                                | Primitive | Q-risk | Severity |
| ---                                                    | ---       | ---    | ---      |
| Block signatures (sequencer)                           | ed25519   | Shor → forgery        | **Critical** |
| Agent identity signatures                              | ed25519   | Shor → identity theft | **Critical** |
| Agent message signatures (propose/accept/exec/dispute) | ed25519   | Shor → contract forgery | **Critical** |
| View-key encryption (compliance / privacy)             | X25519 KEM | Shor → decrypt past witnesses (HNDL) | **High** |
| Trace hashes                                           | BLAKE3-256 | Grover | Low |
| MPT roots                                              | BLAKE3-256 | Grover | Low |
| Block-header hashes                                    | BLAKE3-256 | Grover | Low |
| `weights_hash` (committed in trace, irrevocable)       | BLAKE3-256 | Grover | **Medium-high** (irrevocable commitment) |

The harvest-now-decrypt-later (HNDL) threat is **active today** for
the High-severity surfaces: an adversary capturing encrypted
witnesses today can decrypt them once a CRQC exists.

## Decision

### Core strategy

**Hybrid before pure post-quantum.** Combine classical (ed25519,
X25519) with post-quantum (ML-DSA, ML-KEM) such that an attack must
break **both** to succeed. This is what Cloudflare, Google, Signal,
and Apple iMessage shipped in 2024-2025. It hedges against a flaw
being discovered in either the classical or the PQ scheme. Classical
remains the security floor until a CRQC exists; PQ becomes the floor
once one does.

### Algorithm choices

| Role                                   | Scheme                                     | Why |
| ---                                    | ---                                        | --- |
| Signatures                             | **ed25519 + ML-DSA-65** (hybrid concatenation) | NIST level 3, conservative middle. Larger margin than ML-DSA-44; significantly cheaper than ML-DSA-87. |
| Key encapsulation                      | **X25519 + ML-KEM-768** (hybrid HKDF combiner) | NIST level 3 to match. |
| Symmetric AEAD                         | AES-256-GCM (no change)                    | Symmetric primitives not at risk from Shor; Grover halves effective security; AES-256 → 128-bit quantum security, still very strong. |
| Hashes — short-lived                   | BLAKE3-256 (no change)                     | Trace hashes, MPT roots, block headers. Forward path documented; not urgent. |
| Hashes — **long-lived commitments**    | **BLAKE3-512** (per ADR-0008)              | `weights_hash` and long-lived contract hashes. Once committed, cannot be retroactively re-hashed; multi-decade durability matters. |

### Schemes explicitly NOT chosen (and why)

- **SLH-DSA (SPHINCS+)** — hash-based, would be the most
  conservative. Signatures 7-30 KB, signing slow (millisecond on
  consumer hardware). Reserved as a future option for
  validator-only signatures via cryptographic agility (ADR-0007).
- **FN-DSA (Falcon)** — smallest signatures, but reference
  implementations require constant-time floating-point arithmetic.
  **Architecturally incompatible with PSL's hard rule against
  floating-point on the verifier path.** Excluded permanently from
  PSL by this ADR.
- **Pure post-quantum without classical hedge.** Rejected for the
  forecast horizon (out to 2030 at minimum). The classical part is
  effectively-free additional security; giving it up gains nothing.

### Hybrid composition

**Hybrid signature** uses concatenation, not XOR:

```
hybrid_sig(msg) = (ed25519_sig(msg), ml_dsa_sig(msg))
```

Verification accepts iff **both** signatures verify. This is the
NIST SP 800-227 (draft) and IETF
`draft-ietf-pquip-hybrid-signature-spectrums` "concatenation"
combiner — simpler to implement, simpler to audit, no novel
combinator cryptanalysis surprises.

**Hybrid KEM** uses HKDF over the concatenated shared secrets and a
transcript binding both ciphertexts:

```
hybrid_ss = HKDF-SHA-512(salt=fixed,
                        ikm = x25519_ss || ml_kem_ss,
                        info = "PSL-hybrid-kem-v1" || transcript_hash)
```

`transcript_hash = BLAKE3(x25519_ct || ml_kem_ct)` binds both
ciphertexts so neither component can be substituted independently.
This is the IETF `draft-ietf-tls-hybrid-design` pattern.

### Wire format

Adopt a multi-codec-style **varint scheme prefix** on every key,
every signature, every encrypted blob (ADR-0007 details). Verifiers
parse the prefix, dispatch to the appropriate `Verifier` impl, and
**refuse unknown schemes with an explicit error** (not silent
acceptance).

### Forward secrecy for witness encryption

Per-witness ephemeral hybrid keypair:

1. Generate ephemeral X25519 + ML-KEM keypair when witness is
   created.
2. Encrypt witness payload under the hybrid KEM derived from
   ephemeral *public* keys + recipient's long-term public keys.
3. Store ephemeral public keys with the ciphertext.
4. Destroy ephemeral private keys via `zeroize` immediately after
   encryption.

Even if a CRQC later compromises the recipient's long-term key,
past witnesses remain protected because the ephemeral private keys
are gone. Standard ECIES-with-ephemeral pattern, applied to the
hybrid case. This is the load-bearing HNDL defense.

## Migration phases

Six phases, each independently testable and committable:

| Phase | Scope                                                          |
| ---   | ---                                                            |
| 1     | Cryptographic agility infrastructure (`crypto_agility/` crate, traits, varint codec, refactor existing call sites through traits) — bulk plumbing work, no behavior change |
| 2     | BLAKE3-512 for long-lived commitments (`weights_hash`, long-lived contract hashes); bump trace-hash format version |
| 3     | Hybrid signatures (ed25519 + ML-DSA-65); transition window accepts ed25519 OR hybrid; new sequencer key in hybrid form |
| 4     | Hybrid KEM (X25519 + ML-KEM-768) for view-keys + witness encryption; per-witness ephemeral keypairs |
| 5     | Agent layer migration — agent identities + 5 message types in hybrid form by default |
| 6     | Hybrid required, ed25519-only deprecated; migration tool for existing accounts; deprecation warnings |

Migration deadline (transition → hybrid-only): suggested 12 months
from v0.1.0 release; institutional partners may require shorter via
contract.

## Acceptance criteria for "PQ migration done"

- [ ] All sequencer block signatures hybrid.
- [ ] All agent identity / proposal / accept / dispute signatures hybrid.
- [ ] All view-key encryptions hybrid.
- [ ] Forward-secret ephemeral keypairs on witness encryption.
- [ ] All `weights_hash` and long-lived contract hashes BLAKE3-512.
- [ ] `crypto_agility/` crate with `Scheme/Signer/Verifier/Kem/HashScheme` + varint codec.
- [ ] State tree stores hash-of-pubkey, full pubkeys in registry subtree.
- [ ] Ten testing requirements pass (round-trip, hybrid combiner correctness, wire-format, cross-platform determinism, forward secrecy, migration, agility, performance, Lean theorem, adversarial).
- [ ] Lean theorem proved: hybrid signature is EUF-CMA secure if either component is EUF-CMA secure.
- [ ] Cross-platform determinism CI passes for all hybrid operations on x86_64 + aarch64.
- [ ] **External cryptographer review** of the hybrid combiner, wire format, and agility layer (1-2 week consulting engagement, separate from full audit gate 17).
- [ ] STATUS.md gate row added with ✅.
- [ ] CHANGELOG entry per migration commit.

## Determinism invariant

Every PQ scheme implementation is **deterministic-by-construction**
or uses deterministic randomness derived from `(message || private_key)`.
ML-DSA in deterministic mode is the default. ML-KEM is naturally
deterministic on decapsulation. Per-scheme determinism statement
documented in the Scheme implementation file.

## Consequences

- Wire formats grow: hybrid signatures are ~3.4 KB vs ed25519's 64
  B — about 53× larger. For per-block signatures (one per block)
  negligible. For per-message agent signatures, meaningful but
  acceptable.
- State-tree schema change: pubkeys stored as hash; full keys in a
  registry subtree. Bumps state-format version; migration tool
  required. ADR-0007 details.
- Cannot pivot to pure-PQ without explicit ADR superseding this one.
- Cannot introduce Falcon (FN-DSA) without superseding the
  fp-incompatibility decision recorded here.
- v0.1.0 SDK semver contract: hybrid is the new default but
  `KeyPair::ed25519_legacy()` constructor remains in the API with a
  deprecation warning. Removal requires a major version bump.

## Alternatives considered

- **Wait for the audit (gate 17) before starting PQ work.** Rejected;
  audit and PQ are independent workstreams and v0.2 needs PQ
  regardless of audit outcome.
- **Deploy pure post-quantum without classical hedge.** Rejected;
  the classical half is free security and there is non-trivial
  uncertainty about lattice-based PQ schemes' long-term durability.
- **SLH-DSA for everything.** Rejected for cost; signatures 7-30
  KB and millisecond signing time are not workable for per-message
  agent signatures.
- **Defer PQ to v0.2 entirely.** Rejected; institutional partners
  will require it as a procurement requirement; better to ship it
  in the post-v0.1.0 / pre-v0.2 window so v0.2 ships with PQ as
  default.
