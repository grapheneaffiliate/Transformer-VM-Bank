//! Packed weight format + BLAKE3 weights hash.
//!
//! ## On-disk format
//!
//! Header (canonical big-endian everywhere):
//!   - `magic`: 8 bytes, ASCII `b"TVMW0001"` (Ternary VM Weights v0001)
//!   - `version`: 4 bytes u32
//!   - `primitive_name_len`: 2 bytes u16
//!   - `primitive_name`: UTF-8 bytes
//!   - `input_dim`: 4 bytes u32
//!   - `output_dim`: 4 bytes u32
//!   - `n_layers`: 4 bytes u32
//!
//! Per-layer block (n_layers times):
//!   - `input_dim`: u32
//!   - `output_dim`: u32
//!   - `relu`: u8 (0 or 1)
//!   - `nnz_pos`: u64
//!   - `nnz_neg`: u64
//!   - `pos_ptr[output_dim+1]`: u32 each — CSR-style row offsets into pos
//!   - `pos_col[nnz_pos]`: u32 each
//!   - `neg_ptr[output_dim+1]`: u32 each
//!   - `neg_col[nnz_neg]`: u32 each
//!   - `bias[output_dim]`: i64 each (little-endian f64 not used)
//!
//! Final 32 bytes: BLAKE3 digest over ALL preceding bytes.
//!
//! `weights_hash(P)` per `docs/ARCHITECTURE.md` § 0.8 is the BLAKE3 of
//! everything *before* the trailing digest. The trailing digest is the
//! canonical commitment to the file's contents — verifiers check it on
//! load.

use crate::error::TernaryError;
use crate::network::SparseTernaryLayer;

/// Compact in-memory header. The on-disk format is more verbose; this
/// is what the runtime carries.
///
/// Carries **both** the v1 and v2 weights_hash per ADR-0008 dual-version
/// trace-hash contract (`docs/decisions/0008-blake3-512-for-long-lived-commitments.md`).
/// `pack_weights_dual` and `unpack_weights` populate both from the same
/// canonical packed payload (one BLAKE3 finalize at 32 bytes, one at
/// 64 bytes). Verifiers pick the field matching the trace-hash contract
/// version they are verifying.
///
/// ## Construction
///
/// **External code must not construct `WeightsHeader` via struct
/// literal.** The digest fields are `pub(crate)` so the dual-digest
/// invariant (both v1 and v2 are present and derived from the same
/// payload) cannot be violated by accident. Use one of:
///
/// - [`unpack_weights`] for loading from a packed byte stream — both
///   digests are computed from the payload.
/// - [`pack_weights_dual`] + [`WeightsHeader::new`] for constructing
///   a fresh network in code (per-primitive `build()` functions in
///   `ternary_vm::primitives::*` use this path).
///
/// In-crate code can still use the struct literal directly because
/// `pub(crate)` is visible inside the crate; that's appropriate for
/// the test-fixture and primitive-build paths and keeps the type-
/// system enforcement structural rather than convention-based.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightsHeader {
    /// Schema version. Currently 1.
    pub version: u32,
    /// Primitive name (descriptive; not a security-critical field).
    pub primitive: String,
    /// Input dimension of the first layer.
    pub input_dim: u32,
    /// Output dimension of the last layer.
    pub output_dim: u32,
    /// **v1 weights_hash.** BLAKE3-256 digest of the canonical packed-
    /// weights byte stream (see module docstring). Filled in by
    /// `pack_weights` / `unpack_weights` / [`WeightsHeader::new`].
    /// **Frozen** per ADR-0008 — `trace_hash_v1` reads this field.
    /// `pub(crate)` to enforce the dual-digest invariant: the only
    /// way for external code to obtain a populated field is through
    /// the constructor or `unpack_weights`.
    pub(crate) weights_hash: [u8; 32],
    /// **v2 weights_hash.** BLAKE3-512 digest of the canonical packed-
    /// weights byte stream — same input as `weights_hash`, just a
    /// 64-byte XOF read instead of 32. Per ADR-0008 this is the
    /// load-bearing commitment for new traces (256-bit Grover-halved
    /// quantum margin). `trace_hash_v2` reads this field.
    /// `pub(crate)` for the same dual-digest-invariant reason.
    pub(crate) weights_hash_v2: [u8; 64],
}

