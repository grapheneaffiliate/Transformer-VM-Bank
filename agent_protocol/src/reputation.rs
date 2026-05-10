//! Per-agent reputation counters. Each agent's pubkey maps to a small
//! struct of monotonic counters. The sequencer increments these as
//! contracts complete and disputes resolve. Other agents query the
//! reputation subtree before accepting proposals.
//!
//! Reputation is non-monetary — there is no token reward — but is
//! load-bearing for filtering counterparties.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReputationCounters {
    pub initiated: u64,
    pub completed: u64,
    pub disputed: u64,
    pub disputes_lost: u64,
}

impl ReputationCounters {
    /// Heuristic reputation score in [0, 1000]. Production may use a
    /// different formula; this is the default. 1000 = perfect, never
    /// lost a dispute, completed everything started.
    pub fn score(&self) -> u32 {
        if self.initiated == 0 {
            return 500; // neutral
        }
        let completion = (self.completed as u128 * 1000) / self.initiated.max(1) as u128;
        let dispute_penalty = if self.disputed == 0 {
            0u128
        } else {
            (self.disputes_lost as u128 * 1000) / self.disputed.max(1) as u128
        };
        let raw = completion.saturating_sub(dispute_penalty);
        raw.min(1000) as u32
    }
}

#[derive(Default, Debug)]
pub struct ReputationLog {
    by_agent: HashMap<[u8; 32], ReputationCounters>,
}

impl ReputationLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entry(&mut self, agent: [u8; 32]) -> &mut ReputationCounters {
        self.by_agent.entry(agent).or_default()
    }

    pub fn get(&self, agent: &[u8; 32]) -> ReputationCounters {
        self.by_agent.get(agent).copied().unwrap_or_default()
    }

    pub fn record_initiated(&mut self, agent: [u8; 32]) {
        let c = self.entry(agent);
        c.initiated = c.initiated.saturating_add(1);
    }
    pub fn record_completed(&mut self, agent: [u8; 32]) {
        let c = self.entry(agent);
        c.completed = c.completed.saturating_add(1);
    }
    pub fn record_dispute_opened(&mut self, agent: [u8; 32]) {
        let c = self.entry(agent);
        c.disputed = c.disputed.saturating_add(1);
    }
    pub fn record_dispute_lost(&mut self, agent: [u8; 32]) {
        let c = self.entry(agent);
        c.disputes_lost = c.disputes_lost.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_neutral_and_score_500() {
        let r = ReputationCounters::default();
        assert_eq!(r.score(), 500);
    }

    #[test]
    fn perfect_record_is_1000() {
        let r = ReputationCounters {
            initiated: 100,
            completed: 100,
            disputed: 0,
            disputes_lost: 0,
        };
        assert_eq!(r.score(), 1000);
    }

    #[test]
    fn lost_disputes_drag_score_down() {
        let r = ReputationCounters {
            initiated: 100,
            completed: 100,
            disputed: 10,
            disputes_lost: 5,
        };
        // completion = 1000, dispute_penalty = 500 → score = 500
        assert_eq!(r.score(), 500);
    }

    #[test]
    fn log_increments_and_reads_back() {
        let mut log = ReputationLog::new();
        let pk = [0x11u8; 32];
        log.record_initiated(pk);
        log.record_initiated(pk);
        log.record_completed(pk);
        let c = log.get(&pk);
        assert_eq!(c.initiated, 2);
        assert_eq!(c.completed, 1);
    }
}
