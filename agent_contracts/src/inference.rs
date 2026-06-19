//! `risk_gated_transfer` — the first **model-gated settlement** contract.
//!
//! ## What is novel here
//!
//! The eight contracts that shipped in v0.1.0 (`transfer`, `swap`,
//! `escrow_*`, `time_locked_release`, `multisig_2of3`,
//! `conditional_payment`) all reach their decision the same way: they
//! read a **trusted flag** out of the witness. `conditional_payment`
//! fires iff `input[56] == 1`; `multisig_2of3` counts three flag bytes;
//! `time_locked_release` compares a supplied timestamp. In every case
//! *someone off-chain* — an oracle, a signer, a clock — is trusted to
//! have set that flag correctly. The ternary VM is used only as an
//! adding machine (carry/borrow chains).
//!
//! `risk_gated_transfer` is different: the decision is **computed by a
//! ternary neural network from raw evidence**, on the verifier path,
//! with no trusted flag in between. The payment fires iff a small
//! integer-only multilayer perceptron — evaluated on a feature vector
//! carried in the witness — outputs `APPROVE`.
//!
//! This is what the rest of the stack makes possible but never used:
//! because `TernaryNetwork::forward` is bit-exact on every conformant
//! integer host (`docs/ARCHITECTURE.md` § 0.8), *a model's output is a
//! deterministic function of `(weights_hash, features)`*. So the model
//! can be the thing that both:
//!
//! 1. **gates the money** (APPROVE ⇒ transfer; DENY ⇒ canonical no-op), and
//! 2. **resolves the dispute** — when an executor lies about what the
//!    model decided, the judge re-runs the *same* network on the *same*
//!    features, gets the *same* bytes, and slashes the liar. No human
//!    underwriter, no oracle, no appeal to an off-chain score.
//!
//! The contract's `program_hash` commits to the model's `weights_hash`
//! (alongside the arithmetic sub-nets), so a given model **is** a given
//! contract identity. Changing a weight changes the `program_hash` —
//! model governance falls out of content-addressing for free.
//!
//! ## The embedded model (`credit_risk_model_v1`)
//!
//! A 4-layer, integer-only ternary MLP that adjudicates a micro-credit
//! / parametric-underwriting decision from four `u16` features:
//!
//! ```text
//!   f0 = income_score      f1 = debt_load
//!   f2 = collateral_score  f3 = risk_flags
//!
//!   creditworthy := (f0 + f2 − f1) ≥ MIN_NET_CREDIT        (= 500)
//!   within_risk  := f3 ≤ MAX_RISK_FLAGS                    (= 10)
//!   APPROVE      := creditworthy AND within_risk
//! ```
//!
//! The `AND` is a genuine non-linearity (`ReLU(b1 + b2 − 1)`); it cannot
//! be expressed as a single linear threshold, which is the point — this
//! is a real network, not arithmetic in disguise. Every weight ∈
//! {−1, 0, +1}, every bias and activation is `i64`, ReLU is the only
//! non-linearity. See `build_credit_model` for the layer-by-layer
//! construction and the unit tests for the bit-exact verification
//! against a plain-Rust ground truth.
//!
//! ## Wire format
//!
//! ```text
//! Input  (64 B): from(16) ‖ to(16) ‖ amount(16) ‖ nonce(8)
//!                ‖ income(2) ‖ debt(2) ‖ collateral(2) ‖ risk_flags(2)
//!                                      (each feature: u16, little-endian)
//! Output (40 B): new_from(16) ‖ new_to(16) ‖ new_nonce(8)
//!                — or 40 zero bytes on DENY / insufficient balance / overflow.
//! ```

use crate::error::ContractError;
use crate::guarded::{no_op_output, wrapped_transfer};
use crate::program::{build_program_hashes, ProgramHash, TernaryProgram};
use psl_ternary_vm::network::{SparseTernaryLayer, TernaryNetwork};
use psl_ternary_vm::primitives::{byte_add_with_carry, byte_sub_with_borrow};
use psl_ternary_vm::weights::{pack_weights_dual, WeightsHeader};

/// Stable identifier of the embedded decision model. The contract's
/// `program_hash` commits to this network's `weights_hash`; a different
/// model is a different contract.
pub const MODEL_NAME: &str = "credit_risk_model_v1";

/// Minimum net credit `(income + collateral − debt)` required for the
/// `creditworthy` sub-decision to hold.
pub const MIN_NET_CREDIT: i64 = 500;

/// Maximum `risk_flags` allowed for the `within_risk` sub-decision to
/// hold. Above this the model denies regardless of creditworthiness.
pub const MAX_RISK_FLAGS: i64 = 10;

/// Exact input length: 56 financial bytes + 4 × u16 features.
pub const INPUT_LEN: usize = 56 + 8;

