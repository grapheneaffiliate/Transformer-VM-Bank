//! Typed error enum. No panics in production paths — all fallible
//! operations return `Result<T, TernaryError>`.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TernaryError {
    #[error("integer overflow in ternary kernel at layer {layer}, row {row}")]
    Overflow { layer: usize, row: usize },

    #[error("input length {got}, expected {expected}")]
    InputShape { got: usize, expected: usize },

    #[error("input value {value} out of range [0, {max}] for primitive {primitive}")]
    InputRange {
        primitive: &'static str,
        value: i64,
        max: i64,
    },

    #[error("output decode failed: {0}")]
    OutputDecode(String),

    #[error("weights hash mismatch: expected {expected}, got {got}")]
    WeightsHashMismatch { expected: String, got: String },

    #[error("layer shape mismatch at layer {layer}: input dim {got}, expected {expected}")]
    LayerShape {
        layer: usize,
        got: usize,
        expected: usize,
    },

    #[error("argmax produced no result (empty logits)")]
    EmptyArgmax,
}
