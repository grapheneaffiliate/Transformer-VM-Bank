//! Transport abstraction. The SDK is transport-agnostic; production
//! agents use mutual-TLS HTTPS, while tests / demos use the
//! in-process bus below.

use psl_agent_protocol::ProtocolMessage;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub trait Transport: Send + Sync {
    /// Deliver a message to the agent identified by pubkey. Returns
    /// `Ok(())` on success; transport errors propagate as Err.
    fn send(&self, to: &[u8; 32], msg: ProtocolMessage) -> Result<(), String>;

    /// Pull all queued messages addressed to this agent. Empties the
    /// inbox; the SDK loop is expected to drain on each iteration.
    fn poll(&self, me: &[u8; 32]) -> Vec<ProtocolMessage>;
}

/// One per-agent inbox. Wrapped in Arc<Mutex<…>> so the bus can
/// deliver messages while the agent's main loop drains.
pub type Mailbox = Arc<Mutex<VecDeque<ProtocolMessage>>>;

/// Synchronous in-process bus used by the reference agents and the
/// SDK end-to-end test. Each registered pubkey gets its own
/// `Mailbox`. Send pushes onto the recipient's mailbox; poll drains.
#[derive(Clone, Default)]
pub struct InProcessBus {
    inboxes: Arc<Mutex<std::collections::HashMap<[u8; 32], Mailbox>>>,
}

impl InProcessBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an agent's inbox. Idempotent.
    pub fn register(&self, pubkey: [u8; 32]) {
        let mut g = self.inboxes.lock().expect("inbox map poisoned");
        g.entry(pubkey)
            .or_insert_with(|| Arc::new(Mutex::new(VecDeque::new())));
    }
}

impl Transport for InProcessBus {
    fn send(&self, to: &[u8; 32], msg: ProtocolMessage) -> Result<(), String> {
        let g = self.inboxes.lock().map_err(|e| format!("inbox map: {e}"))?;
        let inbox = g
            .get(to)
            .ok_or_else(|| format!("no inbox registered for {to:?}"))?
            .clone();
        drop(g);
        inbox
            .lock()
            .map_err(|e| format!("inbox: {e}"))?
            .push_back(msg);
        Ok(())
    }

    fn poll(&self, me: &[u8; 32]) -> Vec<ProtocolMessage> {
        let g = self.inboxes.lock().expect("inbox map poisoned");
        if let Some(inbox) = g.get(me).cloned() {
            drop(g);
            let mut q = inbox.lock().expect("inbox poisoned");
            let drained: Vec<_> = q.drain(..).collect();
            drained
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psl_agent_protocol::{Accept, ProposalHash};
    use ed25519_dalek::SigningKey;
    use rand::SeedableRng;

    #[test]
    fn in_process_bus_round_trip() {
        let bus = InProcessBus::new();
        let alice_pk = [0xa1u8; 32];
        let bob_pk = [0xb2u8; 32];
        bus.register(alice_pk);
        bus.register(bob_pk);

        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        let bob_signing = SigningKey::generate(&mut rng);
        let h: ProposalHash = [0x55u8; 32];
        let accept = Accept::sign(&bob_signing, h, 100);
        bus.send(&alice_pk, ProtocolMessage::Accept(accept.clone())).unwrap();

        let inbox = bus.poll(&alice_pk);
        assert_eq!(inbox.len(), 1);
        assert!(matches!(&inbox[0], ProtocolMessage::Accept(a) if a.proposal_hash == h));
        // second poll drained
        assert_eq!(bus.poll(&alice_pk).len(), 0);
    }
}
