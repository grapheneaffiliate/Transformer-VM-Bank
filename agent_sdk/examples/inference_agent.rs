//! Reference **verifiable-inference** agent.
//!
//! Demonstrates the novel capability the rest of PSL makes possible but
//! the v0.1.0 contract library never used: a **neural network whose
//! output is a deterministic, re-executable settlement decision**.
//!
//! `risk_gated_transfer` releases money iff an integer-only ternary MLP
//! (`credit_risk_model_v1`) — evaluated on the applicant's features on
//! the verifier path — outputs APPROVE. Because the forward pass is
//! bit-exact on every conformant host, the *model's verdict* can play
//! the exact role a flag byte plays in `conditional_payment`: it gates
//! the transfer, and it resolves disputes by re-execution. No human
//! underwriter, no oracle, no off-chain score to appeal to.
//!
//! Two flows are shown:
//!   1. Happy path — the model APPROVES a creditworthy applicant and the
//!      payment settles through the full propose → accept → execute loop.
//!   2. Dispute path — the model DENIES a high-risk applicant; a
//!      malicious executor signs an Execute claiming the money moved
//!      anyway; the judge re-runs the model, gets DENY, and slashes him.
//!
//! Run with:
//!   cargo run -p psl-agent-sdk --example inference_agent

use ed25519_dalek::SigningKey;
use psl_agent_contracts::inference::pack_input;
use psl_agent_contracts::{RiskGatedTransfer, TernaryProgram};
use psl_agent_protocol::{
    dispute::{Dispute, DisputeOutcome},
    message::{Execute, ExpectedOutput, ProtocolMessage},
};
use psl_agent_sdk::{
    AgentIdentity, AgentSdk, InMemoryOnChain, InProcessBus, ProposeDecision, Transport,
};
use psl_agent_wallet::{KeyPolicy, PolicyEnvelope};
use rand::SeedableRng;
use std::sync::Arc;

fn make_identity(seed: u64, contract_name: &str) -> AgentIdentity {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let parent = SigningKey::generate(&mut rng);
    let child = SigningKey::generate(&mut rng);
    let policy = KeyPolicy {
        child_pubkey: child.verifying_key().to_bytes(),
        parent_pubkey: parent.verifying_key().to_bytes(),
        cap_per_window: u128::MAX,
        window_secs: 3600,
        allowed_contracts: vec![contract_name.into()],
        allowed_counterparties: vec![],
        expiry_unix: 0,
        version: 1,
    };
    let policy_envelope = PolicyEnvelope::sign(&parent, policy).unwrap();
    AgentIdentity {
        parent,
        child,
        policy_envelope,
    }
}

/// Happy path: a creditworthy, low-risk applicant. The model APPROVEs on
/// the verifier path and the transfer settles end-to-end.
fn happy_path() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== happy path: model APPROVES, payment settles (no underwriter) ===");

    let mut lender = AgentSdk::new(make_identity(41, "risk_gated_transfer"));
    let mut disburser = AgentSdk::new(make_identity(42, "risk_gated_transfer"));
    let contract = Arc::new(RiskGatedTransfer::build());
    lender.register_contract(contract.clone());
    disburser.register_contract(contract.clone());

    let bus = InProcessBus::new();
    bus.register(lender.identity.pubkey());
    bus.register(disburser.identity.pubkey());
    let onchain = InMemoryOnChain::new();

    // income 900, debt 300, collateral 200, risk_flags 5:
    //   net credit = 900 + 200 − 300 = 800 ≥ 500  ✓
    //   risk_flags = 5 ≤ 10                        ✓  → APPROVE
    let features = [900u16, 300, 200, 5];
    println!(
        "[lender]  applicant features {features:?} → model says {}",
        if contract.model_decision(features)? {
            "APPROVE"
        } else {
            "DENY"
        }
    );
    let witness = pack_input(1_000, 500, 250, 7, features);

    let propose = lender.propose(
        contract.program_hash_v2(),
        witness.clone(),
        disburser.identity.pubkey(),
        0,
        u64::MAX,
        1,
    );
    bus.send(
        &disburser.identity.pubkey(),
        ProtocolMessage::Propose(propose),
    )?;

    for msg in bus.poll(&disburser.identity.pubkey()) {
        if let ProtocolMessage::Propose(p) = msg {
            disburser.handle_propose(p, 100, |_| ProposeDecision::Accept, &bus)?;
        }
    }
    for msg in bus.poll(&lender.identity.pubkey()) {
        if let ProtocolMessage::Accept(a) = msg {
            lender.handle_accept(a, witness.clone(), 200, &bus)?;
        }
    }
    for msg in bus.poll(&disburser.identity.pubkey()) {
        if let ProtocolMessage::Execute(e) = msg {
            let agreed = disburser.handle_execute(e, &onchain)?;
            assert!(agreed, "honest execution must agree on re-run");
            println!("[disburser] re-ran contract: output agrees → loan disbursed, dispute-free.");
        }
    }
    Ok(())
}

