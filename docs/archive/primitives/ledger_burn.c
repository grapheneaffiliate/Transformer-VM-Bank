/* PSL primitive: burn asset from a single account.
 *
 * Sequencer pre-checks:
 *   - tx is signed by issuer_registry[asset_id].authority_pubkey
 *   - issuer_registry[asset_id].burn_enabled
 *   - from.asset_id matches the issuer's asset_id
 *
 * Trace-checked: from.balance >= amount.
 *
 * Input format:
 *   "epoch from_byte_0 ... from_byte_63 amount_byte_0 ... amount_byte_15"
 *
 * Output: 64-byte updated account (insufficient balance → 64 zero bytes).
 */

#include "common.h"

static char g_from[ACCOUNT_BYTES];
static char g_amount[BALANCE_BYTES];

void compute(const char *input) {
    int epoch = 0;
    sscanf(input, "%d", &epoch);

    const char *p = input;
    SKIP_DECIMALS(p, 1);

    int i = 0;
    int idx = 0;
    while (i < ACCOUNT_BYTES) {
        PARSE_NEXT_BYTE(p, g_from, idx);
        i = i + 1;
    }
    i = 0;
    idx = 0;
    while (i < BALANCE_BYTES) {
        PARSE_NEXT_BYTE(p, g_amount, idx);
        i = i + 1;
    }

    int has_balance = u128_geq(g_from + BALANCE_OFFSET, g_amount);

    if (!has_balance) {
        i = 0;
        idx = 0;
        while (i < ACCOUNT_BYTES) {
            g_from[idx] = (char)0;
            idx = idx + 1;
            i = i + 1;
        }
    } else {
        u128_sub_inplace(g_from + BALANCE_OFFSET, g_amount);
        u64_set(g_from + LAST_ACTIVE_OFFSET, epoch);
    }

    PRINT_ACCOUNT(g_from, 0);
    printf("\n");
}
