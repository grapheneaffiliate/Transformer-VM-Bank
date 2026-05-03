//! PSL trace_hash: BLAKE3(utf8(" ".join(predicted_tokens))).
//!
//! Reference: docs/ARCHITECTURE.md § 0.2. Must match `tools/verify_trace.py`
//! byte-for-byte.

use crate::hash::{hash_bytes, Hash};

pub fn hash_trace(tokens: &[&str]) -> Hash {
    let canonical = tokens.join(" ");
    hash_bytes(canonical.as_bytes())
}

pub fn hash_trace_owned(tokens: &[String]) -> Hash {
    let canonical = tokens.join(" ");
    hash_bytes(canonical.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_trace_hashes_blake3_of_empty_string() {
        let h = hash_trace(&[]);
        assert_eq!(h, hash_bytes(b""));
    }

    #[test]
    fn matches_python_reference() {
        // Replicate tools/verify_trace.py output for "start halt"
        let h = hash_trace(&["start", "halt"]);
        let expected = hash_bytes(b"start halt");
        assert_eq!(h, expected);
    }

    #[test]
    fn space_is_separator_not_terminator() {
        let h1 = hash_trace(&["a", "b"]);
        let h2 = hash_bytes(b"a b");
        let h3 = hash_bytes(b"a b ");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3, "trailing space must change hash");
    }
}
