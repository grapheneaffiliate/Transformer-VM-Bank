use psl_agent_contracts::ProgramHash;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SdkError {
    #[error("wallet: {0}")]
    Wallet(#[from] psl_agent_wallet::WalletError),

    #[error("protocol: {0}")]
    Protocol(#[from] psl_agent_protocol::ProtocolError),

    #[error("contract: {0}")]
    Contract(#[from] psl_agent_contracts::ContractError),

    #[error("ternary kernel: {0}")]
    Ternary(#[from] psl_ternary_vm::TernaryError),

    #[error("transport: {0}")]
    Transport(String),

    /// Carries the canonical v2 ProgramHash (64-byte BLAKE3-512 per
    /// ADR-0008) of the unknown contract. The hash is the public
    /// on-chain contract identifier — not a secret — so disclosing
    /// it in the error doesn't leak crypto state. (Same partition
    /// as the engineer-reviewer's #41 hardening: identifying info is
    /// carried at parse-time / lookup-time errors because no secret
    /// material has been touched yet.)
    #[error("agent has no contract registered for program_hash {0:?}")]
    UnknownContract(ProgramHash),
}
