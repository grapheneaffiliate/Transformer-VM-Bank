# PSL Python SDK examples

```bash
# 1. Install the PSL Python SDK (UniFFI binding over agent_sdk).
pip install psl-sdk     # once published to PyPI; for now: pip install -e ../../agent_sdk/python

# 2. Run the happy-path example.
python trader_agent.py

# 3. Run the dispute-path example.
python service_agent.py
```

Both scripts mirror their Rust counterparts in
`agent_sdk/examples/`. Behavior is byte-identical — the Python SDK
is a UniFFI binding over the same Rust runtime.

## What you should see

`trader_agent.py`:
```
alice proposes transfer 250 to bob
bob accepts
alice executes
settled. alice balance = 750, bob balance = 250
```

`service_agent.py`:
```
bob signs Execute claiming all-zero output (lying)
alice opens Dispute (her local re-execution disagrees with bob's claim)
judge outcome: SlashExecutor(<bob_pubkey_hex>)
slash attributable to bob.pubkey = ...
```

## Caveats

These are reference examples — error handling is minimal, identity
loading reads keys in plain bytes, transport is the in-process bus.
Production usage requires:
- HSM / keychain for identity secrets, not flat files.
- Mutual-TLS HTTPS transport, not the in-process bus.
- Retry / backoff on transient failures.

The Rust example is the canonical one (`cargo run -p psl-agent-sdk
--release --example trader_agent`); the Python example exists
specifically to show that the SDK is genuinely cross-language.
