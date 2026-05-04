/* transfer_add_binary: u128 add (binary I/O).
 * Input layout (33 raw bytes): success(1) to_balance[16] amount[16]
 * Output layout (16 raw bytes): new_to_balance (zeros if !success).
 */
#include "common.h"

void compute(const char *input) {
    int success = (int)(unsigned char)input[0];
    static char to_balance[16];
    static char amount_buf[16];
    int i = 0, safety = 0;
    while (i < 16 && safety < 50) { to_balance[i] = input[1 + i]; i = i + 1; safety = safety + 1; }
    i = 0; safety = 0;
    while (i < 16 && safety < 50) { amount_buf[i] = input[17 + i]; i = i + 1; safety = safety + 1; }
    if (success) {
        u128_add_inplace(to_balance, amount_buf);
    } else {
        i = 0;
        while (i < 16) { to_balance[i] = 0; i = i + 1; }
    }
    int j = 0, safety2 = 0;
    while (j < 16 && safety2 < 50) {
        putchar((int)(unsigned char)to_balance[j]);
        j = j + 1; safety2 = safety2 + 1;
    }
}
