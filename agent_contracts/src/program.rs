//! `TernaryProgram` trait — the interface every Layer 2 contract
//! satisfies.

use crate::error::ContractError;

/// 32-byte BLAKE3 commitment to a contract's identity.
pub type ProgramHash = [u8; 32];

/// A pure-integer, deterministic, single-shot contract.
///
/// Same `input` → same `output` on any conformant integer-arithmetic
/// host (`docs/ARCHITECTURE.md` § 0.8). The `program_hash` is BLAKE3
/// over a canonical encoding of the contract's identity (name +
/// sub-network weights_hashes in fixed order) and plays the role of
/// `weights_hash(P)` in `trace_hash_ternary`.
pub trait TernaryProgram {
    /// Stable program identifier — name string in lower_snake_case.
    fn name(&self) -> &'static str;

    /// Run the contract on raw input bytes; return the raw output bytes.
    /// Pre-conditions (e.g. balance ≥ amount) are checked inside; on
    /// violation the contract returns a no-op output (defined per
    /// contract) rather than erroring, matching the `ledger_*.c`
    /// conventions used for the gate-1 sweep.
    fn run(&self, input: &[u8]) -> Result<Vec<u8>, ContractError>;

    /// BLAKE3 over `name() || sub_network_hashes...` in canonical
    /// order. Stable across re-builds of the same source.
    fn program_hash(&self) -> ProgramHash;

    /// Compute `trace_hash_ternary` for a (input, output) pair.
    fn trace_hash(&self, input: &[u8], output: &[u8]) -> ProgramHash {
        let mut h = blake3::Hasher::new();
        h.update(&self.program_hash());
        h.update(&(input.len() as u32).to_be_bytes());
        h.update(input);
        h.update(&(output.len() as u32).to_be_bytes());
        h.update(output);
        let mut out = [0u8; 32];
        out.copy_from_slice(h.finalize().as_bytes());
        out
    }
}
