# ADR-0011 — Hybrid X25519 + ML-KEM-768 KEM with forward-secret witness encryption

**Status:** accepted by maintainer-reviewer; awaiting external cryptographer review per ADR-0006.
**Date:** 2026-05-10.
**Companion:** [ADR-0006](0006-post-quantum-cryptography-strategy.md) (PQ strategy locks the schemes), [ADR-0007](0007-cryptographic-agility-architecture.md) (agility wire format), [ADR-0008](0008-blake3-512-for-long-lived-commitments.md) (long-lived hash commitments).

## Refinements applied during review (2026-05-10)

Maintainer-reviewer feedback applied before implementation cut. Recorded
inline so the reasoning travels with the ADR rather than living only in
PR-thread history.

### Round 2 (skeleton review)

7. **`decapsulate` is truly infallible at the type level.** Original
   spec said "Result only for parsing errors." Reviewer pushed back:
   any `Result` variant gives reviewers a hook to write match arms
   for failures that don't exist semantically, defeating the
   implicit-rejection design. Resolved: trait uses **typed
   parameters** (`Ciphertext`, `SecretKey`, `PublicKey` associated
   types). Constructor-validation lives on the type
   (`Ciphertext::from_bytes(...) -> Result<Self, KemError>`); once
   you hold a `Ciphertext`, decapsulate is total: `fn
   decapsulate(&self, ct: &Self::Ciphertext, sk: &Self::SecretKey)
   -> SharedSecret`. No `Result`, no `match` for failure modes that
   don't exist.

