//! Gate 8.5 vector sweep: re-run gate-1 vectors (10k per primitive) through
//! the pure-Rust runner. Validates arithmetic correctness — same property
//! that gate-1 verified through the C++ engine.
//!
//! Primitives covered (the 5 with short, single-pass traces):
//!   - byte_add_with_carry        (~117 tokens)
//!   - byte_sub_with_borrow       (~404 tokens)
//!   - transfer_check             (~1624 tokens)
//!   - transfer_finalize          (~656 tokens)
//!   - mpt_emit_record            (~3741 tokens)
//!
//! freeze_setup / freeze_apply are chained (output → input) and run an
//! order of magnitude longer (17k / 7.7k tokens). They would take days at
//! 10k vectors with the current Rust runner; covered separately.
//!
//! Usage:
//!   cargo run -p psl-rust-runner --release --bin run_gate1 -- \
//!       --primitive byte_add --count 10000

use anyhow::{Context, Result};
use clap::Parser;
use psl_rust_runner::weights::load_weights;
use psl_rust_runner::{generate, GenerateConfig};
use std::path::PathBuf;
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
    /// Repo root (defaults to CARGO_MANIFEST_DIR/..).
    #[arg(long)]
    repo_root: Option<PathBuf>,
}

/// Splitmix64 — small, fast, deterministic per-thread RNG.
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

/// Render a witness byte array into the input prompt format used by the
/// Transformer-VM specialized models. Matches `render_binary_spec` in
/// tools/run_per_byte_10k.py.
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

/// Parse `out(...)` tokens from a predicted token stream into a byte vec.
/// `out(<XX>)` where XX is a 2-hex-digit byte or a single printable char.
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

fn run_one(
    name: &str,
    w: &psl_rust_runner::weights::Weights,
    witness: &[u8],
    expected: &[u8],
    max_new: usize,
) -> std::result::Result<bool, String> {
    let toks = render_spec(witness);
    let toks_ref: Vec<&str> = toks.iter().map(|s| s.as_str()).collect();
    let cfg = GenerateConfig { max_new_tokens: max_new };
    let predicted = generate(w, &toks_ref, &cfg).map_err(|e| format!("{name}: {e}"))?;
    let bytes = parse_out_bytes(&predicted);
    Ok(bytes == expected)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = cli
        .repo_root
        .clone()
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf());

    let prim = cli.primitive.clone();
    let weights_name = match prim.as_str() {
        "byte_add" => "byte_add_with_carry",
        "byte_sub" => "byte_sub_with_borrow",
        "transfer_check" => "transfer_check",
        "transfer_finalize" => "transfer_finalize",
        "mpt_emit" | "mpt_emit_record" => "mpt_emit_record",
        other => anyhow::bail!("unknown primitive: {other}"),
    };

    let weights_path = repo_root.join("weights").join(format!("{weights_name}.bin"));
    eprintln!("[gate-1-rust] loading {}", weights_path.display());
    let w = load_weights(&weights_path).context("loading weights")?;
    eprintln!(
        "[gate-1-rust] vocab={} d_model={} n_layers={} n_heads={} ffn={:?}",
        w.header.vocab, w.header.d_model, w.header.n_layers, w.header.n_heads, w.header.d_ffn_per_layer
    );

    let mut rng = Splitmix64::new(cli.seed.wrapping_add(seed_for(&prim)));
    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut fail_examples: Vec<(usize, Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::new();

    let max_new = match prim.as_str() {
        "mpt_emit" | "mpt_emit_record" => 10_000,
        _ => 5_000,
    };

    let t0 = Instant::now();
    let progress_every = (cli.count / 10).max(50);
    for i in 0..cli.count {
        let (witness, expected) = gen_vector(&prim, &mut rng);
        match run_one(weights_name, &w, &witness, &expected, max_new) {
            Ok(true) => pass += 1,
            Ok(false) => {
                fail += 1;
                if fail_examples.len() < cli.print_failures {
                    let toks = render_spec(&witness);
                    let cfg = GenerateConfig { max_new_tokens: max_new };
                    let toks_ref: Vec<&str> = toks.iter().map(|s| s.as_str()).collect();
                    let predicted = generate(&w, &toks_ref, &cfg).unwrap_or_default();
                    let got = parse_out_bytes(&predicted);
                    fail_examples.push((i, witness.clone(), expected.clone(), got));
                }
            }
            Err(e) => {
                fail += 1;
                eprintln!("  [{i:5}] ERROR: {e}");
            }
        }
        if (i + 1) % progress_every == 0 {
            let dt = t0.elapsed().as_secs_f64();
            let rate = (i + 1) as f64 / dt;
            let eta = (cli.count - i - 1) as f64 / rate;
            eprintln!(
                "  [{:>5}/{}] {} ok / {} fail  ({:.1}/s, ETA {:.0}s)",
                i + 1,
                cli.count,
                pass,
                fail,
                rate,
                eta
            );
        }
    }

    let dt = t0.elapsed().as_secs_f64();
    println!("\n=== {prim} ({weights_name}) summary ===");
    println!("  pass: {pass}/{}    fail: {fail}", cli.count);
    println!("  time: {dt:.1}s    rate: {:.1}/s", cli.count as f64 / dt);
    for (i, w_, exp, got) in &fail_examples {
        println!("  FAIL #{i}: witness={:?}  expected={:?}  got={:?}", w_, exp, got);
    }
    if fail > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn seed_for(p: &str) -> u64 {
    // Distinct per-primitive bias so each sweep gets different vectors when
    // run with the same --seed flag.
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
            // little-endian u128 compare
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
        _ => panic!("unknown primitive {prim}"),
    }
}
