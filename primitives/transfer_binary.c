/* transfer_binary: single-primitive transfer with raw-byte I/O.
 *
 * Sequencer-side spec writer encodes each input byte as ONE wire token.
 * The C primitive reads bytes directly (no decimal-ASCII parsing) and
 * writes results via putchar.
 *
 * Input layout (57 raw bytes):
 *   [0]:        success_in (always 1; sequencer pre-validated frozen+balance)
 *   [1..17):    from_balance[16]
 *   [17..33):   to_balance[16]
 *   [33..49):   amount[16]
 *   [49..57):   from_nonce[8]
 *
 * Output layout (41 raw bytes):
 *   [0]:        success_out (1 if balance >= amount, else 0)
 *   [1..17):    new_from_balance (zeros if !success)
 *   [17..33):   new_to_balance   (zeros if !success)
 *   [33..41):   new_from_nonce   (zeros if !success)
 *
 * Note: frozen check is sequencer-side (native). This primitive trusts the
 * sequencer's `success_in` and additionally re-validates balance >= amount.
 */

#include "common.h"

static char from_balance[16];
static char to_balance[16];
static char amount_buf[16];
static char from_nonce[8];

void compute(const char *input) {
    int success_in = (int)(unsigned char)input[0];

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
        to_balance[i] = input[17 + i];
        i = i + 1;
        safety = safety + 1;
    }
    i = 0;
    safety = 0;
    while (i < 16 && safety < 50) {
        amount_buf[i] = input[33 + i];
        i = i + 1;
        safety = safety + 1;
    }
    i = 0;
    safety = 0;
    while (i < 8 && safety < 50) {
        from_nonce[i] = input[49 + i];
        i = i + 1;
        safety = safety + 1;
    }

    int has_balance = u128_geq(from_balance, amount_buf);
    int success_out = (success_in != 0 && has_balance != 0) ? 1 : 0;

    if (success_out) {
        u128_sub_inplace(from_balance, amount_buf);
        u128_add_inplace(to_balance, amount_buf);
        u64_inc_inplace(from_nonce);
    } else {
        i = 0;
        while (i < 16) { from_balance[i] = 0; to_balance[i] = 0; i = i + 1; }
        i = 0;
        while (i < 8) { from_nonce[i] = 0; i = i + 1; }
    }

    putchar(success_out);

    int j = 0;
    int safety2 = 0;
    while (j < 16 && safety2 < 50) {
        putchar((int)(unsigned char)from_balance[j]);
        j = j + 1;
        safety2 = safety2 + 1;
    }
    j = 0;
    safety2 = 0;
    while (j < 16 && safety2 < 50) {
        putchar((int)(unsigned char)to_balance[j]);
        j = j + 1;
        safety2 = safety2 + 1;
    }
    j = 0;
    safety2 = 0;
    while (j < 8 && safety2 < 50) {
        putchar((int)(unsigned char)from_nonce[j]);
        j = j + 1;
        safety2 = safety2 + 1;
    }
}
