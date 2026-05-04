//! Gate 8.5 / canonical-gate-1 vector sweep.
//!
//! Re-runs the gate-1 random-witness validation against the pure-Rust
//! runner. As of `76a4fbf` the Rust runner is the **canonical** reference
//! engine for the trace-hash contract (per `docs/ARCHITECTURE.md § 0`).
//!
//! Primitives covered:
//!   - `byte_add`         (~117 tokens)
//!   - `byte_sub`         (~404 tokens)
//!   - `transfer_check`   (~1624 tokens)
//!   - `transfer_finalize` (~656 tokens)
//!   - `mpt_emit`         (~3741 tokens)
//!   - `freeze_chain`     freeze_setup → freeze_apply, ~25k tokens total
//!
//! Per-witness work is independent → witness-level parallelism via rayon.
//! `--threads N` caps the rayon thread pool.
//!
//! Usage:
//!   cargo run -p psl-rust-runner --release --bin run_gate1 -- \
//!       --primitive byte_add --count 10000 --threads 8

use anyhow::{Context, Result};
use clap::Parser;
use psl_rust_runner::weights::{load_weights, Weights};
use psl_rust_runner::{generate, GenerateConfig};
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

#[derive(Parser, Clone)]
struct Cli {
    #[arg(long)]
    primitive: String,
    #[arg(long, default_value_t = 10_000)]
    count: usize,
    #[arg(long, default_value_t = 0)]
    seed: u64,
    /// Print first N failure summaries
    #[arg(long, default_value_t = 5)]
    print_failures: usize,
    /// Number of rayon worker threads (0 = rayon default).
    #[arg(long, default_value_t = 0)]
    threads: usize,
    #[arg(long)]
    repo_root: Option<PathBuf>,
}

/// Splitmix64 — small, fast, deterministic per-witness RNG.
struct Splitmix64 { state: u64 }
impl Splitmix64 {
    fn new(seed: u64) -> Self { Self { state: seed.wrapping_add(0x9E3779B97F4A7C15) } }
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
    fn byte(&mut self) -> u8 { (self.next_u64() & 0xFF) as u8 }
    fn bit(&mut self) -> u8 { (self.next_u64() & 1) as u8 }
}

fn render_spec(witness: &[u8]) -> Vec<String> {
    let mut tokens: Vec<String> = vec!["start".into()];
    for &b in witness {
        if (0x20 < b && b < 0x7F) && b != b'{' && b != b'}' {
            tokens.push((b as char).to_string());
        } else {
            tokens.push(format!("{b:02x}"));
        }
    }
    tokens.push("00".into());
    tokens.push("commit(+0,sts=0,bt=0)".into());
    tokens
}

fn parse_out_bytes(tokens: &[String]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for t in tokens {
        if let Some(inner) = t.strip_prefix("out(").and_then(|s| s.strip_suffix(')')) {
            if inner.len() == 1 {
                bytes.push(inner.as_bytes()[0]);
            } else if inner.len() == 2 {
                if let Ok(b) = u8::from_str_radix(inner, 16) {
                    bytes.push(b);
                }
            }
        }
    }
    bytes
}

