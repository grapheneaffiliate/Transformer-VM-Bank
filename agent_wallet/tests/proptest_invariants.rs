//! Property tests for the wallet layer.
//!
//! These tests run a configurable number of randomized cases (default
//! 256 per test) and assert the load-bearing invariants from
//! `docs/SECURITY_REVIEW.md` § 3 hold for every case:
//!
//! - I4 — SLIP-0010 derivation: same seed + path → same key.
//! - I5 — Revocation monotonicity: shuffled inserts converge to the
//!   same final set; revoked stays revoked.
//! - I6 — Conservative spending: no admit sequence exceeds cap.

use ed25519_dalek::SigningKey;
use proptest::prelude::*;
use psl_agent_wallet::{
    revocation::{Revocation, RevocationSet},
    slip10::Ed25519MasterKey,
    KeyPolicy, SpendingTracker,
};
use rand::SeedableRng;

const HARDENED_OFFSET: u32 = 0x80000000;

fn arb_seed_32() -> impl Strategy<Value = [u8; 32]> {
    proptest::collection::vec(any::<u8>(), 32..=32).prop_map(|v| {
        let mut a = [0u8; 32];
        a.copy_from_slice(&v);
        a
    })
}

fn arb_hardened_index() -> impl Strategy<Value = u32> {
    (0u32..0x40000000).prop_map(|n| HARDENED_OFFSET | n)
}

fn arb_path(max_depth: usize) -> impl Strategy<Value = Vec<u32>> {
    proptest::collection::vec(arb_hardened_index(), 0..=max_depth)
}

proptest! {
    /// I4: same seed + same derivation path → same private + chain bytes.
    #[test]
    fn slip10_path_is_deterministic(seed in arb_seed_32(), path in arb_path(8)) {
        let m1 = Ed25519MasterKey::from_seed(&seed).unwrap();
        let m2 = Ed25519MasterKey::from_seed(&seed).unwrap();
        // Both descents independently
        let mut a_pubkey: [u8; 32] = m1.public_key().to_bytes();
        let mut b_pubkey: [u8; 32] = m2.public_key().to_bytes();
        prop_assert_eq!(a_pubkey, b_pubkey);
        // Walk the path
        if path.is_empty() {
            return Ok(());
        }
        let mut a_child = m1.derive_child(path[0]).unwrap();
        let mut b_child = m2.derive_child(path[0]).unwrap();
        for &idx in &path[1..] {
            a_child = a_child.derive_child(idx).unwrap();
            b_child = b_child.derive_child(idx).unwrap();
        }
        a_pubkey = a_child.public_key().to_bytes();
        b_pubkey = b_child.public_key().to_bytes();
        prop_assert_eq!(a_pubkey, b_pubkey);
    }

    /// I4 — sibling keys (same parent, different index) differ in public key.
    #[test]
    fn slip10_siblings_have_distinct_pubkeys(
        seed in arb_seed_32(),
        i1 in arb_hardened_index(),
        i2 in arb_hardened_index(),
    ) {
        prop_assume!(i1 != i2);
        let m = Ed25519MasterKey::from_seed(&seed).unwrap();
        let c1 = m.derive_child(i1).unwrap();
        let c2 = m.derive_child(i2).unwrap();
        prop_assert_ne!(c1.public_key().to_bytes(), c2.public_key().to_bytes());
    }
}

// ── Revocation monotonicity (I5) ─────────────────────────────────────

fn sk_from_seed(seed: u64) -> SigningKey {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    SigningKey::generate(&mut rng)
}

