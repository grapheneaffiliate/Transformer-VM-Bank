//! PSL sovereign-mode sequencer.
//!
//! Single Rust binary that produces blocks. Tx pipeline:
//!   1. Mempool ingress (sig + nonce + frozen pre-checks, native).
//!   2. Block assembly (drain mempool, sort by primitive type).
//!   3. Trace execution (per-primitive, via TraceExecutor trait).
//!   4. Apply deltas to the SMT.
//!   5. Build BlockHeader, sign with sequencer key, persist + gossip.
//!
//! Followers run the same pipeline starting from a published block; any
//! discrepancy in `new_state_root` or `trace_hash` is a publicly-provable lie.

use anyhow::Result;
use clap::Parser;
use tracing::info;

use psl_sequencer::{config::Config, node::SequencerNode};

#[derive(Parser)]
#[command(version, about = "PSL sovereign-mode sequencer")]
struct Cli {
    /// Path to config TOML
    #[arg(short, long, default_value = "sequencer.toml")]
    config: String,

    /// Run in follower (verify-only) mode against a remote sequencer
    #[arg(long)]
    follower: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cfg = Config::load(&cli.config)?;
    info!("loaded config from {}", cli.config);

    let mut node = SequencerNode::new(cfg).await?;
    if let Some(remote) = cli.follower {
        node.run_follower(remote).await
    } else {
        node.run_sequencer().await
    }
}
