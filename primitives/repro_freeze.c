/* Repro: full freeze logic. Should match ledger_freeze.c. */

#include "common.h"

static char buf[64];

void compute(const char *input) {
    int flag = 0;
    sscanf(input, "%d", &flag);

    const char *p = input;
    while (*p == ' ') p = p + 1;
    while (*p >= '0' && *p <= '9') p = p + 1;

    int idx = 0;
    int safety = 0;
    while (idx < 64 && safety < 200) {
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
        buf[idx] = (char)v;
        idx = idx + 1;
        safety = safety + 1;
    }

    int cur = (int)(unsigned char)buf[47];
    volatile int low7 = cur & 127;
    volatile int freeze_bit = 0;
    if (flag) {
        freeze_bit = 128;
    }
    int sum = (int)low7 + (int)freeze_bit;
    buf[47] = (char)sum;

    int j = 0;
    int safety2 = 0;
    while (j < 64 && safety2 < 200) {
        int val = (int)(unsigned char)buf[j];
        if (j == 0) {
            printf("%d", val);
        } else {
            printf(" %d", val);
        }
        j = j + 1;
        safety2 = safety2 + 1;
    }
    printf("\n");
}
