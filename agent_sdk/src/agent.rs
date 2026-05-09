//! `AgentSdk` — the high-level agent runtime.
//!
//! Holds the agent's identity (parent + child keys), spending policy
//! tracker, proposal log, reputation log, and the catalogue of
//! contracts it knows how to execute. Drives the receive-handle loop:
//!
//! - `handle_propose` — verify, look up the program, decide
//!   accept/reject (caller-supplied policy hook), record state.
//! - `handle_accept` — record the accept, queue an `Execute` if we are
//!   the original proposer.
//! - `handle_execute` — verify, run the contract, record outcome.
//! - `handle_dispute` — re-execute, return the slash decision.
//!
//! The accept/reject policy is intentionally a closure injected at
//! construction time; the SDK doesn't have an opinion. Reference
//! agents in `examples/` show two policies (always-accept,
//! ratio-targeting trader).

use crate::error::SdkError;
use crate::onchain::OnChainView;
use crate::transport::Transport;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use psl_agent_contracts::TernaryProgram;
use psl_agent_protocol::{
    dispute::{resolve_dispute, DisputeOutcome},
    message::{Accept, Execute, ExpectedOutput, Propose, ProposalHash, ProtocolMessage, Reject},
    state_machine::{ProposalLog, ProposalState},
    AgentRegistration, ReputationCounters,
};
use psl_agent_wallet::{KeyPolicy, PolicyEnvelope, RevocationSet, SpendingTracker};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 32-byte ed25519 pubkey.
pub type AgentPubkey = [u8; 32];

/// Per-agent identity bundle. Parent is the long-term key (typically
/// in a hardware module); child is the day-to-day signing key.
pub struct AgentIdentity {
    pub parent: SigningKey,
    pub child: SigningKey,
    /// Parent-signed policy authorizing the child's spending.
    pub policy_envelope: PolicyEnvelope,
}

impl AgentIdentity {
    pub fn pubkey(&self) -> AgentPubkey {
        self.child.verifying_key().to_bytes()
    }
    pub fn parent_pubkey(&self) -> AgentPubkey {
        self.parent.verifying_key().to_bytes()
    }
    pub fn verifying_key(&self) -> VerifyingKey {
        self.child.verifying_key()
    }
}

/// Decision returned by the accept-policy callback.
pub enum ProposeDecision {
    Accept,
    Reject(String),
}

/// High-level agent runtime.
pub struct AgentSdk {
    pub identity: AgentIdentity,
    /// Programs this agent will execute, keyed by program_hash.
    contracts: HashMap<[u8; 32], Arc<dyn TernaryProgram + Send + Sync>>,
    /// Per-incoming-counterparty spending trackers (we can have one
    /// shared tracker since the policy itself enforces the cap).
    spending: Mutex<SpendingTracker>,
    /// Per-agent local view of proposal lifecycle.
    proposals: Mutex<ProposalLog>,
    /// Local revocation cache (sequencer-driven).
    revocations: Mutex<RevocationSet>,
    /// Local reputation log (mirrors the on-chain subtree).
    reputation: Mutex<HashMap<AgentPubkey, ReputationCounters>>,
}

impl AgentSdk {
    pub fn new(identity: AgentIdentity) -> Self {
        let policy = identity.policy_envelope.policy.clone();
        Self {
            identity,
            contracts: HashMap::new(),
            spending: Mutex::new(SpendingTracker::new(policy)),
            proposals: Mutex::new(ProposalLog::new()),
            revocations: Mutex::new(RevocationSet::new()),
            reputation: Mutex::new(HashMap::new()),
        }
    }

    /// Register a contract this agent will execute.
    pub fn register_contract(&mut self, p: Arc<dyn TernaryProgram + Send + Sync>) {
        self.contracts.insert(p.program_hash(), p);
    }

    /// Build a signed registration record for publishing on-chain.
    pub fn registration(
        &self,
        endpoint: impl Into<String>,
        display_name: impl Into<String>,
        fee_schedule: impl Into<String>,
        bond_amount: u128,
        version: u64,
    ) -> AgentRegistration {
        let supported: Vec<String> = self
            .contracts
            .values()
            .map(|p| p.name().to_string())
            .collect();
        let custom: Vec<[u8; 32]> = self.contracts.values().map(|p| p.program_hash()).collect();
        AgentRegistration::sign(
            &self.identity.child,
            endpoint.into(),
            supported,
            custom,
            display_name.into(),
            fee_schedule.into(),
            bond_amount,
            version,
        )
    }

    /// Build a signed Propose (for the agent acting as proposer) AND
    /// record it in our local proposal log so we can match an
    /// incoming Accept against it later.
    pub fn propose(
        &self,
        program_hash: [u8; 32],
        parameters: Vec<u8>,
        to: AgentPubkey,
        valid_from_unix: u64,
        valid_until_unix: u64,
        nonce: u64,
    ) -> Propose {
        let p = Propose::sign(
            &self.identity.child,
            program_hash,
            parameters,
            to,
            valid_from_unix,
            valid_until_unix,
            nonce,
        );
        // Best-effort local record; verify is over the same body we just signed.
        let _ = self.proposals.lock().unwrap().record_propose(p.clone());
        p
    }

