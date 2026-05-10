//! Per-step logits dumper. Writes raw f64 logits (one [vocab] row per
//! generation step) plus argmax tokens. Used to diff against
//! tools/dump_logits_python.py for parity diagnosis on long primitives.
//!
//! Usage:
//!   cargo run -p psl-rust-runner --release --bin dump_logits -- \
//!       --weights weights/freeze_apply.bin \
//!       --input   data/freeze_apply_spec.txt \
//!       --max-gen 200

use anyhow::{Context, Result};
use clap::Parser;
use ndarray::{s, Array1};
use psl_rust_runner::{
    attention::StandardKVCache,
    weights::{load_weights, Weights},
};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    weights: PathBuf,
    #[arg(long)]
    input: PathBuf,
    #[arg(long, default_value_t = 200)]
    max_gen: usize,
    #[arg(long, default_value = "/tmp/rust")]
    out_prefix: String,
}

#[inline]
fn add_pos_enc(x: &mut Array1<f64>, pos: usize) {
    let p = pos as f64;
    x[0] += p;
    x[1] += 1.0 / (2.0_f64).ln() - 1.0 / ((p + 2.0).ln());
    x[2] += p * p;
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let w: Weights = load_weights(&cli.weights).context("loading weights")?;
    let input_str = std::fs::read_to_string(&cli.input)?;
    let tokens: Vec<&str> = input_str.split_whitespace().collect();
    let mut idx_list: Vec<usize> = tokens
        .iter()
        .map(|t| {
            *w.tok_to_idx
                .get(*t)
                .unwrap_or_else(|| panic!("unknown token {t:?}"))
        })
        .collect();
    let prompt_len = idx_list.len();

    let mut logits_file = BufWriter::new(File::create(format!("{}.logits.bin", cli.out_prefix))?);
    let mut argmax_file = BufWriter::new(File::create(format!("{}.argmax.txt", cli.out_prefix))?);

    let n_layers = w.header.n_layers;
    let n_heads = w.header.n_heads;
    let d_model = w.header.d_model;
    let stop = w.header.stop_token_id;
    let mut cache = StandardKVCache::new(n_layers, n_heads, d_model);

    let total_steps = prompt_len + cli.max_gen;
    let mut steps_written = 0usize;
    for pos in 0..total_steps {
        if pos >= idx_list.len() {
            break;
        }
        let mut x: Array1<f64> = w.tok_embed.row(idx_list[pos]).to_owned();
        add_pos_enc(&mut x, pos);

        for (li, layer) in w.layers.iter().enumerate() {
            let proj: Array1<f64> = layer.in_proj.matvec(&x);
            let q = proj.slice(s![0..d_model]).to_owned();
            let k = proj.slice(s![d_model..2 * d_model]).to_owned();
            let v = proj.slice(s![2 * d_model..3 * d_model]).to_owned();
            let attn_out = cache.layer_step(li, k, q, v);
            let out_proj_v = layer.out_proj.matvec(&attn_out);
            x = &x + &out_proj_v;

            let ffn_proj: Array1<f64> = layer.ff_in.matvec(&x);
            let width = w.header.d_ffn_per_layer[li];
            let gate = ffn_proj.slice(s![0..width]);
            let val = ffn_proj.slice(s![width..2 * width]);
            let mut act: Array1<f64> = Array1::zeros(width);
            for i in 0..width {
                act[i] = gate[i].max(0.0) * val[i];
            }
            let ff_out_v = layer.ff_out.matvec(&act);
            x = &x + &ff_out_v;
        }

        if pos + 1 == idx_list.len() {
            let logits: Array1<f64> = w.head.matvec(&x);
            let bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    logits.as_slice().unwrap().as_ptr() as *const u8,
                    logits.len() * 8,
                )
            };
            logits_file.write_all(bytes)?;
            let mut best_idx = 0usize;
            let mut best = f64::NEG_INFINITY;
            for (i, &v) in logits.iter().enumerate() {
                if v > best {
                    best = v;
                    best_idx = i;
                }
            }
            writeln!(argmax_file, "{}\t{}", best_idx, w.tokens[best_idx])?;
            idx_list.push(best_idx);
            steps_written += 1;
            if best_idx as i32 == stop {
                eprintln!("[stop] at pos {pos}");
                break;
            }
        }
    }
    eprintln!(
        "dumped {steps_written} steps to {}.{{logits.bin,argmax.txt}}",
        cli.out_prefix
    );
    Ok(())
}
