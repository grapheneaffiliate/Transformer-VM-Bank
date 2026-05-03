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

## Once Transformer-VM weights are available

Set `weights_dir` in the pilot config to point at the populated `weights/`
directory (after running `./tools/build_all_primitives.sh`). The pilot's
sequencer will switch automatically to the `SubprocessTraceExecutor`, and
trace_hash will reflect the actual transformer's predicted token sequence.
