//! `psl-runner` CLI: drop-in replacement for `wasm-run` once the port is complete.
//!
//! Usage: `psl-runner --weights <path>.bin --input <input>.txt`

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    weights: PathBuf,
    #[arg(long)]
    input: PathBuf,
    #[arg(long, default_value_t = 50_000)]
    max_new_tokens: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let w = psl_rust_runner::weights::load_weights(&cli.weights)
        .context("loading weights")?;
    let input_str = std::fs::read_to_string(&cli.input)?;
    let input_owned: Vec<String> = input_str.split_whitespace().map(|s| s.to_string()).collect();
    let input: Vec<&str> = input_owned.iter().map(|s| s.as_str()).collect();
    let cfg = psl_rust_runner::GenerateConfig {
        max_new_tokens: cli.max_new_tokens,
    };
    let predicted = psl_rust_runner::generate(&w, &input, &cfg)?;
    println!("{}", predicted.join(" "));
    Ok(())
}
