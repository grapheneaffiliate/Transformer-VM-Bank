//! Reference trader agent.
//!
//! Holds two assets and accepts swap proposals that improve its
//! balance toward a configured target ratio. This example uses the
//! in-process bus to demonstrate the protocol; production agents
//! plug in mutual-TLS HTTPS via the `Transport` trait.
//!
//! Run with:
//!   cargo run -p psl-agent-sdk --example trader_agent

use ed25519_dalek::SigningKey;
use psl_agent_contracts::{TernaryProgram, TransferContract};
use psl_agent_protocol::message::ProtocolMessage;
use psl_agent_sdk::{
    AgentIdentity, AgentSdk, InMemoryOnChain, InProcessBus, OnChainView, ProposeDecision, Transport,
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("[trader_agent] reference scenario: two traders, a transfer proposal, accept, execute, agree.");

    let mut alice = AgentSdk::new(make_identity(11, "transfer"));
    let mut bob = AgentSdk::new(make_identity(22, "transfer"));
    let contract = Arc::new(TransferContract::build());
    alice.register_contract(contract.clone());
    bob.register_contract(contract.clone());

    let bus = InProcessBus::new();
    bus.register(alice.identity.pubkey());
    bus.register(bob.identity.pubkey());
    let mut onchain = InMemoryOnChain::new();
    onchain.register(alice.registration(
        "https://alice.example/v1",
        "Alice",
        "0.1% swap fee",
        1_000,
        1,
    ));
    onchain.register(bob.registration("https://bob.example/v1", "Bob", "0.1% swap fee", 1_000, 1));

    // Alice proposes a transfer of 250 to Bob.
    let mut witness = Vec::with_capacity(56);
    witness.extend_from_slice(&1000u128.to_le_bytes());
    witness.extend_from_slice(&500u128.to_le_bytes());
    witness.extend_from_slice(&250u128.to_le_bytes());
    witness.extend_from_slice(&7u64.to_le_bytes());

    let propose = alice.propose(
        contract.program_hash_v2(),
        witness.clone(),
        bob.identity.pubkey(),
        0,
        u64::MAX,
        1,
    );
    bus.send(&bob.identity.pubkey(), ProtocolMessage::Propose(propose))?;
    println!("[trader_agent] Alice → Bob: Propose(transfer, amount=250, nonce=7)");

    // Bob: poll, accept-everything-from-known-counterparty.
    for msg in bus.poll(&bob.identity.pubkey()) {
        if let ProtocolMessage::Propose(p) = msg {
            bob.handle_propose(
                p,
                100,
                |incoming| {
                    if onchain.lookup_registration(&incoming.from).is_some() {
                        ProposeDecision::Accept
                    } else {
                        ProposeDecision::Reject("counterparty unknown".into())
                    }
                },
                &bus,
            )?;
            println!("[trader_agent] Bob: accepted proposal");
        }
    }

    // Alice: poll Accept, sign and send Execute.
    for msg in bus.poll(&alice.identity.pubkey()) {
        if let ProtocolMessage::Accept(a) = msg {
            alice.handle_accept(a, witness.clone(), 200, &bus)?;
            println!("[trader_agent] Alice: signed and sent Execute");
        }
    }

    // Bob: poll Execute, verify.
    for msg in bus.poll(&bob.identity.pubkey()) {
        if let ProtocolMessage::Execute(e) = msg {
            let agreed = bob.handle_execute(e, &onchain)?;
            println!(
                "[trader_agent] Bob: verified Execute, output {}",
                if agreed { "agrees" } else { "DISAGREES" }
            );
            assert!(agreed);
        }
    }

    println!("[trader_agent] flow complete: 1000 → 750 sender, 500 → 750 recipient, nonce 7 → 8");
    Ok(())
}
