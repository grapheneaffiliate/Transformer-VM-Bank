/* PSL primitive: freeze / unfreeze an account.
 *
 * Sets or clears the frozen-flag bit on an account record. Authorization
 * (issuer authority + court_order_hash) is verified by the sequencer
 * BEFORE this primitive is invoked.
 *
 * Input format:  "flag_value byte0 byte1 ... byte63"
 *   flag_value: 1 to freeze, 0 to unfreeze.
 *   byte0..byte63: 64-byte account_t record (decimal bytes).
 *
 * Output format: "byte0 byte1 ... byte63"
 *   Updated account_t with bit 7 of byte 47 (FLAGS_BYTE) set or cleared.
 *
 * v2-style: args-first, sequential addressing, printf output, no mul_var.
 */

#include "common.h"

static char g_account[ACCOUNT_BYTES];

void compute(const char *input) {
    int flag = 0;
    sscanf(input, "%d", &flag);

    const char *p = input;
    SKIP_DECIMALS(p, 1);

    int idx = 0;
    int i = 0;
    while (i < ACCOUNT_BYTES) {
        PARSE_NEXT_BYTE(p, g_account, idx);
        i = i + 1;
    }

    int cur = (int)(unsigned char)g_account[FLAGS_BYTE];
    if (flag) {
        cur = cur | FROZEN_MASK;
    } else {
        cur = cur & (255 - FROZEN_MASK);
    }
    g_account[FLAGS_BYTE] = (char)cur;

    PRINT_ACCOUNT(g_account, 0);
    printf("\n");
}