8. **Secret-key vs public-key newtype split.** `HybridKemKeypair`
   originally had raw `[u8; 32]` and `Vec<u8>` fields with
   `Zeroize/ZeroizeOnDrop` derived on the whole struct. Reviewer's
   point (per #43-style structural-invariant work): the type
   system should enforce "secret material zeroizes; public
   material does not" rather than relying on a struct-level derive
   to do both. Resolved: secret-side gets dedicated types
   (`EphemeralX25519SecretKey`, `EphemeralMlKemSecretKey`,
   `RecipientX25519SecretKey`, `RecipientMlKemSecretKey`) each with
   `Zeroize/ZeroizeOnDrop`. Public-side gets `X25519PublicKey` /
   `MlKemPublicKey` with `Clone` only. Mixing is a type error.

9. **`ContextString` is a typed enum, not raw `&[u8]` constants.**
   Originally `pub const CONTEXT_WITNESS_ENC: &[u8] = b"PSL-WitnessEnc-v1";`.
   Reviewer's point: the "borderline contexts require an ADR" rule
   is enforceable in code if contexts are an enum (adding a variant
   requires touching this file → natural speed bump for the ADR
   discipline; a string literal at a call site doesn't carry the
   same friction). Resolved: `pub enum ContextString { WitnessEncV1,
   ViewKeyV1, TravelRuleV1 }` with `as_bytes(&self) -> &'static
   [u8]`. `encrypt` / `decrypt` take `ContextString`, not `&[u8]`.

10. **`build_kem_transcript` is centralized.** Originally the
    transcript would have been inlined into encap and decap
    separately — recipe for the two paths to drift. Resolved:
    single `pub(crate) fn build_kem_transcript(...) -> Vec<u8>`
    function (skeleton signature locked here) called from both
    sides. Byte order of the transcript is documented inline in
    the function body matching this ADR exactly.

### Round 1 (initial review)

1. **`format_version` byte removed.** Originally proposed as a separate
   axis from `scheme_id`. Reviewer argued one axis is cleaner: any
   wire-format change effectively is a ciphersuite change for parser
   purposes, and the two-axis encoding adds confusion ("scheme_id=0x02,
   format_version=2 means it's actually a different ciphersuite now")
   without saving discriminant space. Resolved to single-axis with an
   explicit "any wire-format change → new scheme_id" rule. See
   § "Versioning rule (locked)" below.
2. **`scheme_id` varint single-byte constraint** documented explicitly.
   Downstream parsers may assume `eph_x25519_pubkey` starts at byte 1;
   a varint shift would silently break them. Single-byte encoding
   covers ~125 future scheme migrations before the constraint binds.
3. **Test #6 split into 6(a) ephemeral private keys + 6(b) derived
   AEAD key.** Reviewer's point: the AEAD key is the symmetric secret;
   if it leaks the witness is decryptable forever regardless of
   ephemeral-key zeroization. Both buffers must be zeroized and tested.
4. **Edge-size payload tests added** (sizes 0, 1, 16, 1024, 1MB) per
   reviewer's "AEAD libraries occasionally have off-by-one issues at
   boundary sizes" observation.
5. **Cross-platform determinism CI clarified** — encap is intentionally
   non-deterministic (uses fresh randomness per FIPS 203); test uses a
   seeded-RNG fixture for reproducibility. Decap + KDF derivation +
   ciphertext bytes must be byte-identical given the same fixtured
   inputs. See test #8.
6. **Historical context strings stay active forever** for decryption
   even after a context-version bump. Old encrypted material is not
   re-encrypted.

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

**Historical context strings stay active forever.** When a v2 of
any context launches (e.g., `PSL-WitnessEnc-v2`), the v1 context
string remains a legitimate decryption path for as long as
historical encrypted material exists. Old encrypted witnesses are
not re-encrypted; they need to remain decryptable for the lifetime
of the chain. The decryption API exposes both versions as
first-class paths, not as "v1 is deprecated, only v2 is supported."
A future migration may add `PSL-WitnessEnc-v2` to the table; the
v1 row stays.

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
               || eph_x25519_pubkey        // 32 bytes (= x25519 ciphertext)
               || mlkem_ciphertext         // 1088 bytes per FIPS 203
               || nonce                    // 12 bytes (AES-GCM nonce)
               || aead_ciphertext          // variable; includes 16-byte trailing AEAD tag
```

**Decoder hard-fail rules** (no silent acceptance):

- Unknown `scheme_id` → `KemError::SchemeNotSupported`.
- Length shorter than `1 + 32 + 1088 + 12 + 16` (= 1149 bytes
  minimum, empty plaintext) → `KemError::TruncatedBlob`.
- AEAD authentication failure → `KemError::AuthenticationFailed`.

#### Versioning rule (locked)

There is **one discriminant axis**: `scheme_id`. Any change to how
this blob is constructed, parsed, or interpreted gets a **new
`scheme_id` discriminant** per ADR-0007.

Examples that get a new `scheme_id`:
- Cryptographic scheme change (e.g., upgrade to ML-KEM-1024).
- AEAD change (e.g., AES-256-GCM → ChaCha20-Poly1305).
- Nonce-size change.
- Adding a header field.
- Changing the canonical encoding of any existing field.

Examples that do **not** get a new `scheme_id`:
- Internal refactor that produces byte-identical output.
- Bugfix that preserves wire format.
- Documentation updates.

This is a single-axis design (no separate `format_version` byte).
A separate format-version axis was considered and rejected: it
adds confusion ("scheme_id=0x02, format_version=2 means it's
actually a different ciphersuite now") without saving discriminant
space (the varint scheme_id has 127 single-byte slots; v0.1.x uses
2). Auditors and verifiers track one number, not two.

#### Varint encoding constraint

`scheme_id` is encoded as an LEB128 varint per ADR-0007. **For the
foreseeable future it MUST fit in one byte** (value ≤ 127).
Single-byte encoding keeps fixed-offset assumptions in test
fixtures and downstream parsers safe. If a future migration
needs scheme_id > 127, that requires:

1. An ADR superseding this one (because the wire-format invariant
   changes).
2. An audit of every downstream parser for fixed-offset
   assumptions about where `eph_x25519_pubkey` starts (currently
   byte 1 because the leading varint is 1 byte; it would shift to
   byte 2 for varints in the 128-16383 range).
3. A new `scheme_id` discriminant for the post-shift format.

PSL is currently at 2 of 127 slots. The constraint is
non-binding for ~125 future scheme migrations; calling it out so
nobody silently exhausts the slots.

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

6. **Zeroization (both ephemeral private keys AND derived AEAD
   key).** Two assertions, not one:
   (a) Hold a reference to the ephemeral keypair bytes via the
       `Zeroize` trait's contract. Run encryption. Drop the
       reference. Assert the underlying buffer is zeroed.
   (b) Hold a reference to the derived AEAD key bytes. Run
       encryption (which performs AEAD encrypt + zeroize). Assert
       the AEAD key buffer is zeroed.

   Both are load-bearing for forward secrecy. Ephemeral private
   keys gate the asymmetric path; the AEAD key IS the symmetric
   secret — if it leaks (e.g., dumped from process memory), the
   witness is decryptable forever, regardless of what happens to
   the asymmetric keys.

7. **Edge-size payloads.** AEAD libraries occasionally have
   off-by-one issues at boundary plaintext sizes. Round-trip
   asserts pass for plaintexts of size **0, 1, 16 (AES block
   size), 1024, and 1MB**. AES-256-GCM's documented maximum
   plaintext is ~64GB; PSL won't approach that, but exercising
   chunking behavior at 1MB catches AEAD-internal regressions.

8. **Cross-platform determinism (CI).** Be explicit about what's
   deterministic vs not:
   - **Encapsulation generates fresh randomness** (intentionally
     non-deterministic by FIPS 203 design). For test
     reproducibility, the cross-platform CI test uses a *seeded
     RNG fixture* so the encap output is identical across
     architectures.
   - **Decapsulation given the same `(eph_pk, ml_kem_ct, sk)` MUST
     produce the same shared secret** on every conformant
     architecture. This is byte-exact and tested without RNG
     fixturing (decap takes no randomness).
   - **Derived AEAD key from the same `(shared_secrets, transcript)`
     MUST be byte-identical** across architectures (HKDF-SHA-512
     is deterministic by construction).
   - **Final ciphertext byte-equal** on x86_64 and aarch64 GitHub
     runners using the seeded-RNG fixture. Wire format must be
     bit-stable across architectures (this is what crosses partner
     boundaries).

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

## Implementation commit order

Per the engineer-reviewer's preference for surgical-review surfaces.
Each commit is independently reviewable; if a later commit needs to
revise a decision in this ADR, the change goes back to the spec
*before* the impl continues, not silently in the impl commit.

1. **ADR + KEM crate skeleton** (this commit, plus type/trait stubs).
   Reviewer can see the spec and the type signatures land first.
2. **KEM crate impl + KEM-only tests.** `HybridX25519MlKem768Kem`
   implements the `Kem` trait. Tests #1, #2, #3, #4, #6(a), #8.
3. **Witness encryption module + WE-specific tests.** Wraps the KEM
   plus AEAD encrypt/decrypt with the wire format above. Tests #5,
   #6(b), #7. Updates `MIGRATION_GUIDE.md` view-key scope.
4. **Agent-layer wire-format cascade** (deferred from PR #10 per
   the bundle-once discipline): `Propose.program_hash` widening
   `[u8; 32]` → `ProgramHash`, `agent_sdk` HashMap re-keying,
   `ProposalHash` newtype, all `Propose::sign` call sites. Updates
   `agent_protocol`, `agent_sdk`, all tests.
5. **Cross-platform CI matrix.** Adds aarch64 GitHub runner job to
   `.github/workflows/ci.yml` for the byte-exact cross-platform
   determinism property (test #8). Without this commit the
   property is asserted only on x86_64; with it, the matrix
   verifies it across both target architectures.

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