/// Canonical output length (matches every guarded-transfer contract).
pub const OUTPUT_LEN: usize = 40;

/// Build `credit_risk_model_v1`: a deterministic, integer-only ternary
/// MLP computing `APPROVE = (f0 + f2 − f1 ≥ MIN_NET_CREDIT) AND
/// (f3 ≤ MAX_RISK_FLAGS)`. Calling this twice produces bit-identical
/// weights (and therefore an identical `weights_hash`).
///
/// Layer plan (input = `[income, debt, collateral, risk_flags]` as i64):
///
/// ```text
/// L1 (4→2, linear): n0 = f0 − f1 + f2          (net-credit score)
///                   n1 = MAX_RISK_FLAGS − f3   (risk headroom)
/// L2 (2→4, ReLU):   parallel ReLU step-extractors for n0 (vs MIN_NET_CREDIT)
///                   and n1 (vs 0)
/// L3 (2 from each pair, linear): b1 = 1 iff n0 ≥ MIN_NET_CREDIT
///                                b2 = 1 iff n1 ≥ 0  (i.e. f3 ≤ MAX_RISK_FLAGS)
/// L4 (2→1, ReLU):   decision = ReLU(b1 + b2 − 1)   (the AND gate)
/// ```
///
/// The step-extractor `ReLU(s − t + 1) − ReLU(s − t)` evaluates to `1`
/// iff the integer `s ≥ t`, else `0` — the same thermometer trick the
/// `byte_add_with_carry` primitive uses for its decode.
pub fn build_credit_model() -> TernaryNetwork {
    // L1: 4 → 2, linear. n0 = f0 − f1 + f2 ; n1 = −f3 + MAX_RISK_FLAGS.
    let layer1 = SparseTernaryLayer {
        input_dim: 4,
        output_dim: 2,
        pos_indices: vec![vec![0, 2], vec![]],
        neg_indices: vec![vec![1], vec![3]],
        bias: vec![0, MAX_RISK_FLAGS],
        relu: false,
    };

    // L2: 2 → 4, ReLU. Parallel step-extractors:
    //   out0 = ReLU(n0 − MIN_NET_CREDIT + 1)
    //   out1 = ReLU(n0 − MIN_NET_CREDIT)
    //   out2 = ReLU(n1 + 1)
    //   out3 = ReLU(n1)
    let layer2 = SparseTernaryLayer {
        input_dim: 2,
        output_dim: 4,
        pos_indices: vec![vec![0], vec![0], vec![1], vec![1]],
        neg_indices: vec![vec![], vec![], vec![], vec![]],
        bias: vec![1 - MIN_NET_CREDIT, -MIN_NET_CREDIT, 1, 0],
        relu: true,
    };

    // L3: 4 → 2, linear. b1 = out0 − out1 ; b2 = out2 − out3.
    let layer3 = SparseTernaryLayer {
        input_dim: 4,
        output_dim: 2,
        pos_indices: vec![vec![0], vec![2]],
        neg_indices: vec![vec![1], vec![3]],
        bias: vec![0, 0],
        relu: false,
    };

    // L4: 2 → 1, ReLU. decision = ReLU(b1 + b2 − 1) — the AND gate.
    let layer4 = SparseTernaryLayer {
        input_dim: 2,
        output_dim: 1,
        pos_indices: vec![vec![0, 1]],
        neg_indices: vec![vec![]],
        bias: vec![-1],
        relu: true,
    };

    let layers = vec![layer1, layer2, layer3, layer4];
    let (_, digest_v1, digest_v2) = pack_weights_dual(MODEL_NAME, 4, 1, &layers);
    let header = WeightsHeader::new(1, MODEL_NAME, 4, 1, digest_v1, digest_v2);
    TernaryNetwork::new(header, layers)
}

/// `risk_gated_transfer` — a transfer gated by the deterministic
/// `credit_risk_model_v1` ternary network rather than by a trusted
/// flag. The decision is part of the verifier path, so it is
/// re-executable and dispute-resolvable like any other contract output.
pub struct RiskGatedTransfer {
    /// The decision model. Its `weights_hash` is committed by
    /// `program_hash`, so this exact model is part of the contract
    /// identity.
    pub risk_model: TernaryNetwork,
    pub byte_add: TernaryNetwork,
    pub byte_sub: TernaryNetwork,
    pub program_hash: [u8; 32],
    pub program_hash_v2: ProgramHash,
}