fn run_one_pass(w: &Weights, witness: &[u8], max_new: usize) -> Result<Vec<u8>> {
    let toks = render_spec(witness);
    let toks_ref: Vec<&str> = toks.iter().map(|s| s.as_str()).collect();
    let cfg = GenerateConfig { max_new_tokens: max_new };
    let predicted = generate(w, &toks_ref, &cfg)?;
    Ok(parse_out_bytes(&predicted))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = cli
        .repo_root
        .clone()
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf());

    if cli.threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(cli.threads)
            .build_global()
            .ok(); // build_global is one-shot; ignore failure on second build
    }

    let prim = cli.primitive.clone();
    let max_new = match prim.as_str() {
        "mpt_emit" | "mpt_emit_record" => 10_000,
        "freeze_chain" | "freeze_setup" | "freeze_apply" => 50_000,
        _ => 5_000,
    };

    // Load weights once (per primitive, or per pair for freeze_chain).
    eprintln!("[gate-1-rust] primitive={prim} count={} threads={}", cli.count, cli.threads);
    let setup_w_arc;
    let apply_w_arc;
    let single_w_arc;
    let kind: Kind;
    match prim.as_str() {
        "freeze_chain" => {
            let setup = load_weights(&repo_root.join("weights").join("freeze_setup.bin"))
                .context("loading freeze_setup")?;
            let apply = load_weights(&repo_root.join("weights").join("freeze_apply.bin"))
                .context("loading freeze_apply")?;
            eprintln!(
                "[gate-1-rust] freeze_setup: vocab={} d_model={} d_ffn_max={}",
                setup.header.vocab,
                setup.header.d_model,
                setup.header.d_ffn_per_layer.iter().max().unwrap_or(&0)
            );
            eprintln!(
                "[gate-1-rust] freeze_apply: vocab={} d_model={} d_ffn_max={}",
                apply.header.vocab,
                apply.header.d_model,
                apply.header.d_ffn_per_layer.iter().max().unwrap_or(&0)
            );
            setup_w_arc = Some(setup);
            apply_w_arc = Some(apply);
            single_w_arc = None;
            kind = Kind::FreezeChain;
        }
        _ => {
            let weights_name = match prim.as_str() {
                "byte_add" => "byte_add_with_carry",
                "byte_sub" => "byte_sub_with_borrow",
                "transfer_check" => "transfer_check",
                "transfer_finalize" => "transfer_finalize",
                "mpt_emit" | "mpt_emit_record" => "mpt_emit_record",
                "freeze_setup" => "freeze_setup",
                "freeze_apply" => "freeze_apply",
                other => anyhow::bail!("unknown primitive: {other}"),
            };
            let w = load_weights(&repo_root.join("weights").join(format!("{weights_name}.bin")))
                .context("loading weights")?;
            eprintln!(
                "[gate-1-rust] vocab={} d_model={} n_layers={} ffn={:?}",
                w.header.vocab, w.header.d_model, w.header.n_layers, w.header.d_ffn_per_layer
            );
            single_w_arc = Some(w);
            setup_w_arc = None;
            apply_w_arc = None;
            kind = Kind::Single;
        }
    }

    let pass = AtomicUsize::new(0);
    let fail = AtomicUsize::new(0);
    let progress_every = (cli.count / 10).max(50);
    let next_progress = AtomicUsize::new(progress_every);
    let fail_examples: Mutex<Vec<(usize, Vec<u8>, Vec<u8>, Vec<u8>)>> = Mutex::new(Vec::new());
    let t0 = Instant::now();

    (0..cli.count).into_par_iter().for_each(|i| {
        let mut rng = Splitmix64::new(cli.seed.wrapping_add(seed_for(&prim)).wrapping_add(i as u64));
        let (witness, expected) = gen_vector(&prim, &mut rng);

        let got_result: Result<Vec<u8>> = match kind {
            Kind::Single => {
                let w = single_w_arc.as_ref().unwrap();
                run_one_pass(w, &witness, max_new)
            }
            Kind::FreezeChain => {
                let setup = setup_w_arc.as_ref().unwrap();
                let apply = apply_w_arc.as_ref().unwrap();
                run_one_pass(setup, &witness, max_new)
                    .and_then(|setup_out| run_one_pass(apply, &setup_out, max_new))
            }
        };

        let ok = match got_result {
            Ok(ref bytes) => bytes == &expected,
            Err(_) => false,
        };
        if ok {
            pass.fetch_add(1, Ordering::Relaxed);
        } else {
            fail.fetch_add(1, Ordering::Relaxed);
            if let Ok(got) = got_result {
                let mut g = fail_examples.lock().unwrap();
                if g.len() < cli.print_failures {
                    g.push((i, witness.clone(), expected.clone(), got));
                }
            }
        }

        let done = pass.load(Ordering::Relaxed) + fail.load(Ordering::Relaxed);
        let np = next_progress.load(Ordering::Relaxed);
        if done >= np {
            // best-effort progress (race-tolerant)
            if next_progress
                .compare_exchange(np, np + progress_every, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                let dt = t0.elapsed().as_secs_f64();
                let rate = done as f64 / dt;
                let eta = (cli.count - done) as f64 / rate.max(0.01);
                let p = pass.load(Ordering::Relaxed);
                let f = fail.load(Ordering::Relaxed);
                eprintln!(
                    "  [{:>5}/{}] {} ok / {} fail  ({:.2}/s, ETA {:.0}s)",
                    done, cli.count, p, f, rate, eta
                );
            }
        }
    });

    let dt = t0.elapsed().as_secs_f64();
    let p = pass.load(Ordering::Relaxed);
    let f = fail.load(Ordering::Relaxed);
    println!("\n=== {prim} summary ===");
    println!("  pass: {p}/{}    fail: {f}", cli.count);
    println!("  time: {dt:.1}s    rate: {:.2}/s", cli.count as f64 / dt);
    let exs = fail_examples.lock().unwrap();
    for (i, w_, exp, got) in exs.iter() {
        println!("  FAIL #{i}: witness[..8]={:?}  expected={:?}  got={:?}",
            &w_[..w_.len().min(8)], exp, got);
    }
    if f > 0 {
        std::process::exit(1);
    }
    Ok(())
}

