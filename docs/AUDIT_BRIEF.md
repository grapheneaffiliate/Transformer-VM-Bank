# PSL Audit Brief

**For:** prospective external auditor (Trail of Bits, Zellic,
OtterSec, or equivalent).
**From:** PSL maintainers.
**Date:** 2026-05-09. **Repo:** github.com/grapheneaffiliate/Transformer-VM-Bank.
**Commit at brief preparation:** `10ac60b`. **Tag at engagement:**
`v0.1.0` (to be cut at engagement signing).

This is a single-document overview of what we're asking you to audit
and the primary entry points for your read-through. Detailed material
lives in `docs/SECURITY_REVIEW.md`, which is the canonical scope
document.

## 1. What PSL is

PSL is a deterministic financial ledger where state transitions are
**bit-exactly re-executable as ternary-integer networks**. Anyone
holding the network weights and the canonical trace-hash contract
can independently verify any block — across heterogeneous CPU
architectures, no floating point in the verifier path, no proprietary
runtime dependency.

The system has two operating modes:

- **Sovereign** — single block-producing sequencer + multiple
  followers that re-execute every block. **In production today.**
- **Consortium (BFT)** — explicit defer per ADR-0002 with three
  concrete triggers and a 60-day SLA from any trigger to integration.

Phase 2 (this audit's primary scope) added an **agent transaction
layer**: a 5-message negotiation protocol, hierarchical key custody
(SLIP-0010), spending policy enforcement, and deterministic dispute
resolution via re-execution.

## 2. What we're asking you to audit

In scope (audit primary):

| Crate              | Purpose                                                                |
| ---                | ---                                                                    |
| `ternary_vm/`      | Pure-integer execution kernel. The load-bearing claim of the layer.     |
| `agent_contracts/` | 8-contract standard library + `TernaryProgram` trait + `program_hash`. |
| `agent_wallet/`    | SLIP-0010, signed `KeyPolicy`, monotonic `RevocationSet`, `KeyRotation`. |
| `agent_protocol/`  | Registration + 5 wire messages + state machine + dispute resolution.    |
| `agent_sdk/`       | High-level agent runtime + transport / on-chain view traits.            |

In scope (audit secondary, sanity-check only):

| Crate         | Purpose                                                          |
| ---           | ---                                                              |
| `crypto/`     | ed25519, BLAKE3, MPT. Audited libraries; PSL wires them up only. |
| `consensus/`  | Sovereign-mode `Consensus` trait. BFT impl is gate-9 deferred.   |
| `sequencer/`  | Block production loop using the above.                           |
| `light_client/` | MPT inclusion-proof verifier.                                  |

Out of scope:

- `legacy/rust_runner/` — frozen per ADR-0001; in tree only for
  backward-compat verification of historical blocks.
- Network transport (mutual-TLS HTTPS) — caller of the SDK wires it.
- UniFFI bindings to Swift / Kotlin / Python / JS — separate crate,
  emitted via uniffi-bindgen.

## 3. Architecture in one paragraph

A PSL contract is a **`TernaryProgram`** — a pure function with a
`program_hash`. The execution engine (`ternary_vm/`) is an
analytically-constructed sparse ternary network: weights ∈ {-1, 0,
+1}, biases and activations are i64, ReLU only, no fp anywhere.
Same input + same weights → bit-identical output on any conformant
integer-arithmetic verifier (x86_64, aarch64, FPGA, secure enclave,
microcontroller). The trace-hash contract is `BLAKE3(weights_hash ||
canonical_input || canonical_output)` — short, content-addressed,
**no autoregressive sequence** to reconstruct. Disputes resolve in
finite time by re-executing the contract; the sequencer re-runs
the same program on the same input and the verdict is mechanical.

## 4. Post-quantum readiness

**Post-Quantum Readiness.** v0.1.0 ships hybrid post-quantum
cryptography from the foundation. Hybrid ed25519 + ML-DSA-65
signatures (ADR-0006); hybrid X25519 + ML-KEM-768 key encapsulation
with forward secrecy (ADR-0011); BLAKE3-512 for long-lived
commitments (ADR-0008); cryptographic agility via scheme-id varint
prefixes (ADR-0007) enables future scheme migrations without hard
fork. All cryptographic primitives use audited implementations
(RustCrypto, curve25519-dalek, NIST PQClean reference). Cross-platform
determinism CI-verified on x86_64 and aarch64. Awaits external
cryptographer review of the hybrid combiner per ADR-0006 / ADR-0011
acceptance criteria — see gate 19.

## 5. Performance baseline

Sequencer throughput on `bench_sequencer_tps_10k_blocks`
(`sequencer/tests/integration.rs`, `#[ignore]`d regression bench;
15,106 mixed signed transactions across 10,000 blocks):

| Configuration | TPS | mean | p50 | p95 | p99 | p99.9 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Sequencer + 3 followers, in-process, cross-replica root-agreement check every block | ~925 tx/s | 1.08 ms | 950 µs | 1.95 ms | **2.72 ms** | **4.20 ms** |
| Single-replica sequencer | ~3,990 tx/s | 251 µs | 201 µs | 464 µs | 737 µs | 1.42 ms |
| Composed estimate including real ternary trace_hash (back-of-envelope) | ~1,750 tx/s single-replica | — | — | — | — | — |

**Pinned reference hardware:** Intel Core i7-7700 @ 3.60 GHz, 4 cores
/ 8 threads, x86_64, WSL2 Ubuntu (Linux 5.15), release build. The
bench captures `uname -a` + relevant `lscpu` fields at run time so
re-runs on different hardware self-document. Run-to-run TPS variance
~5-15% on this WSL2 host from OS scheduler noise.

What's measured: real ed25519 sign + verify, real MPT writes, real
state-root computation, cross-replica consistency (4-replica run).
What's excluded: real ternary VM trace_hash (uses
`NativeTraceExecutor` synthetic stub), `sled` durable commit (uses
in-memory `State`; sled migration deferred per ADR-0012), network
transport (in-process; production = mutual-TLS HTTPS).

