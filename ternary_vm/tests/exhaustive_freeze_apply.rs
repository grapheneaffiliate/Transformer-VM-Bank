//! Exhaustive verification of `freeze_apply` — all
//! 2 × 256 = 512 input combinations.

use psl_ternary_vm::primitives::freeze_apply::{build, run};

fn ground_truth(flag: u8, byte47: u8) -> u8 {
    let low7 = byte47 & 0x7f;
    if flag != 0 {
        low7 | 0x80
    } else {
        low7
    }
}

#[test]
fn freeze_apply_exhaustive_512() {
    let net = build();
    let mut pass = 0u32;
    let mut fail = 0u32;
    let mut first_fail: Option<(u8, u8, u8, u8)> = None;
    for flag in 0u8..=1 {
        for byte47 in 0u8..=255 {
            let got = run(&net, flag, byte47).unwrap();
            let want = ground_truth(flag, byte47);
            if got == want {
                pass += 1;
            } else {
                fail += 1;
                if first_fail.is_none() {
                    first_fail = Some((flag, byte47, got, want));
                }
            }
        }
    }
    assert_eq!(pass, 512, "first fail: {first_fail:?}");
    assert_eq!(fail, 0);
}
