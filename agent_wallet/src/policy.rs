//! Per-key spending policy.
//!
//! Each child key carries a parent-signed policy:
//! - `cap_per_window`: max total spend (u128 amount-units) allowed in a
//!   rolling time window.
//! - `window_secs`: window length, in seconds.
//! - `allowed_contracts`: set of contract names this key may invoke.
//!   Empty means "no restriction" — keep this conservative; default
//!   to a positive allowlist in production.
//! - `allowed_counterparties`: set of pubkeys this key may transact with.
//!   Empty means "any counterparty allowed".
//! - `expiry_unix`: 0 means no expiry; otherwise the policy invalidates
//!   at or after this unix timestamp.
//!
//! The policy is signed by the parent key over its canonical
//! serialization (`PolicyEnvelope::canonical_bytes`). The mempool
//! validates the signature on every tx.
//!
//! `SpendingTracker` is the runtime state — a small per-key bookkeeper
//! that records (timestamp, amount) for each spend and enforces the
//! window cap on the next attempted spend.

use crate::error::WalletError;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use std::collections::VecDeque;

/// Wire-format policy. The signature is over `canonical_bytes`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyPolicy {
    /// Pubkey this policy authorizes.
    #[serde(with = "BigArray")]
    pub child_pubkey: [u8; 32],
    /// Pubkey of the parent that signed this policy.
    #[serde(with = "BigArray")]
    pub parent_pubkey: [u8; 32],
    /// Max total spend (sum of u128 amounts) within the window.
    pub cap_per_window: u128,
    /// Sliding window length in seconds.
    pub window_secs: u64,
    /// Allowed contract names (empty = unrestricted, but production
    /// should use positive allowlists).
    pub allowed_contracts: Vec<String>,
    /// Allowed counterparties; empty = unrestricted.
    pub allowed_counterparties: Vec<[u8; 32]>,
    /// Policy expiry as unix timestamp; 0 = no expiry.
    pub expiry_unix: u64,
    /// Monotonic policy version (so a parent can rotate policies for
    /// the same child without ambiguity over which is current).
    pub version: u64,
}

impl KeyPolicy {
    /// Canonical byte serialization for signing. Stable across Rust
    /// versions / serde versions because we lay out the fields by
    /// hand.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(1024);
        out.extend_from_slice(b"PSL-KEY-POLICY-V1");
        out.extend_from_slice(&self.child_pubkey);
        out.extend_from_slice(&self.parent_pubkey);
        out.extend_from_slice(&self.cap_per_window.to_be_bytes());
        out.extend_from_slice(&self.window_secs.to_be_bytes());
        out.extend_from_slice(&self.expiry_unix.to_be_bytes());
        out.extend_from_slice(&self.version.to_be_bytes());
        out.extend_from_slice(&(self.allowed_contracts.len() as u32).to_be_bytes());
        for name in &self.allowed_contracts {
            let b = name.as_bytes();
            out.extend_from_slice(&(b.len() as u32).to_be_bytes());
            out.extend_from_slice(b);
        }
        out.extend_from_slice(&(self.allowed_counterparties.len() as u32).to_be_bytes());
        for cp in &self.allowed_counterparties {
            out.extend_from_slice(cp);
        }
        out
    }

    pub fn allows_contract(&self, contract: &str) -> bool {
        self.allowed_contracts.is_empty() || self.allowed_contracts.iter().any(|c| c == contract)
    }

    pub fn allows_counterparty(&self, cp: &[u8; 32]) -> bool {
        self.allowed_counterparties.is_empty()
            || self.allowed_counterparties.iter().any(|p| p == cp)
    }
}

/// Signed policy as it travels on-chain. The sequencer verifies `sig`
/// against `policy.parent_pubkey` over `policy.canonical_bytes`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PolicyEnvelope {
    pub policy: KeyPolicy,
    #[serde(with = "BigArray")]
    pub sig: [u8; 64],
}

impl PolicyEnvelope {
    /// Sign + envelope a policy with the parent's signing key. Errors
    /// if the parent key in the policy doesn't match the signing key.
    pub fn sign(parent: &SigningKey, policy: KeyPolicy) -> Result<Self, WalletError> {
        if parent.verifying_key().to_bytes() != policy.parent_pubkey {
            return Err(WalletError::PolicySignatureInvalid);
        }
        let sig = parent.sign(&policy.canonical_bytes());
        Ok(Self {
            policy,
            sig: sig.to_bytes(),
        })
    }