#[derive(Copy, Clone)]
enum Kind { Single, FreezeChain }

fn seed_for(p: &str) -> u64 {
    let mut h = 1469598103934665603u64;
    for b in p.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

fn gen_vector(prim: &str, rng: &mut Splitmix64) -> (Vec<u8>, Vec<u8>) {
    match prim {
        "byte_add" => {
            let a = rng.byte();
            let b = rng.byte();
            let c = rng.bit();
            let s = a as u32 + b as u32 + c as u32;
            let (res, co) = if s >= 256 { ((s - 256) as u8, 1u8) } else { (s as u8, 0u8) };
            (vec![a, b, c], vec![res, co])
        }
        "byte_sub" => {
            let m = rng.byte();
            let s = rng.byte();
            let b = rng.bit();
            let diff = m as i32 - s as i32 - b as i32;
            let (res, bo) = if diff < 0 { ((diff + 256) as u8, 1u8) } else { (diff as u8, 0u8) };
            (vec![m, s, b], vec![res, bo])
        }
        "transfer_check" => {
            let from: Vec<u8> = (0..16).map(|_| rng.byte()).collect();
            let amt: Vec<u8> = (0..16).map(|_| rng.byte()).collect();
            let f = u128::from_le_bytes(from.clone().try_into().unwrap());
            let a = u128::from_le_bytes(amt.clone().try_into().unwrap());
            let ok = if f >= a { 1u8 } else { 0u8 };
            let mut input = from;
            input.extend_from_slice(&amt);
            (input, vec![ok])
        }
        "transfer_finalize" => {
            let nonce: Vec<u8> = (0..8).map(|_| rng.byte()).collect();
            let n = u64::from_le_bytes(nonce.clone().try_into().unwrap());
            let new_n = n.wrapping_add(1);
            (nonce, new_n.to_le_bytes().to_vec())
        }
        "mpt_emit" | "mpt_emit_record" => {
            let record: Vec<u8> = (0..64).map(|_| rng.byte()).collect();
            (record.clone(), record)
        }
        "freeze_chain" => {
            // 65-byte witness: [flag, acc(64)]
            let flag = rng.bit();
            let acc: Vec<u8> = (0..64).map(|_| rng.byte()).collect();
            let b47 = acc[47];
            let expected = if flag == 1 { (b47 & 0x7F) | 0x80 } else { b47 & 0x7F };
            let mut witness = vec![flag];
            witness.extend_from_slice(&acc);
            (witness, vec![expected])
        }
        _ => panic!("unknown primitive {prim}"),
    }
}
