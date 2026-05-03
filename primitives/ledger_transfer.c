/* PSL primitive: transfer between two same-asset accounts.
 *
 * Sequencer pre-checks (BEFORE this primitive runs):
 *   - signature on tx is valid for from.pubkey
 *   - tx.nonce == from.nonce + 1
 *   - from.asset_id == to.asset_id == tx.asset_id
 *   - issuer registry contains tx.asset_id
 *
 * This primitive checks (in the trace, transformer-verifiable):
 *   - from is not frozen (FROZEN_MASK bit clear in FLAGS_BYTE)
 *   - from.balance >= amount  (u128 little-endian compare)
 *
 * On success: emits 128 bytes (updated from_account || updated to_account).
 * On failure (frozen or insufficient balance): emits 128 zero bytes.
 *
 * Input format:
 *   "epoch from_byte_0 ... from_byte_63 to_byte_0 ... to_byte_63
 *    amount_byte_0 ... amount_byte_15"
 *
 * epoch is a u32 timestamp written into both accounts' last_active fields.
 *
 * Output format:
 *   "from'_byte_0 ... from'_byte_63 to'_byte_0 ... to'_byte_63"
 */

#include "common.h"

static char g_from[ACCOUNT_BYTES];
static char g_to[ACCOUNT_BYTES];
static char g_amount[BALANCE_BYTES];
static char g_zero[ACCOUNT_BYTES * 2];

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

    int frozen = ((int)(unsigned char)g_from[FLAGS_BYTE]) & FROZEN_MASK;
    int has_balance = u128_geq(g_from + BALANCE_OFFSET, g_amount);

    if (frozen || !has_balance) {
        i = 0;
        idx = 0;
        while (i < ACCOUNT_BYTES * 2) {
            g_zero[idx] = (char)0;
            idx = idx + 1;
            i = i + 1;
        }
        i = 0;
        idx = 0;
        while (i < ACCOUNT_BYTES * 2) {
            int v = (int)(unsigned char)g_zero[idx];
            if (i == 0) printf("%d", v); else printf(" %d", v);
            idx = idx + 1;
            i = i + 1;
        }
        printf("\n");
        return;
    }

    u128_sub_inplace(g_from + BALANCE_OFFSET, g_amount);
    u128_add_inplace(g_to + BALANCE_OFFSET, g_amount);
    u64_inc_inplace(g_from + NONCE_OFFSET);
    u64_set(g_from + LAST_ACTIVE_OFFSET, epoch);
    u64_set(g_to + LAST_ACTIVE_OFFSET, epoch);

    PRINT_ACCOUNT(g_from, 0);
    i = 0;
    idx = 0;
    while (i < ACCOUNT_BYTES) {
        int v = (int)(unsigned char)g_to[idx];
        printf(" %d", v);
        idx = idx + 1;
        i = i + 1;
    }
    printf("\n");
}
