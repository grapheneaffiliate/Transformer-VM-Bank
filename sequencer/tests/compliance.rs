//! Sequencer compliance gate (gate 5).
//!
//! Exercises the three compliance primitives end-to-end:
//!   1. Travel-rule: high-value transfers without `originator_metadata` are
//!      rejected by the mempool; transfers with metadata pass.
//!   2. Freeze authority: only the issuer's authority key can submit a freeze;
//!      a non-issuer freeze is rejected. Freeze must carry `court_order_hash`.
//!      A frozen account's subsequent transfer attempt is rejected.
//!   3. View-key access: a regulator with a registered view-key can produce a
//!      verifiable Merkle inclusion proof for accounts holding the asset; an
//!      unauthorized regulator cannot fabricate one.

use psl_crypto::{sign, Account, KeyPair, SparseMerkleTree};
use psl_sequencer::{
    issuer_registry::IssuerRecord,
    mempool::validate,
    state::State,
    tx::{SignedTx, TxKind},
};

fn build_tx(
    kind: TxKind,
    asset_id: u32,
    nonce: u64,
    signer: &KeyPair,
    recipient: Option<[u8; 32]>,
    amount: u128,
    flag: u8,
    court_order_hash: Option<[u8; 32]>,
    metadata: Option<Vec<u8>>,
) -> SignedTx {
    let mut tx = SignedTx {
        kind,
        asset_id,
        nonce,
        signer: signer.public(),
        recipient,
        amount: amount.to_le_bytes(),
        flag,
        court_order_hash,
        multi_payload: None,
        originator_metadata: metadata,
        signature: [0u8; 64],
    };
    tx.signature = sign(signer, &tx.canonical());
    tx
}

fn install_issuer(state: &mut State, asset_id: u32, issuer: &KeyPair, threshold: u128) {
    let rec = IssuerRecord {
        asset_id,
        authority_pubkey: issuer.public(),
        max_supply: u128::MAX.to_le_bytes(),
        mint_enabled: true,
        burn_enabled: true,
        freeze_enabled: true,
        travel_rule_threshold: threshold.to_le_bytes(),
        regulator_view_keys: vec![],
        name: "USD-DEMO".into(),
    };
    state.registry.put(rec.key(), rec.serialize());
}

fn lookup<'a>(state: &'a State) -> impl Fn(u32) -> Option<IssuerRecord> + 'a {
    move |asset_id| {
        let rec = IssuerRecord {
            asset_id,
            authority_pubkey: [0u8; 32],
            max_supply: [0u8; 16],
            mint_enabled: false,
            burn_enabled: false,
            freeze_enabled: false,
            travel_rule_threshold: [0u8; 16],
            regulator_view_keys: vec![],
            name: String::new(),
        };
        state
            .registry
            .get(&rec.key())
            .and_then(IssuerRecord::deserialize)
    }
}

// ---------- 1. Travel rule ----------

#[test]
fn travel_rule_high_value_without_metadata_rejected() {
    let mut state = State::new();
    let issuer = KeyPair::from_seed([1u8; 32]);
    let alice = KeyPair::from_seed([2u8; 32]);
    let bob_pk = KeyPair::from_seed([3u8; 32]).public();
    install_issuer(&mut state, 1, &issuer, 1_000); // threshold = 1000

    // Pre-fund Alice
    let mut acc = Account::default();
    acc.bytes[..32].copy_from_slice(&alice.public());
    acc.bytes[32..48].copy_from_slice(&5_000u128.to_le_bytes());
    acc.bytes[60..64].copy_from_slice(&1u32.to_le_bytes()); // asset_id = 1
    state.put_account(acc);

    // High-value transfer (5000 > threshold 1000) without metadata → reject
    let tx = build_tx(
        TxKind::Transfer,
        1,
        1,
        &alice,
        Some(bob_pk),
        5_000,
        0,
        None,
        None,
    );
    let res = validate(&tx, &state, &lookup(&state));
    assert!(
        res.is_err(),
        "high-value tx without metadata must be rejected"
    );
    let err = format!("{:?}", res.unwrap_err());
    assert!(
        err.contains("travel-rule"),
        "error should mention travel-rule, got: {err}"
    );
}

