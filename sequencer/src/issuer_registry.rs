//! Issuer registry: asset_id → IssuerRecord.
//!
//! The registry is part of system state (its SMT root joins the global state
//! commitment). Adding/updating an issuer requires authority signature
//! (system-root in sovereign mode, 2/3 supermajority in BFT mode).

use psl_crypto::{hash_bytes, Hash, PublicKey};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IssuerRecord {
    pub asset_id: u32,
    /// Authority that may mint, burn, and freeze for this asset.
    pub authority_pubkey: PublicKey,
    /// Cap on total supply (u128 little-endian). 0 = no cap.
    pub max_supply: [u8; 16],
    pub mint_enabled: bool,
    pub burn_enabled: bool,
    pub freeze_enabled: bool,
    /// Travel-rule threshold for this asset (u128 le); 0 = always require.
    pub travel_rule_threshold: [u8; 16],
    /// Pubkeys allowed to read view-key proofs for this asset.
    pub regulator_view_keys: Vec<PublicKey>,
    /// Optional sub-asset name like "USD-DEMO".
    pub name: String,
}

impl IssuerRecord {
    pub fn key(&self) -> Hash {
        let mut buf = b"issuer:".to_vec();
        buf.extend_from_slice(&self.asset_id.to_le_bytes());
        hash_bytes(&buf)
    }

    pub fn serialize(&self) -> Vec<u8> {
        // Canonical: bincode would normally fit, but for now a JSON serialization
        // keeps debugging simple. Move to bincode/protobuf in the perf pass.
        serde_json::to_vec(self).expect("issuer record serialization")
    }

    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
}
