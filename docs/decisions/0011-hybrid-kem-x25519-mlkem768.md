# ADR-0011 — Hybrid X25519 + ML-KEM-768 KEM with forward-secret witness encryption

**Status:** proposed (awaiting maintainer-reviewer sign-off + external cryptographer review per ADR-0006).
**Date:** 2026-05-10.
**Companion:** [ADR-0006](0006-post-quantum-cryptography-strategy.md) (PQ strategy locks the schemes), [ADR-0007](0007-cryptographic-agility-architecture.md) (agility wire format), [ADR-0008](0008-blake3-512-for-long-lived-commitments.md) (long-lived hash commitments).

## Context

Per ADR-0006, PSL needs hybrid post-quantum key encapsulation for
**every long-lived encrypted artifact** — the harvest-now-decrypt-
later (HNDL) defense for material that may be captured today and
decrypted after a CRQC exists.

Current state of encrypted artifacts in v0.1.x: **none.** The
encryption surface is greenfield. `ViewKey` (`sequencer/src/compliance.rs`)
is currently an authorization grant (issuer signature over
regulator pubkey + asset filter), not an encryption blob. There is
no X25519 KEM in production use; ADR-0007 reserves the
`KemSchemeId::X25519 = 0x01` discriminant but no implementation exists.

This ADR locks the design for the hybrid KEM that ships with v0.1.x
encryption surfaces (witness encryption, regulator view-key
delivery, travel-rule metadata, future compliance-private payloads)
and that becomes the v0.2 default for any new encrypted artifact.