    pub fn verify(&self) -> Result<(), WalletError> {
        let parent_pk = VerifyingKey::from_bytes(&self.policy.parent_pubkey)
            .map_err(|e| WalletError::Ed25519(format!("parent pubkey: {e}")))?;
        let sig = Signature::from_bytes(&self.sig);
        parent_pk
            .verify(&self.policy.canonical_bytes(), &sig)
            .map_err(|_| WalletError::PolicySignatureInvalid)
    }
}

/// Window-based spending tracker. Records (unix_secs, amount) per spend,
/// drops entries older than `window_secs`, sums the rest to enforce the
/// `cap_per_window`.
pub struct SpendingTracker {
    pub policy: KeyPolicy,
    spends: VecDeque<(u64, u128)>,
}

impl SpendingTracker {
    pub fn new(policy: KeyPolicy) -> Self {
        Self {
            policy,
            spends: VecDeque::new(),
        }
    }

    /// Drop entries older than the window relative to `now`.
    fn evict_expired(&mut self, now: u64) {
        let cutoff = now.saturating_sub(self.policy.window_secs);
        while let Some(&(t, _)) = self.spends.front() {
            if t < cutoff {
                self.spends.pop_front();
            } else {
                break;
            }
        }
    }

    /// Sum of spends currently inside the window.
    pub fn current_window_total(&mut self, now: u64) -> u128 {
        self.evict_expired(now);
        let mut total: u128 = 0;
        for &(_, amount) in &self.spends {
            total = total.saturating_add(amount);
        }
        total
    }

    /// Try to admit a new spend. Errors if it would exceed the cap or
    /// the policy expired.
    pub fn try_spend(&mut self, now: u64, amount: u128) -> Result<(), WalletError> {
        if self.policy.expiry_unix != 0 && now >= self.policy.expiry_unix {
            return Err(WalletError::PolicyExpired {
                expiry: self.policy.expiry_unix,
                now,
            });
        }
        let current = self.current_window_total(now);
        let would_spend = current
            .checked_add(amount)
            .ok_or(WalletError::PolicyOverspend {
                would_spend: u128::MAX,
                cap: self.policy.cap_per_window,
            })?;
        if would_spend > self.policy.cap_per_window {
            return Err(WalletError::PolicyOverspend {
                would_spend,
                cap: self.policy.cap_per_window,
            });
        }
        self.spends.push_back((now, amount));
        Ok(())
    }

