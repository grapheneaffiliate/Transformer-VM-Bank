/* transfer_sub_binary: u128 sub via raw-byte I/O.
 *
 * Wire format: each input/output value is ONE wire byte (no decimal-ASCII
 * parsing). Sequencer-side spec writer uses tokens directly for each byte.
 *
 * Input layout (33 raw bytes):
 *   [0]:    success
 *   [1..17): from_balance (16 LE bytes of u128)
 *   [17..33): amount (16 LE bytes of u128)
 *
 * Output layout (16 raw bytes):
 *   [0..16): new_from_balance (zeros if success=0)
 *
 * Trace estimate: 33 reads + 16 sub ops + 16 putchars = ~65 ops, ~3k tokens.
 */

#include "common.h"

void compute(const char *input) {
    int success = (int)(unsigned char)input[0];

    static char from_balance[16];
    static char amount_buf[16];

    int i = 0;
    int safety = 0;
    while (i < 16 && safety < 50) {
        from_balance[i] = input[1 + i];
        i = i + 1;
        safety = safety + 1;
    }
    i = 0;
    safety = 0;
    while (i < 16 && safety < 50) {
        amount_buf[i] = input[17 + i];
        i = i + 1;
        safety = safety + 1;
    }

    if (success) {
        u128_sub_inplace(from_balance, amount_buf);
    } else {
        i = 0;
        while (i < 16) { from_balance[i] = 0; i = i + 1; }
    }

    /* Raw-byte output: one putchar per byte. */
    int j = 0;
    int safety2 = 0;
    while (j < 16 && safety2 < 50) {
        putchar((int)(unsigned char)from_balance[j]);
        j = j + 1;
        safety2 = safety2 + 1;
    }
}