impl RiskGatedTransfer {
    pub fn build() -> Self {
        let risk_model = build_credit_model();
        let byte_add = byte_add_with_carry::build();
        let byte_sub = byte_sub_with_borrow::build();
        // Identity commits to the model AND the arithmetic sub-nets, in
        // canonical order. Changing the model changes the program_hash.
        let (program_hash_v2, program_hash_v1) =
            build_program_hashes("risk_gated_transfer", &[&risk_model, &byte_add, &byte_sub]);
        Self {
            risk_model,
            byte_add,
            byte_sub,
            program_hash: program_hash_v1.0,
            program_hash_v2,
        }
    }

    /// Evaluate just the embedded model on a feature vector. Returns
    /// `true` for APPROVE. Useful for callers that want to know the
    /// decision before constructing a witness (and for tests).
    pub fn model_decision(&self, features: [u16; 4]) -> Result<bool, ContractError> {
        let f: [i64; 4] = [
            features[0] as i64,
            features[1] as i64,
            features[2] as i64,
            features[3] as i64,
        ];
        let out = self.risk_model.forward(&f)?;
        Ok(out.first().copied().unwrap_or(0) == 1)
    }
}

/// Pack a `risk_gated_transfer` witness from typed fields. Feature
/// order matches the model: `[income, debt, collateral, risk_flags]`.
pub fn pack_input(from: u128, to: u128, amount: u128, nonce: u64, features: [u16; 4]) -> Vec<u8> {
    let mut v = Vec::with_capacity(INPUT_LEN);
    v.extend_from_slice(&from.to_le_bytes());
    v.extend_from_slice(&to.to_le_bytes());
    v.extend_from_slice(&amount.to_le_bytes());
    v.extend_from_slice(&nonce.to_le_bytes());
    for f in features {
        v.extend_from_slice(&f.to_le_bytes());
    }
    v
}

