//! Agent registry record. Each agent publishes its pubkey, endpoint
//! URL, supported contract names + custom contract weight hashes,
//! optional human-readable metadata, and the bond it has staked.
//!
//! Registration is signed by the agent's pubkey over the canonical
//! serialization of the record (excluding the signature itself).

use crate::error::ProtocolError;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRegistration {
    #[serde(with = "BigArray")]
    pub pubkey: [u8; 32],
    pub endpoint: String,
    /// Names of contracts from the standard library this agent supports.
    pub supported_contracts: Vec<String>,
    /// 32-byte BLAKE3 hashes of any custom contract weight payloads
    /// this agent will execute (in addition to the standard contracts).
    pub custom_program_hashes: Vec<[u8; 32]>,
    pub display_name: String,
    pub fee_schedule: String,
    /// Bond staked at registration in u128 amount-units. Sequencer-
    /// enforced; the field is here so it travels with the record.
    pub bond_amount: u128,
    /// Monotonic registration version (rotation, endpoint update, etc.).
    pub version: u64,
    #[serde(with = "BigArray")]
    pub sig: [u8; 64],
}

impl AgentRegistration {
    // The argument list mirrors the canonical wire encoding field-for-
    // field, in encoding order. Folding them into a struct would hide
    // the byte-layout contract this function exists to pin down.
    #[allow(clippy::too_many_arguments)]
    fn canonical_bytes(
        pubkey: &[u8; 32],
        endpoint: &str,
        supported_contracts: &[String],
        custom_program_hashes: &[[u8; 32]],
        display_name: &str,
        fee_schedule: &str,
        bond_amount: u128,
        version: u64,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(512);
        out.extend_from_slice(b"PSL-AGENT-REGISTRATION-V1");
        out.extend_from_slice(pubkey);
        push_str(&mut out, endpoint);
        push_str_list(&mut out, supported_contracts);
        out.extend_from_slice(&(custom_program_hashes.len() as u32).to_be_bytes());
        for h in custom_program_hashes {
            out.extend_from_slice(h);
        }
        push_str(&mut out, display_name);
        push_str(&mut out, fee_schedule);
        out.extend_from_slice(&bond_amount.to_be_bytes());
        out.extend_from_slice(&version.to_be_bytes());
        out
    }

    // Mirrors `canonical_bytes` argument-for-argument; see the note there.
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        signer: &SigningKey,
        endpoint: String,
        supported_contracts: Vec<String>,
        custom_program_hashes: Vec<[u8; 32]>,
        display_name: String,
        fee_schedule: String,
        bond_amount: u128,
        version: u64,
    ) -> Self {
        let pubkey = signer.verifying_key().to_bytes();
        let body = Self::canonical_bytes(
            &pubkey,
            &endpoint,
            &supported_contracts,
            &custom_program_hashes,
            &display_name,
            &fee_schedule,
            bond_amount,
            version,
        );
        let sig = signer.sign(&body);
        Self {
            pubkey,
            endpoint,
            supported_contracts,
            custom_program_hashes,
            display_name,
            fee_schedule,
            bond_amount,
            version,
            sig: sig.to_bytes(),
        }
    }

    pub fn verify(&self) -> Result<(), ProtocolError> {
        let pk = VerifyingKey::from_bytes(&self.pubkey)
            .map_err(|e| ProtocolError::Ed25519(format!("agent pubkey: {e}")))?;
        let sig = Signature::from_bytes(&self.sig);
        let body = Self::canonical_bytes(
            &self.pubkey,
            &self.endpoint,
            &self.supported_contracts,
            &self.custom_program_hashes,
            &self.display_name,
            &self.fee_schedule,
            self.bond_amount,
            self.version,
        );
        pk.verify(&body, &sig)
            .map_err(|_| ProtocolError::SignatureInvalid)
    }

    pub fn supports(&self, contract: &str) -> bool {
        self.supported_contracts.iter().any(|c| c == contract)
    }
}

fn push_str(buf: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    buf.extend_from_slice(&(b.len() as u32).to_be_bytes());
    buf.extend_from_slice(b);
}
fn push_str_list(buf: &mut Vec<u8>, ss: &[String]) {
    buf.extend_from_slice(&(ss.len() as u32).to_be_bytes());
    for s in ss {
        push_str(buf, s);
    }
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
    fn signed_registration_round_trips() {
        let signer = sk(1);
        let reg = AgentRegistration::sign(
            &signer,
            "https://agent.example/v1".into(),
            vec!["transfer".into(), "swap".into()],
            vec![],
            "Trader Bot".into(),
            "0.10% fee on swaps".into(),
            1_000_000,
            1,
        );
        reg.verify().unwrap();
        assert!(reg.supports("transfer"));
        assert!(!reg.supports("freeze_account"));
    }

    #[test]
    fn tampered_registration_rejected() {
        let signer = sk(1);
        let mut reg = AgentRegistration::sign(
            &signer,
            "https://agent.example/v1".into(),
            vec!["transfer".into()],
            vec![],
            "X".into(),
            "Y".into(),
            10,
            1,
        );
        reg.bond_amount = 0; // attempt to tamper down the bond
        assert!(matches!(reg.verify(), Err(ProtocolError::SignatureInvalid)));
    }
}
