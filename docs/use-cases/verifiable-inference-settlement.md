# Verifiable Inference Settlement — model-gated contracts on PSL

**Status:** reference implementation shipped (`risk_gated_transfer` +
`credit_risk_model_v1` + `inference_agent` example). Portfolio section
below is design-only.

**One sentence:** PSL's integer-only ternary VM makes a *machine-learning
inference* a bit-exact, re-executable artifact — so a model's output can
both **gate a payment** and **resolve the resulting dispute**, with no
oracle and no human in the loop.

---

## 1. What PSL is, and the capability nobody used yet

PSL (the Percepta Settlement Layer) is two things in one repo: a
deterministic settlement ledger, and an agent-to-agent transaction layer
whose defining trick is **dispute resolution by re-execution**. When two
agents disagree about the result of a contract, the chain does not call a
human arbiter or an oracle — it *re-runs the contract* on the disputed
input and compares bytes. That only works because every contract is a
**pure integer program** on a ternary VM (weights ∈ {−1, 0, +1}, integer
biases, ReLU, no floating point anywhere on the verifier path), so the
same input produces the *same bytes* on every conformant host
(`docs/ARCHITECTURE.md` § 0.8).

Here is the observation this document acts on:

> The PSL contract VM **is a neural-network VM** — but none of the eight
> contracts that shipped in v0.1.0 use it as one.

`transfer`, `swap`, the three `escrow_*` contracts, `time_locked_release`,
`multisig_2of3`, and `conditional_payment` all use the ternary VM purely
as an *adding machine* (carry/borrow chains for u128 balance math). Their
actual **decision** — should this payment fire? — is read from a
**trusted flag** in the witness:

| Contract              | How it decides to fire                                   |
| ---                   | ---                                                      |
| `conditional_payment` | `input[56] == 1` — a flag *someone* must set correctly   |
| `multisig_2of3`       | counts three supplied signature-flag bytes               |
| `time_locked_release` | compares a supplied `current_time` to `unlock_time`      |

In every case an off-chain party (an oracle, a signer, a clock) is
*trusted* to have set that flag. The verifier never re-derives the
judgment; it only re-checks the arithmetic that follows.

## 2. The novel capability: the model *is* the decision

Because `TernaryNetwork::forward` is bit-exact, a model's output is a
deterministic function of `(weights_hash, features)`. That means a
**neural-network inference can play the exact role the flag byte plays** —
except nobody has to be trusted to set it, because any participant can
recompute it:

1. **It gates the money.** APPROVE ⇒ the transfer executes; DENY ⇒ the
   canonical 40-byte no-op, identical to a failed guard in
   `conditional_payment`.
2. **It resolves the dispute.** When an executor lies about what the
   model decided, the judge re-runs the *same network* on the *same
   features*, gets the *same verdict*, and slashes the liar.

This is impossible on a conventional ML stack. Floating-point inference
reorders reductions per CPU/BLAS, so two honest verifiers can disagree on
the last bits of a logit — fine for serving a recommendation, **fatal for
a verifier** that must agree byte-for-byte. PSL's ternary integer kernel
removes that surface entirely, which is precisely what turns "the model
decided X" into a *protocol fact* rather than a claim.

We call a contract built this way a **model-gated contract**, and the
general pattern **verifiable inference settlement**.

## 3. What shipped (the reference implementation)

### 3.1 `risk_gated_transfer` (`agent_contracts/src/inference.rs`)

A transfer gated by a real ternary network instead of a flag. Wire
format:

```text
Input  (64 B): from(16) ‖ to(16) ‖ amount(16) ‖ nonce(8)
               ‖ income(2) ‖ debt(2) ‖ collateral(2) ‖ risk_flags(2)   (u16 LE features)
Output (40 B): new_from(16) ‖ new_to(16) ‖ new_nonce(8)
               — or 40 zero bytes on DENY / insufficient balance / overflow.
```

The contract's `run()` decodes the four features, calls
`risk_model.forward(...)` **on the verifier path**, and only then either
runs `wrapped_transfer` (APPROVE) or returns the no-op (DENY). The single
line `let decision = self.risk_model.forward(...)` is the whole novelty:
the gate is a forward pass, not a trusted flag.

### 3.2 `credit_risk_model_v1` — the embedded network

A 4-layer, integer-only ternary MLP adjudicating a micro-credit /
parametric-underwriting decision:

```text
  creditworthy := (income + collateral − debt) ≥ 500   (MIN_NET_CREDIT)
  within_risk  := risk_flags ≤ 10                       (MAX_RISK_FLAGS)
  APPROVE      := creditworthy AND within_risk
```

The `AND` is a genuine non-linearity — `ReLU(b1 + b2 − 1)` — which is the
point: it **cannot** be expressed as one linear threshold, so this is a
real (if small) network, not arithmetic in disguise. The threshold
indicators themselves use the same `ReLU(s − t + 1) − ReLU(s − t)`
thermometer trick the `byte_add_with_carry` primitive uses for its
decode. Every weight ∈ {−1, 0, +1}; every bias and activation is `i64`.

**Model governance falls out of content-addressing.** The contract's
`program_hash` commits to the model's `weights_hash` (alongside the
arithmetic sub-nets), so a specific model *is* a specific contract
identity. Change one weight and the `program_hash` changes — there is no
way to silently swap the model under a live contract.

### 3.3 `inference_agent` example (`agent_sdk/examples/inference_agent.rs`)

Two flows over the real SDK / protocol / dispute machinery:

- **Happy path** — a creditworthy, low-risk applicant; the model
  APPROVEs and the loan settles through the full propose → accept →
  execute loop, dispute-free.