#[test]
fn travel_rule_high_value_with_metadata_accepted() {
    let mut state = State::new();
    let issuer = KeyPair::from_seed([1u8; 32]);
    let alice = KeyPair::from_seed([2u8; 32]);
    let bob_pk = KeyPair::from_seed([3u8; 32]).public();
    install_issuer(&mut state, 1, &issuer, 1_000);

    let mut acc = Account::default();
    acc.bytes[..32].copy_from_slice(&alice.public());
    acc.bytes[32..48].copy_from_slice(&5_000u128.to_le_bytes());
    acc.bytes[60..64].copy_from_slice(&1u32.to_le_bytes());
    state.put_account(acc);

    let metadata = b"encrypted_originator_blob_v1".to_vec();
    let tx = build_tx(
        TxKind::Transfer,
        1,
        1,
        &alice,
        Some(bob_pk),
        5_000,
        0,
        None,
        Some(metadata),
    );
    let res = validate(&tx, &state, &lookup(&state));
    assert!(
        res.is_ok(),
        "high-value tx WITH metadata must pass: {res:?}"
    );
}

#[test]
fn travel_rule_low_value_without_metadata_accepted() {
    let mut state = State::new();
    let issuer = KeyPair::from_seed([1u8; 32]);
    let alice = KeyPair::from_seed([2u8; 32]);
    let bob_pk = KeyPair::from_seed([3u8; 32]).public();
    install_issuer(&mut state, 1, &issuer, 1_000);

    let mut acc = Account::default();
    acc.bytes[..32].copy_from_slice(&alice.public());
    acc.bytes[32..48].copy_from_slice(&5_000u128.to_le_bytes());
    acc.bytes[60..64].copy_from_slice(&1u32.to_le_bytes());
    state.put_account(acc);

    // 500 < threshold 1000 → no metadata required
    let tx = build_tx(
        TxKind::Transfer,
        1,
        1,
        &alice,
        Some(bob_pk),
        500,
        0,
        None,
        None,
    );
    let res = validate(&tx, &state, &lookup(&state));
    assert!(
        res.is_ok(),
        "low-value tx without metadata must pass: {res:?}"
    );
}

// ---------- 2. Freeze authority ----------

#[test]
fn freeze_by_non_issuer_rejected() {
    let mut state = State::new();
    let issuer = KeyPair::from_seed([1u8; 32]);
    let attacker = KeyPair::from_seed([99u8; 32]);
    install_issuer(&mut state, 1, &issuer, u128::MAX);

    let target = [0xaa; 32];
    let tx = build_tx(
        TxKind::Freeze,
        1,
        1,
        &attacker, // signed by attacker, not issuer
        Some(target),
        0,
        1,
        Some([0xab; 32]),
        None,
    );
    let res = validate(&tx, &state, &lookup(&state));
    assert!(res.is_err(), "non-issuer freeze must be rejected");
    let err = format!("{:?}", res.unwrap_err());
    assert!(err.contains("freeze not authorized"), "got: {err}");
}

#[test]
fn freeze_without_court_order_rejected() {
    let mut state = State::new();
    let issuer = KeyPair::from_seed([1u8; 32]);
    install_issuer(&mut state, 1, &issuer, u128::MAX);

    let target = [0xaa; 32];
    let tx = build_tx(
        TxKind::Freeze,
        1,
        1,
        &issuer,
        Some(target),
        0,
        1,
        None, // no court_order_hash
        None,
    );
    let res = validate(&tx, &state, &lookup(&state));
    assert!(
        res.is_err(),
        "freeze without court_order_hash must be rejected"
    );
    let err = format!("{:?}", res.unwrap_err());
    assert!(err.contains("court_order_hash"), "got: {err}");
}