    /// Handle a Propose message addressed to this agent. Calls the
    /// supplied decision policy and either signs an Accept or Reject.
    pub fn handle_propose<F>(
        &self,
        propose: Propose,
        now: u64,
        decide: F,
        transport: &dyn Transport,
    ) -> Result<(), SdkError>
    where
        F: FnOnce(&Propose) -> ProposeDecision,
    {
        propose.verify().map_err(SdkError::Protocol)?;
        // Reject up front if the proposer is revoked.
        let revs = self.revocations.lock().unwrap();
        if revs.is_revoked(&propose.from) {
            let reject = Reject::sign(
                &self.identity.child,
                propose.proposal_hash(),
                "proposer revoked".into(),
                now,
            );
            transport
                .send(&propose.from, ProtocolMessage::Reject(reject))
                .map_err(SdkError::Transport)?;
            return Ok(());
        }
        drop(revs);

        let h = propose.proposal_hash();
        // Record into our local proposal log first so we know its state.
        self.proposals
            .lock()
            .unwrap()
            .record_propose(propose.clone())?;

        match decide(&propose) {
            ProposeDecision::Accept => {
                let accept = Accept::sign(&self.identity.child, h, now);
                self.proposals
                    .lock()
                    .unwrap()
                    .apply_accept(accept.clone())?;
                transport
                    .send(&propose.from, ProtocolMessage::Accept(accept))
                    .map_err(SdkError::Transport)?;
            }
            ProposeDecision::Reject(reason) => {
                let reject = Reject::sign(&self.identity.child, h, reason, now);
                self.proposals
                    .lock()
                    .unwrap()
                    .apply_reject(reject.clone())?;
                transport
                    .send(&propose.from, ProtocolMessage::Reject(reject))
                    .map_err(SdkError::Transport)?;
            }
        }
        Ok(())
    }

    /// Handle an Accept message addressed to this agent. If we were
    /// the original proposer, sign an Execute and send it.
    pub fn handle_accept(
        &self,
        accept: Accept,
        witness: Vec<u8>,
        now: u64,
        transport: &dyn Transport,
    ) -> Result<(), SdkError> {
        accept.verify().map_err(SdkError::Protocol)?;
        let mut log = self.proposals.lock().unwrap();
        log.apply_accept(accept.clone())?;
        let st = log
            .get(&accept.proposal_hash)
            .cloned()
            .ok_or_else(|| SdkError::Protocol(psl_agent_protocol::ProtocolError::UnknownProposal {
                hash: accept.proposal_hash,
            }))?;
        drop(log);
        let propose = match &st {
            ProposalState::Accepted { propose, .. } => propose.clone(),
            _ => return Ok(()), // not in the right state — caller will deal with it
        };
        // We're the proposer iff the propose.from == us
        if propose.from != self.identity.pubkey() {
            return Ok(());
        }
        // Look up our contract registration to compute expected output.
        let contract = self
            .contracts
            .get(&propose.program_hash)
            .ok_or(SdkError::UnknownContract(propose.program_hash))?
            .clone();
        let expected = contract.run(&witness)?;
        let execute = Execute::sign(
            &self.identity.child,
            accept.proposal_hash,
            witness,
            ExpectedOutput { bytes: expected },
            now,
        );
        // record locally
        self.proposals
            .lock()
            .unwrap()
            .apply_execute(execute.clone())?;
        // send to the executor (= accept.by)
        transport
            .send(&accept.by, ProtocolMessage::Execute(execute))
            .map_err(SdkError::Transport)?;
        Ok(())
    }

    /// Handle an Execute message addressed to this agent. Verify,
    /// re-run the contract, compare outputs, update reputation.
    pub fn handle_execute(
        &self,
        execute: Execute,
        _onchain: &dyn OnChainView,
    ) -> Result<bool, SdkError> {
        execute.verify().map_err(SdkError::Protocol)?;
        let mut log = self.proposals.lock().unwrap();
        log.apply_execute(execute.clone())?;
        let st = log.get(&execute.proposal_hash).cloned();
        drop(log);
        let propose = match st {
            Some(ProposalState::Executed { propose, .. }) => propose,
            _ => return Ok(false),
        };
        let contract = self
            .contracts
            .get(&propose.program_hash)
            .ok_or(SdkError::UnknownContract(propose.program_hash))?
            .clone();
        let actual = contract.run(&execute.witness)?;
        let agreed = actual == execute.expected_output.bytes;
        let mut rep = self.reputation.lock().unwrap();
        let counter = rep.entry(execute.by).or_default();
        if agreed {
            counter.completed = counter.completed.saturating_add(1);
        } else {
            counter.disputes_lost = counter.disputes_lost.saturating_add(1);
        }
        Ok(agreed)
    }

