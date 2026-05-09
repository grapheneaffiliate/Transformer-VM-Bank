# ADR-0004 — Public test network: deferred to v0.2

**Status:** accepted.
**Date:** 2026-05-09.
**Deciders:** PSL maintainers.

## Context

A public test network ("testnet") is the standard developer-facing
artifact for a settlement layer: a long-running instance with a
faucet, public RPC, public block explorer, and stable invariants
that external developers can build against without coordinating with
the core team.

For v0.1.0 we have to decide whether to ship one.

## Decision

**Defer testnet to v0.2.0.** The reference deployment under `infra/`
plus `cargo run --example trader_agent` is the v0.1.0 developer
on-ramp.

## Rationale

A public testnet is operationally non-trivial:
- Stable RPC endpoint with TLS, rate-limiting, abuse prevention.
- Faucet that hands out test tokens without becoming a free DOS
  vector.
- Public block explorer (third-party hosting, monitoring).
- Stability commitment — once an external developer integrates, we
  cannot reset the chain without giving notice.
- 24/7-ish uptime expectation, even if soft.

For v0.1.0 we have:
- An untested DR drill (gate 18 is 🟢, not ✅).
- An unstarted external audit (gate 17 is 🟢, not ✅).
- BFT consensus deferred (ADR-0002).

Operating a public testnet under those constraints would either
require us to make commitments we can't keep, or to caveat the
testnet so heavily that it provides little developer value. Either
outcome is worse than not having a testnet.

The substitute path for an interested external developer:

```bash
git clone https://github.com/grapheneaffiliate/Transformer-VM-Bank
cd Transformer-VM-Bank
cargo run -p psl-agent-sdk --release --example trader_agent
cargo run -p psl-agent-sdk --release --example service_agent
# then build their own agent against the same SDK
```

Plus the reference Terraform under `infra/` if they want a private
test network in their own cloud account.

## v0.2 trigger conditions

We commit to a public testnet when **all** of the following are true:
1. Gate 17 (external audit) is ✅ with no unaddressed HIGH findings.
2. Gate 18 (DR drill) is ✅ with at least one passing drill.
3. ADR-0002 trigger fires for BFT consensus, OR we explicitly decide
   sovereign-mode testnet is acceptable for the developer audience.
4. There is a designated operator with on-call capacity.

Until then: private deployments via `infra/`, no public face.

## Consequences

- v0.1.0 launch story is repository + whitepaper + reference
  deployment, not testnet + faucet + explorer.
- Some external developers will bounce because they want a hosted
  testnet. Acceptable cost.
- v0.2 scope grows by one substantial item.

## Alternatives considered

- **Ship a public testnet at v0.1.0** — rejected per rationale above.
- **Ship a "developer testnet" with explicit "may reset weekly"
  caveat** — considered. Rejected because in practice nobody reads
  the caveat, and a reset still breaks everyone integrated. Better
  to wait until we can commit to stability.
- **Ship the testnet as a docker-compose anyone can run locally**
  — already covered by `infra/` plus `cargo run --example`. The
  word "testnet" implies hosted; we don't host yet.