impl WeightsHeader {
    /// Construct a `WeightsHeader` with both digests supplied. The
    /// canonical path for external code; the constructor signature
    /// makes it impossible to forget either digest.
    ///
    /// In-crate callers (per-primitive `build()` functions) typically
    /// use the struct-literal form because both digests are computed
    /// in the immediately-preceding `pack_weights_dual()` call;
    /// either form preserves the invariant equivalently.
    pub fn new(
        version: u32,
        primitive: impl Into<String>,
        input_dim: u32,
        output_dim: u32,
        weights_hash: [u8; 32],
        weights_hash_v2: [u8; 64],
    ) -> Self {
        Self {
            version,
            primitive: primitive.into(),
            input_dim,
            output_dim,
            weights_hash,
            weights_hash_v2,
        }
    }

    /// Read access to the v1 (BLAKE3-256) weights_hash. Used by
    /// `trace_hash_v1` and by callers (e.g., `agent_contracts`) that
    /// commit to `program_hash` over the v1 weights_hash.
    pub fn weights_hash(&self) -> &[u8; 32] {
        &self.weights_hash
    }

    /// Read access to the v2 (BLAKE3-512) weights_hash. Used by
    /// `trace_hash_v2` and by callers committing to `program_hash`
    /// under the v2 contract.
    pub fn weights_hash_v2(&self) -> &[u8; 64] {
        &self.weights_hash_v2
    }
}

const MAGIC: &[u8; 8] = b"TVMW0001";

/// Serialize a network to the canonical byte stream and return the
/// stream + its BLAKE3-256 digest (the v1 weights_hash). The
/// BLAKE3-512 v2 digest is computed separately on demand via
/// [`pack_weights_v2`]; both digests are populated on the
/// `WeightsHeader` by the per-primitive `build()` functions.
pub fn pack_weights(
    primitive: &str,
    input_dim: u32,
    output_dim: u32,
    layers: &[SparseTernaryLayer],
) -> (Vec<u8>, [u8; 32]) {
    let mut buf = Vec::new();
    buf.extend_from_slice(MAGIC);
    buf.extend_from_slice(&1u32.to_be_bytes());
    let name_bytes = primitive.as_bytes();
    buf.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(name_bytes);
    buf.extend_from_slice(&input_dim.to_be_bytes());
    buf.extend_from_slice(&output_dim.to_be_bytes());
    buf.extend_from_slice(&(layers.len() as u32).to_be_bytes());
    for layer in layers {
        buf.extend_from_slice(&(layer.input_dim as u32).to_be_bytes());
        buf.extend_from_slice(&(layer.output_dim as u32).to_be_bytes());
        buf.push(if layer.relu { 1 } else { 0 });
        let nnz_pos: usize = layer.pos_indices.iter().map(|v| v.len()).sum();
        let nnz_neg: usize = layer.neg_indices.iter().map(|v| v.len()).sum();
        buf.extend_from_slice(&(nnz_pos as u64).to_be_bytes());
        buf.extend_from_slice(&(nnz_neg as u64).to_be_bytes());
        // pos CSR
        let mut ptr = 0u32;
        for row in &layer.pos_indices {
            buf.extend_from_slice(&ptr.to_be_bytes());
            ptr = ptr.checked_add(row.len() as u32).expect("ptr overflow");
        }
        buf.extend_from_slice(&ptr.to_be_bytes());
        for row in &layer.pos_indices {
            for &c in row {
                buf.extend_from_slice(&c.to_be_bytes());
            }
        }
        // neg CSR
        let mut ptr = 0u32;
        for row in &layer.neg_indices {
            buf.extend_from_slice(&ptr.to_be_bytes());
            ptr = ptr.checked_add(row.len() as u32).expect("ptr overflow");
        }
        buf.extend_from_slice(&ptr.to_be_bytes());
        for row in &layer.neg_indices {
            for &c in row {
                buf.extend_from_slice(&c.to_be_bytes());
            }
        }
        // bias
        for &b in &layer.bias {
            buf.extend_from_slice(&b.to_be_bytes());
        }
    }
    let digest = blake3::hash(&buf);
    let mut digest_arr = [0u8; 32];
    digest_arr.copy_from_slice(digest.as_bytes());
    buf.extend_from_slice(&digest_arr);
    (buf, digest_arr)
}

