/* transfer_check_binary: validate transfer can proceed (binary I/O).
 * Input layout (33 raw bytes): frozen(1) from_balance[16] amount[16]
 * Output layout (1 raw byte): success
 */
#include "common.h"

void compute(const char *input) {
    int frozen = (int)(unsigned char)input[0];
    static char from_balance[16];
    static char amount_buf[16];
    int i = 0, safety = 0;
    while (i < 16 && safety < 50) { from_balance[i] = input[1 + i]; i = i + 1; safety = safety + 1; }
    i = 0; safety = 0;
    while (i < 16 && safety < 50) { amount_buf[i] = input[17 + i]; i = i + 1; safety = safety + 1; }
    int has_balance = u128_geq(from_balance, amount_buf);
    int success = (frozen == 0 && has_balance != 0) ? 1 : 0;
    putchar(success);
}
