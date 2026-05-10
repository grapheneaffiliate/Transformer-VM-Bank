# PSL — A Settlement Layer with Re-Executable Agent Contracts

**Draft v0.1.** Companion to the v0.1.0 release tag. Submission to arXiv (cs.CR / cs.DC) planned within 30 days of release per ADR-0003.

**Authors:** PSL maintainers (see `MAINTAINERS.md`).

## Abstract

We present **PSL** (Percepta Settlement Layer), a deterministic
financial-settlement layer paired with an agent-to-agent transaction
layer in which contract disputes are resolved by **deterministic
re-execution** rather than human arbitration or off-chain oracles.
The contract execution kernel is integer-only — weights ∈ {-1, 0, +1}
encoded sparsely, integer biases, ReLU activations, no floating-point
on the verifier path — which is the property that makes "the chain
re-executes the contract" tractable as a protocol primitive rather
than a research aspiration. We describe (1) the ternary integer
contract VM, (2) an 8-contract standard library (transfer, swap,
escrow, time-locked release, multisig, conditional payment), (3) a
5-message signed negotiation protocol with idempotent replay
handling, (4) the dispute-by-re-execution mechanism with formal
guarantees, and (5) a hierarchical agent identity system based on
SLIP-0010 with parent-signed spending policies and revocation. We
report 18 acceptance gates closed (16 ✅, 2 🟢 awaiting external
sign-off), bit-exact verification on 7 primitives at 10k random
witnesses each, exhaustive verification on `byte_add_with_carry` and
`byte_sub_with_borrow` (131072/131072 cases each), 25 baseline + 7
adversarial dispute scenarios on the protocol layer, and a complete
production-operations stack with six runbooks and a pre-committed
quarterly DR drill protocol.

## 1. Introduction

### 1.1 The dispute problem in agent transactions

Two software agents transact. They negotiate a contract off-chain
(say, "Alice transfers 250 units to Bob in exchange for service S").
Either agent may sign and submit the executed result on-chain. If
either side later claims the executed result is wrong, the chain has
to decide who is correct. Today's options are:

1. **Human arbitration.** Slow, expensive, doesn't scale to
   high-frequency machine-to-machine commerce.
2. **Off-chain oracle.** Trust-shifting rather than trust-eliminating;
   the oracle becomes the new trust root, with the same problem one
   level removed.
3. **Restrict to contracts so simple disagreement is impossible.**
   Severely limits expressiveness; rules out the domains where agent
   transactions are actually useful.

We propose a fourth option: **the chain re-executes the contract**
on the disputed input and computes the correct output itself. If the
re-execution matches the executor's claimed output, the dispute is
dismissed. If it disagrees, the executor is slashed. There is no
human arbiter and no oracle. The dispute outcome is a deterministic
function of `(contract code, input)`.

### 1.2 Why this hasn't been done before

For "the chain re-executes" to work as a protocol, **the
re-execution must produce the exact same bytes** on every honest
participant. This property is harder than it looks.

A floating-point matrix multiplication reorders reductions per CPU
vector width and per BLAS implementation. Two honest verifiers
running the same code on different machines can — and do — disagree
on the last few bits. That is fine for ML inference. It is fatal
for a verifier that must produce the same output on the dispute
resolver as on the executor.

Smart-contract platforms that aim for cross-machine determinism
(EVM, Solana SVM, Move) do so by restricting the contract language
to integer arithmetic and disciplined memory access. PSL takes the
same discipline a step further: contracts are compiled to a
**ternary integer execution kernel** in which weights are constrained
to {-1, 0, +1}, biases are integer, activations are ReLU, and there
is no floating-point primitive in the language at all. The kernel
is checked-arithmetic; production paths contain zero `unwrap()` or
`expect()` outside two audited categories (lock-poison and
structurally-impossible-overflow; see § 6.4).

### 1.3 Contributions

1. **Ternary-integer contract VM** (`ternary_vm/`) with bit-exact
   reproducibility across x86_64 and aarch64. 7 primitives validated
   at 10k random witnesses each; exhaustive validation on the two
   smallest (131072 cases each).
