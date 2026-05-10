# Percepta Settlement Layer (PSL)

**A deterministic financial ledger with a first-of-its-kind agent-to-agent
transaction layer that resolves every dispute by re-execution, not arbitration.**

PSL ships two things in one repository:

1. A deterministic settlement layer for tokenized assets (USD, CBDC,
   gold, treasuries) — bit-exactly re-executable, ed25519 + BLAKE3
   native crypto, mobile light-client.
2. **An agent execution layer on top of it.** Two agents negotiate a
   transaction off-chain (5 signed messages), execute it on-chain, and
   if either side disputes, the chain resolves the dispute by
   re-executing the contract — deterministically, byte-for-byte, with
   no human arbiter and no oracle.

The novelty is the second part. Every contract in PSL is a pure
ternary integer program (weights ∈ {-1, 0, +1}, integer biases, ReLU
activations, **no floating point anywhere on the verifier path**). Pure
integer + sparse encoding makes contract execution **bit-exactly
reproducible across machines**. That property is what lets dispute
resolution be a function of the protocol, not an off-chain process.

## Status

| Gate | What it covers                                       | Status |
| ---  | ---                                                  | ---    |
| 1    | Primitive bit-exact (10k vectors each)               | ✅ |
| 2    | Crypto + SMT determinism                             | ✅ |
| 3    | Lean `lake build` (3 sorrys with target dates)       | ✅ |
| 4    | Sequencer + 3 followers, 100 mixed blocks            | ✅ |
| 5    | Compliance enforcement (9/9)                         | ✅ |
| 6    | Light client cross-verifies 1000 balances + 6 adv.   | ✅ |
| 7    | End-to-end pilot (register → mint → xfer → burn)     | ✅ |
| 8    | Pure-Rust runner (canonical engine; legacy fp64 retired per ADR-0001) | ✅ |
| 9    | Consortium swap (ABCI + CometBFT)                    | ⏸ deferred (ADR-0002) |
| 10   | Ternary execution engine — Phase 2 Layer 1           | ✅ |
| 11   | Contract DSL standard library (8 contracts)          | ✅ |
| 12   | Identity & wallet (SLIP-0010 + spending policies)    | ✅ |
| 13   | Negotiation protocol (5 messages, signed, idempotent)| ✅ |
| 14   | Dispute resolution by re-execution                   | ✅ |
| 15   | Reference agents (trader + service)                  | ✅ |
| 16   | SDK 0.1.0                                            | ✅ |
| 17   | External security audit hand-off                     | 🟢 awaits engagement letter |
| 18   | Production-readiness (runbooks + DR drill plan)      | 🟢 awaits first staging DR drill |
| 19   | Post-quantum cryptographic agility (Phase G phase 1) | 🟡 infrastructure shipped; phases 2-6 + external cryptographer review pending |

Per-gate command, output, commit hash: `docs/STATUS.md`. **Doc index:** `docs/INDEX.md`.

## The agent layer in 60 seconds

```
Alice (trader)                              Bob (service)
    │                                            │
    ├── Propose ──────────────────────────────▶  │   (signed offer; content-addressed via proposal_hash)
    │                                            │
    │  ◀──────────────────────── Accept  ───────┤   (Bob counter-signs)
    │                                            │
    ├── Execute ───────────────────────────▶    │   (Alice runs the contract;
    │                                            │    publishes input + claimed output + signature)
    │                                            │
    │            (no dispute → tx settles)       │
    │                                            │
    │  ──── Dispute (claimed output mismatch) ──▶│
    │                                            │
    │            judge agent re-executes the     │
    │            ternary contract from input     │
    │            and compares to Alice's claim.  │
    │            Bytes match → DismissDispute.   │
    │            Bytes differ → SlashExecutor.   │
```

There is no human arbiter and no off-chain oracle. The dispute outcome
is a deterministic function of `(contract code, input)` that any
participant can independently verify. Demo:

```bash
cargo run -p psl-agent-sdk --release --example trader_agent     # happy path
cargo run -p psl-agent-sdk --release --example service_agent    # adversarial dispute
```

`service_agent` shows the dispute path: Bob (executor) signs an
`Execute` claiming an all-zero output for a transfer; Alice opens a
dispute; the judge agent re-executes the `TransferContract`
deterministically; outcome is `SlashExecutor(bob_pubkey)`. No human
in the loop, no oracle.

## Why ternary integers (no floating point)

A floating-point matmul reorders reductions per CPU vector width and
per BLAS implementation. Two honest verifiers running the same code
on different machines can disagree on the last few bits. That is fine
for ML inference. It is **fatal for a verifier** that must produce
the same output on the dispute resolver as on the executor.

PSL's contract VM is integer-only (weights ∈ {-1, 0, +1} via thermometer
encoding, integer biases, ReLU activations). Sparse encoding keeps
the working set small. The kernel is checked-arithmetic; there are
**zero `unwrap()` / `expect()` on production paths** that aren't
either lock-poison (a programming-bug-class event) or
structurally-impossible-overflow (audited and justified inline; see
`docs/UNWRAP_AUDIT.md`).

This is what makes "dispute = re-execute" a tractable protocol rather
than a research idea.

## Components

