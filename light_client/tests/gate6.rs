//! Light-client gate 6: cross-verify 1000 balances against published
//! block headers. Adversarial: tampered proofs and tampered headers
//! must both be rejected.

use psl_crypto::{Account, KeyPair, SparseMerkleTree};
use psl_light_client::{
    header::{Header, SignedHeader},
    verify_balance, VerifyError,
};
use rand::{rngs::StdRng, Rng, SeedableRng};

fn build_state_with_n_accounts(n: usize, rng: &mut StdRng) -> (SparseMerkleTree, Vec<([u8; 32], u128)>) {
    let mut smt = SparseMerkleTree::new();
    let mut accounts = Vec::with_capacity(n);
    for _ in 0..n {
        let mut pk = [0u8; 32];
        rng.fill(&mut pk);
        let bal: u128 = rng.gen_range(1..u128::MAX >> 8);
        let mut acc = Account::new(pk);
        acc.set_balance(bal);
        smt.put(pk, acc.bytes.to_vec());
        accounts.push((pk, bal));
    }
    (smt, accounts)
}

fn make_header(seq: &KeyPair, root: [u8; 32]) -> SignedHeader {
    let h = Header {
        block_n: 1,
        parent_hash: [0u8; 32],
        prev_state_root: [0u8; 32],
        tx_list_hash: [0u8; 32],
        trace_hash: [0u8; 32],
        new_state_root: root,
        issuer_registry_root: [0u8; 32],
        timestamp_ms: 1,
        sequencer_pubkey: seq.public(),
    };
    SignedHeader::sign(h, seq)
}

#[test]
fn cross_verify_1000_random_balances() {
    let mut rng = StdRng::seed_from_u64(0xc0ffee);
    let seq = KeyPair::from_seed([7u8; 32]);
    let (smt, accounts) = build_state_with_n_accounts(1000, &mut rng);
    let root = smt.root();
    let signed = make_header(&seq, root);

    for (pk, expected_bal) in &accounts {
        let proof = smt.proof(pk);
        let bal = verify_balance(
            [0u8; 32],
            std::slice::from_ref(&signed),
            &seq.public(),
            pk,
            &proof,
        )
        .expect("proof must verify");
        assert_eq!(bal, *expected_bal, "balance mismatch for pubkey {:x?}", pk);
    }
}

#[test]
fn tampered_proof_value_rejected() {
    let mut rng = StdRng::seed_from_u64(11);
    let seq = KeyPair::from_seed([7u8; 32]);
    let (smt, accounts) = build_state_with_n_accounts(50, &mut rng);
    let root = smt.root();
    let signed = make_header(&seq, root);

    for (pk, _) in accounts.iter().take(20) {
        let mut proof = smt.proof(pk);
        // Tamper: bump the balance in the proof's value bytes
        proof.value[32] = proof.value[32].wrapping_add(1);
        let res = verify_balance(
            [0u8; 32],
            std::slice::from_ref(&signed),
            &seq.public(),
            pk,
            &proof,
        );
        assert!(matches!(res, Err(VerifyError::ProofFailed)),
            "tampered proof must be rejected, got {res:?}");
    }
}

#[test]
fn tampered_proof_siblings_rejected() {
    let mut rng = StdRng::seed_from_u64(12);
    let seq = KeyPair::from_seed([7u8; 32]);
    let (smt, accounts) = build_state_with_n_accounts(50, &mut rng);
    let root = smt.root();
    let signed = make_header(&seq, root);

    for (pk, _) in accounts.iter().take(20) {
        let mut proof = smt.proof(pk);
        // Tamper: flip a bit in one sibling
        proof.siblings[100][0] ^= 0x01;
        let res = verify_balance(
            [0u8; 32],
            std::slice::from_ref(&signed),
            &seq.public(),
            pk,
            &proof,
        );
        assert!(matches!(res, Err(VerifyError::ProofFailed)),
            "tampered sibling must be rejected, got {res:?}");
    }
}

