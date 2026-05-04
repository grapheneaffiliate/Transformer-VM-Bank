/* transfer_finalize: increment nonce by 1.
 *
 * Input layout (8 raw bytes):
 *   [0..8): nonce_in (u64 little-endian)
 *
 * Output layout (8 raw bytes):
 *   nonce_in + 1 (mod 2^64), little-endian
 *
 * The sequencer sets last_active independently from the block timestamp
 * (system-clock data; not part of the verifiable trace), so this primitive
 * only handles the nonce increment.
 *
 * Trace estimate: 8 reads + 8 increments-with-carry + 8 writes = ~50 ops,
 * ~250 token trace.
 */

#include "common.h"

void compute(const char *input) {
    static char nonce[8];
    int i = 0;
    int safety = 0;
    while (i < 8 && safety < 16) {
        nonce[i] = input[i];
        i = i + 1;
        safety = safety + 1;
    }

    int carry = 1;
    int j = 0;
    int safety2 = 0;
    while (j < 8 && safety2 < 16) {
        int v = (int)(unsigned char)nonce[j] + carry;
        if (v >= 256) {
            v = v - 256;
            carry = 1;
        } else {
            carry = 0;
        }
        nonce[j] = (char)v;
        j = j + 1;
        safety2 = safety2 + 1;
    }

    int k = 0;
    int safety3 = 0;
    while (k < 8 && safety3 < 16) {
        putchar((int)(unsigned char)nonce[k]);
        k = k + 1;
        safety3 = safety3 + 1;
    }
}
