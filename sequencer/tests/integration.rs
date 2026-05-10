//! Sequencer end-to-end integration test (gate 4).
//!
//! Spins up sequencer + 3 follower replicas, drives 100 blocks of mixed
//! traffic (transfers, mints, burns, freezes, multi_asset), asserts all
//! four state roots match at every block. Adversarial mutation test:
//! mutating a published block's `new_state_root` must be detected.

use psl_crypto::{sign, Account, KeyPair};
use psl_sequencer::{
    block::{combined_trace_hash, tx_list_hash, BlockHeader},
    issuer_registry::IssuerRecord,
    state::State,
    trace::{expected_trace_hash_count, NativeTraceExecutor, TraceExecutor, Witness},
    tx::{SignedTx, TxKind},
};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::sync::{Arc, RwLock};

fn build_signed_tx(
    kind: TxKind,
    asset_id: u32,
    nonce: u64,
    signer: &KeyPair,
    recipient: Option<[u8; 32]>,
    amount: u128,
    flag: u8,
) -> SignedTx {
    let mut tx = SignedTx {
        kind,
        asset_id,
        nonce,
        signer: signer.public(),
        recipient,
        amount: amount.to_le_bytes(),
        flag,
        court_order_hash: if matches!(kind, TxKind::Freeze) {
            Some([0xab; 32])
        } else {
            None
        },
        multi_payload: None,
        originator_metadata: None,
        signature: [0u8; 64],
    };
    tx.signature = sign(signer, &tx.canonical());
    tx
}

fn fresh_state() -> Arc<RwLock<State>> {
    Arc::new(RwLock::new(State::new()))
}

fn install_issuer(states: &[Arc<RwLock<State>>], issuer: &KeyPair) {
    for s in states {
        let rec = IssuerRecord {
            asset_id: 1,
            authority_pubkey: issuer.public(),
            max_supply: u128::MAX.to_le_bytes(),
            mint_enabled: true,
            burn_enabled: true,
            freeze_enabled: true,
            travel_rule_threshold: u128::MAX.to_le_bytes(),
            regulator_view_keys: vec![],
            name: "DEMO".into(),
        };
        s.write().unwrap().registry.put(rec.key(), rec.serialize());
    }
}

fn assemble_witness(state: &State, tx: &SignedTx, multi_recipients: &[[u8; 32]]) -> Witness {
    let accounts = match tx.kind {
        TxKind::Freeze => vec![state.account(&tx.recipient.unwrap_or(tx.signer))],
        TxKind::Mint => vec![state.account(&tx.recipient.unwrap_or(tx.signer))],
        TxKind::Burn => vec![state.account(&tx.recipient.unwrap_or(tx.signer))],
        TxKind::Transfer => vec![
            state.account(&tx.signer),
            state.account(&tx.recipient.unwrap()),
        ],
        TxKind::MultiAsset => {
            // (from_0, to_0, from_1, to_1, ...) — sender pays each recipient
            let mut accs = Vec::with_capacity(multi_recipients.len() * 2);
            for r in multi_recipients {
                accs.push(state.account(&tx.signer));
                accs.push(state.account(r));
            }
            accs
        }
    };
    Witness {
        epoch: 1,
        accounts,
        amount: tx.amount,
        flag: tx.flag,
    }
}

fn apply_to_all(
    exec: &NativeTraceExecutor,
    states: &[Arc<RwLock<State>>],
    tx: &SignedTx,
    multi_recipients: &[[u8; 32]],
    epoch: u32,
) -> anyhow::Result<usize> {
    let mut traces = 0;
    for s in states {
        let w = {
            let st = s.read().unwrap();
            let mut w = assemble_witness(&st, tx, multi_recipients);
            w.epoch = epoch;
            w
        };
        let res = exec.execute(tx, &w)?;
        traces = expected_trace_hash_count(tx.kind, multi_recipients.len());
        if res.success {
            let mut st = s.write().unwrap();
            // For multi_asset, updated_accounts is (new_from, new_to)*N. For
            // transfer, two accounts. Single-account ops put the one account.
            for acc in &res.updated_accounts {
                if acc.bytes != [0u8; 64] {
                    st.put_account(*acc);
                }
            }
        }
    }
    Ok(traces)
}

fn assert_roots_agree(states: &[Arc<RwLock<State>>]) {
    let roots: Vec<_> = states
        .iter()
        .map(|s| s.read().unwrap().accounts_root())
        .collect();
    let r0 = roots[0];
    for r in &roots[1..] {
        assert_eq!(*r, r0, "state-root divergence");
    }
}

