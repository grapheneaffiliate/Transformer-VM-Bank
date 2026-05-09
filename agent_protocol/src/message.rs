//! The 5 wire message types of the negotiation protocol.
//!
//! All messages are signed by the sender's pubkey. The
//! `proposal_hash` is a content-addressed identifier for the original
//! proposal — every message in a proposal's lifecycle references the
//! same hash, which lets the state machine absorb out-of-order
//! delivery and dedupe replays.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::error::ProtocolError;

pub type ProposalHash = [u8; 32];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Propose {
    /// Hash of the contract weights this proposal is for. Anchors the
    /// proposal to a specific deterministic contract.
    pub program_hash: [u8; 32],
    /// Caller-side parameters as opaque bytes. The contract decoder
    /// determines the layout.
    pub parameters: Vec<u8>,
    /// Pubkey of the proposer.
    #[serde(with = "BigArray")]
    pub from: [u8; 32],
    /// Pubkey of the agent the proposer wants to transact with.
    #[serde(with = "BigArray")]
    pub to: [u8; 32],
    /// Earliest unix timestamp the receiver may execute on.
    pub valid_from_unix: u64,
    /// Latest unix timestamp the receiver may accept by.
    pub valid_until_unix: u64,
    /// Strictly increasing per (from, to) so retransmissions of the
    /// SAME proposal hash collapse but a re-issued proposal with the
    /// same parameters but a new nonce produces a different hash.
    pub nonce: u64,
    #[serde(with = "BigArray")]
    pub sig: [u8; 64],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Accept {
    pub proposal_hash: ProposalHash,
    #[serde(with = "BigArray")]
    pub by: [u8; 32],
    pub accepted_at_unix: u64,
    #[serde(with = "BigArray")]
    pub sig: [u8; 64],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reject {
    pub proposal_hash: ProposalHash,
    #[serde(with = "BigArray")]
    pub by: [u8; 32],
    pub reason: String,
    pub rejected_at_unix: u64,
    #[serde(with = "BigArray")]
    pub sig: [u8; 64],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CounterPropose {
    pub original_proposal_hash: ProposalHash,
    pub new_parameters: Vec<u8>,
    #[serde(with = "BigArray")]
    pub by: [u8; 32],
    pub nonce: u64,
    #[serde(with = "BigArray")]
    pub sig: [u8; 64],
}

/// Caller's expected output for the executed contract. Carried so the
/// dispute path can compare it against the deterministic re-execution.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExpectedOutput {
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Execute {
    pub proposal_hash: ProposalHash,
    pub witness: Vec<u8>,
    pub expected_output: ExpectedOutput,
    #[serde(with = "BigArray")]
    pub by: [u8; 32],
    pub executed_at_unix: u64,
    #[serde(with = "BigArray")]
    pub sig: [u8; 64],
}

/// One discriminated wire envelope so a transport can multiplex.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ProtocolMessage {
    Propose(Propose),
    Accept(Accept),
    Reject(Reject),
    CounterPropose(CounterPropose),
    Execute(Execute),
}

// ── canonical bytes + signing helpers ─────────────────────────────────

impl Propose {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256);
        out.extend_from_slice(b"PSL-PROPOSE-V1");
        out.extend_from_slice(&self.program_hash);
        push_bytes(&mut out, &self.parameters);
        out.extend_from_slice(&self.from);
        out.extend_from_slice(&self.to);
        out.extend_from_slice(&self.valid_from_unix.to_be_bytes());
        out.extend_from_slice(&self.valid_until_unix.to_be_bytes());
        out.extend_from_slice(&self.nonce.to_be_bytes());
        out
    }

    pub fn proposal_hash(&self) -> ProposalHash {
        let mut h = blake3::Hasher::new();
        h.update(&self.canonical_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(h.finalize().as_bytes());
        out
    }

    pub fn sign(
        signer: &SigningKey,
        program_hash: [u8; 32],
        parameters: Vec<u8>,
        to: [u8; 32],
        valid_from_unix: u64,
        valid_until_unix: u64,
        nonce: u64,
    ) -> Self {
        let from = signer.verifying_key().to_bytes();
        let mut p = Propose {
            program_hash,
            parameters,
            from,
            to,
            valid_from_unix,
            valid_until_unix,
            nonce,
            sig: [0u8; 64],
        };
        let sig = signer.sign(&p.canonical_bytes());
        p.sig = sig.to_bytes();
        p
    }

    pub fn verify(&self) -> Result<(), ProtocolError> {
        verify_sig(&self.from, &self.canonical_bytes(), &self.sig)
    }
}

// We avoid macro complexity by hand-coding sign/verify for each
// message type — the trait surface stays tiny and explicit.

impl Accept {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(96);
        out.extend_from_slice(b"PSL-ACCEPT-V1");
        out.extend_from_slice(&self.proposal_hash);
        out.extend_from_slice(&self.by);
        out.extend_from_slice(&self.accepted_at_unix.to_be_bytes());
        out
    }
    pub fn sign(signer: &SigningKey, proposal_hash: ProposalHash, accepted_at_unix: u64) -> Self {
        let by = signer.verifying_key().to_bytes();
        let mut m = Accept { proposal_hash, by, accepted_at_unix, sig: [0u8; 64] };
        m.sig = signer.sign(&m.canonical_bytes()).to_bytes();
        m
    }
    pub fn verify(&self) -> Result<(), ProtocolError> {
        verify_sig(&self.by, &self.canonical_bytes(), &self.sig)
    }
}

