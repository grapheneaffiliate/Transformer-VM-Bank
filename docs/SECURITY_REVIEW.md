# PSL Security Review — Audit Package

**Audience:** external security auditor (Trail of Bits / Zellic /
OtterSec), institutional partner technical due diligence, regulator.

**Status:** prepared 2026-05-09 against commit `91ed18a`. Update when
the audit kicks off.

This document is the auditor's entry point. It pins the scope,
inventories the trust boundaries and invariants, lists the attack
surfaces by layer, and points to the load-bearing test artifacts.

---

## 1. Scope

In scope for the audit:

- **Layer 1** — `ternary_vm/` pure-integer execution kernel. The
  load-bearing claim of the agent execution layer.
- **Layer 2** — `agent_contracts/` standard contract library + the
  `TernaryProgram` trait + `program_hash` commitment.
- **Layer 3** — `agent_wallet/` (SLIP-0010 derivation, signed
  `KeyPolicy`, monotonic `RevocationSet`, `KeyRotation`).
- **Layer 4** — `agent_protocol/` (`AgentRegistration`, 5 wire
  messages, `ProposalLog` state machine, `Reputation`,
  dispute resolution).
- **Layer 5** — `agent_sdk/` (high-level agent runtime, on-chain
  view trait, transport trait).

Out of scope (audit separately):

- `consensus/` — ABCI + CometBFT integration (gate 9 deferred).
- Network transport layer (mutual-TLS HTTPS) — caller of the SDK
  wires this; audit when the chosen TLS stack is finalized.
- UniFFI bindings to Swift / Kotlin / Python / JS (separate crate,
  emitted via `uniffi-bindgen`).

## 2. Trust boundaries

```
┌────────────────────── User / Agent operator (trusted) ──────────────┐
│                                                                       │
│   ┌──────────── parent SigningKey (cold / HSM in production) ─────┐  │
│   │                                                                 │  │
│   │   PolicyEnvelope ── signs ──▶  child SigningKey (hot)          │  │
│   │                                                                 │  │
│   │   Revocation     ── signs ──▶  RevocationSet (monotonic)       │  │
│   │                                                                 │  │
│   │   KeyRotation    ── signs ──▶  (old → new) child mapping       │  │
│   │                                                                 │  │
│   └─────────────────────────────────────────────────────────────────┘  │
│                                                                       │
└─────────────────────── trusted ─── untrusted ──────────────────────────
                                  │
                                  ▼
┌──────────────── Mempool / Sequencer / Network ─── adversarial ────────┐
│                                                                       │
│   * verifies child sig over tx                                        │
│   * verifies parent sig over policy envelope                          │
│   * checks revocation set                                             │
│   * runs SpendingTracker.admit                                        │
│   * runs canonical TernaryProgram (deterministic, integer)            │
│   * commits to MPT                                                    │
└────────────────────────────────────────────────────────────────────────┘
```

The single trust surface for any execution outcome is the
`weights_hash` of the `TernaryProgram` plus the canonical input
encoding. Same input + same weights_hash → bit-identical output on
**any** conformant integer-arithmetic verifier (x86_64, aarch64,
secure enclave, FPGA). This is the load-bearing property of the whole
architecture (`docs/ARCHITECTURE.md § 0.8`).

## 3. Invariants (tested + intended)

| # | Invariant | Where tested |
|---|-----------|--------------|
| I1 | Same input → same output for any `TernaryProgram` on any conformant integer-arithmetic host. | `ternary_vm` exhaustive tests (byte_add 131072/131072, byte_sub 131072/131072, freeze_apply 512/512); `agent_contracts` 100-witness u128 sweeps. |
| I2 | Forward-pass kernel never panics on production inputs. All overflow-prone ops use `checked_add` / `checked_sub`. | `ternary_vm::network::tests::forward_overflow_errors_does_not_panic`. |
| I3 | Weights file integrity covered by BLAKE3 trailing digest; tampered byte → load fails. | `ternary_vm::weights::tests::flipped_byte_fails_integrity_check`. |
| I4 | SLIP-0010 ed25519 derivation matches the spec test vectors. | `agent_wallet::slip10::tests::slip10_test_vector_1_master`/`first_child`. |
| I5 | Revocation is monotonic — once a pubkey is revoked, no API path un-revokes it. | `agent_wallet::revocation::tests::revocation_is_monotonic`. |
| I6 | Spending policy is conservative — `try_spend` admits a transaction iff it stays within the rolling window cap. | `agent_wallet::policy::tests::spend_under_cap_admitted`. |
| I7 | Tampered policy envelope / registration / message rejected by signature check. | tampered_* tests across `policy.rs`, `registry.rs`, `revocation.rs`, `rotation.rs`, `message.rs`. |
| I8 | `ProposalLog` rejects illegal transitions with named (from-state, event) and does not corrupt state on the rejected branch. | `state_machine::tests::*`. |
| I9 | `resolve_dispute` slashes on output mismatch and dismisses on match — both paths drive deterministic re-execution. | `dispute::tests::slash_executor_when_executor_lied` + `dismiss_dispute_when_executor_correct`. |
| I10 | No SDK API path leaks private key bytes. Private keys held in `Zeroizing<…>` and consumed only by `ed25519-dalek::Signer`. | manual review (no public accessor in `slip10.rs::Ed25519MasterKey` / `Ed25519ChildKey`). |

