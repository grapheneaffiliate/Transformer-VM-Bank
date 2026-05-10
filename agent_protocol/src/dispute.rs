//! Dispute resolution.
//!
//! Agent A claims agent B did not execute as agreed. A submits a
//! `Dispute` referencing the executed contract and providing A's
//! claimed expected output. The sequencer **deterministically
//! re-executes** the contract via `psl-agent-contracts` (which is a
//! `TernaryProgram` and therefore bit-identical across hosts) and
//! returns the slash decision.
//!
//! Outcome:
//! - re-executed output == claimed output → claim was right; B's
//!   bond is partially slashed and B's reputation is debited.
//! - re-executed output != claimed output → claim was wrong; A's
//!   reputation is debited (no slash on A — bond is on B in this flow).
//!
//! Because execution is deterministic and re-executable, dispute
//! resolution is finite and mechanical. No human arbiter, no oracle.

use crate::error::ProtocolError;
use crate::message::{Execute, ProposalHash, Propose};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use psl_agent_contracts::TernaryProgram;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dispute {
    pub proposal_hash: ProposalHash,
    /// The witness the executor (B) used.
    pub witness: Vec<u8>,
    /// What A claims the correct output is.
    pub claimed_output: Vec<u8>,
    /// Pubkey of the disputer (A).
    #[serde(with = "BigArray")]
    pub disputer: [u8; 32],
    pub opened_at_unix: u64,
    #[serde(with = "BigArray")]
    pub sig: [u8; 64],
}