/// Compute the v2 (BLAKE3-512) weights_hash from a canonical packed
/// weight stream produced by [`pack_weights`].
///
/// Per ADR-0008 the v2 digest is BLAKE3 read at 64 bytes via the XOF
/// API; the first 32 bytes equal the v1 BLAKE3-256 digest as a
/// determinism property (locked by `Blake3_512` test in
/// `crypto_agility/src/hash.rs`). v2 is hashed over the **payload**
/// (packed bytes excluding the trailing 32-byte v1 digest) — same
/// input domain as v1, just a wider output.
pub fn weights_hash_v2(packed: &[u8]) -> [u8; 64] {
    // The packed stream from pack_weights is `payload || v1_digest_32B`.
    // Hash the payload only, matching v1's input domain.
    if packed.len() < 32 {
        return [0u8; 64];
    }
    let payload = &packed[..packed.len() - 32];
    let mut hasher = blake3::Hasher::new();
    hasher.update(payload);
    let mut out = [0u8; 64];
    hasher.finalize_xof().fill(&mut out);
    out
}

/// Convenience: pack a network and return both v1 (BLAKE3-256, also
/// stored at the tail of the byte stream for unpack integrity) and v2
/// (BLAKE3-512) digests in one call. Per-primitive `build()`
/// functions use this to populate both fields on `WeightsHeader`.
pub fn pack_weights_dual(
    primitive: &str,
    input_dim: u32,
    output_dim: u32,
    layers: &[SparseTernaryLayer],
) -> (Vec<u8>, [u8; 32], [u8; 64]) {
    let (buf, digest_v1) = pack_weights(primitive, input_dim, output_dim, layers);
    let digest_v2 = weights_hash_v2(&buf);
    (buf, digest_v1, digest_v2)
}

/// Inverse of `pack_weights`. Verifies the BLAKE3 digest before
/// returning. Errors on integrity mismatch or shape mismatch.
pub fn unpack_weights(
    bytes: &[u8],
) -> Result<(WeightsHeader, Vec<SparseTernaryLayer>), TernaryError> {
    if bytes.len() < 32 + MAGIC.len() {
        return Err(TernaryError::OutputDecode("weights file too short".into()));
    }
    let payload_len = bytes.len() - 32;
    let stored_digest = &bytes[payload_len..];
    let computed = blake3::hash(&bytes[..payload_len]);
    if stored_digest != computed.as_bytes() {
        return Err(TernaryError::WeightsHashMismatch {
            expected: hex_str(stored_digest),
            got: hex_str(computed.as_bytes()),
        });
    }
    let mut digest = [0u8; 32];
    digest.copy_from_slice(stored_digest);

    let mut cur = Cursor {
        buf: &bytes[..payload_len],
        off: 0,
    };
    let magic = cur.take(8)?;
    if magic != MAGIC {
        return Err(TernaryError::OutputDecode(format!(
            "bad magic: {:?}",
            magic
        )));
    }
    let version = cur.read_u32()?;
    let name_len = cur.read_u16()? as usize;
    let name_bytes = cur.take(name_len)?;
    let primitive = String::from_utf8(name_bytes.to_vec())
        .map_err(|e| TernaryError::OutputDecode(format!("primitive name utf8: {e}")))?;
    let input_dim = cur.read_u32()?;
    let output_dim = cur.read_u32()?;
    let n_layers = cur.read_u32()? as usize;

    let mut layers = Vec::with_capacity(n_layers);
    for _ in 0..n_layers {
        let l_in = cur.read_u32()? as usize;
        let l_out = cur.read_u32()? as usize;
        let relu = cur.read_u8()? != 0;
        let nnz_pos = cur.read_u64()? as usize;
        let nnz_neg = cur.read_u64()? as usize;

        let pos_ptrs = (0..=l_out)
            .map(|_| cur.read_u32())
            .collect::<Result<Vec<_>, _>>()?;
        let pos_cols = (0..nnz_pos)
            .map(|_| cur.read_u32())
            .collect::<Result<Vec<_>, _>>()?;
        let neg_ptrs = (0..=l_out)
            .map(|_| cur.read_u32())
            .collect::<Result<Vec<_>, _>>()?;
        let neg_cols = (0..nnz_neg)
            .map(|_| cur.read_u32())
            .collect::<Result<Vec<_>, _>>()?;
        let bias = (0..l_out)
            .map(|_| cur.read_i64())
            .collect::<Result<Vec<_>, _>>()?;

        let mut pos_indices = Vec::with_capacity(l_out);
        for i in 0..l_out {
            let lo = pos_ptrs[i] as usize;
            let hi = pos_ptrs[i + 1] as usize;
            pos_indices.push(pos_cols[lo..hi].to_vec());
        }
        let mut neg_indices = Vec::with_capacity(l_out);
        for i in 0..l_out {
            let lo = neg_ptrs[i] as usize;
            let hi = neg_ptrs[i + 1] as usize;
            neg_indices.push(neg_cols[lo..hi].to_vec());
        }

        layers.push(SparseTernaryLayer {
            input_dim: l_in,
            output_dim: l_out,
            pos_indices,
            neg_indices,
            bias,
            relu,
        });
    }

    // Compute v2 (BLAKE3-512) digest from the same payload (everything
    // before the trailing v1 32-byte digest). This populates both
    // weights_hash fields on unpack so v1 and v2 trace_hash callers
    // can both verify against this network.
    let payload = &bytes[..payload_len];
    let mut v2 = [0u8; 64];
    let mut hasher = blake3::Hasher::new();
    hasher.update(payload);
    hasher.finalize_xof().fill(&mut v2);

    Ok((
        WeightsHeader {
            version,
            primitive,
            input_dim,
            output_dim,
            weights_hash: digest,
            weights_hash_v2: v2,
        },
        layers,
    ))
}

