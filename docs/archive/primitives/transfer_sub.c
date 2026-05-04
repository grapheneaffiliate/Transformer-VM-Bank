/* transfer_sub: compute new_from_balance = from_balance - amount (if success).
 *
 * Input wire (33 decimals): success(1) from_balance[16] amount[16]
 * Output wire (16 decimals): new_from_balance[16]   (zeros if success=0)
 */

#include "common.h"

static char from_balance[16];
static char amount_buf[16];

void compute(const char *input) {
    const char *p = input;

    /* Parse success */
    while (*p == ' ') p = p + 1;
    int success = 0;
    while (*p >= '0' && *p <= '9') {
        int d = *p - '0';
        int t2 = success + success;
        int t4 = t2 + t2;
        int t8 = t4 + t4;
        success = t8 + t2 + d;
        p = p + 1;
    }

    /* Parse from_balance */
    int i = 0;
    int safety = 0;
    while (i < 16 && safety < 100) {
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
        from_balance[i] = (char)v;
        i = i + 1;
        safety = safety + 1;
    }

    /* Parse amount */
    i = 0;
    safety = 0;
    while (i < 16 && safety < 100) {
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
        amount_buf[i] = (char)v;
        i = i + 1;
        safety = safety + 1;
    }

    if (success) {
        u128_sub_inplace(from_balance, amount_buf);
    } else {
        i = 0;
        while (i < 16) { from_balance[i] = 0; i = i + 1; }
    }

    int j = 0;
    int safety2 = 0;
    int first = 1;
    while (j < 16 && safety2 < 100) {
        int v = (int)(unsigned char)from_balance[j];
        if (first) { printf("%d", v); first = 0; }
        else { printf(" %d", v); }
        j = j + 1;
        safety2 = safety2 + 1;
    }
    printf("\n");
}
