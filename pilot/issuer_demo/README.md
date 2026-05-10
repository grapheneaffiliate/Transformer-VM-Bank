# Pilot: issuer_demo

End-to-end demonstration of the PSL flow:

1. Register an issuer for `asset_id = 1` ("USD-DEMO").
2. Mint 1,000,000 USD-DEMO to a treasury account.
3. Treasury transfers 100 USD-DEMO to a customer.
4. Customer transfers 50 USD-DEMO to a merchant.
5. Issuer burns 100 USD-DEMO from the treasury.
6. Light client (in-process) verifies the merchant's balance against the
   final block header's `new_state_root` via a Merkle proof.

This is gate 7 in `docs/ARCHITECTURE.md`. The pilot uses the
`NativeTraceExecutor` (Rust simulation of the C primitives) so it runs
without Transformer-VM. The trace_hash in this mode is a fixed marker; a
real follower verifying the published headers would only accept blocks where
`trace_hash` matches a re-execution of the actual specialized transformer.

## Run

```bash
cargo run --bin issuer_demo -- --full-flow
```

Expected output: log lines for each step, ending with
`light-client verified: merchant balance = 50`.

## Trace executor

> **Status note (2026-05-09):** As of v0.1.0 the canonical execution
> engine is the **ternary integer kernel** (`ternary_vm/`) per
> [ADR-0001](../../docs/decisions/0001-retire-legacy-fp64-runner.md).
> The fp64 autoregressive `SubprocessTraceExecutor` referenced
> historically below is on the legacy path (`legacy/rust_runner/`,
> frozen). New verifiers using the canonical engine compare bytes
> against the ternary kernel's deterministic forward pass, not against
> a transformer's predicted token sequence. The pilot's
> `NativeTraceExecutor` was always Rust-native so it remains in use
> here unchanged.

### Historical (Phase 1.5 era)

Setting `weights_dir` in the pilot config to point at a populated
`weights/` directory (built via `./tools/build_all_primitives.sh`) used
to switch the pilot's sequencer to `SubprocessTraceExecutor`. That path
is no longer the canonical one; for the trace contract that *is*
canonical, see [`docs/ARCHITECTURE.md § 0.2`](../../docs/ARCHITECTURE.md).