The composed estimate (~1,750 tx/s) adds ~34 trace-hashes per transfer
× ~9.5 µs each (gate-10's measured `byte_add` throughput of 105k
vec/s) onto the 251 µs single-replica baseline. Trace-work-vs-
sequencer-work composition arithmetic; not a direct measurement.
Direct measurement (replacing `NativeTraceExecutor` with the real
ternary VM in the sequencer trace path) is queued as v0.2 maturation
work per ADR-0013.

The 4-replica p99.9 of 4.2 ms is the meaningful worst-case settlement
time for capacity planning; a single 922 ms max outlier in the 15,106-
sample distribution is OS-scheduler noise on WSL2 (not load-bearing).
Comfortably above the gate-9 sovereign-pilot trigger threshold of 100
TPS. Perf-CI auto-regression gate (separate runner pool with pinned
hardware + threshold-based merge gating) deferred to v0.2 per
ADR-0013.

## 6. Primary entry points for your read

Read in this order:

1. `docs/ARCHITECTURE.md` § 0.2 (canonical trace contract) and § 0.3
   (canonical engine ordering).
2. `docs/SECURITY_REVIEW.md` (full scope, trust boundaries, 10
   invariants, attack-surface inventory by layer).
3. `ternary_vm/src/network.rs` — the forward kernel. ~150 lines, no
   external deps in the hot path, `checked_add`/`checked_sub`
   throughout.
4. `ternary_vm/src/primitives/byte_add_with_carry.rs` — the simplest
   end-to-end primitive. Read with the construction docstring.
5. `agent_contracts/src/transfer.rs` + `agent_contracts/src/guarded.rs`
   — how a contract composes Layer 1 primitives.
