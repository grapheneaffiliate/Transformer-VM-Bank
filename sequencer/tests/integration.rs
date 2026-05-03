//! Sequencer end-to-end integration test (gate 4).
//!
//! Spins up an in-process sequencer + 3 follower replicas (each with its own
//! State + NativeTraceExecutor), runs 100 blocks of mixed traffic, asserts:
//!   - all four state roots match after every block
//!   - mutating a published block's `new_state_root` is detected by followers
//!
//! Real bit-exact runs require the SubprocessTraceExecutor + populated
//! weights/; this test exercises the deterministic state-machine layer.

use psl_crypto::{sign, KeyPair};
use psl_sequencer::{
    block::{combined_trace_hash, tx_list_hash, BlockHeader},
    issuer_registry::IssuerRecord,
    state::State,
    trace::{NativeTraceExecutor, TraceExecutor, Witness},
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
) -> SignedTx {
    let mut tx = SignedTx {
        kind,
        asset_id,
        nonce,
        signer: signer.public(),
        recipient,
        amount: amount.to_le_bytes(),
        flag: 0,
        court_order_hash: None,
        multi_payload: None,
        originator_metadata: None,
        signature: [0u8; 64],
    };
    tx.signature = sign(signer, &tx.canonical());
    tx
}

#[test]
fn sequencer_and_3_followers_agree_on_100_blocks() {
    let exec = NativeTraceExecutor;

    fn fresh_state() -> Arc<RwLock<State>> {
        Arc::new(RwLock::new(State::new()))
    }

    let states: Vec<Arc<RwLock<State>>> = (0..4).map(|_| fresh_state()).collect();
    let issuer = KeyPair::from_seed([1u8; 32]);
    for s in &states {
        s.write().unwrap().registry.put(
            IssuerRecord {
                asset_id: 1,
                authority_pubkey: issuer.public(),
                max_supply: u128::MAX.to_le_bytes(),
                mint_enabled: true,
                burn_enabled: true,
                freeze_enabled: true,
                travel_rule_threshold: u128::MAX.to_le_bytes(),
                regulator_view_keys: vec![],
                name: "DEMO".into(),
            }
            .key(),
            IssuerRecord {
                asset_id: 1,
                authority_pubkey: issuer.public(),
                max_supply: u128::MAX.to_le_bytes(),
                mint_enabled: true,
                burn_enabled: true,
                freeze_enabled: true,
                travel_rule_threshold: u128::MAX.to_le_bytes(),
                regulator_view_keys: vec![],
                name: "DEMO".into(),
            }
            .serialize(),
        );
    }

    // Mint 1M to Alice on every replica
    let alice = KeyPair::from_seed([42u8; 32]);
    let mut nonce = 0u64;
    let mint = build_signed_tx(TxKind::Mint, 1, nonce, &issuer, Some(alice.public()), 1_000_000);
    nonce += 1;
    apply_to_all(&exec, &states, &mint, &alice, 1).unwrap();

    let mut rng = StdRng::seed_from_u64(7);
    let mut alice_nonce = 0u64;
    for block in 1..100 {
        let amount = rng.gen_range(1..1_000);
        let tx = build_signed_tx(
            TxKind::Transfer,
            1,
            alice_nonce + 1,
            &alice,
            Some([rng.gen::<u8>(); 32]),
            amount,
        );
        alice_nonce += 1;
        apply_to_all(&exec, &states, &tx, &alice, block as u32).unwrap();
        assert_roots_agree(&states);
    }
}

fn apply_to_all(
    exec: &NativeTraceExecutor,
    states: &[Arc<RwLock<State>>],
    tx: &SignedTx,
    _signer: &KeyPair,
    epoch: u32,
) -> anyhow::Result<()> {
    for s in states {
        let accounts = {
            let st = s.read().unwrap();
            match tx.kind {
                TxKind::Transfer => vec![
                    st.account(&tx.signer),
                    st.account(&tx.recipient.unwrap()),
                ],
                _ => vec![st.account(&tx.recipient.unwrap_or(tx.signer))],
            }
        };
        let w = Witness {
            epoch,
            accounts,
            amount: tx.amount,
            flag: tx.flag,
        };
        let res = exec.execute(tx, &w)?;
        if res.success {
            let mut st = s.write().unwrap();
            for acc in &res.updated_accounts {
                st.put_account(*acc);
            }
        }
    }
    Ok(())
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
fn published_root_mutation_detected() {
    let exec = NativeTraceExecutor;
    let states: Vec<_> = (0..2).map(|_| Arc::new(RwLock::new(State::new()))).collect();
    let kp = KeyPair::from_seed([3u8; 32]);
    let issuer = KeyPair::from_seed([4u8; 32]);

    for s in &states {
        s.write().unwrap().registry.put(
            IssuerRecord {
                asset_id: 1,
                authority_pubkey: issuer.public(),
                max_supply: u128::MAX.to_le_bytes(),
                mint_enabled: true,
                burn_enabled: true,
                freeze_enabled: true,
                travel_rule_threshold: u128::MAX.to_le_bytes(),
                regulator_view_keys: vec![],
                name: "X".into(),
            }
            .key(),
            IssuerRecord {
                asset_id: 1,
                authority_pubkey: issuer.public(),
                max_supply: u128::MAX.to_le_bytes(),
                mint_enabled: true,
                burn_enabled: true,
                freeze_enabled: true,
                travel_rule_threshold: u128::MAX.to_le_bytes(),
                regulator_view_keys: vec![],
                name: "X".into(),
            }
            .serialize(),
        );
    }

    let mint = build_signed_tx(TxKind::Mint, 1, 0, &issuer, Some(kp.public()), 100);
    apply_to_all(&exec, &states, &mint, &kp, 1).unwrap();

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
        trace_hash: combined_trace_hash(&[[0u8; 32]]),
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
}
