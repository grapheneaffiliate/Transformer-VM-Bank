//! Unsigned LEB128 varint codec for scheme prefixes.
//!
//! Same encoding as WebAssembly section IDs and IPFS multicodec.
//! Per ADR-0007: every signature, public key, KEM ciphertext, and
//! hash blob in PSL is prefixed with a varint scheme identifier in
//! this encoding.

/// Maximum length (in bytes) of a varint we accept on the wire.
/// 5 bytes is enough for any `u32` (max 0xFFFF_FFFF = 5×7 bits =
/// 35 bits coverage). Reading more than 5 bytes is malformed and
/// rejected.
pub const MAX_VARINT_LEN: usize = 5;

/// Encode a `u32` as an unsigned LEB128 varint, appending to `out`.
/// Returns the number of bytes written.
pub fn encode_varint(mut value: u32, out: &mut Vec<u8>) -> usize {
    let start = out.len();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
            out.push(byte);
        } else {
            out.push(byte);
            return out.len() - start;
        }
    }
}

/// Decode a varint from `bytes`. Returns `(value, bytes_consumed)`.
/// Malformed input returns [`VarintError`].
pub fn decode_varint(bytes: &[u8]) -> Result<(u32, usize), VarintError> {
    let mut result: u32 = 0;
    let mut shift: u32 = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if i >= MAX_VARINT_LEN {
            return Err(VarintError::TooLong);
        }
        let chunk = (byte & 0x7f) as u32;
        result = result
            .checked_add(chunk.checked_shl(shift).ok_or(VarintError::Overflow)?)
            .ok_or(VarintError::Overflow)?;
        if byte & 0x80 == 0 {
            return Ok((result, i + 1));
        }
        shift = shift.checked_add(7).ok_or(VarintError::Overflow)?;
    }
    Err(VarintError::Truncated)
}

/// Errors decoding a varint.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VarintError {
    /// Input ended before the high-bit-cleared terminator byte.
    #[error("varint input ended before terminator")]
    Truncated,
    /// Varint is longer than `MAX_VARINT_LEN` bytes (would overflow `u32`).
    #[error("varint is longer than {MAX_VARINT_LEN} bytes")]
    TooLong,
    /// Decoded value exceeds the destination type's range.
    #[error("varint value overflowed u32")]
    Overflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_small() {
        for v in [
            0u32,
            1,
            0x7f,
            0x80,
            0x3fff,
            0x4000,
            0xffff,
            0x10000,
            0xffff_ffff,
        ] {
            let mut buf = Vec::new();
            let n = encode_varint(v, &mut buf);
            assert_eq!(buf.len(), n);
            let (decoded, consumed) = decode_varint(&buf).unwrap();
            assert_eq!(decoded, v);
            assert_eq!(consumed, n);
        }
    }

    #[test]
    fn one_byte_for_small_values() {
        let mut buf = Vec::new();
        encode_varint(0x7f, &mut buf);
        assert_eq!(buf.len(), 1);
    }

    #[test]
    fn two_bytes_at_boundary() {
        let mut buf = Vec::new();
        encode_varint(0x80, &mut buf);
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn truncated_is_rejected() {
        // High bit set means "more bytes follow", but there are none.
        let buf = [0x80];
        assert_eq!(decode_varint(&buf), Err(VarintError::Truncated));
    }

    #[test]
    fn too_long_is_rejected() {
        let buf = [0x80; MAX_VARINT_LEN + 1];
        assert_eq!(decode_varint(&buf), Err(VarintError::TooLong));
    }

    #[test]
    fn extra_bytes_after_terminator_not_consumed() {
        let mut buf = Vec::new();
        encode_varint(42, &mut buf);
        buf.extend_from_slice(&[0xaa, 0xbb, 0xcc]);
        let (v, n) = decode_varint(&buf).unwrap();
        assert_eq!(v, 42);
        assert_eq!(n, 1);
    }
}
