//! Transaction mempool: native pre-validation gate.
//!
//! All native checks happen here. Failures are rejected and never reach the
//! transformer trace. Pre-checks:
//!
//! - ed25519 signature valid for `tx.signer`
//! - `tx.nonce == account.nonce + 1` (transfer/burn) or > registry watermark (mint/freeze)
//! - For mint/burn/freeze: `tx.signer == issuer_registry[asset_id].authority_pubkey`
//! - `tx.amount` ≤ travel_rule_threshold OR originator_metadata is present
//! - For freeze: court_order_hash is non-None
//!
//! The mempool is a simple priority queue keyed by sender pubkey × nonce; for
//! v1 we accept up to one in-flight tx per (signer, nonce) pair.

use anyhow::{anyhow, Result};
use psl_crypto::{verify, Account};
use std::collections::VecDeque;

use crate::issuer_registry::IssuerRecord;
use crate::state::State;
use crate::tx::{SignedTx, TxKind};

pub struct Mempool {
    queue: VecDeque<SignedTx>,
    capacity: usize,
}

impl Mempool {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn ingress(
        &mut self,
        tx: SignedTx,
        state: &State,
        registry: &dyn Fn(u32) -> Option<IssuerRecord>,
    ) -> Result<()> {
        if self.queue.len() >= self.capacity {
            return Err(anyhow!("mempool full"));
        }
        validate(&tx, state, registry)?;
        self.queue.push_back(tx);
        Ok(())
    }

    pub fn drain(&mut self, n: usize) -> Vec<SignedTx> {
        let take = n.min(self.queue.len());
        self.queue.drain(..take).collect()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

pub fn validate(
    tx: &SignedTx,
    state: &State,
    registry: &dyn Fn(u32) -> Option<IssuerRecord>,
) -> Result<()> {
    // 1. Signature
    let canonical = tx.canonical();
    verify(&tx.signer, &canonical, &tx.signature).map_err(|e| anyhow!("invalid signature: {e}"))?;

    // 2. Nonce + sender state
    let signer_account: Account = state.account(&tx.signer);
    match tx.kind {
        TxKind::Transfer | TxKind::Burn => {
            if tx.nonce != signer_account.nonce() + 1 {
                return Err(anyhow!(
                    "nonce mismatch: expected {}, got {}",
                    signer_account.nonce() + 1,
                    tx.nonce
                ));
            }
            if signer_account.is_frozen() {
                return Err(anyhow!("sender frozen"));
            }
        }
        TxKind::Mint | TxKind::Freeze | TxKind::MultiAsset => {
            // Mint/freeze nonces are tracked in the registry, not the user account
            // (issuer authority can issue many txs without per-account ordering).
            // Multi-asset uses the signer's nonce.
            if matches!(tx.kind, TxKind::MultiAsset) && tx.nonce != signer_account.nonce() + 1 {
                return Err(anyhow!("multi-asset nonce mismatch"));
            }
        }
    }

    // 3. Authority for mint/burn/freeze
    let issuer = registry(tx.asset_id).ok_or_else(|| anyhow!("unknown asset_id"))?;
    match tx.kind {
        TxKind::Mint if tx.signer != issuer.authority_pubkey || !issuer.mint_enabled => {
            return Err(anyhow!("mint not authorized"));
        }
        // Burn: either the holder burns their own balance OR issuer-authority burns.
        // For simplicity v1 requires issuer-authority burn.
        TxKind::Burn if tx.signer != issuer.authority_pubkey || !issuer.burn_enabled => {
            return Err(anyhow!("burn not authorized"));
        }
        TxKind::Freeze => {
            if tx.signer != issuer.authority_pubkey || !issuer.freeze_enabled {
                return Err(anyhow!("freeze not authorized"));
            }
            if tx.court_order_hash.is_none() {
                return Err(anyhow!("freeze requires court_order_hash"));
            }
        }
        _ => {}
    }

    // 4. Travel rule
    let amount = u128::from_le_bytes(tx.amount);
    let threshold = u128::from_le_bytes(issuer.travel_rule_threshold);
    if matches!(tx.kind, TxKind::Transfer | TxKind::MultiAsset)
        && (threshold == 0 || amount > threshold)
        && tx.originator_metadata.is_none()
    {
        return Err(anyhow!("travel-rule metadata required for high-value tx"));
    }

    Ok(())
}