2. **8-contract standard library** (`agent_contracts/`) covering the
   common shapes for agent commerce: transfer, swap, three-step
   escrow, time-locked release, 2-of-3 multisig, conditional
   payment.
3. **Negotiation protocol** (`agent_protocol/`) — 5 wire messages
   (`Propose / Accept / Reject / CounterPropose / Execute`),
   content-addressed via `proposal_hash`, idempotent replay
   handling, signed by SLIP-0010-derived hybrid identities.
4. **Dispute-by-re-execution** (`agent_protocol::dispute`) — judge
   re-executes the contract and returns `SlashExecutor` or
   `DismissDispute` deterministically.
5. **SDK** (`agent_sdk/`) in Rust (canonical) plus Python and
   TypeScript bindings, with two reference agent binaries
   demonstrating the happy and dispute paths end-to-end.

## 2. Threat model

(Detailed treatment in `docs/SECURITY_REVIEW.md` § 8 — adversary
inventory.)

We assume:
- **Network adversary** can drop, reorder, replay, or fabricate any
  message. Mitigation: signed messages, sequence numbers, bounded
  expiry.
- **Byzantine executor** signs an `Execute` claiming a different
  output than the contract's true output on the proposal input.
  Mitigation: dispute-by-re-execution.
- **Byzantine disputer** opens disputes against honest executors as
  a denial-of-service or grief vector. Mitigation: dispute fee +
  loser pays + reputation tracking; sustained behavior surfaces in
  `PSLAgentDisputeStorm` alert (see runbook).
- **Byzantine sequencer** in sovereign mode can reorder
  transactions but cannot forge signatures or invent state. The
  trust assumption is documented in `docs/SOVEREIGN_MODE_TRUST.md`
  and is the gating motivation for the deferred BFT consensus
  swap-in (ADR-0002).
- **Future quantum adversary**. PSL is migrating to hybrid
  classical+post-quantum signatures and KEM (separate workstream;
  see ADR-0006 once ratified).

## 3. The ternary integer contract VM

### 3.1 Encoding

A **ternary network** is a sequence of layers where each layer is
characterized by:
- An integer-valued weight matrix `W ∈ {-1, 0, +1}^{m × n}` stored
  sparsely (only nonzero entries).
- An integer bias vector `b ∈ ℤ^m`.
- A ReLU activation: `y_i = max(0, sum_j W_ij × x_j + b_i)`.

The forward pass is implemented in `ternary_vm/src/forward.rs` as a
straight-line checked-integer loop. There is no SIMD, no BLAS
dispatch, no reordered reductions. The result is the same bytes on
every machine in the supported architecture matrix.

### 3.2 Thermometer encoding

Continuous-looking inputs (e.g., a balance amount) are encoded into
the ternary network's input layer via **thermometer encoding**:
position `i` of the encoding is 1 iff the value is at least the
i-th threshold. This converts a u64 balance into a sparse {0,1}
vector that the ternary forward pass operates on natively.

### 3.3 Program hashing

A `TernaryProgram` is a typed sequence of network forward passes
plus a small layer of integer guards (no-op zeros on precondition
failure; no panics, no fallthroughs). Its `program_hash` is the
BLAKE3 hash of `(name || sub-network weights_hashes)` in canonical
order. This is the on-chain identifier of a contract.

`weights_hash` for a single network is BLAKE3 over the canonical
serialization of `(W_sparse, b)`. (For the post-quantum migration
this becomes BLAKE3-512 — see ADR-0008 once ratified.)

### 3.4 Determinism property

We claim:

> **Theorem (informal).** For every `TernaryProgram P` and every
> input `x` in P's input domain, the byte sequence
> `P.run(x).serialize()` is invariant across:
> - Compiler version (within the supported toolchain pin).
> - Target architecture (x86_64-unknown-linux-gnu and
>   aarch64-apple-darwin tested in CI).
> - Optimization level.
> - Run-to-run nondeterminism (none introduced by P; PRNG is
>   excluded from contract code by construction).