#[test]
fn tampered_header_signature_rejected() {
    let mut rng = StdRng::seed_from_u64(13);
    let seq = KeyPair::from_seed([7u8; 32]);
    let (smt, accounts) = build_state_with_n_accounts(10, &mut rng);
    let root = smt.root();
    let mut signed = make_header(&seq, root);
    // Tamper: flip a bit in the signature
    signed.signature[0] ^= 0xff;

    let (pk, _) = accounts[0];
    let proof = smt.proof(&pk);
    let res = verify_balance(
        [0u8; 32],
        std::slice::from_ref(&signed),
        &seq.public(),
        &pk,
        &proof,
    );
    assert!(matches!(res, Err(VerifyError::InvalidSignature(_))),
        "bad sig must be rejected, got {res:?}");
}

#[test]
fn tampered_header_root_rejected() {
    let mut rng = StdRng::seed_from_u64(14);
    let seq = KeyPair::from_seed([7u8; 32]);
    let (smt, accounts) = build_state_with_n_accounts(10, &mut rng);
    let root = smt.root();
    let mut signed = make_header(&seq, root);
    // Tamper: change new_state_root after signing — sig won't match, but
    // even before we get to sig check, a forged root means the proof
    // (which is for the real root) won't verify. The signature check
    // fires first because signing_bytes includes the root.
    signed.header.new_state_root[0] ^= 0xff;

    let (pk, _) = accounts[0];
    let proof = smt.proof(&pk);
    let res = verify_balance(
        [0u8; 32],
        std::slice::from_ref(&signed),
        &seq.public(),
        &pk,
        &proof,
    );
    assert!(res.is_err(),
        "header tampering must be rejected (sig mismatch or proof failure), got {res:?}");
}

#[test]
fn wrong_signer_rejected() {
    let mut rng = StdRng::seed_from_u64(15);
    let seq = KeyPair::from_seed([7u8; 32]);
    let imposter_pk = KeyPair::from_seed([99u8; 32]).public();
    let (smt, accounts) = build_state_with_n_accounts(10, &mut rng);
    let root = smt.root();
    let signed = make_header(&seq, root);

    let (pk, _) = accounts[0];
    let proof = smt.proof(&pk);
    let res = verify_balance(
        [0u8; 32],
        std::slice::from_ref(&signed),
        &imposter_pk, // expected != actual signer
        &pk,
        &proof,
    );
    assert!(matches!(res, Err(VerifyError::InvalidSignature(_))),
        "wrong-signer expectation must be rejected, got {res:?}");
}

#[test]
fn out_of_order_header_chain_rejected() {
    // Two headers where the second's parent_hash points at something
    // other than the first's header_hash. Light client must catch this.
    let mut rng = StdRng::seed_from_u64(16);
    let seq = KeyPair::from_seed([7u8; 32]);
    let (smt, accounts) = build_state_with_n_accounts(10, &mut rng);
    let root = smt.root();

    let h1 = Header {
        block_n: 1,
        parent_hash: [0u8; 32],
        prev_state_root: [0u8; 32],
        tx_list_hash: [0u8; 32],
        trace_hash: [0u8; 32],
        new_state_root: root,
        issuer_registry_root: [0u8; 32],
        timestamp_ms: 1,
        sequencer_pubkey: seq.public(),
    };
    let signed1 = SignedHeader::sign(h1, &seq);

    let mut h2 = Header {
        block_n: 2,
        parent_hash: [0xff; 32], // WRONG — should equal signed1.header_hash()
        prev_state_root: root,
        tx_list_hash: [0u8; 32],
        trace_hash: [0u8; 32],
        new_state_root: root,
        issuer_registry_root: [0u8; 32],
        timestamp_ms: 2,
        sequencer_pubkey: seq.public(),
    };
    h2.parent_hash = [0xff; 32];
    let signed2 = SignedHeader::sign(h2, &seq);

    let (pk, _) = accounts[0];
    let proof = smt.proof(&pk);
    let res = verify_balance(
        [0u8; 32],
        &[signed1, signed2],
        &seq.public(),
        &pk,
        &proof,
    );
    assert!(matches!(res, Err(VerifyError::HeaderChainBroken(_))),
        "broken chain must be rejected, got {res:?}");
}
