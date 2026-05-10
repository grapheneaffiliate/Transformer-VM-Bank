//! Proposal lifecycle state machine.
//!
//! ```text
//!   ┌──────────┐  reject ┌──────────┐
//!   │ Proposed │────────▶│ Rejected │   (terminal)
//!   └────┬─────┘         └──────────┘
//!        │
//!        ├── counter ──▶ Counter   (transition to a new Proposed via the new hash)
//!        │
//!        ├── accept  ──▶ Accepted  ──▶ execute ──▶ Executed (terminal)
//!        │
//!        └── timeout ──▶ Expired   (terminal — by clock, not by message)
//! ```
//!
//! `ProposalLog` indexes everything by `proposal_hash` so out-of-order
//! delivery and replays are absorbed naturally.

use crate::error::ProtocolError;
use crate::message::{Accept, CounterPropose, Execute, ProposalHash, Propose, Reject};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum ProposalState {
    Proposed {
        propose: Propose,
    },
    Accepted {
        propose: Propose,
        accept: Accept,
    },
    Rejected {
        propose: Propose,
        reject: Reject,
    },
    CounterProposed {
        propose: Propose,
        counter: CounterPropose,
    },
    Executed {
        propose: Propose,
        accept: Accept,
        execute: Execute,
    },
    Expired {
        propose: Propose,
        expired_at_unix: u64,
    },
}

impl ProposalState {
    fn label(&self) -> &'static str {
        match self {
            ProposalState::Proposed { .. } => "Proposed",
            ProposalState::Accepted { .. } => "Accepted",
            ProposalState::Rejected { .. } => "Rejected",
            ProposalState::CounterProposed { .. } => "CounterProposed",
            ProposalState::Executed { .. } => "Executed",
            ProposalState::Expired { .. } => "Expired",
        }
    }
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ProposalState::Rejected { .. }
                | ProposalState::Executed { .. }
                | ProposalState::Expired { .. }
        )
    }
}

#[derive(Default, Debug)]
pub struct ProposalLog {
    by_hash: HashMap<ProposalHash, ProposalState>,
}