## 4. Attack surface inventory (per layer)

### Layer 1 — `ternary_vm`

| Surface | Threat | Mitigation |
|---|---|---|
| `unpack_weights` | Malformed weights file (truncated / overlong / inconsistent CSR pointers) crashes verifier. | All cursors are bounds-checked → `OutputDecode` error. BLAKE3 trailing digest verifies integrity. |
| `forward()` | Adversary-controlled input causes overflow in i64 accumulator. | `checked_add` / `checked_sub` everywhere; `Overflow{layer,row}` returned, no panic. |
| Network construction | Malicious primitive constructor produces network that gives wrong arithmetic. | Each primitive's network is exhaustively or large-randomly verified against arithmetic ground truth (`exhaustive_byte_add.rs`, `exhaustive_byte_sub.rs`, `exhaustive_freeze_apply.rs`, plus 1000-witness sweeps for the others). |

### Layer 2 — `agent_contracts`

| Surface | Threat | Mitigation |
|---|---|---|
| Contract `run()` | Arithmetic does not match contract's declared semantics (e.g. transfer leaks funds). | Each contract has random-witness tests against a `checked_add`/`checked_sub` ground-truth function (`transfer.rs`, `swap.rs`, `escrow.rs`, `conditional.rs`). |
| Precondition failure | Caller assumes "no-op zeros" but contract returns wrong-shape bytes. | Every contract returns `vec![0u8; OUTPUT_LEN]` on guard fail; output length is uniform in the success branch too; tests cover both. |
| `program_hash` collisions | Two contracts produce the same `program_hash` and a verifier confuses them. | `program_hash` mixes the contract's `name` + every embedded sub-network's `weights_hash` via BLAKE3. Names are `&'static str` set per contract; manual audit ensures no two contracts share a name. |

### Layer 3 — `agent_wallet`

| Surface | Threat | Mitigation |
|---|---|---|
| `Ed25519MasterKey::from_seed` | Seed of unsupported length (e.g. attacker provides 8-byte seed) silently weakens entropy. | `BadSeedLength` error for any length other than 16 / 32 / 64. |
| Non-hardened derivation | Attacker convinces caller to use index `< 0x80000000` which on ed25519 SLIP-0010 is undefined / unsafe. | `NotHardened` error returned from `derive_child`; tested. |
| Policy envelope tampering | Adversary modifies the cap upward in transit. | Parent's signature is over canonical bytes; tampering invalidates it — `tampered_policy_rejected`. |
| Revocation race | Two revocation messages arrive out of order; later one un-revokes. | Monotonic insert: `RevocationSet::insert` is no-op when pubkey already present. No public un-revoke API. |
| Spending policy bypass | Window edge case — fast burst of spends just before window slides. | Tracker stores per-spend `(timestamp, amount)`; no aggregation that hides individual spends. Edge-case test recommended in audit. |

### Layer 4 — `agent_protocol`

| Surface | Threat | Mitigation |
|---|---|---|
| Cross-pubkey forgery | Attacker signs a message claiming to be from another pubkey. | Each message stores the signer's pubkey explicitly; signature is over canonical body which includes that pubkey. Any tamper fails verification (tested). |
| Replay | Attacker re-broadcasts an already-handled `Propose`. | `proposal_hash` is content-addressed; `ProposalLog::record_propose` is idempotent on the same hash. |
| Out-of-order delivery | `Execute` arrives before `Accept` and corrupts state. | `apply_*` returns `IllegalTransition` and **restores the prior state** (re-inserts the removed entry on the error branch). |
| Malicious counter-proposal | `B` counter-proposes with terms that look acceptable to scripts but include hidden harm. | Counter-proposal is a fresh signed message; recipient re-runs full `decide()` policy. The original proposal is preserved in state for dispute reference. |

