# PSL Compliance

How PSL meets the structural compliance requirements that any G20 jurisdiction
will impose on a settlement layer carrying tokenized claims.

## View keys

A regulator obtains read-only access to a subset of accounts via a `ViewKey`
issued by the asset's issuer:

```rust
struct ViewKey {
    regulator_pubkey: PublicKey,
    asset_id: u32,
    account_filter: Vec<PublicKey>,   // empty = any holder
    issuer_signature: Signature,      // signs (regulator_pubkey, asset_id, filter)
}
```

The sequencer's RPC endpoints accept a view-key-signed query and return the
account record + Merkle proof against the latest published state root. The
proof is unforgeable; the sequencer cannot lie about a balance without
producing a block whose `new_state_root` would diverge from any follower.

## Travel rule

Transfers of an asset above the issuer-configured threshold
(`IssuerRecord.travel_rule_threshold`) must include encrypted originator and
beneficiary metadata in the `originator_metadata` field of the SignedTx. The
mempool rejects high-value txs without this field. The encryption is
addressed to a regulator pubkey published with the asset (or to a default
FATF-compliant trusted-third-party intermediary; out of scope here, on the
deployment side).

For sub-threshold txs: no metadata required, full pseudonymity in the
trace-and-MPT layer.

## Freeze authority

Each asset's issuer (and only that issuer) can submit a `Freeze` transaction
that flips the frozen flag on a specific account. The tx MUST include a
`court_order_hash: Hash` — a 32-byte commitment to the off-chain legal
authorization. The sequencer logs `(block_n, frozen_account, court_order_hash,
asset_id, timestamp, issuer_pubkey)` immutably alongside the block.

A frozen account cannot be the `signer` of a transfer; subsequent transfer
attempts are rejected at the mempool. Mint and burn into a frozen account
remain possible (consistent with deposit-token freeze semantics — the issuer
can settle pending obligations into a frozen account).

## What PSL does NOT enforce

- **Sanctions screening** — out of scope for the chain. Issuers are
  responsible for their own off-chain sanctions checks before issuing or
  redeeming.
- **KYC/AML at issuance** — issuers run KYC; PSL only enforces that KYC'd
  users are the only ones with valid keys.
- **Cross-jurisdictional reporting** — view-keys provide the data; the
  reporting workflow is operational, not protocol-level.

## Reporting workflow (recommended)

1. Issuer registers each user's pubkey via off-chain KYC binding.
2. Regulator obtains a view-key from the issuer for accounts under
   jurisdiction.
3. Regulator can run an audit at any time — RPC query → Merkle proof →
   account state at any block height.
4. For high-value transfers: travel-rule metadata is encrypted to the
   regulator's key; regulator decrypts as part of their case-by-case audit.
