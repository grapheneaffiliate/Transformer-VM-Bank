/* PSL primitive: mint asset to a single account.
 *
 * Sequencer pre-checks:
 *   - tx is signed by issuer_registry[asset_id].authority_pubkey
 *   - issuer_registry[asset_id].mint_enabled
 *   - to.asset_id matches the issuer's asset_id
 *
 * Trace-checked: balance overflow guarded by carry from u128_add_inplace.
 *
 * Input format:
 *   "epoch to_byte_0 ... to_byte_63 amount_byte_0 ... amount_byte_15"
 *
 * Output format on success: 64-byte updated account.
 * Output format on overflow: 64 zero bytes.
 */

#include "common.h"

static char g_to[ACCOUNT_BYTES];
static char g_amount[BALANCE_BYTES];

void compute(const char *input) {
    int epoch = 0;
    sscanf(input, "%d", &epoch);

    const char *p = input;
    SKIP_DECIMALS(p, 1);

    int i = 0;
    int idx = 0;
    while (i < ACCOUNT_BYTES) {
        PARSE_NEXT_BYTE(p, g_to, idx);
        i = i + 1;
    }
    i = 0;
    idx = 0;
    while (i < BALANCE_BYTES) {
        PARSE_NEXT_BYTE(p, g_amount, idx);
        i = i + 1;
    }

    int carry = u128_add_inplace(g_to + BALANCE_OFFSET, g_amount);
    if (carry) {
        i = 0;
        idx = 0;
        while (i < ACCOUNT_BYTES) {
            g_to[idx] = (char)0;
            idx = idx + 1;
            i = i + 1;
        }
    }

    u64_set(g_to + LAST_ACTIVE_OFFSET, epoch);

    PRINT_ACCOUNT(g_to, 0);
    printf("\n");
}
