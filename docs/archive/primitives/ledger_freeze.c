/* PSL primitive: freeze / unfreeze an account.
 *
 * Sets bit 7 of byte 47 (FLAGS_BYTE / FROZEN_MASK) on the account record.
 * Authorization is verified by the sequencer BEFORE this primitive is invoked.
 *
 * Input:  "flag account_byte_0 account_byte_1 ... account_byte_63"
 *           flag ∈ {0, 1}.
 * Output: 64 bytes (account record with FROZEN_MASK applied to byte 47).
 *
 * v2-style: args-first, sequential addressing, printf output, no mul_var,
 * safety counters on every loop. Macros from common.h are NOT used here so
 * the WASM compilation is as straightforward as possible.
 */

#include "common.h"

static char g_account[64];

void compute(const char *input) {
    int flag = 0;
    sscanf(input, "%d", &flag);

    /* Advance p past the flag's decimal digits */
    const char *p = input;
    while (*p == ' ') p = p + 1;
    while (*p >= '0' && *p <= '9') p = p + 1;

    /* Parse 64 bytes inline */
    int idx = 0;
    int safety = 0;
    while (idx < 64 && safety < 200) {
        while (*p == ' ') p = p + 1;
        int v = 0;
        while (*p >= '0' && *p <= '9') {
            int d = *p - '0';
            int t2 = v + v;
            int t4 = t2 + t2;
            int t8 = t4 + t4;
            v = t8 + t2 + d;
            p = p + 1;
        }
        g_account[idx] = (char)v;
        idx = idx + 1;
        safety = safety + 1;
    }

    /* Apply / clear the freeze bit, preserving low 7 bits of byte 47.
     *
     * Background: Transformer-VM's lower.py only lowers i32.or correctly when
     * an i32.const immediately precedes it (constant-form lowering). The
     * fallback "no preceding const" lowering is BOOLEAN (a|b → b ? 1 : a)
     * which is wrong for full-integer bitwise OR. Any clang -O2
     * transformation that puts a non-const op between i32.const and
     * i32.or (or fuses two branches into a select) triggers the wrong
     * lowering and writes garbage to byte 47.
     *
     * Workaround: compute via addition with volatile intermediates so clang
     * can't fold add → or via the "non-overlapping bit ranges" optimization. */
    int cur = (int)(unsigned char)g_account[47];
    volatile int low7 = cur & 127;
    volatile int freeze_bit = 0;
    if (flag) {
        freeze_bit = 128;
    }
    int sum = (int)low7 + (int)freeze_bit;
    g_account[47] = (char)sum;

    /* Print 64 bytes space-separated */
    int j = 0;
    int safety2 = 0;
    while (j < 64 && safety2 < 200) {
        int val = (int)(unsigned char)g_account[j];
        if (j == 0) {
            printf("%d", val);
        } else {
            printf(" %d", val);
        }
        j = j + 1;
        safety2 = safety2 + 1;
    }
    printf("\n");
}
