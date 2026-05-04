/* transfer_check: validate a transfer can proceed.
 *
 * Sequencer pre-extracts (frozen, from_balance, amount) and feeds to this
 * primitive, which checks frozen and balance. Output: 1-byte success.
 *
 * Input wire (33 decimals):
 *   frozen(1) from_balance[16] amount[16]
 *
 * Output wire (1 decimal):
 *   success
 *
 * Estimated trace: parse 33 + print 1 = ~34 ops, ~17k tokens.
 */

#include "common.h"

static char from_balance[16];
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
    int frozen = parse_one(&p);

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
        amount_buf[i] = (char)parse_one(&p);
        i = i + 1;
        safety = safety + 1;
    }

    int has_balance = u128_geq(from_balance, amount_buf);
    int success = (frozen == 0 && has_balance != 0) ? 1 : 0;

    printf("%d\n", success);
}
