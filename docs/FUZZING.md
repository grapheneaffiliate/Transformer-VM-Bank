# Fuzzing PSL

`cargo-fuzz` harnesses live alongside each crate (`<crate>/fuzz/`).
They use `libfuzzer-sys` and require nightly Rust.

## Install

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
```

## Run a single harness

```bash
# 1 CPU-hour budget (per the audit checklist)
cargo +nightly fuzz run -p psl-ternary-vm-fuzz unpack_weights -- -max_total_time=3600
```

## Targets

| Crate                  | Target                       | What it fuzzes                                   |
| ---                    | ---                          | ---                                              |
| `psl-ternary-vm-fuzz`  | `unpack_weights`             | Ternary weights file decoder                     |
| `psl-ternary-vm-fuzz`  | `byte_add_run`               | byte_add_with_carry encode → forward → decode    |
| `psl-agent-protocol-fuzz` | `decode_protocol_message` | JSON ProtocolMessage deserialization             |
| `psl-agent-contracts-fuzz` | `transfer_run`            | TransferContract::run on adversarial bytes       |
| `psl-agent-contracts-fuzz` | `swap_run`                | SwapContract::run on adversarial bytes           |

## Acceptance bar

Every target must run for **at least 1 CPU-hour** without finding
crashes or non-typed-error panics. The corpora produced by these
runs are committed under `tests/fuzz_corpora/<target>/` and act as
regression seeds for future runs.

## CI integration

CI runs each fuzz target for **5 minutes** per PR (smoke-mode) and
**1 hour** nightly. Crash files surface as artifacts on the run
page. See `.github/workflows/fuzz.yml` (added in the gate-17
finalization).
