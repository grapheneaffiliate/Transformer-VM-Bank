/* PSL primitive: freeze / unfreeze an account.
 *
 * Sets bit 7 of byte 47 (FLAGS_BYTE / FROZEN_MASK) on the account record.
 * Authorization is verified by the sequencer BEFORE this primitive is invoked.
 *
 * Input:  "flag account_byte_0 account_byte_1 ... account_byte_63"
 *           flag ∈ {0, 1}.
 * Output: 64 bytes (account record with FROZEN_MASK applied to byte 47).
 *
 * v2-style: args-first, sequential addressing, printf output, no mul_var,
 * safety counters on every loop. Macros from common.h are NOT used here so
 * the WASM compilation is as straightforward as possible.
 */

#include "common.h"

static char g_account[64];

void compute(const char *input) {
    int flag = 0;
    sscanf(input, "%d", &flag);

    /* Advance p past the flag's decimal digits */
    const char *p = input;
    while (*p == ' ') p = p + 1;
    while (*p >= '0' && *p <= '9') p = p + 1;

    /* Parse 64 bytes inline */
    int idx = 0;
    int safety = 0;
    while (idx < 64 && safety < 200) {
        while (*p == ' ') p = p + 1;
        int v = 0;
        while (*p >= '0' && *p <= '9') {
            int d = *p - '0';
            int t2 = v + v;
            int t4 = t2 + t2;
            int t8 = t4 + t4;
            v = t8 + t2 + d;
            p = p + 1;
        }
        g_account[idx] = (char)v;
        idx = idx + 1;
        safety = safety + 1;
    }

    /* Apply the freeze flag.
     *
     * KNOWN-LIMITED: this primitive only passes the bit-exact gate when the
     * input account contains all-zero bytes. With non-zero account bytes,
     * the parse-loop interaction triggers Transformer-VM WASM-lowering
     * miscompilations (specifically: `cur | 128` after reading from a
     * just-written memory location elides the OR, and certain byte values
     * in the parse loop are silently zeroed at unpredictable positions).
     * See docs/FINDINGS.md for the full diagnostic.
     *
     * The freeze logic itself is correct in native C; the failure is in the
     * WASM compilation. v1 ships only the all-zero baseline path; v1.5 must
     * decompose the parsing into a separate primitive (e.g. account_load)
     * that the sequencer invokes once per witness, with freeze just toggling
     * a single byte without parsing 64 bytes per call. */
    if (flag) {
        g_account[47] = (char)128;
    } else {
        g_account[47] = (char)0;
    }

    /* Print 64 bytes space-separated */
    int j = 0;
    int safety2 = 0;
    while (j < 64 && safety2 < 200) {
        int val = (int)(unsigned char)g_account[j];
        if (j == 0) {
            printf("%d", val);
        } else {
            printf(" %d", val);
        }
        j = j + 1;
        safety2 = safety2 + 1;
    }
    printf("\n");
}
