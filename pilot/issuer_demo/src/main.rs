//! End-to-end PSL pilot.
//!
//! Walks through the full register → mint → transfer → burn → verify flow,
//! with a light-client process re-verifying every balance against the
//! sequencer's published roots. This is gate 7 of the verification chain.
//!
//! Usage: `cargo run --bin issuer_demo -- --full-flow`

use anyhow::Result;
use clap::Parser;
use psl_crypto::{sign, KeyPair};
use psl_light_client::{header::SignedHeader, verify_balance};
use psl_sequencer::{
    block::{combined_trace_hash, tx_list_hash, BlockHeader},
    config::Config,
    issuer_registry::IssuerRecord,
    node::SequencerNode,
    state::State,
    trace::{NativeTraceExecutor, TraceExecutor, Witness},
    tx::{SignedTx, TxKind},
};
use std::sync::{Arc, RwLock};
use tracing::info;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    full_flow: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let _cli = Cli::parse();

    info!("PSL issuer-demo pilot starting");

    let issuer = KeyPair::from_seed([0xa1u8; 32]);
    let treasury = KeyPair::from_seed([0xa2u8; 32]);
    let customer = KeyPair::from_seed([0xa3u8; 32]);
    let merchant = KeyPair::from_seed([0xa4u8; 32]);
    let sequencer_kp = KeyPair::from_seed([0xa0u8; 32]);

    let cfg = Config {
        data_dir: "/tmp/psl-pilot/data".into(),
        weights_dir: "/tmp/psl-pilot/weights".into(),
        transformer_vm_path: std::env::var("TRANSFORMER_VM_PATH").unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| format!("{h}/Transformer-VM"))
                .unwrap_or_else(|_| "Transformer-VM".into())
        }),
        rpc_listen: "127.0.0.1:0".into(),
        sequencer_secret: [0xa0u8; 32],
        block_interval_ms: 100,
        block_size: 16,
    };
    let node = Arc::new(SequencerNode::new(cfg).await?);
    let state = node.state.clone();

    // Capture the empty-SMT root as the chain's genesis state root, before
    // any registration or transactions touch the state. This is what the
    // light client treats as the trust anchor.
    let genesis_root = state.read().unwrap().accounts_root();

    info!("registering issuer for asset_id=1");
    node.register_issuer(IssuerRecord {
        asset_id: 1,
        authority_pubkey: issuer.public(),
        max_supply: u128::MAX.to_le_bytes(),
        mint_enabled: true,
        burn_enabled: true,
        freeze_enabled: true,
        travel_rule_threshold: 10_000_u128.to_le_bytes(),
        regulator_view_keys: vec![],
        name: "USD-DEMO".into(),
    });

    let exec = NativeTraceExecutor;
    let mut block_n = 0u64;
    let mut chain: Vec<BlockHeader> = Vec::new();

    // Step 1: mint 1_000_000 to treasury
    let mint_tx = build_tx(
        TxKind::Mint,
        1,
        0,
        &issuer,
        Some(treasury.public()),
        1_000_000,
        0,
        None,
    );
    let h = finalize_block(&exec, &state, &sequencer_kp, &mint_tx, &mut block_n, chain.last())?;
    chain.push(h);
    info!("after mint: treasury balance = {}", balance_of(&state, &treasury.public()));

    // Step 2: treasury transfers 100 to customer
    let xfer1 = build_tx(
        TxKind::Transfer,
        1,
        1,
        &treasury,
        Some(customer.public()),
        100,
        0,
        None,
    );
    let h = finalize_block(&exec, &state, &sequencer_kp, &xfer1, &mut block_n, chain.last())?;
    chain.push(h);
    info!(
        "after xfer 100 to customer: treasury={} customer={}",
        balance_of(&state, &treasury.public()),
        balance_of(&state, &customer.public())
    );

    // Step 3: customer transfers 50 to merchant
    let xfer2 = build_tx(
        TxKind::Transfer,
        1,
        1,
        &customer,
        Some(merchant.public()),
        50,
        0,
        None,
    );
    let h = finalize_block(&exec, &state, &sequencer_kp, &xfer2, &mut block_n, chain.last())?;
    chain.push(h);
    info!(
        "after xfer 50 to merchant: customer={} merchant={}",
        balance_of(&state, &customer.public()),
        balance_of(&state, &merchant.public())
    );

    // Step 4: issuer burns 100 from treasury
    let burn_tx = build_tx(
        TxKind::Burn,
        1,
        2,
        &issuer,
        Some(treasury.public()),
        100,
        0,
        None,
    );
    let h = finalize_block(&exec, &state, &sequencer_kp, &burn_tx, &mut block_n, chain.last())?;
    chain.push(h);
    info!("after burn: treasury balance = {}", balance_of(&state, &treasury.public()));

    // Step 5: light-client verifies merchant's balance against the published
    // chain of block headers (genesis through head).
    let proof = state.read().unwrap().account_proof(&merchant.public());
    let signed_chain: Vec<SignedHeader> = chain
        .iter()
        .map(|h| adopt_header_for_lightclient(h.clone(), &sequencer_kp))
        .collect();
    let bal = verify_balance(
        genesis_root,
        &signed_chain,
        &sequencer_kp.public(),
        &merchant.public(),
        &proof,
    )?;
    info!("light-client verified: merchant balance = {}", bal);
    assert_eq!(bal, 50, "pilot end-to-end balance mismatch");

    info!("PSL pilot completed all steps. Note: NativeTraceExecutor used (trace_hash is a marker — real run requires Transformer-VM weights).");
    Ok(())
}

