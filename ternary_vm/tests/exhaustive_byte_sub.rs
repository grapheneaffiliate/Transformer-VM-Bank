//! Exhaustive verification of `byte_sub_with_borrow` — all
//! 256 × 256 × 2 = 131,072 input combinations.

use psl_ternary_vm::primitives::byte_sub_with_borrow::{build, run};

fn ground_truth(m: u8, s: u8, b: u8) -> (u8, u8) {
    let d = m as i32 - s as i32 - b as i32;
    if d < 0 {
        ((d + 256) as u8, 1)
    } else {
        (d as u8, 0)
    }
}

#[test]
fn byte_sub_exhaustive_131072() {
    let net = build();
    let mut pass = 0u32;
    let mut fail = 0u32;
    let mut first_fail: Option<(u8, u8, u8, (u8, u8), (u8, u8))> = None;

    for m in 0u8..=255 {
        for s in 0u8..=255 {
            for b in 0u8..=1 {
                let got = run(&net, m, s, b).expect("run failed");
                let want = ground_truth(m, s, b);
                if got == want {
                    pass += 1;
                } else if first_fail.is_none() {
                    fail += 1;
                    first_fail = Some((m, s, b, got, want));
                } else {
                    fail += 1;
                }
            }
        }
    }
    assert_eq!(pass, 131_072, "first fail: {first_fail:?}");
    assert_eq!(fail, 0);
}
