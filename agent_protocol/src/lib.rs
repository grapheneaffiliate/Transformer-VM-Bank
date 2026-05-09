//! PSL agent negotiation protocol — Phase 2 Layer 4.
//!
//! Defines the on-chain semantics of agent-to-agent contract proposals:
//!
//! - [`registry`] — `AgentRegistration` record an agent publishes to
//!   announce its pubkey, supported contract names, optional metadata,
//!   and the bond it has staked. Bond returns on deregistration; is
//!   slashable for protocol violations (per `dispute`).
//! - [`message`] — the 5 wire message types: `Propose`, `Accept`,
//!   `Reject`, `CounterPropose`, `Execute`. Each is signed; each
//!   carries a stable `proposal_hash` that anchors idempotency.
//! - [`state_machine`] — `ProposalState` and `ProposalLog` together
//!   enforce the legal transitions (`Proposed → Accepted/Rejected/
//!   CounterProposed → Executed/Expired`) without relying on network
//!   ordering. Replays / out-of-order delivery are absorbed by
//!   matching on `proposal_hash`.
//! - [`reputation`] — per-agent reputation counters: contracts
//!   initiated, completed, disputed, lost-disputes. Updates are emitted
//!   by the sequencer based on `state_machine` outcomes.
//! - [`dispute`] — `Dispute` tx + `resolve_dispute` driver that
//!   re-executes the contract through `psl-agent-contracts` (which is
//!   itself a `TernaryProgram` and therefore deterministic) and
//!   returns the slash decision.
//!
//! ## What this crate is *not*
//!
//! - Not the network layer. Mutual-TLS HTTPS transport, rate limiting,
//!   and idempotent retry are caller responsibilities (the SDK in
//!   Layer 5 wires them up).
//! - Not the MPT subtree storage. Sequencer-side persistence of the
//!   reputation / registry / revocation subtrees is in `psl-sequencer`
//!   (Phase 1).

pub mod dispute;
pub mod error;
pub mod message;
pub mod registry;
pub mod reputation;
pub mod state_machine;

pub use dispute::{Dispute, DisputeOutcome};
pub use error::ProtocolError;
pub use message::{
    Accept, CounterPropose, Execute, ExpectedOutput, Propose, ProposalHash, ProtocolMessage, Reject,
};
pub use registry::AgentRegistration;
pub use reputation::ReputationCounters;
pub use state_machine::{ProposalLog, ProposalState};
