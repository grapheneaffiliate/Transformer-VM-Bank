/* freeze_setup: parse witness, extract just (flag, byte_47).
 *
 * Composition rule: freeze is decomposed into two primitives because the
 * full freeze trace (~600k tokens at -O0) exceeds the specialized model's
 * precision envelope. freeze_setup handles the parsing-heavy work; the
 * sequencer chains its output into freeze_apply which does the bit math.
 *
 * Input wire:  "flag b0 b1 ... b63"  (1 flag + 64 account bytes, decimal)
 * Output wire: "flag byte_47"         (2 decimals)
 *
 * Estimated trace: 47 skip-iters + 1 parse + 1 printf = ~15k tokens.
 * Well under the ~200k precision envelope.
 */

#include "common.h"

void compute(const char *input) {
    int flag = 0;
    sscanf(input, "%d", &flag);

    const char *p = input;
    /* Skip flag digits */
    while (*p == ' ') p = p + 1;
    while (*p >= '0' && *p <= '9') p = p + 1;

    /* Skip first 47 account bytes (positions 0..46) */
    int skip = 0;
    int safety = 0;
    while (skip < 47 && safety < 200) {
        while (*p == ' ') p = p + 1;
        while (*p >= '0' && *p <= '9') p = p + 1;
        skip = skip + 1;
        safety = safety + 1;
    }

    /* Parse byte at position 47 */
    while (*p == ' ') p = p + 1;
    int byte47 = 0;
    while (*p >= '0' && *p <= '9') {
        int d = *p - '0';
        int t2 = byte47 + byte47;
        int t4 = t2 + t2;
        int t8 = t4 + t4;
        byte47 = t8 + t2 + d;
        p = p + 1;
    }

    printf("%d %d\n", flag, byte47);
}