### Layer 5 — `agent_sdk`

| Surface | Threat | Mitigation |
|---|---|---|
| Transport injection | Hostile transport delivers messages claiming to be from any pubkey. | SDK only trusts the signature, never the transport's claim of "from". Unsigned messages reach no decision path. |
| Untrusted on-chain view | Hostile `OnChainView` claims an agent is registered when it isn't. | Reputation / registration are advisory; the SDK's safety doesn't depend on them — the underlying signature checks are authoritative. |
| Policy hook misuse | `decide()` callback that always-accepts admits hostile counterparties. | Reference agents in `examples/` show two policies; production callers must implement an explicit allowlist or reputation check. Documented in the `AgentSdk::handle_propose` doc. |

## 5. Audit deliverables (recommended scope)

For an auditor:

1. **Read-through** of all 5 layer crates with focus on
   `unsafe` blocks (none currently — check), `unwrap`/`expect` in
   non-test code, and the listed invariants in § 3.
2. **Fuzz**:
   - `ternary_vm::weights::unpack_weights` against
     arbitrary-length inputs. Corpus seed: a known-good packed
     weights file.
   - Each contract's `run()` against arbitrary `&[u8]` inputs.
3. **Property tests** (extending the existing `proptest` suite):
   - SLIP-0010 deeper-path determinism (10+ levels deep).
   - Policy: window cap holds under all interleavings of
     (admit, evict).
   - Revocation: monotonicity holds under shuffled insert order.
4. **Cross-platform determinism**: build `ternary_vm` on
   x86_64-linux, aarch64-darwin, aarch64-linux, run a fixed
   10000-witness corpus on each; assert byte-identical outputs.
   This is the load-bearing property of the architecture.
5. **Cryptographic hygiene**: no homemade crypto. Confirm
   `ed25519-dalek` (audited), `blake3`, `sha2`, `hmac` are the
   only sources of cryptographic primitives. Confirm `zeroize` is
   used on every private-key storage site.

## 6. Pinned dependencies + SBOM

Run `tools/sbom.sh > sbom.txt` to emit a SBOM via `cargo tree
--workspace --edges normal`. `cargo audit` runs in CI via
`tools/audit.sh` (added gate 17 prep). All workspace dependencies
are pinned via `Cargo.lock` in tree — no floating versions.

## 7. Out-of-scope reminders

- The transformer-VM specialized models (Phase 1 path) and the
  PyTorch+MKL trace contract continue to coexist but are tagged
  legacy (see `docs/ARCHITECTURE.md § 0.3` caveat). The agent
  execution layer (Phase 2) is the audit target.
- Monetary / regulatory frameworks (KYC, travel rule) live in
  `compliance/` and are out of scope for this audit.

## 8. Adversary model

PSL must remain safe under each of the following adversaries acting
independently:

### Passive observer
- **Capability:** read-only access to the public block stream,
  registry subtree, and reputation subtree.
- **Concern:** privacy / linkability of agent activity.
- **Mitigation:** none at the trace layer (compliance-aware view-key
  encryption is a separate, opt-in layer in `compliance/`). PSL does
  not promise unlinkability against a passive observer; this is
  documented in `docs/SOVEREIGN_MODE_TRUST.md`.

### Active network attacker
- **Capability:** rewrite / replay / drop network messages between
  the SDK and the sequencer, and between two agents.
- **Concern:** message forgery, replay, downgrade.
- **Mitigation:** every wire message is signed. `proposal_hash` is
  content-addressed → replay collapses. The SDK trusts only signed
  bodies, never transport metadata.

### Byzantine sequencer
- **Capability:** the sovereign sequencer's signing key is
  compromised or the operator goes rogue.
- **Concern:** forged blocks, hidden state changes, censorship,
  fork attempts.
- **Mitigation:** followers re-execute every block via the canonical
  `TernaryProgram`. Any state-root mismatch is immediately public
  (gate 4). Hidden forks are detectable by any follower comparing
  signed headers. **Real-time prevention requires gate 9 BFT**;
  sovereign-mode detection is reactive, not preventative — see
  `docs/SOVEREIGN_MODE_TRUST.md`.