Mechanized statement and proof are in `lean/ternary/Determinism.lean`
(the proof is partial as of v0.1.0; see `docs/STATUS.md` § Lean
sorrys for tracking).

## 4. Standard contract library

The 8 contracts in `agent_contracts/` are listed below with their
input/output shapes.

| Contract               | Inputs                                          | Output (on success)                  | Notes |
| ---                    | ---                                             | ---                                  | --- |
| `transfer`             | sender_bal, recipient_bal, amount, nonce        | (sender'-amount, recipient'+amount, nonce+1) | byte-decomposed; no monolithic u128 add |
| `swap`                 | a_bal, b_bal, a_in, b_in, ratio_num, ratio_den  | (a'-a_in+ratio·b_in, b'-b_in+...)    | atomic; no-op on rate-mismatch |
| `escrow_create`        | depositor_bal, amount, condition_hash           | (escrow_id, escrow_bal=amount)       | three-party (depositor, beneficiary, arbiter) |
| `escrow_release`       | escrow_id, condition_witness                    | beneficiary_bal += escrow_bal        | condition_witness verified deterministically |
| `escrow_refund`        | escrow_id, refund_witness                       | depositor_bal += escrow_bal          | symmetric; arbiter-controlled |
| `time_locked_release`  | depositor_bal, amount, unlock_height            | beneficiary gets amount when h ≥ unlock | block-height check |
| `multisig_2of3`        | 3 pubkeys, 2 signatures, payload                | payload-execute on 2-valid           | flat 2/3, not threshold-derived |
| `conditional_payment`  | sender_bal, recipient_bal, amount, guard_value, guard_threshold | transfer iff guard_value ≥ guard_threshold | guard is a ternary program |

Every contract emits **canonical no-op zeros on precondition
failure**: insufficient balance, recipient overflow, guard not
satisfied, out-of-range flags. The contract never panics on
attacker-controllable input; it returns a structurally-determined
"nothing happened" output that the dispute layer understands.

## 5. Negotiation protocol

### 5.1 Five messages

```
Propose (proposer → counterparty)
Accept (counterparty → proposer)
Reject (counterparty → proposer)
CounterPropose (counterparty → proposer)
Execute (executor → counterparty + chain)
```

Each message is signed by its issuer's hybrid identity (currently
ed25519; hybrid post-quantum is in flight). Each carries a
`proposal_hash` that is a BLAKE3 commitment to the canonical
serialization of the underlying `Proposal` struct. Idempotent
replay: receiving the same message twice is a no-op; receiving a
conflicting message (e.g., `Accept` after `Reject`) is rejected.

### 5.2 State machine

```
Proposed → Accepted → Executed → Settled
Proposed → Rejected → Closed
Proposed → CounterProposed → (back to Proposed for the new offer)
Proposed → Expired → Closed
Accepted → Disputed → (DismissDispute → Settled | SlashExecutor → Closed)
```

The `ProposalLog` (`agent_protocol/src/log.rs`) maintains the
authoritative state per `proposal_hash`. The state machine is
exhaustively unit-tested for every legal transition and every
illegal one (the 7 adversarial dispute scenarios in
`agent_protocol/tests/adversarial_dispute.rs`).

### 5.3 Dispute resolution

The judge agent (which is not a separate party — it is logic that
any participant can run) executes the following:

```
fn resolve_dispute<P: TernaryProgram + ?Sized>(
    contract: &P,
    proposal: &Proposal,
    executor_claimed: &[u8],
) -> DisputeOutcome {
    let recomputed = contract.run(&proposal.input_bytes());
    if recomputed.serialize() == executor_claimed {
        DisputeOutcome::DismissDispute
    } else {
        DisputeOutcome::SlashExecutor(proposal.executor_pubkey())
    }
}
```

The implementation lives in `agent_protocol/src/dispute.rs`.

## 6. Identity, wallet, spending policies

### 6.1 SLIP-0010 hierarchical derivation

Agent identities derive from a master seed via SLIP-0010 ed25519
HMAC-SHA512 (`agent_wallet/src/derivation.rs`). A `parent` identity
signs `child` identity creation; the child derives from a
deterministic path (`m/<purpose>/<account>/<index>`); the parent's
signature on the child's public key + spending policy is the
authorization root.

