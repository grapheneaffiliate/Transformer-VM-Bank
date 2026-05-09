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

    #[error("agent has no contract registered for program_hash {0:?}")]
    UnknownContract([u8; 32]),
}
