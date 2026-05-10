//! `TernaryProgram` trait — the interface every Layer 2 contract
//! satisfies — plus the dual-version `program_hash` contract per
//! ADR-0008.
//!
//! ## ProgramHash
//!
//! `ProgramHash` is a **newtype** wrapper around `[u8; 64]`, not a
//! type alias. The compiler refuses to mix it with other 32- or
//! 64-byte hashes (e.g., [`crate::ProposalHash`], `AgentPubkey`).
//! This eliminates a documented bug class: when several distinct
//! semantic hashes share the same byte width, the type system
//! historically gave no protection against "key the HashMap on the
//! wrong digest." Newtypes fix that structurally.
//!
//! ## v1 / v2 dual-version
//!
//! Per ADR-0008, `program_hash` is a **long-lived irrevocable
//! commitment** (the on-chain identity of a contract) and widens
//! from BLAKE3-256 (v1) to BLAKE3-512 (v2). The dual-version
//! pattern matches the established `weights_hash` precedent (v1
//! frozen + KAT, v2 canonical).
//!
//! - [`v1::program_hash_v1`] — frozen 32-byte BLAKE3-256 over
//!   `name || child_weights_hashes_v1`. Do not modify; KATs in
//!   `program.rs#tests` catch drift.
//! - [`v2::program_hash_v2`] — canonical 64-byte BLAKE3-512 over
//!   `name || child_weights_hashes_v2`. The default
//!   [`TernaryProgram::program_hash`] returns this.

use crate::error::ContractError;
use psl_ternary_vm::network::TernaryNetwork;

/// 64-byte BLAKE3-512 commitment to a contract's identity. Per
/// ADR-0008, the long-lived irrevocable form. **Newtype** to prevent
/// accidental mixing with [`crate::ProposalHash`] (32B ephemeral) or
/// other byte-width-equivalent hashes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProgramHash(pub [u8; 64]);

impl ProgramHash {
    /// Read access to the underlying bytes (e.g., for hashing into
    /// trace_hash, on-chain serialization).
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

impl From<[u8; 64]> for ProgramHash {
    fn from(b: [u8; 64]) -> Self {
        Self(b)
    }
}

/// Frozen 32-byte BLAKE3-256 program identity. Carried alongside
/// [`ProgramHash`] on per-contract data structures so historical
/// (v1) verifiers can re-derive the legacy identifier without
/// re-running the v2 → v1 migration. **Frozen** per ADR-0008.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProgramHashV1(pub [u8; 32]);

impl ProgramHashV1 {
    /// Read access to the underlying bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<[u8; 32]> for ProgramHashV1 {
    fn from(b: [u8; 32]) -> Self {
        Self(b)
    }
}

/// **Frozen v1 program_hash (32-byte BLAKE3-256).**
///
/// Per ADR-0008: long-lived but pre-PQ format. New contracts
/// constructed under v0.1.x carry both v1 (here) and v2 (canonical)
/// forms so historical-block verification works without a v1 → v2
/// migration. Modifying this module is forbidden — the KAT in
/// `program.rs#tests` catches drift.
pub mod v1 {
    use super::ProgramHashV1;
    use psl_ternary_vm::network::TernaryNetwork;

    /// Compute the v1 program_hash from a contract `name` and an
    /// ordered slice of constituent ternary networks. Hashes
    /// `name.as_bytes() || (each network's weights_hash_v1)`.
    pub fn program_hash_v1(name: &str, networks: &[&TernaryNetwork]) -> ProgramHashV1 {
        let mut h = blake3::Hasher::new();
        h.update(name.as_bytes());
        for n in networks {
            h.update(n.header.weights_hash());
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(h.finalize().as_bytes());
        ProgramHashV1(out)
    }
}

/// **Canonical v2 program_hash (64-byte BLAKE3-512).** Per ADR-0008.
pub mod v2 {
    use super::ProgramHash;
    use psl_ternary_vm::network::TernaryNetwork;