### 6.2 Spending policies

A `PolicyEnvelope` (`agent_wallet/src/policy.rs`) constrains a child
key to:
- `cap_per_window`: maximum cumulative outflow per rolling window.
- `window_seconds`: window size.
- `allowed_contracts`: whitelist of `program_hash` values.
- `allowed_counterparties`: optional whitelist of pubkeys.
- `expiry_unix`: hard cap on policy validity.

Policies are signed by the parent and presented alongside every
outgoing message. Verifiers check the policy on inbound and on the
sequencer side.

### 6.3 Revocation

`agent_wallet::revocation` maintains a monotonic revocation set
(once revoked, stays revoked, even under message reordering). New
revocations are signed `Revocation` records appended to the log.
Property tests in `tests/proptest_invariants.rs` exercise the
monotonicity invariant under arbitrary shuffles of legitimate +
adversarial reorderings.

### 6.4 Operating principles

The codebase enforces:
1. **No `unwrap()` / `expect()` on production paths** outside the
   two audited categories: lock-poison (programming-bug-class event;
   panicking is the correct response) and
   structurally-impossible-overflow (audited and justified inline
   per `docs/UNWRAP_AUDIT.md`).
2. **No floating point on the verifier path.** Period.
3. **No silent failures.** All input-driven errors return `Result`.
4. **Tests are the spec.** Adversarial scenarios are first-class.

## 7. Post-quantum cryptographic readiness

PSL ships with hybrid post-quantum cryptography from v0.1.0, not
as a future migration. This section summarises the threat model,
the strategy, the implementation status, and the remaining work.

### 7.1 Threat model

Two categories of quantum attack matter for a settlement layer
that must remain verifiable for decades:

- **Shor's algorithm against discrete-log signatures.** A
  cryptographically-relevant quantum computer (CRQC) breaks
  ed25519 in polynomial time. Every signature ever made becomes
  forgeable. Without mitigation, the chain becomes unrecoverable.
- **Grover's algorithm against hashes.** Quadratic speedup on
  preimage attacks. BLAKE3-256's effective preimage security
  drops from 256 to 128 bits under quantum. Still very strong for
  short-lived hashes; insufficient margin for long-lived
  irrevocable commitments under a multi-decade durability
  requirement.

The active threat today is **harvest-now-decrypt-later (HNDL)**:
an adversary capturing encrypted material today can decrypt it
once a CRQC exists. View-key encryption and witness-payload
encryption migrate first because the captured-once-and-stored
exposure window is open today; signature migration is more
patient because signatures are verified-once-and-discarded.

### 7.2 Strategy: hybrid, agile, irreversible-where-it-must-be

Three principles, in order of priority (per ADR-0006):

1. **Hybrid before pure post-quantum.** Combine classical
   (ed25519, X25519) with post-quantum (ML-DSA-65, ML-KEM-768)
   such that an attack must break **both** to succeed. This is
   what Cloudflare, Google, Signal, and Apple iMessage shipped in
   2024-2025. Until a CRQC exists, classical security is the
   floor; once one exists, PQ becomes the floor. The classical
   half is essentially-free additional security.
2. **Cryptographic agility in the wire format.** Every signature,
   public key, and encryption blob carries an explicit varint
   scheme prefix per ADR-0007. Verifiers dispatch by scheme.
   Adding a new scheme is a one-byte prefix change, not a hard
   fork. Verifiers refuse unknown schemes with a typed error
   (never silent fallback).
3. **Some commitments are irreversible — choose them carefully.**
   `weights_hash` and long-lived `program_hash` are committed
   on-chain and cannot be retroactively re-hashed. Per ADR-0008
   they widen to BLAKE3-512 (256-bit Grover-halved quantum
   margin). Short-lived hashes (per-tx trace_hash, MPT roots,
   block-header hashes) stay BLAKE3-256 with a documented forward
   path.

### 7.3 Algorithm choices (locked)

