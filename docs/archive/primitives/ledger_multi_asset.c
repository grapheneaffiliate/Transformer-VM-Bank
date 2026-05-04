/* PSL primitive: batched transfer of N (≤4) same-format payloads.
 *
 * Each payload is a single transfer (from || to || amount). Payloads are
 * processed sequentially. This primitive is for *throughput* — it batches
 * 4 transfers per trace invocation, amortizing the PyO3 / runner-call cost.
 *
 * Each payload is *independent* — failure of one (frozen or insufficient
 * balance) does not affect the others. Failed payloads emit 128 zero bytes
 * for their (from', to') slot; successful ones emit the updated bytes.
 *
 * Input format:
 *   "epoch n_payloads p0_from[64] p0_to[64] p0_amount[16] p1_from[64] ..."
 *
 * Output format:
 *   "p0_from'[64] p0_to'[64] p1_from'[64] p1_to'[64] ..."
 *   (n_payloads * 128 bytes total)
 *
 * Constraints (v2 style guide):
 *   - Outer loop bounded by n_payloads ≤ 4 with safety counter.
 *   - Per-payload work duplicates ledger_transfer logic; no nested helper
 *     so dependency chains stay shallow.
 */

#include "common.h"

#define MAX_PAYLOADS 4
#define PAYLOAD_SIZE (ACCOUNT_BYTES * 2 + BALANCE_BYTES)

static char g_buf[MAX_PAYLOADS * (ACCOUNT_BYTES * 2)];
static char g_from[ACCOUNT_BYTES];
static char g_to[ACCOUNT_BYTES];
static char g_amount[BALANCE_BYTES];

void compute(const char *input) {
    int epoch = 0;
    int n = 0;
    sscanf(input, "%d %d", &epoch, &n);
    if (n < 0) n = 0;
    if (n > MAX_PAYLOADS) n = MAX_PAYLOADS;

    const char *p = input;
    SKIP_DECIMALS(p, 2);

    int out_idx = 0;
    int payload = 0;
    int safety = 0;
    while (payload < n && safety < 16) {
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

        if (!frozen && has_balance) {
            u128_sub_inplace(g_from + BALANCE_OFFSET, g_amount);
            u128_add_inplace(g_to + BALANCE_OFFSET, g_amount);
            u64_inc_inplace(g_from + NONCE_OFFSET);
            u64_set(g_from + LAST_ACTIVE_OFFSET, epoch);
            u64_set(g_to + LAST_ACTIVE_OFFSET, epoch);
        } else {
            i = 0;
            while (i < ACCOUNT_BYTES) {
                g_from[i] = (char)0;
                g_to[i] = (char)0;
                i = i + 1;
            }
        }

        i = 0;
        while (i < ACCOUNT_BYTES) {
            g_buf[out_idx] = g_from[i];
            out_idx = out_idx + 1;
            i = i + 1;
        }
        i = 0;
        while (i < ACCOUNT_BYTES) {
            g_buf[out_idx] = g_to[i];
            out_idx = out_idx + 1;
            i = i + 1;
        }

        payload = payload + 1;
        safety = safety + 1;
    }

    int i = 0;
    while (i < out_idx) {
        int v = (int)(unsigned char)g_buf[i];
        if (i == 0) printf("%d", v); else printf(" %d", v);
        i = i + 1;
    }
    printf("\n");
}