impl Reject {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128);
        out.extend_from_slice(b"PSL-REJECT-V1");
        out.extend_from_slice(&self.proposal_hash);
        out.extend_from_slice(&self.by);
        push_str(&mut out, &self.reason);
        out.extend_from_slice(&self.rejected_at_unix.to_be_bytes());
        out
    }
    pub fn sign(
        signer: &SigningKey,
        proposal_hash: ProposalHash,
        reason: String,
        rejected_at_unix: u64,
    ) -> Self {
        let by = signer.verifying_key().to_bytes();
        let mut m = Reject { proposal_hash, by, reason, rejected_at_unix, sig: [0u8; 64] };
        m.sig = signer.sign(&m.canonical_bytes()).to_bytes();
        m
    }
    pub fn verify(&self) -> Result<(), ProtocolError> {
        verify_sig(&self.by, &self.canonical_bytes(), &self.sig)
    }
}

impl CounterPropose {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128);
        out.extend_from_slice(b"PSL-COUNTER-V1");
        out.extend_from_slice(&self.original_proposal_hash);
        push_bytes(&mut out, &self.new_parameters);
        out.extend_from_slice(&self.by);
        out.extend_from_slice(&self.nonce.to_be_bytes());
        out
    }
    pub fn sign(
        signer: &SigningKey,
        original_proposal_hash: ProposalHash,
        new_parameters: Vec<u8>,
        nonce: u64,
    ) -> Self {
        let by = signer.verifying_key().to_bytes();
        let mut m = CounterPropose {
            original_proposal_hash,
            new_parameters,
            by,
            nonce,
            sig: [0u8; 64],
        };
        m.sig = signer.sign(&m.canonical_bytes()).to_bytes();
        m
    }
    pub fn verify(&self) -> Result<(), ProtocolError> {
        verify_sig(&self.by, &self.canonical_bytes(), &self.sig)
    }
}

impl Execute {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256);
        out.extend_from_slice(b"PSL-EXECUTE-V1");
        out.extend_from_slice(&self.proposal_hash);
        push_bytes(&mut out, &self.witness);
        push_bytes(&mut out, &self.expected_output.bytes);
        out.extend_from_slice(&self.by);
        out.extend_from_slice(&self.executed_at_unix.to_be_bytes());
        out
    }
    pub fn sign(
        signer: &SigningKey,
        proposal_hash: ProposalHash,
        witness: Vec<u8>,
        expected_output: ExpectedOutput,
        executed_at_unix: u64,
    ) -> Self {
        let by = signer.verifying_key().to_bytes();
        let mut m = Execute {
            proposal_hash,
            witness,
            expected_output,
            by,
            executed_at_unix,
            sig: [0u8; 64],
        };
        m.sig = signer.sign(&m.canonical_bytes()).to_bytes();
        m
    }
    pub fn verify(&self) -> Result<(), ProtocolError> {
        verify_sig(&self.by, &self.canonical_bytes(), &self.sig)
    }
}

// ── helpers ──────────────────────────────────────────────────────────

fn verify_sig(pk: &[u8; 32], body: &[u8], sig: &[u8; 64]) -> Result<(), ProtocolError> {
    let pk = VerifyingKey::from_bytes(pk).map_err(|e| ProtocolError::Ed25519(format!("pk: {e}")))?;
    let s = Signature::from_bytes(sig);
    pk.verify(body, &s).map_err(|_| ProtocolError::SignatureInvalid)
}

fn push_str(buf: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    buf.extend_from_slice(&(b.len() as u32).to_be_bytes());
    buf.extend_from_slice(b);
}
fn push_bytes(buf: &mut Vec<u8>, b: &[u8]) {
    buf.extend_from_slice(&(b.len() as u32).to_be_bytes());
    buf.extend_from_slice(b);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn sk(seed: u64) -> SigningKey {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        SigningKey::generate(&mut rng)
    }

    #[test]
    fn propose_signs_and_verifies() {
        let alice = sk(1);
        let bob = sk(2);
        let p = Propose::sign(
            &alice,
            [0xa1u8; 32],
            vec![1, 2, 3],
            bob.verifying_key().to_bytes(),
            100,
            200,
            7,
        );
        p.verify().unwrap();
        // proposal_hash stable
        let h1 = p.proposal_hash();
        let h2 = p.proposal_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn tampered_propose_fails() {
        let alice = sk(1);
        let bob = sk(2);
        let mut p = Propose::sign(
            &alice,
            [0xa1u8; 32],
            vec![1, 2, 3],
            bob.verifying_key().to_bytes(),
            100,
            200,
            7,
        );
        p.parameters[0] = 99;
        assert!(matches!(p.verify(), Err(ProtocolError::SignatureInvalid)));
    }

    #[test]
    fn accept_reject_counter_execute_sign_and_verify() {
        let alice = sk(1);
        let h: ProposalHash = [0x55u8; 32];
        let a = Accept::sign(&alice, h, 1000);
        let r = Reject::sign(&alice, h, "not interested".into(), 1000);
        let c = CounterPropose::sign(&alice, h, vec![9, 8, 7], 11);
        let e = Execute::sign(&alice, h, vec![1, 2], ExpectedOutput { bytes: vec![3, 4] }, 1100);
        a.verify().unwrap();
        r.verify().unwrap();
        c.verify().unwrap();
        e.verify().unwrap();
    }
}