| Role                                | Scheme                              | NIST level | ADR |
| ---                                 | ---                                 | --- | --- |
| Signatures                          | ed25519 + ML-DSA-65 (hybrid concat) | 3 | ADR-0006 |
| Key encapsulation                   | X25519 + ML-KEM-768 (hybrid HKDF)   | 3 | ADR-0011 |
| Symmetric AEAD                      | AES-256-GCM                         | n/a | ADR-0011 |
| Long-lived hashes (`weights_hash`, `program_hash`) | BLAKE3-512 | n/a | ADR-0008 |
| Short-lived hashes (`trace_hash`, `proposal_hash`, MPT) | BLAKE3-256 | n/a | ADR-0008 |

Schemes explicitly **not** chosen: SLH-DSA (signatures 7-30 KB,
too large for per-message use; reserved for future validator-only
use via the agility layer); FN-DSA / Falcon (reference
implementations require constant-time floating-point arithmetic,
permanently excluded by PSL's no-fp-on-verifier-path rule).

### 7.4 Hybrid composition

**Hybrid signature** uses the concatenation combiner per NIST SP
800-227 (draft) and IETF
`draft-ietf-pquip-hybrid-signature-spectrums`:

```
hybrid_sig(msg) = ed25519_sig(msg) || ml_dsa_sig(msg)
```

Verification accepts iff **both** signatures verify. No XOR
mixing, no novel combiner.

**Hybrid KEM** uses HKDF-SHA-512 over a transcript binding both
shared secrets and both ciphertexts per IETF
`draft-ietf-tls-hybrid-design`:

```
hybrid_ss := HKDF-SHA-512(
    salt = "PSL-hybrid-kem-salt-v1",
    ikm  = varint(scheme_id) || x25519_ss || ml_kem_ss
                             || x25519_eph_pk || ml_kem_ct,
    info = context_string,
).expand(32)
```

The transcript includes both ciphertexts so neither component can
be substituted independently — a swap-attack on the hybrid
ciphertext fails AEAD authentication downstream. Context strings
provide domain separation across uses (witness encryption, view-
key delivery, travel-rule metadata).

### 7.5 Forward secrecy for witness encryption

Per-witness ephemeral hybrid keypairs. The lifecycle (per
ADR-0011 § "Ephemeral key lifecycle"):

1. Generate fresh hybrid ephemeral keypair (X25519 + ML-KEM-768).
2. Encapsulate to recipient's long-term hybrid public key.
3. Build canonical transcript (above).
4. Derive AEAD key via HKDF with `ContextString::as_bytes()` as
   `info`.
5. Encrypt witness payload with AES-256-GCM under the derived key.
6. **Zeroize ephemeral private keys immediately** (load-bearing
   forward-secrecy invariant; type system enforces via
   `ZeroizeOnDrop` derive).
7. Zeroize derived AEAD key on scope exit.

Even if the recipient's long-term key is later compromised, past
witnesses remain protected because the ephemeral private keys are
gone. This is the HNDL defense.

### 7.6 Implementation status (gate 19 🟢)

The PQ migration shipped as the ADR-0011 5-commit plan (PRs
#11 → #15) plus follow-up test additions (PR #18):

| # | What | Tests added |
| --- | --- | --- |
| #11 | Spec + skeleton (types + trait stubs) | Trait-shape sanity |
| #12 | KEM impl (HybridX25519MlKem768Kem) | 6 KEM tests + 4 from_bytes-never-panic proptests |
| #13 | Witness encryption impl | 7 of 8 ADR-0011 blocking tests (round-trip, forward secrecy, implicit rejection, component swap, wrong-context, zeroization, edge sizes) |
| #14 | Agent-layer cascade (Propose.program_hash widening + ProposalHash newtype) | 249 workspace tests pass |
| #15 | Cross-platform CI matrix (x86_64 + aarch64) | 252 workspace tests pass on both architectures |
| #18 | Cross-version isolation + pinned (ct, sk) → ss byte-identity | v1-Propose rejection by v2 verifier + pinned-decap byte-identity oracle (254 workspace tests pass) |

Total: **254 workspace tests pass on both x86_64 and aarch64
GitHub-hosted runners**, byte-identical pinned digests for the
HKDF salt + 3 context strings (deterministic-by-construction
inputs to AEAD-key derivation) plus a pinned (ciphertext, secret
key) → shared-secret byte-identity oracle locking the strongest
cross-platform property the KEM can hold.

The cryptographic pieces use audited reference implementations:
- ed25519 + X25519 via `ed25519-dalek` + `x25519-dalek` (Trail of
  Bits + NCC Group audits).
- ML-DSA-65 + ML-KEM-768 via `pqcrypto-mldsa` + `pqcrypto-mlkem`,
  which wrap NIST PQClean reference C implementations (audited
  during NIST's standardization). The Rust binding crates have not
  been independently audited as Rust crates; their constant-time
  invariants are inherited from PQClean and the binding layer is
  community-maintained — flagged here as honest disclosure for
  reviewer scope.
- BLAKE3 via the `blake3` crate (reference implementation,
  widely deployed).
- HKDF + AES-256-GCM via RustCrypto's `hkdf` + `aes-gcm`
  (community-audited).

### 7.7 Remaining work

Gate 19 sits at 🟢. The only remaining ✅ blocker per ADR-0006
acceptance criteria is **external cryptographer review** of the
hybrid combiner, transcript construction, and ephemeral-key
lifecycle. This is a focused 1-2 week scoped engagement (smaller
than the full audit per gate 17), yielding a signed cryptographer
review of the hybrid combiner. The review scope (combiner,
transcript construction, ephemeral-key lifecycle) is defined in
ADR-0006 (acceptance criteria) and ADR-0011 (combiner specifics);
outreach itself is a separate operational track — not covered by
ADR-0003, which scopes only public-artifact publication.

The agility layer enables future scheme migrations without hard
fork: when ML-DSA-65 needs to migrate (e.g., to ML-DSA-87 if
cryptanalysis reveals a flaw, or to a successor scheme entirely),
the migration is a new `SignatureScheme` discriminant + a new
`Verifier` impl. Existing on-chain artifacts remain verifiable
under their original scheme via the documented
backwards-compatibility mechanism.

## 8. Comparison to related systems

| System              | Determinism property                            | Dispute mechanism                          |
| ---                 | ---                                             | ---                                        |
| EVM (Ethereum)      | Integer EVM; deterministic by construction      | Re-execute via fraud proofs (Optimism / Arbitrum) |
| Solana SVM          | Deterministic BPF; cross-cluster agreement      | Implicit (full re-execution per validator)  |
| Move (Aptos / Sui)  | Resource-typed deterministic VM                 | Implicit (consensus-time re-execution)      |
| **PSL agent layer** | **Ternary integer; cross-machine bit-exact**    | **Explicit re-execution by judge (any party)** |

The novelty is not "deterministic VM" — that's been done. The
novelty is using cross-machine determinism as the substrate for an
**explicit, contract-level dispute primitive** that is intended to
be invoked at the application layer between specific transacting
agents, rather than implicitly at the consensus layer between
validators.

## 9. Implementation status

Per `docs/STATUS.md` (which is the authoritative ground-truth
table): 19 gates defined; 15 closed ✅; 1 ⏸ (gate 9, BFT consensus
deferred per ADR-0002); 3 at 🟢 (gate 17 external audit, gate 18
first DR drill, gate 19 external cryptographer review) — material
is shipped, awaits human action.

Reproducibility: `REPRODUCE.md` plus `docs/REPRODUCIBILITY_REPORT.md`
records pinned toolchain, per-gate command, expected timing on a
clean Ubuntu 24.04 cloud VM.

Operational stack: six runbooks (`docs/runbooks/`), full
docker-compose observability (`ops/`), Terraform reference
deployment (`infra/`), pre-committed DR drill protocol
(`docs/DR_DRILL_PLAN.md`).

Throughput: the sequencer regression bench
(`bench_sequencer_tps_10k_blocks` in `sequencer/tests/integration.rs`)
processes 15,106 mixed signed transactions across 10,000 blocks with
real ed25519 signatures, real MPT writes, and real state-root
computation. On the pinned reference hardware (**Intel Core i7-7700
@ 3.60 GHz, 4 cores / 8 threads, x86_64, WSL2 Ubuntu, release
build**), the measured sustained throughput is **~925 tx/s with
sequencer + 3 followers verifying state-root agreement on every
block** (mean 1.08 ms/tx; p50 950 µs, p95 1.95 ms, p99 2.72 ms,
p99.9 4.20 ms), and **~3,990 tx/s single-replica** (mean 251 µs;
p50 201 µs, p95 464 µs, p99 737 µs, p99.9 1.42 ms). The bench
captures `uname -a` + `lscpu` at run time so re-runs on different
hardware self-document. Composing in real ternary trace_hash
(~34 trace-hashes per transfer × ~9.5 µs each from gate-10's
measured `byte_add` throughput) yields a back-of-envelope estimate of
**~1,750 tx/s single-replica end-to-end with real trace_hash**. All
numbers comfortably exceed the gate-9 sovereign-pilot trigger
threshold of 100 TPS. Bench excludes `sled` durable commit (in-
memory `State`; sled migration deferred per ADR-0012) and network
transport (in-process; production = mutual-TLS HTTPS). Direct
measurement of the real ternary VM in the sequencer trace path and a
perf-CI auto-regression gate are queued as v0.2 maturation work per
ADR-0013.

## 10. Open questions and future work

- **BFT consensus for federated mode.** Sovereign-mode v0.1.0 ships
  with a documented trust assumption. ADR-0002 defines three
  objective triggers for BFT engagement (multi-issuer
  pre-commitment, regulator written request, DR drill failure
  attributable to single-sequencer). Engineering on tendermint-rs
  ABCI + CometBFT begins on first trigger fire; 60-day SLA.
- **Post-quantum cryptography.** Shipped in v0.1.0 — hybrid ed25519
  + ML-DSA-65 signatures, hybrid X25519 + ML-KEM-768 KEM with
  forward secrecy, BLAKE3-512 for long-lived commitments,
  cryptographic agility via scheme-id varint prefixes (gate 19 🟢,
  ADR-0006/0007/0008/0011). Remaining work: external cryptographer
  review of the hybrid combiner — see § 7.7.
- **Public test network.** Deferred to v0.2 per ADR-0004 (rationale:
  cannot operate a public testnet under audit-pending +
  DR-drill-pending posture).
- **Mobile SDK bindings (Swift, Kotlin).** Architecturally trivial
  via UniFFI; not in v0.1.0 scope.

## 11. Conclusion

Cross-machine deterministic re-execution as a **contract-level
dispute primitive** rests on a single non-negotiable property: no
floating-point on the verifier path. PSL ships that property, the
contract VM that enforces it, an 8-contract standard library that
uses it, a 5-message negotiation protocol that builds on it, a
hierarchical identity system that authorizes participation in it,
and a reference SDK in three languages that makes it available to
application authors. We claim no novelty in any individual layer;
we claim novelty in the composition: the property of "dispute = the
chain re-executes" being available as a protocol primitive that any
two agents can rely on without coordinating with the chain operator
or a human arbiter.

The full source is at
`github.com/grapheneaffiliate/Transformer-VM-Bank` under the MIT
license. Audit hand-off material is in `docs/AUDIT_BRIEF.md`.

## References

(To be expanded for arXiv submission.)

- NIST FIPS 203 (ML-KEM), 204 (ML-DSA), 205 (SLH-DSA), 206 (FN-DSA).
- SLIP-0010, "Universal private key derivation from master private
  key."
- BIP-32, "Hierarchical Deterministic Wallets."
- The BLAKE3 specification.
- IETF ed25519 RFC 8032.
- Optimism / Arbitrum fraud-proof literature for the EVM dispute
  comparison.
- Solana SVM determinism documentation.
- Move language reference for the resource-typed VM comparison.

## Acknowledgments

(To be filled at arXiv submission.)
