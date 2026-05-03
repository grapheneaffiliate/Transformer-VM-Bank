//! ABCI-bridge consensus: PSL execution layer + external BFT (CometBFT).
//!
//! Per `docs/CONSENSUS_DECISION.md`, v2 of PSL ships on tendermint-rs ABCI
//! talking to a CometBFT binary running the consensus algorithm. This module
//! is the application-side ABCI handler; CometBFT runs as a sibling process
//! and pipes proposals/commits over the ABCI socket.
//!
//! For now this is a SCAFFOLD — the actual `tendermint-abci` integration
//! requires a non-trivial dependency stack that we'll add when the audit
//! verdict is revisited (target: 6 months after v1 sovereign pilot is live).

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

use psl_sequencer::block::Block;
use psl_sequencer::node::SequencerNode;
use psl_sequencer::tx::SignedTx;

use crate::Consensus;

pub struct AbciCometBft {
    pub node: Arc<SequencerNode>,
}

#[async_trait]
impl Consensus for AbciCometBft {
    async fn submit_tx(&self, _tx: SignedTx) -> Result<()> {
        anyhow::bail!("ABCI integration not yet implemented; see docs/CONSENSUS_DECISION.md")
    }

    async fn next_block(&self) -> Result<Block> {
        anyhow::bail!("ABCI integration not yet implemented")
    }

    fn height(&self) -> u64 {
        0
    }

    fn mode(&self) -> &'static str {
        "abci-cometbft"
    }
}

// ── ABCI message handlers (will be wired into tendermint-abci once added) ──
//
// CheckTx       → mempool::validate (already implemented, can be reused)
// PrepareProposal → drain mempool, order txs by primitive type
// ProcessProposal → re-execute traces and verify roots match (follower mode)
// FinalizeBlock → produce_block path, sign new header
// Commit         → persist new state to sled, advance head
//
// Each handler delegates to existing SequencerNode methods. The wiring is
// mechanical once the tendermint-abci dependency is added.
