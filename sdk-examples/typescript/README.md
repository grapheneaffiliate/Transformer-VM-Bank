# PSL TypeScript SDK examples

```bash
cd sdk-examples/typescript
npm install
npm run trader     # happy path
npm run service    # dispute path
```

The `@psl/sdk` package is a napi-rs native addon over the same Rust
runtime as `agent_sdk/examples/`. Behavior is byte-identical.

## Expected output

`npm run trader`:
```
alice proposes transfer 250 to bob
bob accepts
alice executes
settled. alice balance = 750, bob balance = 250
```

`npm run service`:
```
bob signs Execute claiming all-zero output (lying)
alice opens Dispute
judge outcome: SlashExecutor
slash attributable to bob.pubkey = ...
```

## Caveats

Same as the Python examples:
- In-process bus, not real transport.
- Plain-bytes identity, not HSM/keychain.
- Reference quality, not production.

Production usage of `@psl/sdk` from a Node service would wire mutual-
TLS HTTPS as the transport, store keys via the OS keychain, and add
retry/backoff on transient failures. None of that is in the SDK
itself — the SDK exposes the protocol primitives, application
authors compose them.
