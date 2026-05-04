/* transfer_nonce_binary: u64 nonce increment (binary I/O).
 * Input layout (9 raw bytes): success(1) from_nonce[8]
 * Output layout (8 raw bytes): new_nonce (zeros if !success).
 */
#include "common.h"

void compute(const char *input) {
    int success = (int)(unsigned char)input[0];
    static char from_nonce[8];
    int i = 0, safety = 0;
    while (i < 8 && safety < 50) { from_nonce[i] = input[1 + i]; i = i + 1; safety = safety + 1; }
    if (success) {
        u64_inc_inplace(from_nonce);
    } else {
        i = 0;
        while (i < 8) { from_nonce[i] = 0; i = i + 1; }
    }
    int j = 0, safety2 = 0;
    while (j < 8 && safety2 < 50) {
        putchar((int)(unsigned char)from_nonce[j]);
        j = j + 1; safety2 = safety2 + 1;
    }
}
