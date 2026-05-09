# PSL SDK examples

Reference clients for the PSL agent SDK in three languages:

| Language    | Path                          | Build / run                                            |
| ---         | ---                           | ---                                                    |
| Rust        | `agent_sdk/examples/`         | `cargo run -p psl-agent-sdk --release --example trader_agent` |
| Python      | `sdk-examples/python/`        | `python sdk-examples/python/trader_agent.py`           |
| TypeScript  | `sdk-examples/typescript/`    | `cd sdk-examples/typescript && npm run trader`         |

The Rust example is the **canonical** one. The Python and TypeScript
examples are wrappers over the same `agent_sdk` runtime via UniFFI
(Python) and napi-rs (TypeScript). They demonstrate how to:

1. Construct an agent identity (parent key + child key + signed
   spending policy).
2. Connect to a sequencer endpoint (or in-process bus for testing).
3. Send a `Propose` and handle the inbound `Accept`.
4. Sign and submit `Execute`.
5. Verify the result against a light-client proof.

The dispute path (the novel piece) is exercised by `service_agent` in
each language — Bob signs a malicious `Execute`; Alice opens a
`Dispute`; the judge agent re-executes the contract; SlashExecutor
outcome is attributable to Bob's pubkey.

## UniFFI bindings

The Python SDK is generated from `agent_sdk/uniffi.toml` (one-time
generation step; bindings ship in `sdk-examples/python/psl_sdk/`).

The TypeScript SDK is generated via napi-rs and ships as a native
addon (`sdk-examples/typescript/native/`). Both target Linux x86_64
and macOS aarch64 in v0.1.0; Windows is best-effort.

## Why three languages

PSL's agent layer is meant to be embedded in real applications. The
languages cover:
- **Rust**: high-performance services, embedded systems.
- **Python**: data-engineering pipelines, quants writing custom
  agents, ML research integrations.
- **TypeScript**: web frontends, mobile (React Native),
  Node-based bots.

Other languages (Swift, Kotlin, Go, Java) are architecturally trivial
to add via UniFFI but not shipped in v0.1.0 — file a request if you
need one.

## Caveats

- These examples use the in-process `InProcessBus` transport for
  determinism. Production usage requires a real transport
  (mutual-TLS HTTPS is the recommended default; transport is
  caller-wired, not in scope for the SDK).
- The Python and TypeScript examples are reference-quality, not
  production-quality. Specifically: error handling is minimal,
  there is no retry/backoff on transient failures, and the
  identity loading reads keys from disk in plain bytes (real
  deployments use HSM / keychain integration).
