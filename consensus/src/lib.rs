//! PSL consensus layer.
//!
//! The execution layer (tx → trace → state delta → MPT root) is identical
//! across both modes. Only block ordering differs:
//!
//! - **Sovereign mode** ([`sovereign`]): the sequencer simply produces and
//!   signs blocks; followers verify by re-execution. Single point of
//!   liveness; no point of safety failure (lies are publicly provable).
//!
//! - **Consortium mode** ([`abci`]): block ordering is BFT-consensus-driven.
//!   Per the P1 audit (`docs/CONSENSUS_DECISION.md`), v2 ships on
//!   tendermint-rs ABCI talking to a CometBFT binary. PSL implements the
//!   ABCI app; CometBFT runs the consensus.

pub mod sovereign;
pub mod abci;

use anyhow::Result;
use async_trait::async_trait;
use psl_sequencer::block::Block;
use psl_sequencer::tx::SignedTx;

#[async_trait]
pub trait Consensus: Send + Sync {
    /// Submit a tx to the consensus layer's mempool.
    async fn submit_tx(&self, tx: SignedTx) -> Result<()>;

    /// Block on the next finalized block.
    async fn next_block(&self) -> Result<Block>;

    /// Current finalized block height.
    fn height(&self) -> u64;

    /// Mode identifier — "sovereign", "abci-cometbft".
    fn mode(&self) -> &'static str;
}
