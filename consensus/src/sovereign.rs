//! Sovereign-mode consensus.
//!
//! No consensus algorithm — the configured sequencer is the authority. Block
//! ordering is "whatever the sequencer signs in the order it signs it." Lies
//! are publicly provable via state-root mismatch on follower re-execution.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use psl_sequencer::block::Block;
use psl_sequencer::node::SequencerNode;
use psl_sequencer::tx::SignedTx;

use crate::Consensus;

pub struct Sovereign {
    pub node: Arc<SequencerNode>,
    pub finalized: Mutex<Vec<Block>>,
}

impl Sovereign {
    pub fn new(node: Arc<SequencerNode>) -> Self {
        Self { node, finalized: Mutex::new(Vec::new()) }
    }
}

#[async_trait]
impl Consensus for Sovereign {
    async fn submit_tx(&self, tx: SignedTx) -> Result<()> {
        let registry: &dyn Fn(u32) -> Option<_> = &|asset_id: u32| self.node.lookup_issuer(asset_id);
        let state = self.node.state.read().unwrap();
        let mut mp = self.node.mempool.write().unwrap();
        mp.ingress(tx, &state, registry)
    }

    async fn next_block(&self) -> Result<Block> {
        // The block-production loop runs in node.rs; here we just wait for the
        // next entry in the finalized queue. v1 stub returns an error to
        // preserve type-checking.
        anyhow::bail!("sovereign next_block: connect to node's block-production loop")
    }

    fn height(&self) -> u64 {
        self.node
            .last_header
            .read()
            .unwrap()
            .as_ref()
            .map(|h| h.block_n)
            .unwrap_or(0)
    }

    fn mode(&self) -> &'static str {
        "sovereign"
    }
}
