use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("input length {got}, expected {expected} for contract {contract}")]
    InputShape {
        contract: &'static str,
        got: usize,
        expected: usize,
    },

    #[error("primitive error: {0}")]
    Primitive(#[from] psl_ternary_vm::TernaryError),

    #[error("contract precondition failed: {0}")]
    Precondition(String),

    #[error("integer overflow in contract {contract}")]
    Overflow { contract: &'static str },
}