proptest! {
    /// I5: shuffled inserts converge to the same revocation set.
    /// Insert N revocations in two random orders; the final
    /// is_revoked predicate must agree on every key.
    #[test]
    fn revocation_set_converges_under_shuffle(
        n in 1usize..32,
        seed_a in any::<u64>(),
        seed_b in any::<u64>(),
    ) {
        let parent = sk_from_seed(0xfeed);
        let revocations: Vec<Revocation> = (0..n)
            .map(|i| {
                let child = sk_from_seed(0x1000 + i as u64);
                Revocation::sign(
                    &parent,
                    child.verifying_key().to_bytes(),
                    [(i & 0xff) as u8; 32],
                    1000 + i as u64,
                )
            })
            .collect();
        // Two shuffles via different seeds
        use rand::seq::SliceRandom;
        let mut order_a: Vec<usize> = (0..n).collect();
        let mut order_b = order_a.clone();
        let mut rng_a = rand::rngs::StdRng::seed_from_u64(seed_a);
        let mut rng_b = rand::rngs::StdRng::seed_from_u64(seed_b);
        order_a.shuffle(&mut rng_a);
        order_b.shuffle(&mut rng_b);

        let mut set_a = RevocationSet::new();
        let mut set_b = RevocationSet::new();
        for i in &order_a { set_a.insert(revocations[*i].clone()).unwrap(); }
        for i in &order_b { set_b.insert(revocations[*i].clone()).unwrap(); }

        for r in &revocations {
            prop_assert_eq!(
                set_a.is_revoked(&r.revoked_pubkey),
                set_b.is_revoked(&r.revoked_pubkey),
                "shuffle disagreement on {:?}", r.revoked_pubkey
            );
            prop_assert!(set_a.is_revoked(&r.revoked_pubkey), "key not revoked after insert");
        }
    }

    /// I5 — revoked stays revoked. After insert, no public method
    /// can flip is_revoked back to false.
    #[test]
    fn revoked_stays_revoked(seed in any::<u64>()) {
        let parent = sk_from_seed(0xbeef);
        let child = sk_from_seed(seed);
        let mut set = RevocationSet::new();
        let r = Revocation::sign(&parent, child.verifying_key().to_bytes(), [0u8; 32], 0);
        set.insert(r.clone()).unwrap();
        prop_assert!(set.is_revoked(&r.revoked_pubkey));
        // Re-inserting same hash is a no-op.
        prop_assert!(!set.insert(r.clone()).unwrap());
        prop_assert!(set.is_revoked(&r.revoked_pubkey));
        // Inserting a NEW revocation for the SAME pubkey (different reason) — no-op too.
        let r2 = Revocation::sign(&parent, r.revoked_pubkey, [0xffu8; 32], 9999);
        prop_assert!(!set.insert(r2).unwrap());
        prop_assert!(set.is_revoked(&r.revoked_pubkey));
    }
}

// ── Spending policy is conservative (I6) ─────────────────────────────

fn arb_amount() -> impl Strategy<Value = u128> {
    (0u128..1_000_000_000)
}

proptest! {
    /// I6 — no admit sequence ever pushes total spend within the
    /// active window above cap_per_window. The tracker either errors
    /// or accepts; if it accepts, sum-of-window-spends ≤ cap.
    #[test]
    fn spending_tracker_never_overspends(
        cap in 1u128..10_000,
        window_secs in 60u64..86400,
        spends in proptest::collection::vec((0u64..100, arb_amount()), 0..40),
    ) {
        let parent = sk_from_seed(0xfeed);
        let child = sk_from_seed(0xface);
        let policy = KeyPolicy {
            child_pubkey: child.verifying_key().to_bytes(),
            parent_pubkey: parent.verifying_key().to_bytes(),
            cap_per_window: cap,
            window_secs,
            allowed_contracts: vec![],
            allowed_counterparties: vec![],
            expiry_unix: 0,
            version: 1,
        };
        let cp = sk_from_seed(0xdead).verifying_key().to_bytes();
        let mut tracker = SpendingTracker::new(policy.clone());

        let mut now: u64 = 1_000_000;
        for (delta, amount) in spends {
            now = now.saturating_add(delta);
            let _ = tracker.admit(now, "transfer", &cp, amount);
            // After every admit attempt — pass or fail — current
            // window total must be ≤ cap.
            let total = tracker.current_window_total(now);
            prop_assert!(total <= cap, "tracker admitted total {total} > cap {cap}");
        }
    }
}
