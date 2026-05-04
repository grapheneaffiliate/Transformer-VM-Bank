/* transfer_balance: compute new from_balance and new to_balance.
 *
 * Input wire (49 decimals):
 *   success(1) from_balance[16] to_balance[16] amount[16]
 *
 * Output wire (32 decimals):
 *   new_from_balance[16] new_to_balance[16]
 *
 * If success=0, output is zeros.
 *
 * Estimated trace: parse 49 + u128 sub/add (32 byte ops) + print 32
 *   = ~90 ops, ~45k tokens. Borderline but within envelope.
 */

#include "common.h"

static char from_balance[16];
static char to_balance[16];
static char amount_buf[16];

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
    int success = parse_one(&p);

    int i = 0;
    int safety = 0;
    while (i < 16 && safety < 100) {
        from_balance[i] = (char)parse_one(&p);
        i = i + 1;
        safety = safety + 1;
    }
    i = 0;
    safety = 0;
    while (i < 16 && safety < 100) {
        to_balance[i] = (char)parse_one(&p);
        i = i + 1;
        safety = safety + 1;
    }
    i = 0;
    safety = 0;
    while (i < 16 && safety < 100) {
        amount_buf[i] = (char)parse_one(&p);
        i = i + 1;
        safety = safety + 1;
    }

    if (success) {
        u128_sub_inplace(from_balance, amount_buf);
        u128_add_inplace(to_balance, amount_buf);
    } else {
        i = 0;
        while (i < 16) { from_balance[i] = 0; to_balance[i] = 0; i = i + 1; }
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
    j = 0;
    safety2 = 0;
    while (j < 16 && safety2 < 100) {
        int v = (int)(unsigned char)to_balance[j];
        printf(" %d", v);
        j = j + 1;
        safety2 = safety2 + 1;
    }
    printf("\n");
}