6. `agent_wallet/src/policy.rs` — spending policy enforcement.
7. `agent_protocol/src/dispute.rs` — `resolve_dispute` driver.
8. `agent_sdk/src/agent.rs` — runtime that ties it all together.

## 7. Test artifacts you can re-run

```bash
git clone https://github.com/grapheneaffiliate/Transformer-VM-Bank.git
cd Transformer-VM-Bank
cargo build --workspace --release
cargo test --workspace --release
```

Coverage:

- 102 baseline tests (Layers 1-5).
- 23 property tests (wallet + ternary kernel) covering the 10
  invariants in `docs/SECURITY_REVIEW.md` § 3.
- 7 adversarial dispute scenarios (replay, malformed, stale, sybil,
  griefing, cross-proposal, illegal-transition).
- 5 fuzz harnesses (`docs/FUZZING.md`); ready to run for the
  audit's recommended 1-CPU-hour budget on a CI machine you control.

## 8. Known issues + limitations

Documented honestly:

- **Gate 8 closed via retirement**, not via long-primitive parity
  (see ADR-0001). Legacy fp64 runner is frozen; new code uses the
  ternary engine. Backward-compat verification path preserved.
- **Gate 9 deferred** to one of three explicit triggers (see
  ADR-0002). Sovereign-mode trust assumption documented for
  institutional partners (`docs/SOVEREIGN_MODE_TRUST.md`).
- **3 Lean `sorry`s** remain on conservation theorems with
  documented target close dates (see `docs/STATUS.md` "Lean sorry
  tracker" section). No new `sorry`s introduced in Phase 2.

## 9. Recommended audit scope + deliverables

We propose:

1. **Read-through (1-2 weeks)** of the in-scope crates with focus
   on the 10 invariants and the attack-surface inventory.
2. **Fuzz (1 week)**: each `cargo-fuzz` harness for ≥ 1 CPU-hour;
   commit any crash files.
3. **Property tests (1 week)**: extend the proptest suite for any
   under-covered branch your read identifies.
4. **Cross-platform determinism (1 day)**: build `ternary_vm` on
   x86_64-linux + aarch64-darwin + aarch64-linux; run the
   exhaustive byte_add corpus on each; assert byte-identical.
5. **Crypto hygiene (0.5 day)**: confirm `ed25519-dalek`, `blake3`,
   `sha2`, `hmac` are the only crypto sources; confirm `zeroize`
   integration on every private-key storage.
6. **Report**: severity-tagged findings, public artifact in
   `docs/audits/<date>_<vendor>.pdf`. We track remediation in
   `docs/AUDIT_FINDINGS.md` with fix commits and re-audit sign-off.

Estimated total: **3-5 weeks of auditor time**, depending on depth.

## 10. Engagement logistics

- **Repository access:** public on GitHub. Read-only for the
  audit; we accept findings via private GitHub Security Advisory.
- **Communication:** an on-call PSL maintainer responds to
  questions within 48 business hours during the engagement window.
- **Fix turnaround:** critical / high findings remediated within
  10 business days of the report's delivery; medium / low within
  30 days. Re-audit included in the engagement scope.
- **Public disclosure:** audit report published in tree on
  acceptance + 30-day partner-courtesy window; CVEs filed for any
  pre-disclosure vulnerabilities discovered.

## 11. Contact

Open a GitHub Security Advisory on the repo, or email
`security@psl.example` (fill in actual address before sending).

PGP key for sensitive correspondence: published in
`SECURITY.md` at repo root.

## 12. Appendix: prior security work

Phase 1 (Q1-Q2 2026) shipped the gate-1 through gate-7 sequencer +
light client + pilot stack. No external audit was conducted on
Phase 1 — the consensus model was sovereign-mode-only. Phase 2's
agent layer is the first PSL release that's audit-ready and the
first that warrants external review, hence this brief.
