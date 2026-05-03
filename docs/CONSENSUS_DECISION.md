# Consensus Layer Decision: malachitebft-rs Audit

**Date:** 2026-05-03
**Decision driver:** Pick consensus for a financial settlement chain (50–150 validators, leader rotation, stable app-logic API).
**Candidates:** (a) malachitebft-rs, (b) tendermint-rs ABCI + Go Tendermint, (c) defer.

---

## Headline finding: ownership has moved

Informal Systems' Malachite was **acquired by Circle in August 2025**; nine engineers moved with it. Active development is now at **`github.com/circlefin/malachite`**, not `informalsystems/malachite` (the latter has 9 stars and is a stale mirror; canonical repo has 404 stars / 120 forks, last push 2026-04-22). Apache-2.0 license preserved. The engine powers Circle's Arc L1 stablecoin chain.

## 1. Last release / shipping cadence

- Latest tag: **v0.6.0-rc.3, 2025-10-17** (release candidate, ~6.5 months old as of 2026-05-03).
- Prior GA: **v0.5.0, 2025-08-01**. Cadence pre-acquisition was monthly; post-acquisition has slowed to RCs only — no GA cut in 6+ months.
- Repo is *not* archived; commits continue (pushed_at 2026-04-22). Shipping is active but no stable GA in H1 2026.
- README still self-labels as **alpha software, not externally audited, "use at your own risk."**

## 2. Issue close rate

- 80 open issues against the active repo; long-tail of unresolved correctness bugs (e.g., "processes can get stuck during ValueSync," "consensus does not transition Ready→Running") have sat open since Sept 2025. Several `need-triage` labels indicate backlog.
- This is consistent with a team mid-integration into a new parent org, not a hardened library.

## 3. Test coverage on the core consensus loop

Real and non-trivial:
- **Quint formal specs** at `specs/consensus/quint/` (`consensus.qnt`, `votekeeper.qnt`, `driver.qnt`, `statemachineAsync.qnt`, `TendermintDSL.qnt`).
- **Model-based tests** at `code/crates/test/mbt/` consume Quint traces to drive the Rust state machine.
- Integration harness at `code/crates/test/{framework,app,tests}` plus `mempool` and `proto` test crates.
- No dedicated `cargo-fuzz` corpus visible; coverage of Byzantine adversary scenarios via Quint MBT, not fuzzing.

Verdict: stronger formal-methods story than tendermint-rs has ever had; weaker negative-path fuzzing.

## 4. Application API surface (where execution layer plugs in)

- Two integration paths: low-level `informalsystems-malachitebft-core-consensus` traits, and the higher-level **`informalsystems-malachitebft-app-channel`** crate. The channel API is the recommended seam: app receives `AppMsg`, replies with `ConsensusMsg` / `NetworkMsg` over tokio/ractor channels.
- API has churned across 0.1→0.5 (April→Aug 2025). The 0.6 RC line introduces further changes; channel enums have gained variants between minors. Not yet a 1.0 stability promise.
- Reference integration: **`circlefin/malaketh-layered`** wires Malachite to an Ethereum execution client via the Engine API — closest analogue to our use case and worth mining for plug-in patterns.

## 5. Production deployments

- **Circle Arc** (stablecoin L1) is the flagship target — not yet GA on mainnet as of this audit.
- **Starknet decentralized sequencer** (Madara, Pathfinder, Juno integrations) targeted end-2025 mainnet, full decentralization 2026; integration testing reported late 2024, no mainnet postmortems published.
- **No third-party operational postmortems** found. Zero public incident reports from validator operators running malachitebft-rs in anger.

## 6. Verdict — **(c) Defer, with a hard criterion**

Recommendation: **defer the malachite commitment; ship the MVP on tendermint-rs ABCI driving a Go Tendermint (CometBFT) binary**, and revisit malachite at our Q4 2026 architecture review.

Reasoning:
1. A financial settlement chain cannot run on software self-labeled alpha and unaudited.
2. No GA release in 6+ months and an 80-issue backlog during a corporate transition (Informal→Circle) is exactly the wrong window to lock in.
3. The app-channel API is still adding variants between minors — porting cost will recur.
4. CometBFT + ABCI has 5+ years of validator-operator postmortems, hundreds of live chains, and a frozen wire protocol; it matches the 50–150 validator profile precisely.

**Reconsideration trigger (any one):** (i) malachite cuts a 1.0 with semver guarantees on `app-channel`, (ii) Circle Arc runs on mainnet for 90 days without consensus halt, (iii) an external audit of `core-consensus` is published. Re-audit within 30 days of any trigger.

---

## Sources

- https://github.com/circlefin/malachite (canonical repo, tags, README)
- https://github.com/informalsystems/malachite (now-stale mirror; production warning)
- https://crates.io/crates/informalsystems-malachitebft-core-consensus
- https://docs.rs/informalsystems-malachitebft-app-channel/latest/informalsystems_malachitebft_app_channel/
- https://github.com/circlefin/malachite/tree/main/specs/consensus/quint (Quint specs)
- https://github.com/circlefin/malaketh-layered (reference execution-layer integration)
- https://www.prnewswire.com/news-releases/informal-systems-announces-malachite-acquisition-by-circle-to-power-new-arc-blockchain-network-302532317.html
- https://www.circle.com/blog/introducing-arc-an-open-layer-1-blockchain-purpose-built-for-stablecoin-finance
- https://informal.systems/blog/malachite-decentralize-whatever
- https://www.starknet.io/blog/decentralized-starknet-2025/