impl TernaryProgram for RiskGatedTransfer {
    fn name(&self) -> &'static str {
        "risk_gated_transfer"
    }
    fn program_hash(&self) -> [u8; 32] {
        self.program_hash
    }
    fn program_hash_v2(&self) -> ProgramHash {
        self.program_hash_v2
    }
    fn run(&self, input: &[u8]) -> Result<Vec<u8>, ContractError> {
        if input.len() != INPUT_LEN {
            return Err(ContractError::InputShape {
                contract: "risk_gated_transfer",
                got: input.len(),
                expected: INPUT_LEN,
            });
        }

        // Decode the four u16 features and run the decision model on the
        // verifier path. This is the line that makes the contract novel:
        // the gate is a neural-network forward pass, not a trusted flag.
        let income = u16::from_le_bytes([input[56], input[57]]) as i64;
        let debt = u16::from_le_bytes([input[58], input[59]]) as i64;
        let collateral = u16::from_le_bytes([input[60], input[61]]) as i64;
        let risk_flags = u16::from_le_bytes([input[62], input[63]]) as i64;

        let decision = self
            .risk_model
            .forward(&[income, debt, collateral, risk_flags])?;
        if decision.first().copied().unwrap_or(0) != 1 {
            // Model DENY → canonical no-op, exactly like a failed guard
            // in `conditional_payment`.
            return Ok(no_op_output(OUTPUT_LEN));
        }

        // Model APPROVE → execute the underlying transfer (which still
        // enforces balance/overflow preconditions of its own).
        let mut from = [0u8; 16];
        let mut to = [0u8; 16];
        let mut amount = [0u8; 16];
        let mut nonce = [0u8; 8];
        from.copy_from_slice(&input[0..16]);
        to.copy_from_slice(&input[16..32]);
        amount.copy_from_slice(&input[32..48]);
        nonce.copy_from_slice(&input[48..56]);
        wrapped_transfer(&self.byte_add, &self.byte_sub, from, to, amount, nonce)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unpack(out: &[u8]) -> (u128, u128, u64) {
        let mut a = [0u8; 16];
        let mut b = [0u8; 16];
        let mut n = [0u8; 8];
        a.copy_from_slice(&out[0..16]);
        b.copy_from_slice(&out[16..32]);
        n.copy_from_slice(&out[32..40]);
        (
            u128::from_le_bytes(a),
            u128::from_le_bytes(b),
            u64::from_le_bytes(n),
        )
    }

    /// Plain-Rust ground truth for the embedded model. The ternary
    /// network must agree with this on every input.
    fn ground_truth_approve(f: [u16; 4]) -> bool {
        let net_credit = f[0] as i64 + f[2] as i64 - f[1] as i64;
        let creditworthy = net_credit >= MIN_NET_CREDIT;
        let within_risk = (f[3] as i64) <= MAX_RISK_FLAGS;
        creditworthy && within_risk
    }

    #[test]
    fn model_weights_hash_is_deterministic_and_nonzero() {
        let a = build_credit_model();
        let b = build_credit_model();
        assert_eq!(a.header.weights_hash(), b.header.weights_hash());
        assert_ne!(a.header.weights_hash(), &[0u8; 32]);
    }

    #[test]
    fn program_hash_commits_to_the_model() {
        // Two contracts built the same way must share an identity…
        let c1 = RiskGatedTransfer::build();
        let c2 = RiskGatedTransfer::build();
        assert_eq!(c1.program_hash_v2(), c2.program_hash_v2());
        // …and that identity must differ from a plain transfer (the
        // model's weights_hash is folded into program_hash).
        let transfer = crate::TransferContract::build();
        assert_ne!(
            c1.program_hash_v2().as_bytes().as_slice(),
            transfer.program_hash_v2().as_bytes().as_slice()
        );
    }

    #[test]
    fn model_matches_ground_truth_on_grid() {
        let c = RiskGatedTransfer::build();
        // Sweep a grid that straddles both decision boundaries.
        for &income in &[0u16, 300, 500, 900, 2000] {
            for &debt in &[0u16, 200, 400, 1000] {
                for &collateral in &[0u16, 100, 300, 1500] {
                    for &risk in &[0u16, 9, 10, 11, 50] {
                        let f = [income, debt, collateral, risk];
                        let got = c.model_decision(f).unwrap();
                        assert_eq!(
                            got,
                            ground_truth_approve(f),
                            "model disagreed with ground truth at {f:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn approves_creditworthy_low_risk_applicant() {
        let c = RiskGatedTransfer::build();
        // net credit = 900 + 200 − 300 = 800 ≥ 500 ; risk 5 ≤ 10 → APPROVE
        let input = pack_input(1000, 500, 100, 5, [900, 300, 200, 5]);
        let out = c.run(&input).unwrap();
        assert_eq!(unpack(&out), (900, 600, 6));
    }

    #[test]
    fn denies_creditworthy_but_overrisk_applicant() {
        let c = RiskGatedTransfer::build();
        // Money side is fine, but risk_flags = 40 > 10 → model DENY → no-op.
        let input = pack_input(1000, 500, 100, 5, [900, 300, 200, 40]);
        let out = c.run(&input).unwrap();
        assert_eq!(out, vec![0u8; OUTPUT_LEN]);
    }

    #[test]
    fn denies_low_credit_applicant() {
        let c = RiskGatedTransfer::build();
        // net credit = 100 + 50 − 300 = −150 < 500 → DENY even at low risk.
        let input = pack_input(1000, 500, 100, 5, [100, 300, 50, 0]);
        let out = c.run(&input).unwrap();
        assert_eq!(out, vec![0u8; OUTPUT_LEN]);
    }

    #[test]
    fn approved_but_insufficient_balance_is_noop() {
        let c = RiskGatedTransfer::build();
        // Model APPROVEs, but from_balance (10) < amount (100) → inner
        // transfer guard yields the canonical no-op.
        let input = pack_input(10, 500, 100, 5, [900, 300, 200, 5]);
        let out = c.run(&input).unwrap();
        assert_eq!(out, vec![0u8; OUTPUT_LEN]);
    }

    #[test]
    fn decision_boundary_is_exact() {
        let c = RiskGatedTransfer::build();
        // net credit exactly == MIN_NET_CREDIT (500) → APPROVE.
        let at = pack_input(1000, 0, 100, 0, [500, 0, 0, 0]);
        assert_eq!(unpack(&c.run(&at).unwrap()), (900, 100, 1));
        // one below → DENY.
        let below = pack_input(1000, 0, 100, 0, [499, 0, 0, 0]);
        assert_eq!(c.run(&below).unwrap(), vec![0u8; OUTPUT_LEN]);
        // risk exactly == MAX_RISK_FLAGS (10) → still within risk.
        let risk_ok = pack_input(1000, 0, 100, 0, [500, 0, 0, 10]);
        assert_eq!(unpack(&c.run(&risk_ok).unwrap()), (900, 100, 1));
        // one over → DENY.
        let risk_bad = pack_input(1000, 0, 100, 0, [500, 0, 0, 11]);
        assert_eq!(c.run(&risk_bad).unwrap(), vec![0u8; OUTPUT_LEN]);
    }

    #[test]
    fn re_execution_is_bit_exact() {
        // The property the dispute resolver relies on: same input →
        // identical output bytes, every time.
        let c = RiskGatedTransfer::build();
        let input = pack_input(1000, 500, 100, 5, [900, 300, 200, 5]);
        let first = c.run(&input).unwrap();
        for _ in 0..16 {
            assert_eq!(c.run(&input).unwrap(), first);
        }
    }

    #[test]
    fn wrong_input_length_errors() {
        let c = RiskGatedTransfer::build();
        let got = c.run(&[0u8; 63]);
        assert!(matches!(got, Err(ContractError::InputShape { .. })));
    }
}
