//! Minimal JSON-RPC interface.
//!
//! Methods (over HTTP POST /rpc):
//!   - submit_tx({tx})           → { ok, tx_hash | error }
//!   - get_account({pubkey})     → { account_bytes_hex }
//!   - get_proof({pubkey})       → { account_bytes_hex, proof: { siblings: [...], value_hex } }
//!   - get_block({n})            → { block }
//!   - get_head()                → { block_n, header_hash, new_state_root }

use axum::{routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::node::SequencerNode;

#[derive(Clone)]
pub struct RpcState {
    pub node: Arc<SequencerNode>,
}

#[derive(Deserialize)]
pub struct GetAccountReq {
    pub pubkey_hex: String,
}

#[derive(Serialize)]
pub struct GetAccountResp {
    pub account_bytes_hex: String,
}

pub fn router(state: RpcState) -> Router {
    Router::new()
        .route("/rpc/get_account", post(get_account))
        .with_state(state)
}

async fn get_account(
    axum::extract::State(state): axum::extract::State<RpcState>,
    Json(req): Json<GetAccountReq>,
) -> Json<GetAccountResp> {
    let pk_bytes = hex::decode(&req.pubkey_hex).unwrap_or_default();
    let mut pk = [0u8; 32];
    if pk_bytes.len() == 32 {
        pk.copy_from_slice(&pk_bytes);
    }
    let s = state.node.state.read().unwrap();
    let acc = s.account(&pk);
    Json(GetAccountResp {
        account_bytes_hex: hex::encode(acc.bytes),
    })
}