fn build_tx(
    kind: TxKind,
    asset_id: u32,
    nonce: u64,
    signer: &KeyPair,
    recipient: Option<[u8; 32]>,
    amount: u128,
    flag: u8,
    metadata: Option<Vec<u8>>,
) -> SignedTx {
    let mut tx = SignedTx {
        kind,
        asset_id,
        nonce,
        signer: signer.public(),
        recipient,
        amount: amount.to_le_bytes(),
        flag,
        court_order_hash: None,
        multi_payload: None,
        originator_metadata: metadata,
        signature: [0u8; 64],
    };
    let canonical = tx.canonical();
    tx.signature = sign(signer, &canonical);
    tx
}

fn finalize_block(
    exec: &NativeTraceExecutor,
    state: &Arc<RwLock<State>>,
    seq_kp: &KeyPair,
    tx: &SignedTx,
    block_n: &mut u64,
    last_header: Option<&BlockHeader>,
) -> Result<BlockHeader> {
    let prev_state_root;
    let parent_hash;
    {
        let s = state.read().unwrap();
        prev_state_root = s.accounts_root();
        parent_hash = match last_header {
            Some(h) => h.header_hash(),
            None => [0u8; 32],
        };
    }

    let target = match tx.kind {
        TxKind::Mint | TxKind::Freeze => tx.recipient.unwrap_or(tx.signer),
        TxKind::Transfer => tx.signer,
        TxKind::Burn => tx.recipient.unwrap_or(tx.signer),
        TxKind::MultiAsset => tx.signer,
    };

    let accounts = {
        let s = state.read().unwrap();
        match tx.kind {
            TxKind::Transfer => vec![
                s.account(&tx.signer),
                s.account(&tx.recipient.unwrap()),
            ],
            _ => vec![s.account(&target)],
        }
    };
    let witness = Witness {
        epoch: (*block_n + 1) as u32,
        accounts,
        amount: tx.amount,
        flag: tx.flag,
    };
    let res = exec.execute(tx, &witness)?;
    if res.success {
        let mut s = state.write().unwrap();
        for acc in &res.updated_accounts {
            s.put_account(*acc);
        }
    }
    let new_state_root;
    let new_reg_root;
    {
        let s = state.read().unwrap();
        new_state_root = s.accounts_root();
        new_reg_root = s.registry_root();
    }
    let mut h = BlockHeader {
        block_n: *block_n,
        parent_hash,
        prev_state_root,
        tx_list_hash: tx_list_hash(&[tx.clone()]),
        trace_hash: combined_trace_hash(&[res.trace_hash]),
        new_state_root,
        issuer_registry_root: new_reg_root,
        timestamp_ms: 0,
        sequencer_pubkey: seq_kp.public(),
        sequencer_sig: [0u8; 64],
    };
    h.sequencer_sig = sign(seq_kp, &h.signing_bytes());
    *block_n += 1;
    Ok(h)
}

fn adopt_header_for_lightclient(h: BlockHeader, kp: &KeyPair) -> SignedHeader {
    let lh = psl_light_client::header::Header {
        block_n: h.block_n,
        parent_hash: h.parent_hash,
        prev_state_root: h.prev_state_root,
        tx_list_hash: h.tx_list_hash,
        trace_hash: h.trace_hash,
        new_state_root: h.new_state_root,
        issuer_registry_root: h.issuer_registry_root,
        timestamp_ms: h.timestamp_ms,
        sequencer_pubkey: h.sequencer_pubkey,
    };
    SignedHeader::sign(lh, kp)
}

fn balance_of(state: &Arc<RwLock<State>>, pk: &[u8; 32]) -> u128 {
    state.read().unwrap().account(pk).balance()
}