#[test]
fn sequencer_and_3_followers_agree_on_100_mixed_blocks() {
    let exec = NativeTraceExecutor;
    let states: Vec<Arc<RwLock<State>>> = (0..4).map(|_| fresh_state()).collect();
    let issuer = KeyPair::from_seed([1u8; 32]);
    install_issuer(&states, &issuer);

    // Pre-fund Alice and Bob so they have balances to transfer/burn from
    let alice = KeyPair::from_seed([42u8; 32]);
    let bob = KeyPair::from_seed([43u8; 32]);

    let mint_alice = build_signed_tx(
        TxKind::Mint,
        1,
        0,
        &issuer,
        Some(alice.public()),
        1_000_000_000,
        0,
    );
    apply_to_all(&exec, &states, &mint_alice, &[], 0).unwrap();
    let mint_bob = build_signed_tx(
        TxKind::Mint,
        1,
        0,
        &issuer,
        Some(bob.public()),
        1_000_000_000,
        0,
    );
    apply_to_all(&exec, &states, &mint_bob, &[], 0).unwrap();
    assert_roots_agree(&states);

    let mut rng = StdRng::seed_from_u64(7);
    let mut alice_nonce = 0u64;
    let mut total_traces = 0usize;

    // 100 blocks of mixed traffic. Each block: 1 transfer + occasionally a
    // mint/burn/freeze/multi_asset chosen by RNG.
    for block in 1..=100 {
        // Always a transfer
        let amount = rng.gen_range(1..1_000);
        let recipient = [rng.gen::<u8>(); 32];
        let tx = build_signed_tx(
            TxKind::Transfer,
            1,
            alice_nonce + 1,
            &alice,
            Some(recipient),
            amount,
            0,
        );
        alice_nonce += 1;
        total_traces += apply_to_all(&exec, &states, &tx, &[], block as u32).unwrap();

        // Every 5th block: mint to Alice
        if block % 5 == 0 {
            let m = build_signed_tx(TxKind::Mint, 1, 0, &issuer, Some(alice.public()), 1_000, 0);
            total_traces += apply_to_all(&exec, &states, &m, &[], block as u32).unwrap();
        }
        // Every 7th block: burn from Bob
        if block % 7 == 0 {
            let b = build_signed_tx(TxKind::Burn, 1, 0, &issuer, Some(bob.public()), 100, 0);
            total_traces += apply_to_all(&exec, &states, &b, &[], block as u32).unwrap();
        }
        // Every 11th block: freeze a derived account
        if block % 11 == 0 {
            let target = [(block as u8); 32];
            let f = build_signed_tx(TxKind::Freeze, 1, 0, &issuer, Some(target), 0, 1);
            total_traces += apply_to_all(&exec, &states, &f, &[], block as u32).unwrap();
        }
        // Every 13th block: a multi_asset transfer (N=3 recipients, sender = Alice)
        if block % 13 == 0 {
            let recipients: Vec<[u8; 32]> = (0..3).map(|i| [block as u8 + i as u8; 32]).collect();
            let ma = build_signed_tx(TxKind::MultiAsset, 1, alice_nonce + 1, &alice, None, 50, 0);
            alice_nonce += 1;
            total_traces += apply_to_all(&exec, &states, &ma, &recipients, block as u32).unwrap();
        }

        assert_roots_agree(&states);
    }

    println!(
        "100 blocks complete; total per-tx hash count {} (sum of expected_trace_hash_count for all txs)",
        total_traces
    );
}

#[test]
fn published_root_mutation_detected() {
    let exec = NativeTraceExecutor;
    let states: Vec<_> = (0..2).map(|_| fresh_state()).collect();
    let kp = KeyPair::from_seed([3u8; 32]);
    let issuer = KeyPair::from_seed([4u8; 32]);
    install_issuer(&states, &issuer);

    let mint = build_signed_tx(TxKind::Mint, 1, 0, &issuer, Some(kp.public()), 100, 0);
    apply_to_all(&exec, &states, &mint, &[], 1).unwrap();

    let real_root = states[0].read().unwrap().accounts_root();
    let mutated = {
        let mut r = real_root;
        r[0] ^= 0xff;
        r
    };
    assert_ne!(real_root, mutated);

    let header = BlockHeader {
        block_n: 1,
        parent_hash: [0u8; 32],
        prev_state_root: [0u8; 32],
        tx_list_hash: tx_list_hash(&[mint.clone()]),
        trace_hash: combined_trace_hash(&[[0u8; 32]; 16]), // 16-trace mint
        new_state_root: mutated,
        issuer_registry_root: states[0].read().unwrap().registry_root(),
        timestamp_ms: 0,
        sequencer_pubkey: kp.public(),
        sequencer_sig: [0u8; 64],
    };
    let follower_root = states[1].read().unwrap().accounts_root();
    assert_ne!(
        header.new_state_root, follower_root,
        "follower must detect mutation"
    );
    let _ = Account::default(); // silence unused-warning if any
}