- **Dispute path** — a *creditworthy-but-too-risky* applicant
  (`risk_flags = 40 > 10`) the model DENIES. A malicious executor signs
  an `Execute` claiming the loan disbursed anyway. The judge re-runs
  `credit_risk_model_v1`, reaches DENY, finds the claimed output cannot
  match, and returns `SlashExecutor`. The lie is *about what the model
  decided*, and re-execution catches it.

```bash
cargo run -p psl-agent-sdk --example inference_agent
cargo test -p psl-agent-contracts --lib inference   # 10 tests
```

### 3.4 Test coverage

`model_matches_ground_truth_on_grid` sweeps a feature grid straddling
both decision boundaries and asserts the ternary network agrees
bit-for-bit with a plain-Rust ground truth. Other tests pin the exact
boundaries (`≥ 500`, `≤ 10`), the DENY→no-op behavior, the
balance-precondition interaction, deterministic re-execution (16×), the
input-shape error, and that `program_hash` differs from a plain
`transfer` (i.e. it really folds the model in).

## 4. Why this is groundbreaking — and the honest limits

**Groundbreaking, precisely stated:** PSL is, to our knowledge, the first
settlement layer where a neural network's output is simultaneously the
contract's release condition and the dispute-resolution ground truth,
with *no trusted oracle* — enabled by removing floating point from the
verifier path. "Verifiable inference" usually means an expensive ZK proof
*about* a model run; here verification is just *re-running it*, because
the run is deterministic and cheap (integer add/subtract, ~1 MB weights,
edge-deployable).

**Honest limits (so the claim stays defensible):**

- `credit_risk_model_v1` is a small, **analytically constructed** ternary
  MLP — exactly like the existing `byte_*` primitives — not a *trained*
  network. The architecture supports deeper models; training a ternary
  net and importing its weights is future work.
- Features are public in the witness. **Confidential** features need the
  hybrid witness-encryption path (ADR-0011) — a natural follow-up (§ 5).
- This is not a general ML runtime. Determinism is load-bearing, so the
  same constraints that bound all PSL contracts apply (integer-only,
  bounded trace, no unbounded loops).

## 5. Portfolio: what the same primitive unlocks (design-only)

Each of these is a *different model* dropped into the same model-gated
pattern. None is built yet; all are buildable on today's kernel.

| Use case | Sketch | What it needs beyond today |
| --- | --- | --- |
| **Parametric insurance / auto-claims** | A payout network keyed on objective features (flight delay minutes, measured rainfall, sensor readings) decides claim/no-claim. Disputes = re-execute the adjudicator. No loss adjuster. | A claims model; an attested feed for the feature bytes. |
| **Verifiable inference marketplace** | Agent A pays agent B for an inference; B's claimed output is binding. If A disputes, the judge re-runs B's published model on A's input. Mispriced or wrong inferences get slashed. | Publishing models by `weights_hash`; a fee/escrow wrapper. |
| **Oracle-free eligibility / KYC-tier / sanctions gate** | A screening network maps declared attributes → allow/tier/deny, gating a payment or a registration. The decision is reproducible and auditable byte-for-byte. | A vetted screening model; governance over its versions. |
| **Deterministic auction settlement** | Sealed-bid winner + clearing price computed by a network over the bid vector. "Did the auctioneer pick the right winner?" becomes a re-execution check. | A winner/price network; bid-commitment scheme. |
| **Content-addressed credit bureaus** | A scoring model published by hash; lenders and borrowers both re-derive the score. Disputes about "what's my score" vanish — it's a function of `(model, features)`. | Feature standardization; model-versioning policy. |
| **Per-inference model licensing** | Each licensed inference is a settled `risk_gated_transfer`-style tx: pay-per-call where non-payment or output-tampering is slashable by re-execution. | Metering wrapper; royalty split contract. |
| **Confidential model-gated payments** | Combine model-gating with ADR-0011 forward-secret witness encryption so features are hidden from everyone except the parties + the dispute resolver. | Wire the KEM/AEAD witness path into the contract input. |
| **"Constitutional" agent spending policies** | An agent's autonomous spend gate is itself a network (risk/limit/intent classifier), so its guardrails are auditable and re-executable, not opaque code. | A policy model; integration with `agent_wallet` policies. |

## 6. How to author your own model-gated contract

1. **Build the model** as a `TernaryNetwork` (see `build_credit_model`):
   construct `SparseTernaryLayer`s, then
   `pack_weights_dual(name, in, out, &layers)` →
   `WeightsHeader::new(...)` → `TernaryNetwork::new(header, layers)`.
   Verify it against a plain-Rust ground truth in tests.
2. **Wrap it in a contract** implementing `TernaryProgram`: parse the
   witness, `model.forward(features)`, branch on the decision, reuse
   `wrapped_transfer` (or your own settlement logic) on APPROVE and
   `no_op_output` on DENY.
3. **Fold the model into identity**: pass the model network to
   `build_program_hashes(name, &[&model, &byte_add, &byte_sub])` so the
   `program_hash` commits to the exact weights.
4. **Register and propose** through `AgentSdk` exactly like any other
   contract — the dispute resolver re-executes it for free.

## 7. Pointers

- Contract + model + tests: `agent_contracts/src/inference.rs`
- Re-exports: `agent_contracts/src/lib.rs`
  (`RiskGatedTransfer`, `build_credit_model`)
- Example: `agent_sdk/examples/inference_agent.rs`
- The kernel property this all rests on: `docs/ARCHITECTURE.md` § 0.8
- Dispute mechanism: `agent_protocol/src/dispute.rs`,
  `README.md` "The agent layer in 60 seconds"
