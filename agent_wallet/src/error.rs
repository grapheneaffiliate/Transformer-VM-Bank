use thiserror::Error;

#[derive(Debug, Error)]
pub enum WalletError {
    #[error("seed must be 16, 32, or 64 bytes; got {got}")]
    BadSeedLength { got: usize },

    #[error("hardened-only derivation: child index {index} must have high bit set (≥ 0x80000000)")]
    NotHardened { index: u32 },

    #[error("invalid private key bytes")]
    InvalidPrivateKey,

    #[error("ed25519 signature verification failed")]
    SignatureInvalid,

    #[error("policy envelope signature does not match parent key")]
    PolicySignatureInvalid,

    #[error("policy expired at unix timestamp {expiry}; current is {now}")]
    PolicyExpired { expiry: u64, now: u64 },

    #[error("policy spending window exceeded: would spend {would_spend}, window cap is {cap}")]
    PolicyOverspend { would_spend: u128, cap: u128 },

    #[error("contract {0} not in policy allowlist")]
    PolicyContractDisallowed(String),

    #[error("counterparty {pubkey:?} not in policy allowlist")]
    PolicyCounterpartyDisallowed { pubkey: [u8; 32] },

    #[error("key {pubkey:?} has been revoked")]
    KeyRevoked { pubkey: [u8; 32] },

    #[error("ed25519: {0}")]
    Ed25519(String),
}
