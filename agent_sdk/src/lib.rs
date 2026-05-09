//! PSL agent SDK — Phase 2 Layer 5.
//!
//! Stable, semver-versioned Rust API for building autonomous PSL
//! agents. Combines:
//!
//! - `psl-ternary-vm` (Layer 1) — pure-integer execution kernel
//! - `psl-agent-contracts` (Layer 2) — standard contract library
//! - `psl-agent-wallet` (Layer 3) — keys + spending policies
//! - `psl-agent-protocol` (Layer 4) — registration + 5 message types +
//!   state machine + dispute resolution
//!
//! ## Building an agent in three steps
//!
//! ```ignore
//! // 1. Create / load identity.
//! let agent = AgentSdk::new(seed).unwrap();
//!
//! // 2. Register on the directory.
//! let registration = agent.registration("https://my-agent/v1", &["transfer"]);
//!
//! // 3. Run the loop.
//! loop {
//!     for msg in transport.poll() {
//!         match msg {
//!             ProtocolMessage::Propose(p) => agent.handle_propose(p),
//!             // …
//!         }
//!     }
//! }
//! ```
//!
//! ## Stability
//!
//! This crate is **0.1.0 (semver pre-1.0)**. Public API is documented
//! and unit-tested but minor versions may break compatibility until
//! 1.0. See `CHANGELOG.md` for migration notes.
//!
//! ## What lives outside this SDK
//!
//! - **Network transport** (mutual-TLS HTTPS). The SDK is transport-
//!   agnostic; callers wire their own. The reference agents in
//!   `examples/` use a synchronous in-process channel for the demo.
//! - **MPT subtree storage** (registry, reputation, revocation). Lives
//!   in `psl-sequencer` (Phase 1); SDK reads via the `OnChainView`
//!   trait so callers can adapt to any backing store.
//! - **UniFFI bindings to Swift / Kotlin / Python / JavaScript**.
//!   Architecturally trivial — the Rust API is type-clean enough that
//!   `uniffi-bindgen` can emit bindings; produced as a separate crate
//!   in a follow-up.

pub mod agent;
pub mod error;
pub mod onchain;
pub mod transport;

pub use agent::{AgentIdentity, AgentSdk, ProposeDecision};
pub use error::SdkError;
pub use onchain::{InMemoryOnChain, OnChainView};
pub use transport::{InProcessBus, Mailbox, Transport};
