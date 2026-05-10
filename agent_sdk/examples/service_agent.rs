//! Reference service agent.
//!
//! Demonstrates the dispute path: a malicious executor signs an
//! Execute claiming a wrong output; the service agent (acting as
//! arbiter via deterministic re-execution) slashes them.
//!
//! Run with:
//!   cargo run -p psl-agent-sdk --example service_agent

use ed25519_dalek::SigningKey;
use psl_agent_contracts::{TernaryProgram, TransferContract};
use psl_agent_protocol::{
    dispute::{Dispute, DisputeOutcome},
    message::{Execute, ExpectedOutput},
};
use psl_agent_sdk::{AgentIdentity, AgentSdk, InMemoryOnChain, InProcessBus};
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

fn pack_transfer(from: u128, to: u128, amount: u128, nonce: u64) -> Vec<u8> {
    let mut v = Vec::with_capacity(56);
    v.extend_from_slice(&from.to_le_bytes());
    v.extend_from_slice(&to.to_le_bytes());
    v.extend_from_slice(&amount.to_le_bytes());
    v.extend_from_slice(&nonce.to_le_bytes());
    v
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[service_agent] dispute flow: malicious executor lies about output, gets slashed.");

    let mut alice = AgentSdk::new(make_identity(31, "transfer"));
    let mut bob = AgentSdk::new(make_identity(32, "transfer")); // executor (malicious)
    let mut judge = AgentSdk::new(make_identity(33, "transfer")); // re-executes for dispute
    let contract = Arc::new(TransferContract::build());
    alice.register_contract(contract.clone());
    bob.register_contract(contract.clone());
    judge.register_contract(contract.clone());

    let bus = InProcessBus::new();
    bus.register(alice.identity.pubkey());
    bus.register(bob.identity.pubkey());
    bus.register(judge.identity.pubkey());

    let mut onchain = InMemoryOnChain::new();
    onchain.register(alice.registration("https://alice.example/v1", "Alice", "0", 1_000, 1));
    onchain.register(bob.registration("https://bob.example/v1", "Bob", "0", 1_000, 1));

    let witness = pack_transfer(1000, 500, 250, 7);
    let actual = contract.run(&witness)?;
    println!(
        "[service_agent] true contract output: {} bytes",
        actual.len()
    );

    // Bob (the malicious "executor") signs an Execute claiming all-zero
    // output. Alice (the original proposer) sees the discrepancy and
    // submits a Dispute to the judge.
    let propose = alice.propose(
        contract.program_hash_v2(),
        witness.clone(),
        bob.identity.pubkey(),
        0,
        u64::MAX,
        1,
    );
    let proposal_hash = propose.proposal_hash();

    let lied_output = vec![0u8; actual.len()];
    let lied_execute = Execute::sign(
        &bob.identity.child,
        proposal_hash,
        witness.clone(),
        ExpectedOutput {
            bytes: lied_output.clone(),
        },
        100,
    );

    println!(
        "[service_agent] Bob signed an Execute claiming {} all-zero output bytes (LIE).",
        lied_output.len()
    );

    let dispute = Dispute::sign(
        &alice.identity.child,
        proposal_hash,
        witness.clone(),
        actual.clone(),
        200,
    );

    // Judge re-executes deterministically.
    let outcome = judge.resolve_dispute_for(&propose, &lied_execute, &dispute)?;
    match outcome {
        DisputeOutcome::SlashExecutor {
            executor_pubkey, ..
        } => {
            println!(
                "[service_agent] judge: SLASH executor {} (re-executed output ≠ executor's claim).",
                hex::encode(executor_pubkey)
            );
            assert_eq!(executor_pubkey, bob.identity.pubkey());
        }
        DisputeOutcome::DismissDispute { .. } => {
            panic!("expected SlashExecutor, got DismissDispute");
        }
    }

    println!(
        "[service_agent] flow complete: dispute resolved deterministically, no human arbiter."
    );
    Ok(())
}
