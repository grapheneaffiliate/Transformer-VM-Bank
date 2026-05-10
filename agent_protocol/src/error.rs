use crate::message::ProposalHash;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("ed25519 signature verification failed")]
    SignatureInvalid,

    #[error("proposal hash mismatch — message carries {got:?}, expected {expected:?}")]
    ProposalHashMismatch {
        expected: ProposalHash,
        got: ProposalHash,
    },

    #[error("illegal state transition from {from:?} via {event}")]
    IllegalTransition {
        from: &'static str,
        event: &'static str,
    },

    #[error("unknown proposal {hash:?}")]
    UnknownProposal { hash: ProposalHash },

    #[error("expired at {expiry}; now is {now}")]
    Expired { expiry: u64, now: u64 },

    #[error("dispute decision: contract output {got:?} != claimed {claimed:?}")]
    DisputeOutputMismatch { got: Vec<u8>, claimed: Vec<u8> },

    #[error("contract execution: {0}")]
    Contract(#[from] psl_agent_contracts::ContractError),

    #[error("ed25519: {0}")]
    Ed25519(String),
}