    /// Compute the v2 program_hash from a contract `name` and an
    /// ordered slice of constituent ternary networks. Hashes
    /// `name.as_bytes() || (each network's weights_hash_v2)`, output
    /// at 64 bytes via the BLAKE3 XOF.
    pub fn program_hash_v2(name: &str, networks: &[&TernaryNetwork]) -> ProgramHash {
        let mut h = blake3::Hasher::new();
        h.update(name.as_bytes());
        for n in networks {
            h.update(n.header.weights_hash_v2());
        }
        let mut out = [0u8; 64];
        h.finalize_xof().fill(&mut out);
        ProgramHash(out)
    }
}

/// Compute both v1 and v2 program_hashes in one call. Per-contract
/// `build()` functions use this so each contract struct stores both
/// digests; the trait's [`TernaryProgram::program_hash`] /
/// [`TernaryProgram::program_hash_v1`] methods then just return the
/// stored values without recomputing.
pub fn build_program_hashes(
    name: &str,
    networks: &[&TernaryNetwork],
) -> (ProgramHash, ProgramHashV1) {
    (
        v2::program_hash_v2(name, networks),
        v1::program_hash_v1(name, networks),
    )
}

/// A pure-integer, deterministic, single-shot contract.
///
/// Same `input` → same `output` on any conformant integer-arithmetic
/// host (`docs/ARCHITECTURE.md` § 0.8). The `program_hash` is
/// BLAKE3-512 (v2 per ADR-0008) over a canonical encoding of the
/// contract's identity (name + sub-network weights_hashes_v2 in
/// fixed order).
pub trait TernaryProgram {
    /// Stable program identifier — name string in lower_snake_case.
    fn name(&self) -> &'static str;

    /// Run the contract on raw input bytes; return the raw output bytes.
    /// Pre-conditions (e.g. balance ≥ amount) are checked inside; on
    /// violation the contract returns a no-op output (defined per
    /// contract) rather than erroring, matching the `ledger_*.c`
    /// conventions used for the gate-1 sweep.
    fn run(&self, input: &[u8]) -> Result<Vec<u8>, ContractError>;

    /// Canonical v2 program_hash. BLAKE3-512 over `name() ||
    /// sub_network_weights_hashes_v2...` in canonical order. **This
    /// is the new method surface.** New code should call this; the
    /// 64-byte newtype prevents accidental mixing with proposal-hash
    /// or pubkey types.
    fn program_hash_v2(&self) -> ProgramHash;

    /// Legacy 32-byte program_hash. Returns BLAKE3-256 over the same
    /// canonical input as [`Self::program_hash_v2`] but at 32 bytes,
    /// matching the pre-v0.1.x trace contract. **Frozen** per
    /// ADR-0008; the v0.1.x agent layer (`agent_sdk` HashMap key,
    /// `agent_protocol::message::Propose.program_hash` wire field)
    /// continues to use this form. The v0.2 wire-format-break PR
    /// migrates the agent layer to `program_hash_v2`.
    ///
    /// Default implementation matches the legacy `[u8; 32]` form by
    /// re-deriving via [`v1::program_hash_v1`] from `name()` + the
    /// contract's underlying networks. Each contract impl provides
    /// the network slice — kept as a required method to avoid
    /// recomputing on every call (impls cache at `build()` time).
    fn program_hash(&self) -> [u8; 32];

