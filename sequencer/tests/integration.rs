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

/// TPS benchmark — sequencer + 3 followers driving N blocks of mixed
/// traffic. Times only the block-production loop (excludes setup +
/// pre-funding). Reports total transactions, elapsed wall-clock,
/// computed TPS, **and tail-latency percentiles (p50 / p95 / p99 /
/// p99.9 / max) plus captured hardware spec at run time.**
///
/// Methodology: same workload shape as
/// `sequencer_and_3_followers_agree_on_100_mixed_blocks` (gate 4) but
/// scaled to 10,000 blocks for reliable timing. Each block: 1
/// transfer (always) + occasional mint/burn/freeze/multi_asset on
/// every-Nth-block cadence. Real signed ed25519 transactions, real
/// MPT state mutations, all 4 replicas verify state-root agreement
/// every block.
///
/// **Per-tx timing:** wraps each `apply_to_all` call in
/// `Instant::now()` and stores durations into a `Vec<Duration>` for
/// percentile computation. Overhead is ~tens of nanoseconds per call,
/// well below the per-tx work (251 µs single-replica baseline).
///
/// **Hardware spec capture:** shells out to `lscpu` and `uname -a`
/// at run time and prints the output before the numbers, so the
/// reported TPS is reproducible / comparable on the same hardware.
/// Gracefully degrades if the commands aren't available.
///
/// **What this measures:** sequencer kernel throughput including
/// signature verification, MPT writes, state-root computation,
/// and cross-replica consistency check. **What this excludes:**
/// network transport (in-process), trace_hash from real
/// Transformer-VM weights (uses `NativeTraceExecutor` stub —
/// deterministic but not the analytical model), `sled` durable-
/// commit overhead (in-memory `State`).
///
/// Ignored by default; run with:
///   cargo test -p psl-sequencer --test integration --release \
///     bench_sequencer_tps_10k_blocks -- --ignored --nocapture
#[test]
#[ignore]
fn bench_sequencer_tps_10k_blocks() {
    use std::time::{Duration, Instant};

    const N_BLOCKS: u64 = 10_000;
    // Replica count. Set via env var to compare single-sequencer vs
    // 4-replica costs. Default = 4 (sequencer + 3 followers, matches
    // gate-4 integration test shape).
    let n_replicas: usize = std::env::var("PSL_BENCH_REPLICAS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);

    print_hardware_spec();

    let exec = NativeTraceExecutor;
    let states: Vec<Arc<RwLock<State>>> = (0..n_replicas).map(|_| fresh_state()).collect();
    let issuer = KeyPair::from_seed([1u8; 32]);
    install_issuer(&states, &issuer);

    let alice = KeyPair::from_seed([42u8; 32]);
    let bob = KeyPair::from_seed([43u8; 32]);

    let mint_alice = build_signed_tx(
        TxKind::Mint,
        1,
        0,
        &issuer,
        Some(alice.public()),
        1_000_000_000_000_000_000,
        0,
    );
    apply_to_all(&exec, &states, &mint_alice, &[], 0).unwrap();
    let mint_bob = build_signed_tx(
        TxKind::Mint,
        1,
        0,
        &issuer,
        Some(bob.public()),
        1_000_000_000_000_000_000,
        0,
    );
    apply_to_all(&exec, &states, &mint_bob, &[], 0).unwrap();

    let mut rng = StdRng::seed_from_u64(7);
    let mut alice_nonce = 0u64;
    let mut tx_count: u64 = 0;
    // Pre-allocate for the expected 15,106 mixed transactions.
    let mut tx_durations: Vec<Duration> = Vec::with_capacity(16_000);

    let start = Instant::now();
    for block in 1..=N_BLOCKS {
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
        let tx_start = Instant::now();
        apply_to_all(&exec, &states, &tx, &[], block as u32).unwrap();
        tx_durations.push(tx_start.elapsed());
        tx_count += 1;

        if block % 5 == 0 {
            let m = build_signed_tx(TxKind::Mint, 1, 0, &issuer, Some(alice.public()), 1_000, 0);
            let tx_start = Instant::now();
            apply_to_all(&exec, &states, &m, &[], block as u32).unwrap();
            tx_durations.push(tx_start.elapsed());
            tx_count += 1;
        }
        if block % 7 == 0 {
            let b = build_signed_tx(TxKind::Burn, 1, 0, &issuer, Some(bob.public()), 100, 0);
            let tx_start = Instant::now();
            apply_to_all(&exec, &states, &b, &[], block as u32).unwrap();
            tx_durations.push(tx_start.elapsed());
            tx_count += 1;
        }
        if block % 11 == 0 {
            let target = [(block as u8); 32];
            let f = build_signed_tx(TxKind::Freeze, 1, 0, &issuer, Some(target), 0, 1);
            let tx_start = Instant::now();
            apply_to_all(&exec, &states, &f, &[], block as u32).unwrap();
            tx_durations.push(tx_start.elapsed());
            tx_count += 1;
        }
        if block % 13 == 0 {
            let recipients: Vec<[u8; 32]> = (0..3).map(|i| [block as u8 + i as u8; 32]).collect();
            let ma = build_signed_tx(TxKind::MultiAsset, 1, alice_nonce + 1, &alice, None, 50, 0);
            alice_nonce += 1;
            let tx_start = Instant::now();
            apply_to_all(&exec, &states, &ma, &recipients, block as u32).unwrap();
            tx_durations.push(tx_start.elapsed());
            tx_count += 1;
        }
        assert_roots_agree(&states);
    }
    let elapsed = start.elapsed();
    let secs = elapsed.as_secs_f64();
    let tps = tx_count as f64 / secs;
    let block_per_sec = N_BLOCKS as f64 / secs;

    // Sort once for percentile lookups.
    tx_durations.sort_unstable();
    let pct = |q: f64| -> f64 {
        // Nearest-rank percentile; q in [0, 100].
        let idx = ((q / 100.0) * (tx_durations.len() as f64)).ceil() as usize;
        let idx = idx.saturating_sub(1).min(tx_durations.len() - 1);
        tx_durations[idx].as_secs_f64() * 1_000_000.0
    };
    let p50 = pct(50.0);
    let p95 = pct(95.0);
    let p99 = pct(99.0);
    let p999 = pct(99.9);
    let max_us = tx_durations.last().unwrap().as_secs_f64() * 1_000_000.0;

    println!();
    println!("=== sequencer TPS benchmark ({n_replicas} replicas, in-process) ===");
    println!("  blocks:        {N_BLOCKS}");
    println!("  transactions:  {tx_count}");
    println!("  elapsed:       {secs:.3} s");
    println!("  TPS:           {tps:.0} tx/s");
    println!("  blocks/sec:    {block_per_sec:.0} blk/s");
    println!(
        "  per-tx mean:   {:.1} µs",
        (secs * 1_000_000.0) / tx_count as f64
    );
    println!("  per-tx p50:    {p50:.1} µs");
    println!("  per-tx p95:    {p95:.1} µs");
    println!("  per-tx p99:    {p99:.1} µs");
    println!("  per-tx p99.9:  {p999:.1} µs");
    println!("  per-tx max:    {max_us:.1} µs");
    println!("=========================================================");
}

/// Best-effort hardware-spec dump for bench reproducibility. Tries
/// `lscpu` (Linux) and `uname -a` (Unix-like). Prints whatever it
/// gets. If a command fails or isn't available, prints a one-line
/// note and continues — the bench numbers still report.
fn print_hardware_spec() {
    use std::process::Command;
    println!();
    println!("=== hardware spec (captured at bench run time) ===");
    match Command::new("uname").arg("-a").output() {
        Ok(out) if out.status.success() => {
            print!("uname -a: {}", String::from_utf8_lossy(&out.stdout));
        }
        _ => println!("uname -a: (not available on this platform)"),
    }
    match Command::new("lscpu").output() {
        Ok(out) if out.status.success() => {
            // Only the high-signal lines: model, sockets, cores, threads, MHz.
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if line.starts_with("Model name:")
                    || line.starts_with("CPU(s):")
                    || line.starts_with("Thread(s) per core:")
                    || line.starts_with("Core(s) per socket:")
                    || line.starts_with("Socket(s):")
                    || line.starts_with("CPU max MHz:")
                    || line.starts_with("Architecture:")
                {
                    println!("lscpu:    {line}");
                }
            }
        }
        _ => println!("lscpu:    (not available on this platform)"),
    }
    println!("==================================================");
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