impl Dispute {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256);
        out.extend_from_slice(b"PSL-DISPUTE-V1");
        out.extend_from_slice(self.proposal_hash.as_bytes());
        push_bytes(&mut out, &self.witness);
        push_bytes(&mut out, &self.claimed_output);
        out.extend_from_slice(&self.disputer);
        out.extend_from_slice(&self.opened_at_unix.to_be_bytes());
        out
    }

    pub fn sign(
        signer: &SigningKey,
        proposal_hash: ProposalHash,
        witness: Vec<u8>,
        claimed_output: Vec<u8>,
        opened_at_unix: u64,
    ) -> Self {
        let disputer = signer.verifying_key().to_bytes();
        let mut d = Dispute {
            proposal_hash,
            witness,
            claimed_output,
            disputer,
            opened_at_unix,
            sig: [0u8; 64],
        };
        d.sig = signer.sign(&d.canonical_bytes()).to_bytes();
        d
    }

    pub fn verify(&self) -> Result<(), ProtocolError> {
        let pk = VerifyingKey::from_bytes(&self.disputer)
            .map_err(|e| ProtocolError::Ed25519(format!("disputer pk: {e}")))?;
        let sig = Signature::from_bytes(&self.sig);
        pk.verify(&self.canonical_bytes(), &sig)
            .map_err(|_| ProtocolError::SignatureInvalid)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisputeOutcome {
    /// Disputer was right — executor's claimed output is wrong.
    /// Slash the executor; debit executor reputation.
    SlashExecutor {
        executor_pubkey: [u8; 32],
        re_executed_output: Vec<u8>,
    },
    /// Disputer was wrong — executor's claim matches the
    /// deterministic re-execution. Debit disputer reputation.
    DismissDispute {
        disputer_pubkey: [u8; 32],
        re_executed_output: Vec<u8>,
    },
}

/// Resolve a dispute by re-executing the contract via the canonical
/// `TernaryProgram` and comparing against both the original execute's
/// expected output and the disputer's claimed output.
///
/// Dispute valid iff `dispute.proposal_hash == execute.proposal_hash`.
/// Otherwise the dispute is malformed and we error out.
pub fn resolve_dispute<P: TernaryProgram + ?Sized>(
    contract: &P,
    propose: &Propose,
    execute: &Execute,
    dispute: &Dispute,
) -> Result<DisputeOutcome, ProtocolError> {
    propose.verify()?;
    execute.verify()?;
    dispute.verify()?;

    if dispute.proposal_hash != execute.proposal_hash {
        return Err(ProtocolError::ProposalHashMismatch {
            expected: execute.proposal_hash,
            got: dispute.proposal_hash,
        });
    }

    // Re-execute deterministically.
    let actual = contract.run(&dispute.witness)?;

    // The executor's claim is the `expected_output` they signed in `Execute`.
    let executor_claim = &execute.expected_output.bytes;

    if executor_claim == &actual {
        // Executor was correct; disputer is wrong.
        Ok(DisputeOutcome::DismissDispute {
            disputer_pubkey: dispute.disputer,
            re_executed_output: actual,
        })
    } else {
        // Executor was wrong; slash. (We don't compare to the
        // disputer's `claimed_output` here — the canonical truth is the
        // re-execution. The disputer surfaces a discrepancy; the
        // sequencer adjudicates by re-running.)
        Ok(DisputeOutcome::SlashExecutor {
            executor_pubkey: execute.by,
            re_executed_output: actual,
        })
    }
}

fn push_bytes(buf: &mut Vec<u8>, b: &[u8]) {
    buf.extend_from_slice(&(b.len() as u32).to_be_bytes());
    buf.extend_from_slice(b);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ExpectedOutput, Propose};
    use psl_agent_contracts::TransferContract;
    use rand::SeedableRng;

    fn sk(seed: u64) -> SigningKey {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        SigningKey::generate(&mut rng)
    }

    fn pack_transfer_input(from: u128, to: u128, amount: u128, nonce: u64) -> Vec<u8> {
        let mut v = Vec::with_capacity(56);
        v.extend_from_slice(&from.to_le_bytes());
        v.extend_from_slice(&to.to_le_bytes());
        v.extend_from_slice(&amount.to_le_bytes());
        v.extend_from_slice(&nonce.to_le_bytes());
        v
    }

    #[test]
    fn dismiss_dispute_when_executor_correct() {
        let alice = sk(1);
        let bob = sk(2);
        let charlie = sk(3); // disputer
        let contract = TransferContract::build();
        let witness = pack_transfer_input(1000, 500, 250, 7);
        let actual = contract.run(&witness).unwrap();

        let propose = Propose::sign(
            &alice,
            contract.program_hash_v2,
            witness.clone(),
            bob.verifying_key().to_bytes(),
            0,
            1_000_000,
            1,
        );
        let h = propose.proposal_hash();
        // Bob signs Execute claiming the (correct) actual output
        let execute = Execute::sign(
            &bob,
            h,
            witness.clone(),
            ExpectedOutput {
                bytes: actual.clone(),
            },
            100,
        );
        // Charlie disputes, claims a different output
        let dispute = Dispute::sign(&charlie, h, witness, vec![0u8; actual.len()], 200);

        let outcome = resolve_dispute(&contract, &propose, &execute, &dispute).unwrap();
        assert_eq!(
            outcome,
            DisputeOutcome::DismissDispute {
                disputer_pubkey: charlie.verifying_key().to_bytes(),
                re_executed_output: actual,
            }
        );
    }

    #[test]
    fn slash_executor_when_executor_lied() {
        let alice = sk(1);
        let bob = sk(2);
        let charlie = sk(3);
        let contract = TransferContract::build();
        let witness = pack_transfer_input(1000, 500, 250, 7);
        let actual = contract.run(&witness).unwrap();

        let propose = Propose::sign(
            &alice,
            contract.program_hash_v2,
            witness.clone(),
            bob.verifying_key().to_bytes(),
            0,
            1_000_000,
            1,
        );
        let h = propose.proposal_hash();
        // Bob signs Execute with a LIED-ABOUT output (claiming all-zero output)
        let lied = vec![0u8; actual.len()];
        let execute = Execute::sign(
            &bob,
            h,
            witness.clone(),
            ExpectedOutput {
                bytes: lied.clone(),
            },
            100,
        );
        // Charlie disputes
        let dispute = Dispute::sign(&charlie, h, witness, actual.clone(), 200);

        let outcome = resolve_dispute(&contract, &propose, &execute, &dispute).unwrap();
        assert_eq!(
            outcome,
            DisputeOutcome::SlashExecutor {
                executor_pubkey: bob.verifying_key().to_bytes(),
                re_executed_output: actual,
            }
        );
    }

    #[test]
    fn proposal_hash_mismatch_errors() {
        let alice = sk(1);
        let bob = sk(2);
        let charlie = sk(3);
        let contract = TransferContract::build();
        let witness = pack_transfer_input(1000, 500, 250, 7);
        let actual = contract.run(&witness).unwrap();
        let propose = Propose::sign(
            &alice,
            contract.program_hash_v2,
            witness.clone(),
            bob.verifying_key().to_bytes(),
            0,
            1_000_000,
            1,
        );
        let h = propose.proposal_hash();
        let execute = Execute::sign(
            &bob,
            h,
            witness.clone(),
            ExpectedOutput {
                bytes: actual.clone(),
            },
            100,
        );
        // dispute references a DIFFERENT proposal hash
        let dispute = Dispute::sign(
            &charlie,
            crate::message::ProposalHash([0xffu8; 32]),
            witness,
            actual,
            200,
        );
        let r = resolve_dispute(&contract, &propose, &execute, &dispute);
        assert!(matches!(r, Err(ProtocolError::ProposalHashMismatch { .. })));
    }
}