    /// Validate a transaction: contract name allowed + counterparty
    /// allowed + spend admissible. Does not verify signatures here —
    /// the caller (mempool) does that with the parent + child keys.
    pub fn admit(
        &mut self,
        now: u64,
        contract: &str,
        counterparty: &[u8; 32],
        amount: u128,
    ) -> Result<(), WalletError> {
        if !self.policy.allows_contract(contract) {
            return Err(WalletError::PolicyContractDisallowed(contract.into()));
        }
        if !self.policy.allows_counterparty(counterparty) {
            return Err(WalletError::PolicyCounterpartyDisallowed {
                pubkey: *counterparty,
            });
        }
        self.try_spend(now, amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::SeedableRng;

    fn fresh_signing(seed: u64) -> SigningKey {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        SigningKey::generate(&mut rng)
    }

    fn sample_policy(parent_pk: [u8; 32], child_pk: [u8; 32]) -> KeyPolicy {
        KeyPolicy {
            child_pubkey: child_pk,
            parent_pubkey: parent_pk,
            cap_per_window: 1_000,
            window_secs: 3600,
            allowed_contracts: vec!["transfer".into(), "swap".into()],
            allowed_counterparties: vec![],
            expiry_unix: 0,
            version: 1,
        }
    }

    #[test]
    fn signing_and_verification_round_trip() {
        let parent = fresh_signing(1);
        let child = fresh_signing(2);
        let policy = sample_policy(
            parent.verifying_key().to_bytes(),
            child.verifying_key().to_bytes(),
        );
        let env = PolicyEnvelope::sign(&parent, policy).unwrap();
        env.verify().expect("signature verifies");
    }

    #[test]
    fn tampered_policy_rejected() {
        let parent = fresh_signing(1);
        let child = fresh_signing(2);
        let policy = sample_policy(
            parent.verifying_key().to_bytes(),
            child.verifying_key().to_bytes(),
        );
        let mut env = PolicyEnvelope::sign(&parent, policy).unwrap();
        env.policy.cap_per_window = u128::MAX;
        let r = env.verify();
        assert!(matches!(r, Err(WalletError::PolicySignatureInvalid)));
    }

    #[test]
    fn wrong_parent_signing_key_rejected() {
        let parent_a = fresh_signing(1);
        let parent_b = fresh_signing(99);
        let child = fresh_signing(2);
        let policy = sample_policy(
            parent_a.verifying_key().to_bytes(),
            child.verifying_key().to_bytes(),
        );
        // attempt to sign with parent_b instead of parent_a
        let r = PolicyEnvelope::sign(&parent_b, policy);
        assert!(matches!(r, Err(WalletError::PolicySignatureInvalid)));
    }

    #[test]
    fn spend_under_cap_admitted() {
        let parent = fresh_signing(1);
        let child = fresh_signing(2);
        let cp = fresh_signing(3).verifying_key().to_bytes();
        let policy = sample_policy(
            parent.verifying_key().to_bytes(),
            child.verifying_key().to_bytes(),
        );
        let mut t = SpendingTracker::new(policy);
        t.admit(1000, "transfer", &cp, 200).unwrap();
        t.admit(1100, "transfer", &cp, 300).unwrap();
        t.admit(1200, "transfer", &cp, 499).unwrap();
        // total = 999, cap 1000
        let r = t.admit(1300, "transfer", &cp, 2);
        assert!(matches!(r, Err(WalletError::PolicyOverspend { .. })));
    }

    #[test]
    fn spend_window_evicts_old_entries() {
        let parent = fresh_signing(1);
        let child = fresh_signing(2);
        let cp = fresh_signing(3).verifying_key().to_bytes();
        let policy = sample_policy(
            parent.verifying_key().to_bytes(),
            child.verifying_key().to_bytes(),
        );
        let mut t = SpendingTracker::new(policy);
        t.admit(1000, "transfer", &cp, 800).unwrap();
        // 800 of 1000 used. 1 hour later — old spend evicted.
        t.admit(1000 + 3601, "transfer", &cp, 800).unwrap();
        assert_eq!(t.current_window_total(1000 + 3601), 800);
    }

    #[test]
    fn spend_after_expiry_rejected() {
        let parent = fresh_signing(1);
        let child = fresh_signing(2);
        let cp = fresh_signing(3).verifying_key().to_bytes();
        let mut policy = sample_policy(
            parent.verifying_key().to_bytes(),
            child.verifying_key().to_bytes(),
        );
        policy.expiry_unix = 5000;
        let mut t = SpendingTracker::new(policy);
        t.admit(4000, "transfer", &cp, 100).unwrap();
        let r = t.admit(5001, "transfer", &cp, 1);
        assert!(matches!(r, Err(WalletError::PolicyExpired { .. })));
    }

    #[test]
    fn disallowed_contract_rejected() {
        let parent = fresh_signing(1);
        let child = fresh_signing(2);
        let cp = fresh_signing(3).verifying_key().to_bytes();
        let policy = sample_policy(
            parent.verifying_key().to_bytes(),
            child.verifying_key().to_bytes(),
        );
        let mut t = SpendingTracker::new(policy);
        let r = t.admit(1000, "freeze_account", &cp, 1);
        assert!(matches!(
            r,
            Err(WalletError::PolicyContractDisallowed(s)) if s == "freeze_account"
        ));
    }

    #[test]
    fn disallowed_counterparty_rejected() {
        let parent = fresh_signing(1);
        let child = fresh_signing(2);
        let allowed = fresh_signing(3).verifying_key().to_bytes();
        let other = fresh_signing(99).verifying_key().to_bytes();
        let mut policy = sample_policy(
            parent.verifying_key().to_bytes(),
            child.verifying_key().to_bytes(),
        );
        policy.allowed_counterparties = vec![allowed];
        let mut t = SpendingTracker::new(policy);
        t.admit(1000, "transfer", &allowed, 100).unwrap();
        let r = t.admit(1000, "transfer", &other, 100);
        assert!(matches!(
            r,
            Err(WalletError::PolicyCounterpartyDisallowed { .. })
        ));
    }
}
