//! Ternary network types and forward kernel.
//!
//! Storage: per layer, two sparse-index lists per row — `pos_indices[i]`
//! lists the columns where the weight is +1, `neg_indices[i]` lists -1
//! columns. Weight 0 is implicit (not stored). Bias is a `Vec<i64>`.
//!
//! Activation: ReLU on i64 between layers (final layer is linear).
//!
//! All arithmetic uses `checked_add` / `checked_sub`. Overflow in a
//! settlement-layer execution must fail loudly, not wrap silently.

use crate::error::TernaryError;
use crate::weights::WeightsHeader;

/// One layer of a sparse ternary network. Each output row has a list
/// of input column indices where the weight is +1 (`pos_indices`) and
/// a list where the weight is -1 (`neg_indices`). All other weights
/// are zero. `bias[i]` is added to the accumulator for output row `i`.
///
/// Invariants:
/// - `pos_indices.len() == neg_indices.len() == bias.len() == output_dim`.
/// - Each index in `pos_indices[i]` and `neg_indices[i]` is `< input_dim`.
/// - `pos_indices[i]` and `neg_indices[i]` are disjoint (a position is
///   never both +1 and -1).
#[derive(Clone, Debug)]
pub struct SparseTernaryLayer {
    pub input_dim: usize,
    pub output_dim: usize,
    pub pos_indices: Vec<Vec<u32>>,
    pub neg_indices: Vec<Vec<u32>>,
    pub bias: Vec<i64>,
    /// True when the layer's output is fed through ReLU before the
    /// next layer. False on the final / linear-output layer.
    pub relu: bool,
}

impl SparseTernaryLayer {
    /// y[i] = bias[i] + Σ_{j ∈ pos[i]} x[j] − Σ_{j ∈ neg[i]} x[j]
    /// then ReLU if `self.relu`.
    pub fn forward(&self, x: &[i64], y: &mut [i64], layer_idx: usize) -> Result<(), TernaryError> {
        if x.len() != self.input_dim {
            return Err(TernaryError::LayerShape {
                layer: layer_idx,
                got: x.len(),
                expected: self.input_dim,
            });
        }
        if y.len() != self.output_dim {
            return Err(TernaryError::LayerShape {
                layer: layer_idx,
                got: y.len(),
                expected: self.output_dim,
            });
        }
        for i in 0..self.output_dim {
            let mut acc: i64 = self.bias[i];
            for &j in &self.pos_indices[i] {
                acc = acc
                    .checked_add(x[j as usize])
                    .ok_or(TernaryError::Overflow {
                        layer: layer_idx,
                        row: i,
                    })?;
            }
            for &j in &self.neg_indices[i] {
                acc = acc
                    .checked_sub(x[j as usize])
                    .ok_or(TernaryError::Overflow {
                        layer: layer_idx,
                        row: i,
                    })?;
            }
            y[i] = if self.relu && acc < 0 { 0 } else { acc };
        }
        Ok(())
    }

    /// Number of non-zero weights in this layer (used for sparsity stats
    /// and the packed weight format).
    pub fn nnz(&self) -> usize {
        self.pos_indices.iter().map(|v| v.len()).sum::<usize>()
            + self.neg_indices.iter().map(|v| v.len()).sum::<usize>()
    }
}

/// A ternary network = ordered stack of [`SparseTernaryLayer`]s plus an
/// identifying header.
#[derive(Clone, Debug)]
pub struct TernaryNetwork {
    pub header: WeightsHeader,
    pub layers: Vec<SparseTernaryLayer>,
}

impl TernaryNetwork {
    pub fn new(header: WeightsHeader, layers: Vec<SparseTernaryLayer>) -> Self {
        Self { header, layers }
    }

