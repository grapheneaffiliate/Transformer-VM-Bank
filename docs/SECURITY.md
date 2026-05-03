# PSL Security Model

## Trust boundaries

PSL has three concentric trust surfaces; each requires different verification:

### 1. State-transition arithmetic (innermost) — verified by transformer trace

Every primitive in `primitives/` compiles to a specialized transformer whose
output is bit-exactly equivalent to native WASM (gate 1). Anyone with the
weights, the witness inputs, and the predicted token sequence can re-verify
that arithmetic was correct.

**Trust assumption**: the Transformer-VM's analytical weight construction
preserves the WASM semantics (this is the load-bearing claim that PSL
inherits — gate 1 is its operational verification).

### 2. Authorization & ordering (middle) — verified by native code

Every signed tx is verified natively before invocation. Issuer-registry
authority lookups, nonce monotonicity, frozen-account checks, asset-id
matching, travel-rule metadata presence, and court-order hash presence on
freeze are all native checks (`sequencer/src/mempool.rs`).

**Trust assumption**: ed25519 (vendored ref10 once landed) and BLAKE3
(`blake3` crate) are correct. SBOM in this file pins exact upstream
commits; users running PSL nodes are responsible for validating.

### 3. Block ordering (outermost) — sovereign sig OR BFT

In sovereign mode the sequencer key is the trust root for ordering. Lies are
publicly provable via state-root re-execution; lies are also publicly
recoverable via fork-and-replace.

In consortium mode, BFT consensus replaces the single sequencer key; trust
shifts to the supermajority validator set. PSL ships on tendermint-rs ABCI +
CometBFT per the P1 audit (`docs/CONSENSUS_DECISION.md`).

## What the transformer trace does NOT prove

- **Signatures.** ed25519 verification is native; if the native code has a
  bug, the trace alone won't catch it. Mitigation: vendored ref10 from
  audited supercop; pin and review.
- **Hashes.** BLAKE3 collisions would forge proofs and trace_hashes; we
  inherit BLAKE3's collision resistance.
- **Issuer authority correctness.** The registry could be misconfigured at
  genesis. Mitigation: registry root contributes to the global state root,
  so any change to it (including malicious additions) shows up as a state
  delta that a regulator's view-key proof can audit.

## SBOM (target)

Once vendoring lands, this file pins:

| Crate / source | Pinned commit | Audit reference |
| --- | --- | --- |
| ed25519-dalek (Rust) | TBD | RustSec |
| blake3 (Rust) | TBD | upstream + Trail of Bits 2020 |
| ref10 ed25519 (C) | TBD | NaCl / supercop |
| BLAKE3 official C | TBD | upstream |

## Threat model

In scope:
- Malicious sequencer producing forged blocks → detected by follower
  re-execution + state-root mismatch.
- Compromised user keypair → only their own balance at risk.
- Compromised issuer authority → only that issuer's asset_id at risk;
  freeze/mint/burn powers are constrained to that asset.
- Network adversary delaying/reordering txs → causes liveness degradation
  in sovereign mode; resolved via BFT in consortium mode.

Out of scope:
- Compromise of the Transformer-VM weight-construction pipeline (gate 1
  catches divergence; the social process around weight publication carries
  the rest).
- Compromise of the user's hardware (keys, light-client integrity).
- Regulatory action against the chain itself (legal, not technical).

## Key responsibilities

- **System root authority** (sovereign mode): can register/deregister
  issuers; cannot mint/burn anyone's asset. Should be cold-stored.
- **Issuer authority**: can mint/burn/freeze its own asset. Per-issuer
  HSM-backed deployment recommended.
- **Sequencer**: can order tx and produce blocks; cannot forge state
  transitions (re-execution would diverge). Hot-key, rotated regularly.
- **Validators** (consortium mode): supermajority signs blocks. Each
  validator's key independently HSM-backed.
- **Regulators**: hold view-keys to query proofs for accounts within their
  jurisdiction. Read-only; cannot modify state.
