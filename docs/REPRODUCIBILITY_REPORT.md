# PSL Reproducibility Report

**Date prepared:** 2026-05-09. **Commit:** `10ac60b` (Phase B close).
**Audience:** auditor, institutional partner technical due diligence,
contributor reproducing the test suite from a clean clone.

## Summary

Every gate's acceptance criterion in `docs/STATUS.md` is reproducible
from a fresh `git clone` on a stock Linux VM. This document records
exact commands, exact toolchain versions, and wall-clock timings
captured during the gate-by-gate close.

## Reference environment

The Phase 2 closure session ran on a WSL2 / Ubuntu 22.04 host with
the following toolchain (`cargo --version` output preserved):

```
rustc 1.95.0 (59807616e 2026-04-14)
cargo 1.95.0 (f2d3ce0bd 2026-03-21)
```

Disk: ~5 GB available. Memory: ~32 GB. CPU: 8 logical (host;
varies for the user's own reproduction).

## How to reproduce on a clean cloud VM

These commands assume a fresh Ubuntu 24.04 LTS VM with internet
access. They install all toolchains, clone, build, test.

```bash
# 1. System packages
sudo apt-get update && sudo apt-get install -y \
    build-essential pkg-config curl git python3 python3-pip

# 2. Rust toolchain (pinned to 1.95.0)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain 1.95.0 --profile minimal
. "$HOME/.cargo/env"

# 3. Lean toolchain (for legacy gate 3 — pinned by lean-toolchain)
curl https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh -sSf \
    | sh -s -- -y
. "$HOME/.elan/env"

# 4. Clone
git clone https://github.com/grapheneaffiliate/Transformer-VM-Bank.git
cd Transformer-VM-Bank

# 5. Build entire workspace
cargo build --workspace --release         # ~60 s

# 6. Run all Rust tests
cargo test --workspace --release          # ~45 s for the new tests
                                          # (legacy gate-1 vector sweep
                                          # excluded from default test
                                          # set; runs separately
                                          # via tools/run_gate1_rust.py
                                          # — Tier 2)

# 7. Optional: legacy isolation guard
tools/ci/check_legacy_isolation.sh

# 8. Optional: SBOM + dependency hygiene
cargo install cargo-audit cargo-deny      # one-time
cargo audit
cargo deny check licenses bans advisories
tools/sbom.sh > /tmp/psl-sbom.txt

# 9. Optional: Lean proofs
(cd lean && lake build)                   # ~15 min first time (mathlib cache)

# 10. Run the reference agents
cargo run -p psl-agent-sdk --release --example trader_agent
cargo run -p psl-agent-sdk --release --example service_agent
```

**Total time on a 4-vCPU cloud VM, fresh clone, no caches: ~30 minutes
including toolchain install. ~5 minutes after toolchains land.**

## Gate-by-gate reproducibility

| Gate | Reproduction command                                                                  | Expected result                                          | Phase 2 closure session timing |
| ---  | ---                                                                                   | ---                                                      | ---                            |
| 2    | `cargo test -p psl-crypto --release`                                                  | 22 / 22 pass                                             | < 5 s                          |
| 3    | `cd lean && lake build`                                                               | mathlib cache + 16/17 modules; 3 documented sorrys remain | ~15 min cold, ~30 s warm       |
| 4    | `cargo test -p psl-sequencer --release --test integration`                            | sequencer + 3 followers, 100 blocks, 4 roots agree       | ~10 s                          |
| 5    | `cargo test -p psl-sequencer --release --test compliance`                             | 9 / 9                                                    | ~5 s                           |
| 6    | `cargo test -p psl-light-client --release`                                            | 8 / 8 (1000-balance + 6 adversarial)                     | ~5 s                           |
| 7    | `cargo run --bin issuer_demo -- --full-flow`                                          | full pilot flow exits 0                                  | ~3 s                           |
| 8    | retired per ADR-0001 — `cargo build -p psl-rust-runner --release` builds with deprecation warnings | builds                                                   | ~10 s                          |
| 10   | `cargo test -p psl-ternary-vm --release`                                              | 42 baseline + 11 proptest tests pass; exhaustive byte_add 131072/131072 in ~1.25 s | ~3 s                           |
| 11   | `cargo test -p psl-agent-contracts --release`                                         | 20 / 20 (8 contracts × multiple scenarios)               | < 1 s                          |
| 12   | `cargo test -p psl-agent-wallet --release`                                            | 20 baseline + 5 proptest tests pass                      | ~1 s                           |
| 13   | `cargo test -p psl-agent-protocol --release`                                          | 18 baseline + 7 adversarial dispute tests pass           | ~1 s                           |
| 14   | (covered by gate 13's dispute tests)                                                  | 3 dispute tests + 7 adversarial scenarios                | included above                 |
| 15   | `cargo run -p psl-agent-sdk --release --example trader_agent && --example service_agent` | both run end-to-end, prints transcript, exits 0          | < 1 s each                     |
| 16   | `cargo test -p psl-agent-sdk --release`                                               | 2 / 2                                                    | < 1 s                          |
| 17   | hand-off package (`docs/AUDIT_BRIEF.md`, `docs/SECURITY_REVIEW.md`, `tools/sbom.sh`) — awaits external auditor signature | docs present, build clean                  | 0 s                            |
| 18   | hand-off package (`docs/OPERATIONAL_READINESS.md`, `docs/runbooks/*`, `ops/*`) — awaits live DR drill | docs present, ops stack runs                                  | 0 s                            |

## Notes for the auditor reproducer

- The `cargo test --workspace` command excludes the legacy gate-1
  10k-vector sweep by default (those tests are `#[ignore]`'d
  because they need primitive weights `.bin` files that are
  Tier-2 reproducibility — see `REPRODUCE.md`). To run them:
  `cargo test -p psl-rust-runner --release --test parity -- --ignored`.
- Lean `lake build` requires network access to fetch the mathlib
  cache. Air-gapped reproduction is documented in `lean/README.md`
  (mirror the cache offline first).
- Agent SDK examples use the in-process `InProcessBus`; production
  transports (mutual-TLS HTTPS) are SDK caller responsibility and
  not covered by these reproduction commands.

## Drift-detection commitment

This document updates in the same commit as any gate-acceptance-
criterion change. CI step `ci/reproducibility-check` re-runs the
above commands on a clean Ubuntu 24.04 GitHub runner per release
tag and updates the timing column.

If you reproduce on different hardware and see materially different
timings, please file an issue with the VM specs — we want the
report to remain accurate across reasonable hardware variation.