/// Dispute path: a high-risk applicant the model DENIES. A malicious
/// executor claims the money moved anyway; the judge re-executes the
/// model and slashes him. The lie is *about what the model decided*.
fn dispute_path() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== dispute path: executor lies about the model's verdict, gets slashed ===");

    let lender = AgentSdk::new(make_identity(51, "risk_gated_transfer"));
    let bob = AgentSdk::new(make_identity(52, "risk_gated_transfer")); // malicious executor
    let mut judge = AgentSdk::new(make_identity(53, "risk_gated_transfer"));
    let contract = Arc::new(RiskGatedTransfer::build());
    judge.register_contract(contract.clone());

    // Creditworthy on the money side, but risk_flags = 40 > 10 → DENY.
    let denied_features = [900u16, 300, 200, 40];
    assert!(
        !contract.model_decision(denied_features)?,
        "this applicant must be denied"
    );
    let witness = pack_input(1_000, 500, 250, 7, denied_features);
    let true_output = contract.run(&witness)?; // 40 zero bytes — the model said no.
    println!(
        "[judge]   applicant features {denied_features:?} → model says DENY; \
         correct output is {} zero bytes.",
        true_output.len()
    );

    let propose = lender.propose(
        contract.program_hash_v2(),
        witness.clone(),
        bob.identity.pubkey(),
        0,
        u64::MAX,
        1,
    );
    let proposal_hash = propose.proposal_hash();

    // Bob signs an Execute claiming the *approved* transfer output — i.e.
    // that 250 units moved to the borrower — even though the model denied
    // the loan. (This is the byte string an APPROVE of the same transfer
    // would have produced.)
    let approved_output = contract.run(&pack_input(1_000, 500, 250, 7, [900, 300, 200, 5]))?;
    let lied_execute = Execute::sign(
        &bob.identity.child,
        proposal_hash,
        witness.clone(),
        ExpectedOutput {
            bytes: approved_output,
        },
        100,
    );
    println!("[bob]     signed Execute claiming the loan disbursed (LIE — model denied it).");

    let dispute = Dispute::sign(
        &lender.identity.child,
        proposal_hash,
        witness.clone(),
        true_output,
        200,
    );

    // The judge re-runs credit_risk_model_v1 on the same features. It is
    // a deterministic function of (weights_hash, features), so the judge
    // reaches the same verdict the model reached — DENY — and the claimed
    // "approved" output cannot match.
    match judge.resolve_dispute_for(&propose, &lied_execute, &dispute)? {
        DisputeOutcome::SlashExecutor {
            executor_pubkey, ..
        } => {
            assert_eq!(executor_pubkey, bob.identity.pubkey());
            println!(
                "[judge]   re-executed model: DENY ≠ Bob's claim → SLASH executor {}.",
                hex::encode(executor_pubkey)
            );
        }
        DisputeOutcome::DismissDispute { .. } => panic!("expected SlashExecutor"),
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[inference_agent] settlement gated by a ternary neural network.");
    println!("[inference_agent] the model's output IS the contract decision AND the dispute ground truth.");
    happy_path()?;
    dispute_path()?;
    println!("\n[inference_agent] done: a model adjudicated payments with no oracle and no human.");
    Ok(())
}
