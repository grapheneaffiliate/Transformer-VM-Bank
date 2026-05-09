//! Thermometer encoding helpers.
//!
//! Convention: `thermo(v, max_val)` produces a length-(max_val+1) vector
//! with the first `v+1` entries set to 1 and the rest to 0. So:
//!
//! ```text
//! thermo(0, 3) = [1, 0, 0, 0]
//! thermo(2, 3) = [1, 1, 1, 0]
//! thermo(3, 3) = [1, 1, 1, 1]
//! ```
//!
//! Sum of all entries = `v + 1`.
//!
//! This convention matches the PoC and is what the layer-1 ternary
//! `{+1}-weighted sum` design implicitly uses.

/// Encode an integer `v ∈ [0, max_val]` as a length `max_val + 1`
/// thermometer vector. Returns an error-by-clamping if `v > max_val`
/// (caller should bounds-check at the API boundary, not here).
pub fn encode(v: i64, max_val: i64) -> Vec<i64> {
    let len = (max_val + 1) as usize;
    let mut out = vec![0i64; len];
    let cap = (v.min(max_val).max(0) + 1) as usize;
    let cap = cap.min(len);
    for slot in out.iter_mut().take(cap) {
        *slot = 1;
    }
    out
}

/// Decode a thermometer vector back to its integer value:
/// the count of leading 1s minus 1.
///
/// Tolerates a malformed thermometer (1s after the first 0) by counting
/// only the leading run, since that's what argmax-equivalent decode does
/// in practice and matches the ternary network's downstream layers.
pub fn decode(thermo: &[i64]) -> i64 {
    let mut count = 0i64;
    for &v in thermo {
        if v == 0 {
            break;
        }
        count = count.checked_add(1).expect("thermo length overflow");
    }
    count - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_basic() {
        assert_eq!(encode(0, 3), vec![1, 0, 0, 0]);
        assert_eq!(encode(2, 3), vec![1, 1, 1, 0]);
        assert_eq!(encode(3, 3), vec![1, 1, 1, 1]);
    }

    #[test]
    fn encode_clamp_in_range() {
        // max value: thermometer is fully on
        let t = encode(255, 255);
        assert_eq!(t.len(), 256);
        assert_eq!(t.iter().sum::<i64>(), 256);
    }

    #[test]
    fn encode_decode_roundtrip() {
        for v in 0..=255 {
            let t = encode(v, 255);
            assert_eq!(decode(&t), v);
        }
    }

    #[test]
    fn sum_of_thermo_equals_v_plus_1() {
        for v in 0..=10 {
            let t = encode(v, 10);
            assert_eq!(t.iter().sum::<i64>(), v + 1);
        }
    }
}
