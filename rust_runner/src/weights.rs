//! Weights file parser — Phase 1.5.
//!
//! Reads the binary format produced by
//! `transformer_vm.model.weights::save_weights` (see Transformer-VM/transformer_vm/model/weights.py:691).
//!
//! Layout (little-endian throughout):
//!   1. Header (24 bytes): 6× i32 = vocab, d_model, n_layers, n_heads, d_ffn, stop_token_id
//!      If d_ffn < 0, then |d_ffn| is max width and per-layer widths follow.
//!   2. Token table: vocab × ( u32 length-prefix || UTF-8 string ).
//!   3. If d_ffn < 0: n_layers × i32 = per-layer ffn width.
//!   4. Weights as packed f64 (row-major):
//!        - tok.weight                       [vocab, d_model]
//!        - per layer:
//!            attn[i].in_proj_weight          [3*d_model, d_model]
//!            attn[i].out_proj.weight         [d_model, d_model]
//!            ff_in[i].weight (first 2*w rows)[2*ffn_width[i], d_model]
//!            ff_out[i].weight (first w cols) [d_model, ffn_width[i]]
//!        - head.weight                       [vocab, d_model]
//!   5. has_erase (i32). If 1 → per-layer (attn_erase, ffn_erase) lists. Loaded but
//!      unused — the C++ engine reads them and the Python runner does not reference
//!      them. PSL skips them on load.
//!   6. has_tiebreak (i32). If 1 → per-layer-per-head i32 flags. PSL pins
//!      `StandardKVCache` which has no `set_tiebreak`, so these are also skipped.

use anyhow::{anyhow, Context, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use ndarray::Array2;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Header {
    pub vocab: usize,
    pub d_model: usize,
    pub n_layers: usize,
    pub n_heads: usize,
    pub d_ffn_per_layer: Vec<usize>,
    pub stop_token_id: i32,
}

#[derive(Clone, Debug)]
pub struct LayerWeights {
    pub in_proj: Array2<f64>,        // [3*d_model, d_model]
    pub out_proj: Array2<f64>,       // [d_model, d_model]
    pub ff_in: Array2<f64>,          // [2*d_ffn, d_model]
    pub ff_out: Array2<f64>,         // [d_model, d_ffn]
}

#[derive(Clone, Debug)]
pub struct Weights {
    pub header: Header,
    pub tokens: Vec<String>,
    pub tok_to_idx: HashMap<String, usize>,
    pub tok_embed: Array2<f64>,      // [vocab, d_model]
    pub layers: Vec<LayerWeights>,
    pub head: Array2<f64>,            // [vocab, d_model]
}

fn read_f64_array<R: Read>(r: &mut R, rows: usize, cols: usize) -> Result<Array2<f64>> {
    let n = rows * cols;
    let mut buf = vec![0u8; n * 8];
    r.read_exact(&mut buf).context("reading f64 tensor")?;
    let mut data = Vec::with_capacity(n);
    for chunk in buf.chunks_exact(8) {
        let bytes: [u8; 8] = chunk.try_into().unwrap();
        data.push(f64::from_le_bytes(bytes));
    }
    Array2::from_shape_vec((rows, cols), data)
        .map_err(|e| anyhow!("ndarray shape: {e}"))
}

pub fn load_weights(path: &Path) -> Result<Weights> {
    let f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut r = BufReader::new(f);

    // Header
    let vocab = r.read_i32::<LittleEndian>()? as usize;
    let d_model = r.read_i32::<LittleEndian>()? as usize;
    let n_layers = r.read_i32::<LittleEndian>()? as usize;
    let n_heads = r.read_i32::<LittleEndian>()? as usize;
    let header_d_ffn = r.read_i32::<LittleEndian>()?;
    let stop_token_id = r.read_i32::<LittleEndian>()?;

    // Token table
    let mut tokens = Vec::with_capacity(vocab);
    for _ in 0..vocab {
        let len = r.read_u32::<LittleEndian>()? as usize;
        let mut bytes = vec![0u8; len];
        r.read_exact(&mut bytes).context("reading token bytes")?;
        tokens.push(String::from_utf8(bytes).context("non-UTF-8 token")?);
    }
    let tok_to_idx: HashMap<String, usize> = tokens
        .iter()
        .enumerate()
        .map(|(i, s)| (s.clone(), i))
        .collect();

    // Per-layer ffn widths
    let d_ffn_per_layer: Vec<usize> = if header_d_ffn < 0 {
        let mut widths = Vec::with_capacity(n_layers);
        for _ in 0..n_layers {
            widths.push(r.read_i32::<LittleEndian>()? as usize);
        }
        widths
    } else {
        vec![header_d_ffn as usize; n_layers]
    };

    let header = Header {
        vocab,
        d_model,
        n_layers,
        n_heads,
        d_ffn_per_layer: d_ffn_per_layer.clone(),
        stop_token_id,
    };

    // Token embedding
    let tok_embed = read_f64_array(&mut r, vocab, d_model)?;

    // Per-layer weights
    let mut layers = Vec::with_capacity(n_layers);
    for li in 0..n_layers {
        let in_proj = read_f64_array(&mut r, 3 * d_model, d_model)?;
        let out_proj = read_f64_array(&mut r, d_model, d_model)?;
        let width = d_ffn_per_layer[li];
        let ff_in = read_f64_array(&mut r, 2 * width, d_model)?;
        let ff_out = read_f64_array(&mut r, d_model, width)?;
        layers.push(LayerWeights { in_proj, out_proj, ff_in, ff_out });
    }

    // Head
    let head_w = read_f64_array(&mut r, vocab, d_model)?;

    // Optional erase / tiebreak — loaded but ignored (StandardKVCache cache).
    let has_erase = r.read_i32::<LittleEndian>().unwrap_or(0);
    if has_erase != 0 {
        for _ in 0..n_layers {
            let n = r.read_i32::<LittleEndian>()? as usize;
            for _ in 0..n {
                let _ = r.read_i32::<LittleEndian>()?;
            }
            let n = r.read_i32::<LittleEndian>()? as usize;
            for _ in 0..n {
                let _ = r.read_i32::<LittleEndian>()?;
            }
        }
    }
    let _has_tiebreak = r.read_i32::<LittleEndian>().unwrap_or(0);
    // PSL pins StandardKVCache which has no tiebreak — skip the bytes if present.
    // (The C++ engine and Python both read these regardless; we don't need them.)

    Ok(Weights {
        header,
        tokens,
        tok_to_idx,
        tok_embed,
        layers,
        head: head_w,
    })
}
