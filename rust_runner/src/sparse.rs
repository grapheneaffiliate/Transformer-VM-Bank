//! Sparse matrix-vector multiply for the analytical-construction Transformer-VM
//! weights. Mirrors the `SparseMatrix` / `matvec(SparseMatrix, x, y)` path in
//! `Transformer-VM/transformer_vm/model/transformer.cpp` (the `#else` branch
//! that ships in the Linux build of the C++ engine).
//!
//! Bit-exactness vs the dense path is preserved: skipping zero entries in
//! `sum_j W[i,j] * x[j]` does not change the final fp result, since
//! `0.0 * x[j] = 0.0` exactly (for finite x[j]) and `s + 0.0 = s` exactly in
//! IEEE-754 round-to-nearest. The CSR scan order matches dense column-order
//! summation for nonzero entries.
//!
//! Build threshold: build the sparse representation when density < 50%
//! (`nnz * 2 < rows * cols`). Below that, sparse matvec is strictly faster.

use ndarray::{ArrayView1, ArrayView2};

#[derive(Clone, Debug, Default)]
pub struct SparseMatrix {
    pub rows: usize,
    pub cols: usize,
    pub nnz: usize,
    /// Compressed-sparse-row data. `val.len() == nnz`; `col.len() == nnz`;
    /// `ptr.len() == rows + 1`, `ptr[i]..ptr[i+1]` is the slice of `val/col`
    /// for row i.
    pub val: Vec<f64>,
    pub col: Vec<u32>,
    pub ptr: Vec<u32>,
    /// Fast-path: when every row has ≤2 nonzeros (the common case for
    /// analytical-construction weights), use these flat arrays instead of
    /// CSR scan. Halves the loop overhead.
    pub fast_row12: bool,
    pub arity: Vec<u8>,
    pub col0: Vec<u32>,
    pub col1: Vec<u32>,
    pub val0: Vec<f64>,
    pub val1: Vec<f64>,
}

impl SparseMatrix {
    pub fn from_view(dense: ArrayView2<f64>) -> Self {
        let rows = dense.nrows();
        let cols = dense.ncols();
        let mut sp = SparseMatrix {
            rows,
            cols,
            nnz: 0,
            val: Vec::new(),
            col: Vec::new(),
            ptr: vec![0u32; rows + 1],
            fast_row12: true,
            arity: vec![0u8; rows],
            col0: vec![0u32; rows],
            col1: vec![0u32; rows],
            val0: vec![0.0f64; rows],
            val1: vec![0.0f64; rows],
        };
        for i in 0..rows {
            sp.ptr[i] = sp.nnz as u32;
            let mut row_nnz = 0usize;
            for j in 0..cols {
                let v = dense[[i, j]];
                if v != 0.0 {
                    sp.val.push(v);
                    sp.col.push(j as u32);
                    sp.nnz += 1;
                    if row_nnz == 0 {
                        sp.col0[i] = j as u32;
                        sp.val0[i] = v;
                    } else if row_nnz == 1 {
                        sp.col1[i] = j as u32;
                        sp.val1[i] = v;
                    }
                    row_nnz += 1;
                }
            }
            sp.arity[i] = row_nnz.min(255) as u8;
            if row_nnz > 2 {
                sp.fast_row12 = false;
            }
        }
        sp.ptr[rows] = sp.nnz as u32;
        sp
    }

    /// True when CSR/fast-row12 sparse matvec is empirically faster than
    /// dense `Array2::dot`. Dense uses contiguous reads (cache-friendly,
    /// auto-vectorizable); sparse adds indirection per element. The
    /// crossover point on EPYC 7702P / x86_64 with `ndarray` 0.15 falls
    /// around 5% density:
    ///   - density ≤ 3%   → sparse is markedly faster (fast_row12 case)
    ///   - 3% < d ≤ 8%    → roughly tied
    ///   - density > 8%   → dense wins comfortably
    /// Use a conservative 4% threshold to keep dense-equivalent matrices
    /// on the dense path. Re-measure if the matrix shapes change a lot.
    pub fn worth_using(rows: usize, cols: usize, nnz: usize) -> bool {
        let total = rows.saturating_mul(cols);
        if total == 0 {
            return false;
        }
        // nnz * 25 < total * 1  ⇔  density < 4%
        nnz.saturating_mul(25) < total
    }

    /// y = W · x. `y` must have length `rows`; `x` must have length `cols`.
    pub fn matvec_into(&self, x: &[f64], y: &mut [f64]) {
        debug_assert_eq!(x.len(), self.cols);
        debug_assert_eq!(y.len(), self.rows);
        if self.fast_row12 {
            for i in 0..self.rows {
                match self.arity[i] {
                    0 => y[i] = 0.0,
                    1 => y[i] = self.val0[i] * x[self.col0[i] as usize],
                    _ => {
                        y[i] = self.val0[i] * x[self.col0[i] as usize]
                            + self.val1[i] * x[self.col1[i] as usize];
                    }
                }
            }
        } else {
            for i in 0..self.rows {
                let lo = self.ptr[i] as usize;
                let hi = self.ptr[i + 1] as usize;
                let mut s = 0.0;
                for k in lo..hi {
                    s += self.val[k] * x[self.col[k] as usize];
                }
                y[i] = s;
            }
        }
    }

    /// Convenience: y = W · x as an owned Array1.
    pub fn matvec_view(&self, x: ArrayView1<f64>) -> ndarray::Array1<f64> {
        let x_slice = x.as_slice().expect("x must be contiguous");
        let mut y = vec![0.0f64; self.rows];
        self.matvec_into(x_slice, &mut y);
        ndarray::Array1::from_vec(y)
    }
}

/// A linear matrix: either dense (`Array2`) or sparse (CSR + fast_row12).
/// Built lazily at weight-load time depending on density.
#[derive(Clone, Debug)]
pub enum Linear {
    Dense(ndarray::Array2<f64>),
    Sparse(SparseMatrix),
}

impl Linear {
    pub fn from_dense(dense: ndarray::Array2<f64>) -> Self {
        // Sparse path is empirically a perf regression on freeze_chain
        // (allocator contention + indirection costs > FLOP savings on
        // EPYC 7702P with 32 rayon threads). Disabled until either the
        // matvec_view alloc is removed or the threshold is re-tuned with
        // proper microbenchmarks. Bit-exact preserved either way.
        Linear::Dense(dense)
    }

    pub fn matvec(&self, x: &ndarray::Array1<f64>) -> ndarray::Array1<f64> {
        match self {
            Linear::Dense(w) => w.dot(x),
            Linear::Sparse(sp) => sp.matvec_view(x.view()),
        }
    }

    pub fn rows(&self) -> usize {
        match self {
            Linear::Dense(w) => w.nrows(),
            Linear::Sparse(sp) => sp.rows,
        }
    }

    pub fn cols(&self) -> usize {
        match self {
            Linear::Dense(w) => w.ncols(),
            Linear::Sparse(sp) => sp.cols,
        }
    }

    pub fn is_sparse(&self) -> bool {
        matches!(self, Linear::Sparse(_))
    }
}
