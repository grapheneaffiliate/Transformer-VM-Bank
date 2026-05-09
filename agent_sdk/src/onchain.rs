//! Adapter for the on-chain views the agent reads (registry,
//! reputation, revocation). The SDK is storage-agnostic — concrete
//! implementations (sequencer-backed, light-client-backed, in-memory
//! for tests) plug in via this trait.

use psl_agent_protocol::{AgentRegistration, ReputationCounters};
use std::collections::HashMap;

pub trait OnChainView: Send + Sync {
    /// Look up an agent's registration by pubkey, if any.
    fn lookup_registration(&self, pubkey: &[u8; 32]) -> Option<AgentRegistration>;

    /// Read the reputation counters for an agent.
    fn lookup_reputation(&self, pubkey: &[u8; 32]) -> ReputationCounters;

    /// True iff the pubkey has been revoked.
    fn is_revoked(&self, pubkey: &[u8; 32]) -> bool;
}

/// In-memory `OnChainView` for tests / reference agent demo. Not
/// persistent; not intended for production.
#[derive(Default)]
pub struct InMemoryOnChain {
    pub registrations: HashMap<[u8; 32], AgentRegistration>,
    pub reputation: HashMap<[u8; 32], ReputationCounters>,
    pub revoked: std::collections::HashSet<[u8; 32]>,
}

impl InMemoryOnChain {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, reg: AgentRegistration) {
        self.registrations.insert(reg.pubkey, reg);
    }
    pub fn revoke(&mut self, pubkey: [u8; 32]) {
        self.revoked.insert(pubkey);
    }
    pub fn set_reputation(&mut self, pubkey: [u8; 32], r: ReputationCounters) {
        self.reputation.insert(pubkey, r);
    }
}

impl OnChainView for InMemoryOnChain {
    fn lookup_registration(&self, pubkey: &[u8; 32]) -> Option<AgentRegistration> {
        self.registrations.get(pubkey).cloned()
    }
    fn lookup_reputation(&self, pubkey: &[u8; 32]) -> ReputationCounters {
        self.reputation.get(pubkey).copied().unwrap_or_default()
    }
    fn is_revoked(&self, pubkey: &[u8; 32]) -> bool {
        self.revoked.contains(pubkey)
    }
}
