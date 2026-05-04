//! Transaction format.

use psl_crypto::{Hash, PublicKey, Signature};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TxKind {
    Transfer,
    Mint,
    Burn,
    Freeze,
    MultiAsset,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedTx {
    pub kind: TxKind,
    pub asset_id: u32,
    pub nonce: u64,
    /// Sender pubkey (transfer/burn) OR issuer authority (mint/freeze).
    pub signer: PublicKey,
    /// For transfer/multi-asset.
    pub recipient: Option<PublicKey>,
    /// u128 amount (transfer/mint/burn). 16 bytes little-endian.
    pub amount: [u8; 16],
    /// For freeze: 1 = freeze, 0 = unfreeze.
    pub flag: u8,
    /// For freeze: court-order hash (immutable audit trail).
    pub court_order_hash: Option<Hash>,
    /// For multi-asset: serialized payload.
    pub multi_payload: Option<Vec<u8>>,
    /// Travel-rule encrypted metadata for high-value txs.
    pub originator_metadata: Option<Vec<u8>>,
    /// ed25519 signature over a canonical encoding of all above fields.
    #[serde(with = "BigArray")]
    pub signature: Signature,
}

impl SignedTx {
    /// Canonical bytes for sig verification — every field except `signature` itself.
    pub fn canonical(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(self.kind as u8);
        buf.extend_from_slice(&self.asset_id.to_le_bytes());
        buf.extend_from_slice(&self.nonce.to_le_bytes());
        buf.extend_from_slice(&self.signer);
        if let Some(r) = self.recipient {
            buf.push(1);
            buf.extend_from_slice(&r);
        } else {
            buf.push(0);
        }
        buf.extend_from_slice(&self.amount);
        buf.push(self.flag);
        if let Some(h) = self.court_order_hash {
            buf.push(1);
            buf.extend_from_slice(&h);
        } else {
            buf.push(0);
        }
        if let Some(p) = &self.multi_payload {
            buf.push(1);
            buf.extend_from_slice(&(p.len() as u32).to_le_bytes());
            buf.extend_from_slice(p);
        } else {
            buf.push(0);
        }
        if let Some(m) = &self.originator_metadata {
            buf.push(1);
            buf.extend_from_slice(&(m.len() as u32).to_le_bytes());
            buf.extend_from_slice(m);
        } else {
            buf.push(0);
        }
        buf
    }
}
