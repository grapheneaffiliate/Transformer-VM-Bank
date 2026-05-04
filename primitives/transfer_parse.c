/* transfer_parse: parse the full 145-byte transfer witness, extract a
 * compact slice for transfer_compute. The point: keep this primitive's
 * trace under the precision envelope (~200k tokens).
 *
 * Witness wire (145 decimals):
 *   epoch from[64] to[64] amount[16]
 *
 * Output wire (61 decimals):
 *   epoch_lo(1) frozen(1) from_balance[16] to_balance[16] amount[16] from_nonce[8] reserved(3)
 *
 * The sequencer feeds this output into transfer_compute. transfer_parse
 * does NO arithmetic — just parsing and slice extraction.
 */

#include "common.h"

static char from_acc[64];
static char to_acc[64];
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

    /* epoch (we keep only the low byte for v1; epoch fits in u32) */
    int epoch = parse_one(&p);

    /* from_account: 64 bytes */
    int i = 0;
    int safety = 0;
    while (i < 64 && safety < 200) {
        int v = parse_one(&p);
        from_acc[i] = (char)v;
        i = i + 1;
        safety = safety + 1;
    }

    /* to_account: 64 bytes */
    i = 0;
    safety = 0;
    while (i < 64 && safety < 200) {
        int v = parse_one(&p);
        to_acc[i] = (char)v;
        i = i + 1;
        safety = safety + 1;
    }

    /* amount: 16 bytes */
    i = 0;
    safety = 0;
    while (i < 16 && safety < 100) {
        int v = parse_one(&p);
        amount_buf[i] = (char)v;
        i = i + 1;
        safety = safety + 1;
    }

    /* Print compact slice for transfer_compute.
     * Layout:
     *   epoch_lo(1) frozen(1) from_balance[16] to_balance[16] amount[16] from_nonce[8] reserved(3) = 61 bytes
     */
    int epoch_lo = epoch & 255;
    int frozen = (int)(unsigned char)from_acc[FLAGS_BYTE] & FROZEN_MASK ? 1 : 0;

    printf("%d %d", epoch_lo, frozen);
    int j = 0;
    safety = 0;
    while (j < BALANCE_BYTES && safety < 100) {
        int v = (int)(unsigned char)from_acc[BALANCE_OFFSET + j];
        printf(" %d", v);
        j = j + 1;
        safety = safety + 1;
    }
    j = 0;
    safety = 0;
    while (j < BALANCE_BYTES && safety < 100) {
        int v = (int)(unsigned char)to_acc[BALANCE_OFFSET + j];
        printf(" %d", v);
        j = j + 1;
        safety = safety + 1;
    }
    j = 0;
    safety = 0;
    while (j < BALANCE_BYTES && safety < 100) {
        int v = (int)(unsigned char)amount_buf[j];
        printf(" %d", v);
        j = j + 1;
        safety = safety + 1;
    }
    j = 0;
    safety = 0;
    while (j < NONCE_BYTES && safety < 100) {
        int v = (int)(unsigned char)from_acc[NONCE_OFFSET + j];
        printf(" %d", v);
        j = j + 1;
        safety = safety + 1;
    }
    /* reserved bytes */
    printf(" 0 0 0\n");
}
