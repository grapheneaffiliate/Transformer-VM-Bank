/* byte_sub_with_borrow: one byte of u128 subtraction.
 *
 * Input layout (3 raw bytes): minuend, subtrahend, borrow_in
 * Output layout (2 raw bytes): result_byte, borrow_out
 *
 * Mirror byte_add_with_carry's pattern so clang lowers to cheap
 * sub + lt_u + select (instead of shr_u-based sign extraction which
 * triggers Transformer-VM's expensive byte-shift expansion).
 */

#include "common.h"

void compute(const char *input) {
    int minuend     = (int)(unsigned char)input[0];
    int subtrahend  = (int)(unsigned char)input[1];
    int borrow_in   = (int)(unsigned char)input[2];

    int diff_plus  = minuend + 256 - subtrahend - borrow_in;  /* always non-negative */
    int borrow_out = (diff_plus < 256) ? 1 : 0;
    int result     = borrow_out ? diff_plus : (diff_plus - 256);

    putchar(result);
    putchar(borrow_out);
}