### Byzantine agent (executor)
- **Capability:** an executor with a valid registration signs an
  `Execute` message claiming a wrong contract output.
- **Concern:** counterparty believes the wrong state has been
  reached.
- **Mitigation:** dispute resolution. The disputer files a `Dispute`
  pointing at the discrepancy; the sequencer re-executes via the
  canonical engine and slashes the executor on mismatch (covered
  by `agent_protocol/tests/adversarial_dispute.rs`).

### Byzantine agent (disputer)
- **Capability:** an agent files frivolous / sybil disputes against
  honest executors.
- **Concern:** griefing, reputation attack.
- **Mitigation:** each dispute resolution is deterministic; honest
  executor's reputation is unaffected. Disputer's reputation
  decreases per dismissed dispute. Sequencer rate-limits dispute
  submissions per pubkey; rate-limit threshold is operator-
  configurable.

### Malicious DSL contract author (future)
- **Capability:** publishes a custom contract with a backdoor (e.g.,
  output that subtly diverges from declared semantics).
- **Concern:** counterparties accept the contract believing it does
  what its name says.
- **Mitigation:** every custom contract has a `program_hash`
  committed in its registration record. Counterparties compute the
  hash themselves from the published source / weights and refuse to
  transact with mismatching hashes. The DSL frontend (Layer 2
  follow-up; not yet shipped) will provide tooling to compute
  canonical hashes from source.

## 9. Cryptographic primitive selection

| Primitive   | Library             | Audit status                                            |
| ---         | ---                 | ---                                                     |
| ed25519     | `ed25519-dalek 2.x` | Multiple external audits; current maintained release    |
| BLAKE3      | `blake3 1.5.x`      | Original spec authors; reference implementation         |
| HMAC-SHA512 | `hmac` + `sha2`     | RustCrypto org; widely audited                          |
| Symmetric AEAD (future view-key) | `aes-gcm 0.10.x` | RustCrypto AEAD trait; widely audited |

**No homemade crypto.** Every cryptographic operation is delegated
to one of the libraries above. PSL's only cryptographic original work
is the `program_hash` and `trace_hash_ternary` constructions, which
are domain-separated BLAKE3 wrappers — see `ternary_vm/src/trace_hash.rs`
and `agent_contracts/src/program.rs`.

## 10. Side-channel resistance

`ed25519-dalek 2.x` documents constant-time implementations of all
secret-key operations (sign, key derivation, verification of
batched signatures). PSL relies on this directly; we do not
implement any signing-side code that branches on secret bytes.

The ternary execution kernel does not branch on secret data either
— inputs are user-supplied bytes, not secrets. Timing of forward
pass varies with `nnz` per row but not with input value, so a
timing attacker observing the wall-clock of `forward()` learns the
*structure* of the network (already public via `weights_hash`) but
not the input.

## 11. Memory zeroing (sensitive material)

Every site that holds a private key, view-key, or other long-term
secret material wraps it in `zeroize::Zeroizing<…>`:

| Site                                  | Type             | Zeroize |
| ---                                   | ---              | ---     |
| `agent_wallet::slip10::Ed25519MasterKey::private`   | `[u8; 32]` | ✓ via `Zeroizing` |
| `agent_wallet::slip10::Ed25519MasterKey::chain_code`| `[u8; 32]` | ✓ via `Zeroizing` |
| `agent_wallet::slip10::Ed25519ChildKey::private`    | `[u8; 32]` | ✓ via `Zeroizing` |
| `agent_wallet::slip10::Ed25519ChildKey::chain_code` | `[u8; 32]` | ✓ via `Zeroizing` |
| `ed25519_dalek::SigningKey` (held inside our `AgentIdentity`) | internal | ✓ — library zeroizes on drop per its own audit |

The audit checklist for any new secret-holding code:
1. Wrap in `Zeroizing<…>` or use a type that implements `Drop` to
   zeroize.
2. Never log / format secret values.
3. Never include secret values in `Debug` output (`Zeroizing`
   forbids this by default).
4. Keep the lifetime of the secret as short as possible — derive,
   use, drop.

## 12. After the audit

Each finding gets a row in `docs/AUDIT_FINDINGS.md` (created at
audit kickoff) with severity, status, fix commit, and re-audit
sign-off. Critical / high findings must be remediated before any
non-pilot deployment carrying real value.

The audit report itself becomes a public artifact in
`docs/audits/<date>_<vendor>.pdf`.
