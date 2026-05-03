/* PSL primitive: serialize account deltas for the MPT layer.
 *
 * Takes N (≤8) (account_index, account_record) pairs and emits a
 * canonical byte stream the native MPT layer hashes and applies. The
 * primitive does NO hashing — it sorts and serializes.
 *
 * The point: the transformer trace covers the *ordering and serialization*
 * of deltas. The actual hashing is native (BLAKE3, see crypto/mpt.rs) and
 * is out of the trace.
 *
 * Input format:
 *   "n_pairs idx_0 byte_0_0 ... byte_0_63 idx_1 byte_1_0 ... byte_1_63 ..."
 *
 * Output format (sorted ascending by idx):
 *   "idx_0 byte_0_0 ... byte_0_63 idx_1 byte_1_0 ... byte_1_63 ..."
 *
 * Output is the canonical-ordered serialization. Native code hashes from this.
 */

#include "common.h"

#define MAX_PAIRS 8

static int g_indices[MAX_PAIRS];
static char g_records[MAX_PAIRS * ACCOUNT_BYTES];
static int g_order[MAX_PAIRS];
static int g_aoff[MAX_PAIRS + 1];

void compute(const char *input) {
    int n = 0;
    sscanf(input, "%d", &n);
    if (n < 0) n = 0;
    if (n > MAX_PAIRS) n = MAX_PAIRS;

    build_account_offsets(g_aoff, MAX_PAIRS);

    const char *p = input;
    SKIP_DECIMALS(p, 1);

    int i = 0;
    int safety = 0;
    while (i < n && safety < 16) {
        int idx_val = 0;
        while (*p == ' ') p = p + 1;
        while (*p >= '0' && *p <= '9') {
            int d = *p - '0';
            int t2 = idx_val + idx_val;
            int t4 = t2 + t2;
            int t8 = t4 + t4;
            idx_val = t8 + t2 + d;
            p = p + 1;
        }
        g_indices[i] = idx_val;
        g_order[i] = i;

        int rec_idx = g_aoff[i];
        int j = 0;
        while (j < ACCOUNT_BYTES) {
            PARSE_NEXT_BYTE(p, g_records, rec_idx);
            j = j + 1;
        }

        i = i + 1;
        safety = safety + 1;
    }

    /* Selection sort g_order by g_indices ascending — n ≤ 8 so O(n^2) fine */
    int a = 0;
    while (a < n) {
        int b = a + 1;
        while (b < n) {
            if (g_indices[g_order[a]] > g_indices[g_order[b]]) {
                int tmp = g_order[a];
                g_order[a] = g_order[b];
                g_order[b] = tmp;
            }
            b = b + 1;
        }
        a = a + 1;
    }

    int first = 1;
    int k = 0;
    while (k < n) {
        int slot = g_order[k];
        if (first) {
            printf("%d", g_indices[slot]);
            first = 0;
        } else {
            printf(" %d", g_indices[slot]);
        }
        int rec_idx = g_aoff[slot];
        int j = 0;
        while (j < ACCOUNT_BYTES) {
            int v = (int)(unsigned char)g_records[rec_idx];
            printf(" %d", v);
            rec_idx = rec_idx + 1;
            j = j + 1;
        }
        k = k + 1;
    }
    printf("\n");
}
