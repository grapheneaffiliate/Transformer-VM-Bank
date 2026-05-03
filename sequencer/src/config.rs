use anyhow::Result;
use psl_crypto::KeyPair;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Local data directory for sled blockstore + SMT snapshots.
    pub data_dir: String,
    /// Where to find specialized weights for each primitive.
    pub weights_dir: String,
    /// Where to find Transformer-VM checkout (for subprocess trace executor).
    pub transformer_vm_path: String,
    /// JSON-RPC bind address.
    pub rpc_listen: String,
    /// Sequencer keypair (hex-encoded 32-byte secret seed).
    #[serde(with = "hex::serde")]
    pub sequencer_secret: [u8; 32],
    /// Block production interval in milliseconds (sovereign mode only).
    pub block_interval_ms: u64,
    /// Max txs per block.
    pub block_size: usize,
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let bytes = std::fs::read_to_string(&path)?;
        let cfg: Self = toml::from_str(&bytes)?;
        Ok(cfg)
    }

    pub fn keypair(&self) -> KeyPair {
        KeyPair::from_seed(self.sequencer_secret)
    }
}
