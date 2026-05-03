//! Sequencer node: top-level orchestrator.

use anyhow::Result;
use psl_crypto::{hash_bytes, sign, KeyPair};
use std::sync::{Arc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{info, warn};

use crate::block::{combined_trace_hash, tx_list_hash, Block, BlockHeader};
use crate::config::Config;
use crate::issuer_registry::IssuerRecord;
use crate::mempool::Mempool;
use crate::state::State;
use crate::trace::{NativeTraceExecutor, SubprocessTraceExecutor, TraceExecutor, Witness};
use crate::tx::{SignedTx, TxKind};

pub struct SequencerNode {
    pub cfg: Config,
    pub keypair: KeyPair,
    pub state: Arc<RwLock<State>>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub trace: Arc<dyn TraceExecutor>,
    pub last_header: Arc<RwLock<Option<BlockHeader>>>,
}

impl SequencerNode {
    pub async fn new(cfg: Config) -> Result<Self> {
        let kp = cfg.keypair();
        let state = Arc::new(RwLock::new(State::new()));
        let mempool = Arc::new(RwLock::new(Mempool::new(10_000)));
        // Choose trace executor based on availability of weights.
        let weights = std::path::PathBuf::from(&cfg.weights_dir);
        let trace: Arc<dyn TraceExecutor> = if weights.exists()
            && weights.join("ledger_transfer.bin").exists()
        {
            info!("using SubprocessTraceExecutor with weights at {}", cfg.weights_dir);
            Arc::new(SubprocessTraceExecutor {
                transformer_vm_path: cfg.transformer_vm_path.clone().into(),
                weights_dir: weights,
            })
        } else {
            warn!("weights/ missing → using NativeTraceExecutor (DEV ONLY, trace_hash is a marker)");
            Arc::new(NativeTraceExecutor)
        };
        Ok(Self {
            cfg,
            keypair: kp,
            state,
            mempool,
            trace,
            last_header: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn run_sequencer(&mut self) -> Result<()> {
        info!("sovereign sequencer starting");
        let mut tick = interval(Duration::from_millis(self.cfg.block_interval_ms));
        loop {
            tick.tick().await;
            if let Err(e) = self.produce_block().await {
                warn!("block production error: {e}");
            }
        }
    }

    pub async fn run_follower(&mut self, _remote: String) -> Result<()> {
        // Follower mode: poll the remote sequencer for blocks, re-execute traces,
        // verify roots match. Out of scope for the v1 skeleton — the test harness
        // exercises the same code path in-process.
        warn!("follower mode is a stub for v1; see tests/test_sequencer_followers.rs");
        Ok(())
    }

    async fn produce_block(&self) -> Result<()> {
        let txs: Vec<SignedTx> = {
            let mut mp = self.mempool.write().unwrap();
            if mp.is_empty() {
                return Ok(());
            }
            mp.drain(self.cfg.block_size)
        };
        if txs.is_empty() {
            return Ok(());
        }

        let prev_state_root;
        let prev_registry_root;
        let parent_hash;
        let block_n;
        {
            let s = self.state.read().unwrap();
            prev_state_root = s.accounts_root();
            prev_registry_root = s.registry_root();
            let lh = self.last_header.read().unwrap();
            (parent_hash, block_n) = match &*lh {
                Some(h) => (h.header_hash(), h.block_n + 1),
                None => ([0u8; 32], 0),
            };
        }

        let mut per_tx_traces = Vec::with_capacity(txs.len());
        let epoch = (chrono_now_ms() / 1000) as u32;
        for tx in &txs {
            let witness = self.assemble_witness(tx, epoch)?;
            let res = self.trace.execute(tx, &witness)?;
            if res.success {
                let mut s = self.state.write().unwrap();
                for acc in &res.updated_accounts {
                    s.put_account(*acc);
                }
            }
            per_tx_traces.push(res.trace_hash);
        }

        let new_state_root;
        let new_registry_root;
        {
            let s = self.state.read().unwrap();
            new_state_root = s.accounts_root();
            new_registry_root = s.registry_root();
        }

        let header = BlockHeader {
            block_n,
            parent_hash,
            prev_state_root,
            tx_list_hash: tx_list_hash(&txs),
            trace_hash: combined_trace_hash(&per_tx_traces),
            new_state_root,
            issuer_registry_root: new_registry_root,
            timestamp_ms: chrono_now_ms(),
            sequencer_pubkey: self.keypair.public(),
            sequencer_sig: [0u8; 64],
        };

        let signed = self.sign_header(header);
        let _block = Block { header: signed.clone(), txs };
        info!(
            "produced block {} with {} txs, root={}",
            signed.block_n,
            _block.txs.len(),
            hex::encode(signed.new_state_root)
        );
        *self.last_header.write().unwrap() = Some(signed);
        Ok(())
    }

    fn assemble_witness(&self, tx: &SignedTx, epoch: u32) -> Result<Witness> {
        let s = self.state.read().unwrap();
        let amount = tx.amount;
        let flag = tx.flag;
        let accounts = match tx.kind {
            TxKind::Freeze | TxKind::Mint | TxKind::Burn => {
                let target = match tx.kind {
                    TxKind::Mint | TxKind::Freeze => tx.recipient.unwrap_or(tx.signer),
                    _ => tx.signer,
                };
                vec![s.account(&target)]
            }
            TxKind::Transfer => {
                let to = tx
                    .recipient
                    .ok_or_else(|| anyhow::anyhow!("transfer missing recipient"))?;
                vec![s.account(&tx.signer), s.account(&to)]
            }
            TxKind::MultiAsset => {
                anyhow::bail!("multi-asset witness assembly TODO")
            }
        };
        Ok(Witness { epoch, accounts, amount, flag })
    }

    fn sign_header(&self, mut h: BlockHeader) -> BlockHeader {
        let bytes = h.signing_bytes();
        h.sequencer_sig = sign(&self.keypair, &bytes);
        h
    }

    pub fn register_issuer(&self, issuer: IssuerRecord) {
        let mut s = self.state.write().unwrap();
        s.registry.put(issuer.key(), issuer.serialize());
    }

    pub fn lookup_issuer(&self, asset_id: u32) -> Option<IssuerRecord> {
        let key = {
            let mut buf = b"issuer:".to_vec();
            buf.extend_from_slice(&asset_id.to_le_bytes());
            hash_bytes(&buf)
        };
        let s = self.state.read().unwrap();
        s.registry.get(&key).and_then(IssuerRecord::deserialize)
    }
}

fn chrono_now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