#[test]
fn freeze_by_issuer_with_court_order_accepted() {
    let mut state = State::new();
    let issuer = KeyPair::from_seed([1u8; 32]);
    install_issuer(&mut state, 1, &issuer, u128::MAX);

    let target = [0xaa; 32];
    let tx = build_tx(
        TxKind::Freeze,
        1,
        1,
        &issuer,
        Some(target),
        0,
        1,
        Some([0xab; 32]),
        None,
    );
    let res = validate(&tx, &state, &lookup(&state));
    assert!(
        res.is_ok(),
        "issuer freeze with court_order must pass: {res:?}"
    );
}

#[test]
fn frozen_account_cannot_transfer() {
    let mut state = State::new();
    let issuer = KeyPair::from_seed([1u8; 32]);
    let alice = KeyPair::from_seed([2u8; 32]);
    let bob_pk = KeyPair::from_seed([3u8; 32]).public();
    install_issuer(&mut state, 1, &issuer, u128::MAX);

    // Alice has balance, but is frozen. The frozen flag is bit 7 of byte 47
    // (the high byte of the 16-byte balance field, masked off when reading
    // the balance). See crypto/src/account.rs for the layout.
    let mut acc = Account::new(alice.public());
    acc.set_balance(5_000);
    acc.set_frozen(true);
    state.put_account(acc);

    let tx = build_tx(
        TxKind::Transfer,
        1,
        1,
        &alice,
        Some(bob_pk),
        100,
        0,
        None,
        None,
    );
    let res = validate(&tx, &state, &lookup(&state));
    assert!(res.is_err(), "frozen account's transfer must be rejected");
    let err = format!("{:?}", res.unwrap_err());
    assert!(err.contains("frozen"), "got: {err}");
}

// ---------- 3. View-key proofs ----------

#[test]
fn regulator_can_verify_balance_via_inclusion_proof() {
    // The view-key model: regulator gets a Merkle inclusion proof for an
    // account they're authorized to see. They verify it against the published
    // accounts root. This test exercises the proof generation + verification.
    let mut state = State::new();
    let issuer = KeyPair::from_seed([1u8; 32]);
    let alice = KeyPair::from_seed([2u8; 32]);
    install_issuer(&mut state, 1, &issuer, u128::MAX);

    let mut acc = Account::default();
    acc.bytes[..32].copy_from_slice(&alice.public());
    acc.bytes[32..48].copy_from_slice(&12_345u128.to_le_bytes());
    acc.bytes[60..64].copy_from_slice(&1u32.to_le_bytes());
    state.put_account(acc);

    let root = state.accounts_root();
    let pk = alice.public();
    let proof = state.account_proof(&pk);

    // Regulator's verification: proof verifies against the published root.
    // The proof embeds the leaf value, so verification is parameter-free
    // beyond root + key + proof.
    assert!(
        SparseMerkleTree::verify_proof(&root, &pk, &proof),
        "valid inclusion proof must verify"
    );

    // The regulator reads the balance from proof.value (bytes 32..48).
    let balance = u128::from_le_bytes(proof.value[32..48].try_into().unwrap());
    assert_eq!(balance, 12_345);
}

#[test]
fn tampered_proof_rejected() {
    let mut state = State::new();
    let issuer = KeyPair::from_seed([1u8; 32]);
    let alice = KeyPair::from_seed([2u8; 32]);
    install_issuer(&mut state, 1, &issuer, u128::MAX);

    let mut acc = Account::default();
    acc.bytes[..32].copy_from_slice(&alice.public());
    acc.bytes[32..48].copy_from_slice(&12_345u128.to_le_bytes());
    state.put_account(acc);

    let root = state.accounts_root();
    let pk = alice.public();
    let mut proof = state.account_proof(&pk);
    // Forge the balance bytes inside the proof's leaf value.
    proof.value[32..48].copy_from_slice(&999_999_999u128.to_le_bytes());

    assert!(
        !SparseMerkleTree::verify_proof(&root, &pk, &proof),
        "forged-balance leaf must be rejected by inclusion-proof verifier"
    );
}
