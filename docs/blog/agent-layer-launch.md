# PSL v0.1.0 — an agent execution layer that resolves disputes by re-execution

*Draft. To be published with the v0.1.0 release per ADR-0003.*

---

If two software agents transact and one of them lies about the
result, what happens?

The honest options today are bad:
1. A human arbiter looks at logs. (Slow. Expensive. Doesn't scale.)
2. An off-chain oracle decides. (Trust-shifting, not trust-eliminating.)
3. The contract is so simple that disagreement is impossible.
   (Severely limits what you can build.)

We've shipped a fourth option: **the chain re-executes the
contract** and computes the right answer itself. No human, no oracle,
no off-chain process. Bytes match the executor's claim → dismiss the
dispute. Bytes differ → slash the executor.

This is PSL v0.1.0. Repo:
[github.com/grapheneaffiliate/Transformer-VM-Bank](https://github.com/grapheneaffiliate/Transformer-VM-Bank).

## The 30-second demo

```
$ cargo run -p psl-agent-sdk --release --example service_agent
bob signs Execute claiming all-zero output (lying)
alice opens Dispute (her local re-execution disagrees with bob's claim)
judge outcome: SlashExecutor(<bob_pubkey_hex>)
```

That's the entire dispute-resolution protocol. The judge agent
re-runs Bob's contract on Bob's input, gets a different output, and
attributes the slash to Bob's signing key. There is no part of this
that requires a human or an oracle. It is a function of `(contract
code, input)`.

## Why this hasn't existed before

For "the chain re-executes" to work as a protocol, **the
re-execution must produce the exact same bytes** on every honest
participant. That property is harder than it looks.

A floating-point matmul reorders reductions by CPU vector width and
BLAS implementation. Two honest verifiers running the same code on
different machines can — and do — disagree on the last few bits.
That's fine for ML inference. It's fatal for a verifier.

PSL's contract VM is integer-only:
- Weights are ternary: ∈ {-1, 0, +1}, encoded sparsely.
- Biases are integer.
- Activations are ReLU.
- **No floating point on the verifier path.** Period. (See
  `docs/UNWRAP_AUDIT.md` for the audit of the few `unwrap`s that
  exist on production paths — all are either lock-poison
  programming-bug-class events or structurally-impossible-overflow
  with the proof inlined as a comment.)

The result: a contract executed on Alice's laptop and the same
contract executed on the dispute-resolver's server produce the same
bytes. That's the property that makes the dispute protocol work.

## The standard contract library

We ship eight standard contracts (`agent_contracts/`):
- `transfer` — simple transfer.
- `swap` — atomic swap of two assets at agreed ratio.
- `escrow_create` / `escrow_release` / `escrow_refund` — escrow with
  deterministic release condition.
- `time_locked_release` — release after a height.
- `multisig_2of3` — 2-of-3 multisig.
- `conditional_payment` — payment if a guard predicate is true.

Each one is a `TernaryProgram` — a typed sequence of ternary
network forward passes plus a small layer of integer guards (no-op
zeros on precondition failure; no panics, no fallthroughs).

## What v0.1.0 covers

| Layer                                                    | Status |
| ---                                                      | ---    |
| Ternary execution kernel                                 | ✅ shipped, 42 baseline + 11 proptest tests |
| 8-contract standard library                              | ✅ shipped, 20 tests |
| SLIP-0010 wallet + spending policies + revocation        | ✅ shipped, 25 tests |
| 5-message negotiation protocol + dispute resolver        | ✅ shipped, 25 tests + 7 adversarial |
| SDK (Rust canonical; Python + TypeScript bindings)       | ✅ shipped, 2 reference agents |
| Sequencer + 3-follower agreement                         | ✅ 100 mixed blocks, all roots agree |
| Compliance (travel rule, freeze, view keys)              | ✅ 9/9 |
| Light client                                             | ✅ 1000 balances + 6 adversarial |
| End-to-end pilot (register → mint → transfer → burn)     | ✅ |
| Lean formalization                                       | ✅ 16/17 modules; 3 sorrys with target close dates |
| External audit                                           | 🟢 hand-off package ready, awaits engagement |
| First DR drill                                           | 🟢 plan ready, awaits scheduled drill |

The full table is in `docs/STATUS.md`.

## How fast does it run

The sequencer regression bench
(`bench_sequencer_tps_10k_blocks`) processes 15,106 mixed signed
transactions across 10,000 blocks with real ed25519 signatures, real
MPT writes, and real state-root computation:

- **Sequencer + 3 followers, in-process, root-agreement check every
  block:** ~925 tx/s (mean 1.08 ms; p99 2.72 ms; p99.9 4.20 ms).
- **Single-replica sequencer:** ~3,990 tx/s (mean 251 µs; p99 737 µs;
  p99.9 1.42 ms).
- **Composed estimate including real ternary trace_hash** (~34
  trace-hashes per transfer × ~9.5 µs each from gate-10's measured
  `byte_add` throughput): ~1,750 tx/s single-replica end-to-end.

Pinned reference hardware: Intel Core i7-7700 @ 3.60 GHz, 4 cores /
8 threads, x86_64, WSL2 Ubuntu, release build. Bench captures
`uname -a` + `lscpu` at run time. Comfortably above the 100-TPS
sovereign-pilot trigger threshold; the p99.9 of 4.2 ms is the
meaningful worst-case settlement time for capacity planning.

Caveats: bench uses a synthetic trace executor (real ternary VM
trace adds the ~9.5 µs × 34 above), in-memory state (no `sled`
durable commit; deferred per ADR-0012), in-process transport (not
mutual-TLS HTTPS). Perf-CI auto-regression gate and direct real-
trace measurement deferred to v0.2. Reproduce via `cargo test
-p psl-sequencer --test integration --release
bench_sequencer_tps_10k_blocks -- --ignored --nocapture`.

## What's deliberately NOT in v0.1.0

- **A public testnet.** ADR-0004 explains the deferral. Local
  reference deployment via `infra/` Terraform is the substitute.
- **BFT consensus.** ADR-0002 defers the engine choice (Malachite
  vs CometBFT vs other) to trigger fire, with a 60-day SLA.
  Sovereign-mode ships first.
- **Mobile SDKs (Swift / Kotlin).** Architecturally trivial via
  UniFFI; not in v0.1.0 scope.

## Three operating principles that wouldn't bend

1. **No floating point on the verifier path.** The dispute-by-re-
   execution mechanism doesn't survive without this.
2. **No `unwrap()` / `expect()` on production paths** other than
   lock-poison or structurally-impossible-overflow, both audited.
3. **Tests are the spec.** Anything we want to be true is asserted
   in a test, including adversarial scenarios.

We turned away several otherwise-attractive features that violated
one of these. The audit hand-off (`docs/AUDIT_BRIEF.md`) lays out
exactly where the trust boundaries are so an external reviewer can
hold us to all three.

## What's next

- External security audit (gate 17). Engagement-letter drafts are in
  `outreach/audit-engagement-{trail-of-bits,zellic,ottersec}.md`.
- First DR drill on staging (gate 18). Pre-committed protocol in
  `docs/DR_DRILL_PLAN.md`.
- Whitepaper to arXiv (cs.CR or cs.DC) once the audit lands.
- v0.2 brings BFT consensus (per ADR-0002 triggers) and the public
  testnet (per ADR-0004 conditions).

## Try it

```bash
git clone https://github.com/grapheneaffiliate/Transformer-VM-Bank
cd Transformer-VM-Bank
cargo run -p psl-agent-sdk --release --example trader_agent
cargo run -p psl-agent-sdk --release --example service_agent
```

`REPRODUCE.md` has the full reproduction recipe. ~5 minutes on a
clean VM after toolchains land. ~30 minutes from scratch.

If you build something on PSL, write to us. If you find a bug,
`SECURITY.md` is the channel for security issues; everything else
goes in GitHub issues.

— PSL maintainers
