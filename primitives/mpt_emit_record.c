/* mpt_emit_record: pass-through serialization of one canonical account record.
 *
 * The sequencer extracts and orders account deltas natively (sorting + asset
 * registry lookups are not transformer-trace work). This primitive's role:
 * verify that the canonical-form serialization of one 64-byte record passes
 * through unchanged — i.e., the transformer trace commits to the bytes that
 * will be hashed into the MPT.
 *
 * Input layout (64 raw bytes): one account record (canonical order).
 * Output layout (64 raw bytes): same bytes, identity transform.
 *
 * Per-record. The sequencer chains N invocations for N delta records and
 * commits one trace_hash per record into the block header. This is more
 * granular than the original "single-call multi-record" design and keeps
 * each trace under the precision envelope.
 *
 * Trace estimate: 64 reads + 64 putchars = ~130 ops, ~6.5k token trace.
 */

#include "common.h"

void compute(const char *input) {
    int i = 0;
    int safety = 0;
    while (i < 64 && safety < 128) {
        putchar((int)(unsigned char)input[i]);
        i = i + 1;
        safety = safety + 1;
    }
}