```
agent_sdk/        — high-level runtime (handle_propose / handle_accept / handle_execute / resolve_dispute_for)
agent_protocol/   — 5 wire messages + ProposalLog state machine + dispute resolver
agent_wallet/     — SLIP-0010 ed25519 derivation + spending policies + revocation
agent_contracts/  — 8 standard contracts (transfer, swap, escrow_*, time_locked, multisig_2of3, conditional_payment)
ternary_vm/       — pure-integer execution kernel (the trust-critical inner loop)

sequencer/        — sovereign-mode block producer
consensus/        — Consensus trait (sovereign, ABCI follow-up per ADR-0002)
light_client/     — MPT inclusion proofs; UniFFI-ready for iOS/Android
crypto/           — ed25519 + BLAKE3 + Merkle-Patricia Trie (state root)
crypto_agility/   — scheme-prefixed signatures/KEM/hashes (Phase G phase 1; PQ-ready trait shape per ADR-0007)

legacy/rust_runner/  — frozen per ADR-0001; do not extend
lean/                — Lean 4 + mathlib formalization (3 sorrys with target dates)
pilot/issuer_demo/   — end-to-end pilot binary
sdk-examples/        — Python (UniFFI) + TypeScript (napi-rs) bindings of the SDK
infra/               — reference Terraform deployment (network + sequencer + 3× follower + light-client gateway + observability)
ops/                 — observability stack (Prometheus + Grafana + Alertmanager + Loki + Promtail + Tempo) with alerts + dashboards
```

## Build / reproduce

`REPRODUCE.md` is the canonical guide; `docs/REPRODUCIBILITY_REPORT.md`
records pinned toolchain, per-gate command, and wall-clock timings on
the reference Ubuntu 24.04 cloud VM. The summary is:

```bash
# Toolchain (pinned): rustc 1.95.0
cargo build --workspace --release         # ~60 s
cargo test  --workspace --release         # ~45 s
cargo run -p psl-agent-sdk --release --example trader_agent
cargo run -p psl-agent-sdk --release --example service_agent
```

Total time on a fresh clone, fresh VM, no cache: ~30 minutes including
toolchain install, ~5 minutes after toolchains land.

## For auditors

Start with `docs/AUDIT_BRIEF.md` — that is the day-1 entry document.
It points at the security review (`docs/SECURITY_REVIEW.md`),
reproducibility report (`docs/REPRODUCIBILITY_REPORT.md`), unwrap audit
(`docs/UNWRAP_AUDIT.md`), fuzz harness inventory (`docs/FUZZING.md`),
threat model with adversary inventory, and the in-scope crate list
with file paths.

## For institutional / partner due diligence

`docs/OPERATIONAL_READINESS.md` covers the production posture (SLOs,
metrics, alerts, runbooks). `docs/DR_DRILL_PLAN.md` is the
pre-committed disaster-recovery drill protocol. `infra/` is the
reference Terraform — "redeploy this exactly" is `terraform apply`.

## Operating principles

These are non-negotiable in this codebase:

1. **No new sorrys** in load-bearing Lean theorems; existing 3 have
   target close dates.
2. **No `unwrap()` / `expect()` on production paths** other than
   audited lock-poison + audited structurally-impossible-overflow
   (see `docs/UNWRAP_AUDIT.md`).
3. **No floating point on the verifier path.** Period.
4. **No silent failures.** All input-driven errors return `Result`.
5. **Reproducibility is a property of the repo, not a property of a
   developer's laptop.** Anything in `REPRODUCE.md` must work on a
   fresh clone on a fresh VM.
6. **Tests are the spec.** Anything we want to be true is asserted in
   a test, including adversarial scenarios.

## License

MIT (see [`LICENSE`](LICENSE)). See [ADR-0005](docs/decisions/0005-licensing-export-patent-posture.md)
for the licensing + export-control + patent posture decision and its
rationale (incl. defensive non-assertion of any patents derived from
this work).

## Trust boundary

Signatures, hashes, and the Merkle-Patricia Trie are verified by
**native Rust code** (`crypto/`), not by the contract VM. The contract
VM covers application-layer state-transition arithmetic only (debits,
credits, nonces, freeze flags, swap math, escrow conditions). Both
layers are independently verifiable by any follower or light client.

## Plan & history

- **Doc index (start here):** [`docs/INDEX.md`](docs/INDEX.md)
- Architecture and trace-hash contract: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- Per-gate command + output + commit history: [`docs/STATUS.md`](docs/STATUS.md)
- Architectural decisions: [`docs/decisions/`](docs/decisions/) (ADRs 0001-0008)
- Per-release history: [`CHANGELOG.md`](CHANGELOG.md)
- Empirical findings and case studies: [`docs/FINDINGS.md`](docs/FINDINGS.md) (gate-1 era; ternary kernel is now canonical)
- Whitepaper draft (arXiv-bound per ADR-0003): [`docs/whitepaper/PSL.md`](docs/whitepaper/PSL.md)
- Launch blog post draft: [`docs/blog/agent-layer-launch.md`](docs/blog/agent-layer-launch.md)
- Governance + maintainers + contributing: [`GOVERNANCE.md`](GOVERNANCE.md), [`MAINTAINERS.md`](MAINTAINERS.md), [`CONTRIBUTING.md`](CONTRIBUTING.md)
- Security disclosure channel: [`SECURITY.md`](SECURITY.md)