    /// Single-shot forward pass. Allocates one scratch buffer per layer
    /// — the kernel itself is `checked_add`/`checked_sub` only, no fp.
    pub fn forward(&self, input: &[i64]) -> Result<Vec<i64>, TernaryError> {
        if self.layers.is_empty() {
            return Ok(input.to_vec());
        }
        if input.len() != self.layers[0].input_dim {
            return Err(TernaryError::InputShape {
                got: input.len(),
                expected: self.layers[0].input_dim,
            });
        }

        let mut current: Vec<i64> = input.to_vec();
        for (li, layer) in self.layers.iter().enumerate() {
            let mut next = vec![0i64; layer.output_dim];
            layer.forward(&current, &mut next, li)?;
            current = next;
        }
        Ok(current)
    }

    pub fn total_nnz(&self) -> usize {
        self.layers.iter().map(|l| l.nnz()).sum()
    }

    pub fn total_slots(&self) -> usize {
        self.layers
            .iter()
            .map(|l| l.input_dim.saturating_mul(l.output_dim))
            .sum()
    }
}

/// Argmax of a slice of i64. Returns the index of the maximum value
/// (ties broken by lowest index, matching `np.argmax` and PyTorch
/// `argmax`). Errors if input is empty.
pub fn argmax(logits: &[i64]) -> Result<usize, TernaryError> {
    let mut best_idx = 0usize;
    let mut best_val = i64::MIN;
    let mut found = false;
    for (i, &v) in logits.iter().enumerate() {
        if !found || v > best_val {
            best_val = v;
            best_idx = i;
            found = true;
        }
    }
    if !found {
        return Err(TernaryError::EmptyArgmax);
    }
    Ok(best_idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weights::WeightsHeader;

    #[test]
    fn argmax_empty_errors() {
        let got = argmax(&[]);
        assert!(matches!(got, Err(TernaryError::EmptyArgmax)));
    }

    #[test]
    fn argmax_ties_take_lowest_index() {
        // ties go to the first occurrence
        assert_eq!(argmax(&[3, 5, 5, 1]).unwrap(), 1);
        assert_eq!(argmax(&[5, 5, 5]).unwrap(), 0);
    }

    #[test]
    fn forward_identity_layer() {
        // 1x3 layer that just copies input rows into output (using weights).
        // pos_indices[i] = [i], no neg, bias 0 → y[i] = x[i]
        let layer = SparseTernaryLayer {
            input_dim: 3,
            output_dim: 3,
            pos_indices: vec![vec![0], vec![1], vec![2]],
            neg_indices: vec![vec![]; 3],
            bias: vec![0; 3],
            relu: false,
        };
        let net = TernaryNetwork::new(
            WeightsHeader {
                version: 1,
                primitive: "test".into(),
                input_dim: 3,
                output_dim: 3,
                weights_hash: [0; 32],
            },
            vec![layer],
        );
        let out = net.forward(&[7, -3, 42]).unwrap();
        assert_eq!(out, vec![7, -3, 42]);
    }

    #[test]
    fn forward_relu_clamps_negatives() {
        // single layer y = x with relu
        let layer = SparseTernaryLayer {
            input_dim: 2,
            output_dim: 2,
            pos_indices: vec![vec![0], vec![1]],
            neg_indices: vec![vec![]; 2],
            bias: vec![0; 2],
            relu: true,
        };
        let net = TernaryNetwork::new(
            WeightsHeader {
                version: 1,
                primitive: "test".into(),
                input_dim: 2,
                output_dim: 2,
                weights_hash: [0; 32],
            },
            vec![layer],
        );
        assert_eq!(net.forward(&[5, -7]).unwrap(), vec![5, 0]);
    }

    #[test]
    fn forward_overflow_errors_does_not_panic() {
        // bias = i64::MAX, then add 1 → checked_add returns None → error
        let layer = SparseTernaryLayer {
            input_dim: 1,
            output_dim: 1,
            pos_indices: vec![vec![0]],
            neg_indices: vec![vec![]],
            bias: vec![i64::MAX],
            relu: false,
        };
        let net = TernaryNetwork::new(
            WeightsHeader {
                version: 1,
                primitive: "test".into(),
                input_dim: 1,
                output_dim: 1,
                weights_hash: [0; 32],
            },
            vec![layer],
        );
        let got = net.forward(&[1]);
        assert!(matches!(
            got,
            Err(TernaryError::Overflow { layer: 0, row: 0 })
        ));
    }
}
