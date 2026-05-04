/* freeze_apply: take (flag, current_byte_47), produce new_byte_47.
 *
 * Composition rule: see freeze_setup.c. This is the second of two
 * primitives that together replace the monolithic ledger_freeze.c.
 *
 * Input wire:  "flag current_byte_47"  (2 decimals)
 * Output wire: "new_byte_47"           (1 decimal)
 *
 * Estimated trace: tiny — under 5k tokens. Comfortably inside the
 * specialized model's precision envelope.
 */

#include "common.h"

void compute(const char *input) {
    int flag = 0;
    int byte47 = 0;
    sscanf(input, "%d %d", &flag, &byte47);

    /* Compute new byte without using bitwise OR.
     * volatile prevents clang from folding `low7 + freeze_bit` back into
     * `low7 | freeze_bit` (Transformer-VM's lower.py:1526 mishandles
     * runtime i32.or as boolean instead of bitwise — see docs/FINDINGS.md). */
    int low7 = byte47 & 127;
    volatile int low7_v = low7;
    volatile int freeze_bit = 0;
    if (flag) {
        freeze_bit = 128;
    }
    int new_byte = (int)low7_v + (int)freeze_bit;

    printf("%d\n", new_byte);
}