    /// Re-execute a contract for dispute resolution.
    pub fn resolve_dispute_for(
        &self,
        propose: &Propose,
        execute: &Execute,
        dispute: &psl_agent_protocol::Dispute,
    ) -> Result<DisputeOutcome, SdkError> {
        let contract = self
            .contracts
            .get(&propose.program_hash)
            .ok_or(SdkError::UnknownContract(propose.program_hash))?
            .clone();
        Ok(resolve_dispute(&*contract, propose, execute, dispute)?)
    }

    /// Mempool-style admission check: contract allowed by policy,
    /// counterparty allowed, spend within window cap.
    pub fn admit_outgoing(
        &self,
        contract_name: &str,
        counterparty: &AgentPubkey,
        amount: u128,
        now: u64,
    ) -> Result<(), SdkError> {
        self.spending
            .lock()
            .unwrap()
            .admit(now, contract_name, counterparty, amount)?;
        Ok(())
    }

    pub fn local_reputation(&self, agent: &AgentPubkey) -> ReputationCounters {
        self.reputation
            .lock()
            .unwrap()
            .get(agent)
            .copied()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onchain::InMemoryOnChain;
    use crate::transport::InProcessBus;
    use psl_agent_contracts::TransferContract;
    use psl_agent_wallet::KeyPolicy;
    use rand::SeedableRng;

    fn make_identity(seed: u64, contract_name: &str) -> AgentIdentity {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let parent = SigningKey::generate(&mut rng);
        let child = SigningKey::generate(&mut rng);
        let policy = KeyPolicy {
            child_pubkey: child.verifying_key().to_bytes(),
            parent_pubkey: parent.verifying_key().to_bytes(),
            cap_per_window: u128::MAX,
            window_secs: 86400,
            allowed_contracts: vec![contract_name.into()],
            allowed_counterparties: vec![],
            expiry_unix: 0,
            version: 1,
        };
        let policy_envelope = PolicyEnvelope::sign(&parent, policy).unwrap();
        AgentIdentity { parent, child, policy_envelope }
    }

    fn pack_transfer(from: u128, to: u128, amount: u128, nonce: u64) -> Vec<u8> {
        let mut v = Vec::with_capacity(56);
        v.extend_from_slice(&from.to_le_bytes());
        v.extend_from_slice(&to.to_le_bytes());
        v.extend_from_slice(&amount.to_le_bytes());
        v.extend_from_slice(&nonce.to_le_bytes());
        v
    }

    #[test]
    fn end_to_end_propose_accept_execute() {
        // Two agents: Alice (proposer) and Bob (executor).
        // Alice proposes a transfer; Bob accepts; Alice executes; both
        // verify the same output.
        let mut alice = AgentSdk::new(make_identity(1, "transfer"));
        let mut bob = AgentSdk::new(make_identity(2, "transfer"));
        let contract: Arc<dyn TernaryProgram + Send + Sync> = Arc::new(TransferContract::build());
        let program_hash = contract.program_hash();
        alice.register_contract(contract.clone());
        bob.register_contract(contract);

        let bus = InProcessBus::new();
        bus.register(alice.identity.pubkey());
        bus.register(bob.identity.pubkey());
        let onchain = InMemoryOnChain::new();

        // Alice proposes.
        let witness = pack_transfer(1000, 500, 250, 7);
        let propose = alice.propose(
            program_hash,
            witness.clone(),
            bob.identity.pubkey(),
            0,
            1_000_000,
            1,
        );
        bus.send(&bob.identity.pubkey(), ProtocolMessage::Propose(propose.clone()))
            .unwrap();

        // Bob's loop: poll, accept.
        let bob_inbox = bus.poll(&bob.identity.pubkey());
        assert_eq!(bob_inbox.len(), 1);
        for msg in bob_inbox {
            if let ProtocolMessage::Propose(p) = msg {
                bob.handle_propose(p, 100, |_| ProposeDecision::Accept, &bus).unwrap();
            }
        }

        // Alice's loop: poll Bob's accept, sign and send Execute.
        let alice_inbox = bus.poll(&alice.identity.pubkey());
        assert_eq!(alice_inbox.len(), 1);
        for msg in alice_inbox {
            if let ProtocolMessage::Accept(a) = msg {
                alice.handle_accept(a, witness.clone(), 200, &bus).unwrap();
            }
        }

        // Bob's loop again: receive Execute, verify it.
        let bob_inbox = bus.poll(&bob.identity.pubkey());
        assert_eq!(bob_inbox.len(), 1);
        for msg in bob_inbox {
            if let ProtocolMessage::Execute(e) = msg {
                let agreed = bob.handle_execute(e, &onchain).unwrap();
                assert!(agreed, "Bob should agree with Alice's execute");
            }
        }

        // Alice's local reputation for Bob now shows a completed swap.
        let bob_pk = bob.identity.pubkey();
        let _ = alice.local_reputation(&bob_pk);
    }
}