fn hex_str(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len() * 2);
    for byte in b {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

struct Cursor<'a> {
    buf: &'a [u8],
    off: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], TernaryError> {
        if self.off + n > self.buf.len() {
            return Err(TernaryError::OutputDecode(format!(
                "premature EOF: need {n} at offset {}",
                self.off
            )));
        }
        let s = &self.buf[self.off..self.off + n];
        self.off += n;
        Ok(s)
    }
    fn read_u8(&mut self) -> Result<u8, TernaryError> {
        Ok(self.take(1)?[0])
    }
    fn read_u16(&mut self) -> Result<u16, TernaryError> {
        let s = self.take(2)?;
        Ok(u16::from_be_bytes([s[0], s[1]]))
    }
    fn read_u32(&mut self) -> Result<u32, TernaryError> {
        let s = self.take(4)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn read_u64(&mut self) -> Result<u64, TernaryError> {
        let s = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(u64::from_be_bytes(a))
    }
    fn read_i64(&mut self) -> Result<i64, TernaryError> {
        let s = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(i64::from_be_bytes(a))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_then_unpack_roundtrips() {
        let layer = SparseTernaryLayer {
            input_dim: 3,
            output_dim: 2,
            pos_indices: vec![vec![0, 2], vec![1]],
            neg_indices: vec![vec![1], vec![]],
            bias: vec![5, -3],
            relu: true,
        };
        let (bytes, digest) = pack_weights("test", 3, 2, &[layer.clone()]);
        let (header, layers) = unpack_weights(&bytes).unwrap();
        assert_eq!(header.primitive, "test");
        assert_eq!(header.input_dim, 3);
        assert_eq!(header.output_dim, 2);
        assert_eq!(header.weights_hash, digest);
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].pos_indices, layer.pos_indices);
        assert_eq!(layers[0].neg_indices, layer.neg_indices);
        assert_eq!(layers[0].bias, layer.bias);
        assert_eq!(layers[0].relu, layer.relu);
    }

    #[test]
    fn flipped_byte_fails_integrity_check() {
        let layer = SparseTernaryLayer {
            input_dim: 2,
            output_dim: 1,
            pos_indices: vec![vec![0]],
            neg_indices: vec![vec![1]],
            bias: vec![0],
            relu: false,
        };
        let (mut bytes, _) = pack_weights("test", 2, 1, &[layer]);
        // Flip a byte in the body (not the trailing digest)
        bytes[20] ^= 0xff;
        let got = unpack_weights(&bytes);
        assert!(matches!(got, Err(TernaryError::WeightsHashMismatch { .. })));
    }
}