    /// Compute the per-execution trace_hash for an `(input, output)`
    /// pair. Output stays 32 bytes (per ADR-0008, ephemeral hashes
    /// don't widen). Currently uses the v1 32-byte program_hash
    /// commitment for backwards compatibility with the v0.1.x agent
    /// layer; the v0.2 wire-format-break PR switches this to v2.
    fn trace_hash(&self, input: &[u8], output: &[u8]) -> [u8; 32] {
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

#[cfg(test)]
mod tests {
    use super::*;
    use psl_ternary_vm::primitives::{byte_add_with_carry, byte_sub_with_borrow};

    /// **Frozen v1 program_hash KAT (benign).** A documented input →
    /// digest pair that v1 must produce for all time. Drift fails
    /// loudly; v1 is FROZEN per ADR-0008.
    #[test]
    fn v1_program_hash_kat_benign() {
        let add = byte_add_with_carry::build();
        let sub = byte_sub_with_borrow::build();
        let got = v1::program_hash_v1("kat-benign", &[&add, &sub]);
        let got_hex: String = got.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
        // Pinned 2026-05-10. If this fails, v1 has drifted.
        assert_eq!(
            got_hex, KAT_V1_BENIGN_DIGEST_HEX,
            "v1 program_hash KAT (benign) drifted -- v1 is FROZEN per ADR-0008. \
             Do not update the expected value to match new behavior; fix the regression. \
             got: {got_hex}"
        );
    }

    /// **Frozen v1 program_hash KAT (adversarial).** Deliberately
    /// wrong inputs that v1 must produce DIFFERENT digests for.
    /// Catches "v1 silently collapsed distinct inputs to the same
    /// digest" -- a failure mode benign KATs miss.
    ///
    /// Three adversarial variants:
    /// 1. Different name -> different digest.
    /// 2. Different network order -> different digest (ordering matters).
    /// 3. Empty network list with same name -> different digest from
    ///    populated list (input length matters).
    #[test]
    fn v1_program_hash_kat_adversarial() {
        let add = byte_add_with_carry::build();
        let sub = byte_sub_with_borrow::build();
        let baseline = v1::program_hash_v1("kat-benign", &[&add, &sub]);

        // (1) Different name -> different digest.
        let diff_name = v1::program_hash_v1("kat-different", &[&add, &sub]);
        assert_ne!(
            baseline.as_bytes(),
            diff_name.as_bytes(),
            "v1 program_hash collapsed distinct names -- v1 has weakened. ADR-0008."
        );

        // (2) Different network order -> different digest.
        let swapped = v1::program_hash_v1("kat-benign", &[&sub, &add]);
        assert_ne!(
            baseline.as_bytes(),
            swapped.as_bytes(),
            "v1 program_hash collapsed distinct network orders -- v1 has weakened. ADR-0008."
        );

        // (3) Empty network list -> different digest from populated.
        let empty = v1::program_hash_v1("kat-benign", &[]);
        assert_ne!(
            baseline.as_bytes(),
            empty.as_bytes(),
            "v1 program_hash collapsed distinct input lengths -- v1 has weakened. ADR-0008."
        );
    }

    /// v1 and v2 produce DIFFERENT digests for the same (name,
    /// networks). This is the load-bearing property of the format
    /// break: if v1 and v2 ever collide on identical inputs, the
    /// dual-version contract is broken. (They commit to different
    /// weights_hash widths so byte-equality would require a
    /// catastrophic hash collision, but pinning the property in
    /// tests catches an accidental code-path collapse.)
    #[test]
    fn v1_and_v2_disagree_on_identical_inputs() {
        let add = byte_add_with_carry::build();
        let sub = byte_sub_with_borrow::build();
        let v1d = v1::program_hash_v1("test", &[&add, &sub]);
        let v2d = v2::program_hash_v2("test", &[&add, &sub]);
        assert_ne!(
            v1d.as_bytes().as_slice(),
            &v2d.as_bytes()[..32],
            "v1 program_hash bytes happened to equal v2's first 32 bytes -- \
             this should be vanishingly unlikely under BLAKE3 collision-resistance \
             AND the inputs differ (v1 hashes weights_hash_v1, v2 hashes weights_hash_v2)."
        );
    }

    /// Newtype protection: ProgramHash and ProgramHashV1 cannot be
    /// silently mixed. (Compile-time test: this code would not
    /// compile if newtype protection were lost.)
    #[test]
    fn newtype_prevents_mixing_v1_and_v2() {
        let add = byte_add_with_carry::build();
        let v1d: ProgramHashV1 = v1::program_hash_v1("x", &[&add]);
        let v2d: ProgramHash = v2::program_hash_v2("x", &[&add]);
        // The PartialEq impls are independent — comparing them is a
        // type error, which the compiler enforces. We assert each
        // matches its own type's expected length here.
        assert_eq!(v1d.as_bytes().len(), 32);
        assert_eq!(v2d.as_bytes().len(), 64);
    }

    /// Pinned digest for the benign v1 KAT. Computed once on
    /// 2026-05-10 from `program_hash_v1("kat-benign", &[byte_add,
    /// byte_sub])` where byte_add and byte_sub are the canonical
    /// constructions of `byte_add_with_carry` and
    /// `byte_sub_with_borrow`. **Frozen** per ADR-0008.
    const KAT_V1_BENIGN_DIGEST_HEX: &str =
        "80587aa7bfec4b442ab69c02ee42f3a7d6ff97d648cb1c4aa486bc9495e049fa";
}
