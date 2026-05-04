/* transfer_check: validate from_balance >= amount via MSB-first compare.
 *
 * Binary I/O (raw bytes per wire token).
 *
 * Input layout (32 raw bytes):
 *   [0..16):   from_balance (u128 little-endian)
 *   [16..32):  amount       (u128 little-endian)
 *
 * Output layout (1 raw byte):
 *   ok (1 if from_balance >= amount, else 0)
 *
 * Algorithm: scan from MSB (byte 15) down to LSB (byte 0). On the first
 * byte where they differ, the higher byte's owner wins. If all bytes equal,
 * from_balance == amount → ok = 1 (>= holds).
 *
 * 16 iterations × ~10 ops = ~160 ops, ~800 token trace.
 */

#include "common.h"

void compute(const char *input) {
    int decided = 0;
    int ok = 1;

    int j = 0;
    int safety = 0;
    while (j < 16 && safety < 32) {
        int idx = 15 - j;
        int b = (int)(unsigned char)input[idx];
        int a = (int)(unsigned char)input[16 + idx];
        if (!decided) {
            if (b > a) { ok = 1; decided = 1; }
            else if (b < a) { ok = 0; decided = 1; }
        }
        j = j + 1;
        safety = safety + 1;
    }

    putchar(ok);
}