Because the surface is greenfield, **there is no v1/v2 KEM dual-
version pattern**. Unlike `weights_hash` (PR #7) and `program_hash`
(PR #10), there is no pre-existing 32-byte / 256-bit predecessor
to maintain backwards compatibility with. The ONE format ships
from day one; the format includes an explicit scheme-ID prefix per
ADR-0007 so future scheme migrations don't require a hard fork.

## Decision

### Algorithm choices (locked by ADR-0006, restated)

- **Classical part:** X25519 ECDH (`x25519-dalek`).
- **PQ part:** ML-KEM-768 (FIPS 203, NIST level 3). Same level-3
  conservatism as ML-DSA-65 in ADR-0006.
- **Hybrid combiner:** HKDF-SHA-512 over a transcript binding both
  shared secrets and both ciphertexts (specified below).
- **Symmetric AEAD:** AES-256-GCM. Symmetric primitives not at risk
  from Shor; Grover halves to 128-bit quantum security, still very
  strong.

### Scheme identifier (locked by ADR-0007 wire format)

- `KemSchemeId::HybridX25519MlKem768 = 0x02` — already reserved in
  `crypto_agility::scheme::KemSchemeId`. **Locked.**

### KDF combiner specification

The hybrid shared secret is derived as:

```
hybrid_ss := HKDF-SHA-512(
    salt = fixed("PSL-hybrid-kem-salt-v1"),
    ikm  = transcript,
    info = context_string,
).expand(32)        // 32 bytes for AES-256-GCM
```

where:

```
transcript := varint(0x02)                          // KemSchemeId, locked
           || x25519_shared_secret                   // 32 bytes
           || ml_kem_shared_secret                   // 32 bytes
           || x25519_ephemeral_pubkey                // 32 bytes
           || ml_kem_ciphertext                      // 1088 bytes
```

**Order is locked.** Shared secrets first in classical-then-PQ
order, then binding material in the same classical-then-PQ order.
This matches the IETF
`draft-ietf-tls-hybrid-design` pattern byte-for-byte; deviation
without an explicit security argument is forbidden.

**Why the scheme identifier in the transcript:** binds the derived
key to "this is hybrid X25519 + ML-KEM-768 v1". A future scheme
migration (e.g., upgrading to ML-KEM-1024) cannot have its
derived keys collide with this scheme's outputs even if the
underlying shared-secret bytes happen to coincide.

**Why the context string:** standard cryptographic-domain-
separation hygiene. The same hybrid KEM may be reused in multiple
contexts (witness encryption, view-key delivery, future agent-
channel handshake); each context derives different keys from the
same shared secrets. Context strings used by v0.1.x:

| Context string             | Used by                                      |
| ---                        | ---                                          |
| `PSL-WitnessEnc-v1`        | Compliance-private witness payload encryption |
| `PSL-ViewKey-v1`           | Regulator view-key delivery                  |
| `PSL-TravelRule-v1`        | Travel-rule metadata encryption              |

New contexts are added in their own ADR with the version number
incremented per-context as the underlying format evolves.

### Decapsulation semantics

ML-KEM has **implicit rejection** per FIPS 203 §6.3 (`MLKEM.Decaps`):
on a malformed ciphertext, decapsulation returns a *deterministic
pseudorandom secret* derived from the secret key + ciphertext,
rather than failing visibly. This is a deliberate design choice to
prevent timing oracles — every decapsulation runs in constant time
regardless of ciphertext validity.

**Consequence for PSL code:** the `Kem::decapsulate` interface MUST
NOT return `Result<SharedSecret, ImplicitRejection>` — there is no
such signal to expose. Instead:

- `decapsulate(ciphertext, secret_key) -> SharedSecret` — always
  succeeds at the type level.
- The AEAD authentication step (AES-256-GCM tag verify) is the
  rejection point. If decap returned the pseudorandom fallback
  secret, the AEAD tag will not verify and decryption returns
  `AeadAuthenticationFailed`.
- Code MUST NOT branch on "did decap succeed" because there is no
  such branch.

This is counterintuitive to a reviewer who knows X25519 (where
decap fails visibly on a malformed ciphertext). State it
explicitly in the API doc on `Kem::decapsulate` and on every test
that exercises a malformed-ciphertext path.

### Ephemeral key lifecycle (forward-secrecy invariant)

Witness encryption uses **per-witness ephemeral hybrid keypairs**
to defend against HNDL even if the recipient's long-term key is
later compromised. The lifecycle has 7 explicit steps; **step 6
is load-bearing** for forward secrecy:

```
1. Generate fresh hybrid ephemeral keypair:
   (eph_x25519_pk, eph_x25519_sk) <- X25519::keygen()
   (eph_mlkem_pk,  eph_mlkem_sk)  <- MLKEM768::keygen()

2. Encapsulate to recipient's long-term hybrid public key:
   (x25519_ct, x25519_ss) <- X25519::encap(recipient.x25519_pk, eph_x25519_sk)
   (mlkem_ct,  mlkem_ss)  <- MLKEM768::encap(recipient.mlkem_pk)
   // Note: x25519_ct in the ECDH-as-KEM convention is the eph_x25519_pk
   //       (sender's ephemeral pubkey, shared with recipient).

3. Derive AEAD key via the KDF combiner (above):
   aead_key <- HKDF-SHA-512(salt, transcript, context_string).expand(32)

4. Encrypt witness payload with AES-256-GCM under aead_key:
   nonce <- secure-random(12 bytes)
   aead_ciphertext <- AES-256-GCM::encrypt(aead_key, nonce, plaintext, aad)

5. Serialize the encrypted blob (format below).

6. **Zeroize ephemeral private keys IMMEDIATELY:**
   eph_x25519_sk.zeroize()
   eph_mlkem_sk.zeroize()
   // After this point, even if the recipient's long-term sk is
   // compromised tomorrow, this specific witness cannot be decrypted
   // because the symmetric path through HKDF requires both ML-KEM
   // shared secrets and X25519 shared secrets, and one half is
   // unrecoverable without the now-zeroed ephemeral sk.

7. Zeroize derived AEAD key:
   aead_key.zeroize()
```

Step 6 is the load-bearing forward-secrecy property. Rust's `Drop`
is **not** guaranteed to clear sensitive memory on stack —
optimizers can elide writes whose result is never read. The
`zeroize` crate uses compiler barrier intrinsics (`core::ptr::write_volatile`
+ `compiler_fence`) to force the write to memory. Implementations
MUST use `Zeroize` / `ZeroizeOnDrop` traits for ephemeral private
keys; manual `bytes.fill(0)` is insufficient.

A test asserts the ephemeral private key bytes are zeroed after
`encrypt()` returns (see § Tests).

### Encrypted-blob wire format

```
encrypted_blob := varint(scheme_id)        // KemSchemeId, currently 0x02 = HybridX25519MlKem768
               || varint(format_version)   // u32, currently 1
               || eph_x25519_pubkey        // 32 bytes (= x25519 ciphertext)
               || mlkem_ciphertext         // 1088 bytes per FIPS 203
               || nonce                    // 12 bytes (AES-GCM nonce)
               || aead_ciphertext          // variable; includes 16-byte trailing AEAD tag
```

**Decoder hard-fail rules** (no silent acceptance):

- Unknown `scheme_id` → `KemError::SchemeNotSupported`.
- Unknown `format_version` for the given scheme → `KemError::FormatVersionNotSupported`.
- Length shorter than `1 + 1 + 32 + 1088 + 12 + 16` (= 1150 bytes minimum, empty plaintext)
  → `KemError::TruncatedBlob`.
- AEAD authentication failure → `KemError::AuthenticationFailed`.

The format-version byte exists from day one for forward
compatibility. Currently `version = 1`. A future format-2 (e.g.,
larger nonce, different AEAD) increments this; the decoder
dispatches per-version per-scheme.

### View-key scope

The hybrid KEM applies to:

| Surface                                    | KEM-encrypted? | When |
| ---                                        | ---            | --- |
| **Compliance-private witness payloads**    | Yes            | When the witness encryption surface lands (planned v0.2). |
| **Regulator view-key delivery**            | Yes            | View-key bytes are themselves encrypted under the regulator's long-term hybrid pubkey when delivered out-of-band. |
| **Travel-rule metadata**                   | Yes            | When the travel-rule metadata surface lands. |
| **Agent message contents (Propose body)**  | No             | Message bodies are signed but not encrypted (the protocol assumes mutual-TLS at the transport layer per `agent_sdk::Transport`). |
| **Block bodies / state-tree contents**     | No             | Settlement layer is publicly verifiable by design. |
| **Trace hashes / weights_hash / commitments** | No (these are hashes, not encrypted) | n/a |

The principle: **anything encrypted with a long-lived recipient
key uses hybrid KEM.** Anything signed-only (no encryption) does
not. Anything ephemeral (TLS-layer encryption between agents)
relies on the transport's own crypto.

If a new encryption surface is added, it goes through hybrid KEM
under the same context-string discipline. Borderline cases require
an ADR, like the hash-width principle in MIGRATION_GUIDE § 5.

## Tests (blocking; none optional)

Each must pass before the implementation lands:

1. **Round-trip.** Generate keypair → encrypt witness → decrypt →
   plaintext matches. 1000 random witnesses (proptest).

2. **Forward secrecy.** Generate ephemeral keypair → encrypt witness
   → drop and zeroize ephemeral private key → simulate compromise
   of recipient's long-term private key (just hand it to the test) →
   assert decryption fails. This is the property that makes HNDL
   not a concern.

3. **Implicit rejection.** Take a valid ciphertext, flip a single
   bit in the ML-KEM portion, attempt decryption, assert
   `AuthenticationFailed` (not `DecapFailed` — there is no such
   variant per the implicit-rejection design).

4. **Component swap (transcript binding).** Encrypt message A and
   message B separately. Take the X25519-component bytes from one
   and the ML-KEM-component bytes from the other; concatenate;
   attempt decryption against either A or B. Assert
   `AuthenticationFailed`. The transcript binding (which includes
   both ciphertexts) is what catches this; if it doesn't catch it,
   the binding is wrong.

5. **Wrong-context (domain separation).** Encrypt under
   `context_string = "PSL-WitnessEnc-v1"`, attempt decryption with
   `context_string = "PSL-ViewKey-v1"`. Assert `AuthenticationFailed`.
   Verifies the context-string is correctly threaded into the KDF
   transcript.

6. **Zeroization.** Hold a reference to the ephemeral keypair
   bytes via the `Zeroize` trait's contract. Run encryption. Drop
   the reference. Assert the underlying buffer is zeroed. This
   asserts the load-bearing forward-secrecy primitive.

7. **Format version round-trip.** Encrypt with `format_version=1`,
   decode the blob, assert the format-version byte is correctly
   recovered. Attempt to decode a hand-crafted blob with
   `format_version=99`, assert `FormatVersionNotSupported`.

8. **Cross-platform determinism (CI).** Same plaintext + same
   recipient pubkey → same ciphertext bytes on x86_64 and
   aarch64 GitHub runners (modulo the per-call randomness in
   ephemeral-keypair generation; the test uses a fixed RNG seed
   for reproducibility). Asserts the wire format is byte-stable
   across architectures.

## What we MUST NOT do

1. **Do not invent a novel KDF combiner.** Use HKDF-SHA-512 over
   the IETF-standard transcript pattern. The brief says do not
   invent; this ADR locks that pattern.

2. **Do not omit the scheme_id from the transcript.** Future
   migrations require it for cross-version domain separation.

3. **Do not omit the context_string.** Multiple uses of the same
   KEM otherwise derive the same keys.

4. **Do not skip ephemeral-key zeroization.** Forward secrecy is
   broken without it. The test exists to catch regressions.

5. **Do not assume decap can fail.** The implicit-rejection design
   means the only rejection point is the AEAD auth tag.

6. **Do not introduce a v1/v2 KEM dual-version pattern.** The
   surface is greenfield; no predecessor to preserve compatibility
   with. If the KEM scheme itself is later upgraded (e.g., to
   ML-KEM-1024), it gets a NEW `KemSchemeId` discriminant, not a
   "v2 of HybridX25519MlKem768".

7. **Do not skip the cross-platform determinism CI test.** The
   wire format is what crosses partner boundaries; it must be
   bit-stable across architectures.

8. **Do not declare the implementation complete without external
   cryptographer review** of the KDF combiner, transcript
   construction, and ephemeral-key lifecycle. Per ADR-0006
   acceptance criteria. The HKDF combiner mistake category is the
   one with the worst silent-failure profile in cryptographic
   protocol design.

## Acceptance criteria

The implementation lands when all of:

- [ ] `crypto_agility::kem::HybridX25519MlKem768Kem` impl shipped.
- [ ] `crypto_agility::witness_enc` module with the encrypted-blob
      format above.
- [ ] All 8 tests pass (including cross-platform CI).
- [ ] `Zeroize` / `ZeroizeOnDrop` on every ephemeral private key
      and derived AEAD key.
- [ ] `MIGRATION_GUIDE.md` § "view-key scope" entry references this
      ADR and lists the context strings.
- [ ] `SAFETY.md` updated with the new dependency posture (`pqcrypto-mlkem`,
      `aes-gcm`, `hkdf` if not already present).
- [ ] `LICENSE_REVIEW.md` updated with the new dep licenses.
- [ ] STATUS.md gate 19 row updated to phase-4 progress.
- [ ] External cryptographer review **scheduled** (this ADR ratified
      by the maintainer-reviewer + flagged for the cryptographer's
      attention; their sign-off is required before ⮕✅, but is not
      required to land 🟡 progress).

## Consequences

- v0.2 default for any new encrypted artifact is hybrid X25519 +
  ML-KEM-768 with forward-secret per-message ephemeral keypairs.
- The 1088-byte ML-KEM ciphertext is the dominant wire-format cost
  per encrypted blob; combined with the 32-byte X25519 ephemeral
  pubkey + 12-byte nonce + AEAD overhead, minimum encrypted-blob
  size is ~1150 bytes plus ciphertext payload. For per-witness
  encryption this is acceptable; for per-message encryption (e.g.,
  if we encrypted Propose bodies) it would be meaningful.
- New transitive dependencies: `pqcrypto-mlkem` (PQClean reference
  C), `aes-gcm` (RustCrypto), `hkdf` (RustCrypto), `sha2`
  (RustCrypto). All audited and widely deployed.
- The witness-encryption format ships in v0.1.x with version=1.
  Any future format change requires bumping the version byte and
  shipping a migration tool to re-encrypt long-lived witnesses
  under the new version.

## Alternatives considered and rejected

- **Sign-then-encrypt vs encrypt-then-sign:** PSL doesn't sign
  encrypted blobs (the AEAD tag handles authenticity). The signer
  authority is the issuer (witness creator), and integrity of the
  recipient → ciphertext binding is via AEAD AAD. Standard pattern.

- **Per-recipient symmetric key (no KEM at all):** would require
  pre-shared keys with every regulator, which is operationally
  infeasible. KEM is the right tool.

- **Pure post-quantum (no classical hedge):** rejected by ADR-0006.

- **Different KDF (e.g., concatenating shared secrets directly
  with no HKDF):** rejected — direct concatenation is not a
  cryptographic combiner; HKDF is the standard for shared-secret
  combination per RFC 5869 and matches what TLS 1.3 hybrid drafts
  use.

- **Different AEAD (ChaCha20-Poly1305):** considered. AES-256-GCM
  has hardware acceleration on essentially all production hosts
  (AES-NI, ARM AES extensions); ChaCha20-Poly1305 has neither but
  is faster on hosts without AES acceleration. PSL's deployment
  target is server-class hardware where AES is faster. Re-evaluate
  if PSL ever targets WASM-in-browser or low-end embedded.

- **Bigger ML-KEM (ML-KEM-1024, NIST level 5):** considered. Same
  level-3-conservatism rationale as ML-KEM-768 vs ML-KEM-512 in
  ADR-0006. Level 5 buys margin we don't need at the cost of ~50%
  larger ciphertexts.
