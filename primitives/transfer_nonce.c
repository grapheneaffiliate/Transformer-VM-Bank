/* transfer_nonce: increment the from-account's nonce on successful transfer.
 *
 * Input wire (9 decimals):
 *   success(1) from_nonce[8]
 *
 * Output wire (8 decimals):
 *   new_from_nonce[8]   (nonce + 1 if success, else zeros)
 *
 * Estimated trace: parse 9 + inc + print 8 = ~17 ops, ~8k tokens.
 */

#include "common.h"

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
    int success = parse_one(&p);

    int i = 0;
    int safety = 0;
    while (i < 8 && safety < 100) {
        from_nonce[i] = (char)parse_one(&p);
        i = i + 1;
        safety = safety + 1;
    }

    if (success) {
        u64_inc_inplace(from_nonce);
    } else {
        i = 0;
        while (i < 8) { from_nonce[i] = 0; i = i + 1; }
    }

    int j = 0;
    int safety2 = 0;
    int first = 1;
    while (j < 8 && safety2 < 100) {
        int v = (int)(unsigned char)from_nonce[j];
        if (first) { printf("%d", v); first = 0; }
        else { printf(" %d", v); }
        j = j + 1;
        safety2 = safety2 + 1;
    }
    printf("\n");
}
