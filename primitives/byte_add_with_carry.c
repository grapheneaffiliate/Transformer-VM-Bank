/* byte_add_with_carry: one byte of u128 addition.
 *
 * Input layout (3 raw bytes):
 *   [0]: augend
 *   [1]: addend
 *   [2]: carry_in (0 or 1)
 *
 * Output layout (2 raw bytes):
 *   [0]: result_byte    (= augend + addend + carry_in mod 256)
 *   [1]: carry_out      (1 if augend + addend + carry_in >= 256, else 0)
 *
 * Sequencer chains 16 invocations LSB→MSB threading carry_out→carry_in.
 */

#include "common.h"

void compute(const char *input) {
    int augend   = (int)(unsigned char)input[0];
    int addend   = (int)(unsigned char)input[1];
    int carry_in = (int)(unsigned char)input[2];

    int sum = augend + addend + carry_in;
    int carry_out = 0;
    if (sum >= 256) {
        sum = sum - 256;
        carry_out = 1;
    }

    putchar(sum);
    putchar(carry_out);
}
