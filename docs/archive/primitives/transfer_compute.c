/* transfer_compute: take the 61-byte slice from transfer_parse, do the
 * u128 arithmetic and frozen/balance check, output the new balance bytes
 * and new nonce.
 *
 * Input wire (61 decimals):
 *   epoch_lo(1) frozen(1) from_balance[16] to_balance[16] amount[16] from_nonce[8] reserved(3)
 *
 * Output wire (41 decimals):
 *   success(1) new_from_balance[16] new_to_balance[16] new_from_nonce[8]
 *
 * On failure (frozen or insufficient balance): success=0, balances and
 * nonce zeroed. The sequencer detects success and applies the MPT delta
 * accordingly.
 */

#include "common.h"

static char from_balance[16];
static char to_balance[16];
static char amount_buf[16];
static char from_nonce[8];

__attribute__((noinline))
static int parse_one(const char **pp) {
    const char *p = *pp;
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
    *pp = p;
    return v;
}

void compute(const char *input) {
    const char *p = input;

    int epoch_lo = parse_one(&p);
    int frozen = parse_one(&p);

    int i = 0;
    int safety = 0;
    while (i < 16 && safety < 100) {
        int v = parse_one(&p);
        from_balance[i] = (char)v;
        i = i + 1;
        safety = safety + 1;
    }
    i = 0;
    safety = 0;
    while (i < 16 && safety < 100) {
        int v = parse_one(&p);
        to_balance[i] = (char)v;
        i = i + 1;
        safety = safety + 1;
    }
    i = 0;
    safety = 0;
    while (i < 16 && safety < 100) {
        int v = parse_one(&p);
        amount_buf[i] = (char)v;
        i = i + 1;
        safety = safety + 1;
    }
    i = 0;
    safety = 0;
    while (i < 8 && safety < 100) {
        int v = parse_one(&p);
        from_nonce[i] = (char)v;
        i = i + 1;
        safety = safety + 1;
    }

    int has_balance = u128_geq(from_balance, amount_buf);
    int success = (frozen == 0 && has_balance != 0) ? 1 : 0;

    if (success) {
        u128_sub_inplace(from_balance, amount_buf);
        u128_add_inplace(to_balance, amount_buf);
        u64_inc_inplace(from_nonce);
    } else {
        i = 0;
        while (i < 16) { from_balance[i] = 0; to_balance[i] = 0; i = i + 1; }
        i = 0;
        while (i < 8) { from_nonce[i] = 0; i = i + 1; }
    }

    /* Suppress unused-var warning for epoch_lo (sequencer uses epoch separately). */
    (void)epoch_lo;

    printf("%d", success);
    int j = 0;
    safety = 0;
    while (j < 16 && safety < 100) {
        int v = (int)(unsigned char)from_balance[j];
        printf(" %d", v);
        j = j + 1;
        safety = safety + 1;
    }
    j = 0;
    safety = 0;
    while (j < 16 && safety < 100) {
        int v = (int)(unsigned char)to_balance[j];
        printf(" %d", v);
        j = j + 1;
        safety = safety + 1;
    }
    j = 0;
    safety = 0;
    while (j < 8 && safety < 100) {
        int v = (int)(unsigned char)from_nonce[j];
        printf(" %d", v);
        j = j + 1;
        safety = safety + 1;
    }
    printf("\n");
}
