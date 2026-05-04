/* Minimal repro: write known pattern, print. No parsing. */

#include "common.h"

static char buf[64];

void compute(const char *input) {
    /* Write a pattern: buf[i] = i+1 (so we can spot duplicates / zeros) */
    int i = 0;
    int safety = 0;
    while (i < 64 && safety < 200) {
        buf[i] = (char)(i + 1);
        i = i + 1;
        safety = safety + 1;
    }

    /* Print all 64 bytes */
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