impl ProposalLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ingest a fresh proposal. Verifies the signature; idempotent on
    /// the same hash (returns Ok without changing state).
    pub fn record_propose(&mut self, p: Propose) -> Result<ProposalHash, ProtocolError> {
        p.verify()?;
        let h = p.proposal_hash();
        // Idempotent re-insert
        if !self.by_hash.contains_key(&h) {
            self.by_hash
                .insert(h, ProposalState::Proposed { propose: p });
        }
        Ok(h)
    }

    /// Apply an accept. Errors if the proposal is unknown / not in
    /// `Proposed` / signed by someone other than the proposal's `to`.
    pub fn apply_accept(&mut self, a: Accept) -> Result<(), ProtocolError> {
        a.verify()?;
        let h = a.proposal_hash;
        let st = self
            .by_hash
            .remove(&h)
            .ok_or(ProtocolError::UnknownProposal { hash: h })?;
        let new_st = match st {
            ProposalState::Proposed { propose } => {
                if a.by != propose.to {
                    self.by_hash.insert(h, ProposalState::Proposed { propose });
                    return Err(ProtocolError::SignatureInvalid);
                }
                if a.accepted_at_unix > propose.valid_until_unix {
                    let exp = ProposalState::Expired {
                        propose,
                        expired_at_unix: a.accepted_at_unix,
                    };
                    self.by_hash.insert(h, exp);
                    return Err(ProtocolError::Expired {
                        expiry: 0,
                        now: a.accepted_at_unix,
                    });
                }
                ProposalState::Accepted { propose, accept: a }
            }
            other => {
                let event = "accept";
                let from = other.label();
                self.by_hash.insert(h, other);
                return Err(ProtocolError::IllegalTransition { from, event });
            }
        };
        self.by_hash.insert(h, new_st);
        Ok(())
    }

    pub fn apply_reject(&mut self, r: Reject) -> Result<(), ProtocolError> {
        r.verify()?;
        let h = r.proposal_hash;
        let st = self
            .by_hash
            .remove(&h)
            .ok_or(ProtocolError::UnknownProposal { hash: h })?;
        let new_st = match st {
            ProposalState::Proposed { propose } => {
                if r.by != propose.to {
                    self.by_hash.insert(h, ProposalState::Proposed { propose });
                    return Err(ProtocolError::SignatureInvalid);
                }
                ProposalState::Rejected { propose, reject: r }
            }
            other => {
                let event = "reject";
                let from = other.label();
                self.by_hash.insert(h, other);
                return Err(ProtocolError::IllegalTransition { from, event });
            }
        };
        self.by_hash.insert(h, new_st);
        Ok(())
    }

    pub fn apply_counter(&mut self, c: CounterPropose) -> Result<(), ProtocolError> {
        c.verify()?;
        let h = c.original_proposal_hash;
        let st = self
            .by_hash
            .remove(&h)
            .ok_or(ProtocolError::UnknownProposal { hash: h })?;
        let new_st = match st {
            ProposalState::Proposed { propose } => {
                if c.by != propose.to {
                    self.by_hash.insert(h, ProposalState::Proposed { propose });
                    return Err(ProtocolError::SignatureInvalid);
                }
                ProposalState::CounterProposed {
                    propose,
                    counter: c,
                }
            }
            other => {
                let event = "counter";
                let from = other.label();
                self.by_hash.insert(h, other);
                return Err(ProtocolError::IllegalTransition { from, event });
            }
        };
        self.by_hash.insert(h, new_st);
        Ok(())
    }

    pub fn apply_execute(&mut self, e: Execute) -> Result<(), ProtocolError> {
        e.verify()?;
        let h = e.proposal_hash;
        let st = self
            .by_hash
            .remove(&h)
            .ok_or(ProtocolError::UnknownProposal { hash: h })?;
        let new_st = match st {
            ProposalState::Accepted { propose, accept } => {
                // Execute must be sent by the original proposer.
                if e.by != propose.from {
                    self.by_hash
                        .insert(h, ProposalState::Accepted { propose, accept });
                    return Err(ProtocolError::SignatureInvalid);
                }
                ProposalState::Executed {
                    propose,
                    accept,
                    execute: e,
                }
            }
            other => {
                let event = "execute";
                let from = other.label();
                self.by_hash.insert(h, other);
                return Err(ProtocolError::IllegalTransition { from, event });
            }
        };
        self.by_hash.insert(h, new_st);
        Ok(())
    }

    /// Sweep the log for proposals that have passed their
    /// `valid_until_unix` and mark them Expired. Returns the set of
    /// hashes that flipped this sweep.
    pub fn expire_due(&mut self, now: u64) -> Vec<ProposalHash> {
        let mut expired = Vec::new();
        for (h, st) in self.by_hash.iter_mut() {
            if let ProposalState::Proposed { propose } = st {
                if now > propose.valid_until_unix {
                    let propose = propose.clone();
                    *st = ProposalState::Expired {
                        propose,
                        expired_at_unix: now,
                    };
                    expired.push(*h);
                }
            }
        }
        expired
    }

    pub fn get(&self, h: &ProposalHash) -> Option<&ProposalState> {
        self.by_hash.get(h)
    }

    pub fn len(&self) -> usize {
        self.by_hash.len()
    }
    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::ExpectedOutput;
    use ed25519_dalek::SigningKey;
    use rand::SeedableRng;

    fn sk(seed: u64) -> SigningKey {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        SigningKey::generate(&mut rng)
    }

    fn fresh_propose(alice: &SigningKey, bob_pk: [u8; 32], nonce: u64) -> Propose {
        Propose::sign(
            alice,
            psl_agent_contracts::ProgramHash([0xa1u8; 64]),
            vec![1, 2],
            bob_pk,
            0,
            1_000_000,
            nonce,
        )
    }

    #[test]
    fn happy_path_propose_accept_execute() {
        let alice = sk(1);
        let bob = sk(2);
        let bob_pk = bob.verifying_key().to_bytes();
        let mut log = ProposalLog::new();
        let p = fresh_propose(&alice, bob_pk, 1);
        let h = log.record_propose(p.clone()).unwrap();
        log.apply_accept(Accept::sign(&bob, h, 100)).unwrap();
        let exec = Execute::sign(
            &alice,
            h,
            vec![1, 2, 3],
            ExpectedOutput { bytes: vec![4] },
            200,
        );
        log.apply_execute(exec).unwrap();
        match log.get(&h).unwrap() {
            ProposalState::Executed { .. } => (),
            other => panic!("expected Executed, got {other:?}"),
        }
    }

    #[test]
    fn idempotent_propose_replay() {
        let alice = sk(1);
        let bob = sk(2);
        let mut log = ProposalLog::new();
        let p = fresh_propose(&alice, bob.verifying_key().to_bytes(), 1);
        let h1 = log.record_propose(p.clone()).unwrap();
        let h2 = log.record_propose(p.clone()).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn reject_after_propose_is_terminal() {
        let alice = sk(1);
        let bob = sk(2);
        let mut log = ProposalLog::new();
        let p = fresh_propose(&alice, bob.verifying_key().to_bytes(), 1);
        let h = log.record_propose(p).unwrap();
        log.apply_reject(Reject::sign(&bob, h, "no thanks".into(), 50))
            .unwrap();
        // can't accept after reject
        let r = log.apply_accept(Accept::sign(&bob, h, 60));
        assert!(matches!(
            r,
            Err(ProtocolError::IllegalTransition {
                event: "accept",
                ..
            })
        ));
    }

    #[test]
    fn counter_after_propose_transitions_to_counter() {
        let alice = sk(1);
        let bob = sk(2);
        let mut log = ProposalLog::new();
        let p = fresh_propose(&alice, bob.verifying_key().to_bytes(), 1);
        let h = log.record_propose(p).unwrap();
        log.apply_counter(CounterPropose::sign(&bob, h, vec![9, 8], 2))
            .unwrap();
        match log.get(&h).unwrap() {
            ProposalState::CounterProposed { .. } => (),
            other => panic!("expected CounterProposed, got {other:?}"),
        }
    }

    #[test]
    fn expired_when_clock_passes_valid_until() {
        let alice = sk(1);
        let bob = sk(2);
        let mut log = ProposalLog::new();
        // valid_until = 100
        let p = Propose::sign(
            &alice,
            psl_agent_contracts::ProgramHash([0xa1u8; 64]),
            vec![],
            bob.verifying_key().to_bytes(),
            0,
            100,
            1,
        );
        let h = log.record_propose(p).unwrap();
        let expired = log.expire_due(101);
        assert_eq!(expired, vec![h]);
        assert!(matches!(
            log.get(&h).unwrap(),
            ProposalState::Expired { .. }
        ));
    }

    #[test]
    fn accept_by_wrong_party_rejected() {
        let alice = sk(1);
        let bob = sk(2);
        let charlie = sk(3);
        let mut log = ProposalLog::new();
        let p = fresh_propose(&alice, bob.verifying_key().to_bytes(), 1);
        let h = log.record_propose(p).unwrap();
        // Charlie tries to accept a proposal addressed to Bob
        let r = log.apply_accept(Accept::sign(&charlie, h, 100));
        assert!(matches!(r, Err(ProtocolError::SignatureInvalid)));
    }
}
