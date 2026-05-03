/*
 * Common utilities for PSL ledger primitives.
 *
 * PSL accounts: 64-byte fixed records. Witnesses are encoded as
 * space-separated decimal bytes — same wire format as ARC primitives,
 * different semantic interpretation.
 *
 * ═══════════════════════════════════════════════════════════════
 * v2 STYLE GUIDE (inherited verbatim from Transformer-VM)
 * ═══════════════════════════════════════════════════════════════
 *
 * 1. EXTRA ARGS FIRST in input format.
 *    Good: "flag_value byte0 byte1 ..."
 *    Good: "n_transfers tx0_byte0 tx0_byte1 ..."
 *
 * 2. NO mul_var — use sequential byte addressing.
 *    Account access uses precomputed offsets (g_aoff[]) where needed,
 *    or sequential idx++ in nested loops.
 *
 * 3. USE printf (noinline) for output, NOT print_int.
 *
 * 4. INLINE parsing manually instead of parse_next_int with pointers.
 *
 * 5. USE sscanf for fixed-count header args.
 *
 * 6. TARGET: keep WASM instruction count under 2,000 per primitive.
 *
 * 7. FLATTEN deep loops — max 2-3 nesting levels.
 *
 * Account record layout (64 bytes):
 *   [0..32)   pubkey         (32 bytes, ed25519)
 *   [32..48)  balance        (16 bytes, u128 little-endian)
 *   [48..56)  nonce          (8 bytes,  u64 little-endian)
 *   [56..64)  last_active    (8 bytes,  u64 epoch little-endian)
 *   ... wait, that's only 64. Let me recheck.
 *   [0..32)   pubkey         (32 bytes)
 *   [32..48)  balance        (16 bytes)
 *   [48..56)  nonce          (8  bytes)
 *   [56..64)  last_active    (8  bytes)
 *   = 64 bytes total. asset_id and flags carried separately by transactions
 *   for v1; v2 may extend to 96 bytes when those become per-account fields.
 *
 * For v1: asset_id is implicit per-account (one account = one asset_id)
 * and flags live in the high bits of balance — bit 127 = frozen.
 * This simplifies the v1 wire format and stays under instruction budget.
 *
 * ═══════════════════════════════════════════════════════════════
 */

#ifndef PSL_COMMON_H
#define PSL_COMMON_H

#define ACCOUNT_BYTES   64
#define BALANCE_OFFSET  32
#define BALANCE_BYTES   16
#define NONCE_OFFSET    48
#define NONCE_BYTES     8
#define LAST_ACTIVE_OFFSET  56
#define LAST_ACTIVE_BYTES   8
#define FLAGS_BYTE      47  /* high byte of balance = flags; bit 7 = frozen */
#define FROZEN_MASK     0x80

/* ── Parse next byte (decimal, space-separated) into arr[idx] ─────── */
#define PARSE_NEXT_BYTE(p, arr, idx) do { \
    while (*(p) == ' ') (p) = (p) + 1; \
    int _v = 0; \
    while (*(p) >= '0' && *(p) <= '9') { \
        int _d = *(p) - '0'; \
        int _t2 = _v + _v; int _t4 = _t2 + _t2; int _t8 = _t4 + _t4; \
        _v = _t8 + _t2 + _d; \
        (p) = (p) + 1; \
    } \
    (arr)[(idx)] = (char)_v; \
    (idx) = (idx) + 1; \
} while(0)

/* ── Skip N space-separated decimal numbers in input ──────────────── */
#define SKIP_DECIMALS(p, n) do { \
    int _skip = 0; \
    while (_skip < (n)) { \
        while (*(p) == ' ') (p) = (p) + 1; \
        while (*(p) >= '0' && *(p) <= '9') (p) = (p) + 1; \
        _skip = _skip + 1; \
    } \
} while(0)

/* ── Read 64 bytes (one account) from input into arr starting at base ─ */
#define PARSE_ACCOUNT(p, arr, base) do { \
    int _i = 0; \
    int _idx = (base); \
    while (_i < ACCOUNT_BYTES) { \
        PARSE_NEXT_BYTE(p, arr, _idx); \
        _i = _i + 1; \
    } \
} while(0)

/* ── Print 64 bytes (one account) as space-separated decimals ─────── */
#define PRINT_ACCOUNT(arr, base) do { \
    int _i = 0; \
    int _idx = (base); \
    while (_i < ACCOUNT_BYTES) { \
        int _val = (int)(unsigned char)(arr)[_idx]; \
        if (_i == 0) { \
            printf("%d", _val); \
        } else { \
            printf(" %d", _val); \
        } \
        _idx = _idx + 1; \
        _i = _i + 1; \
    } \
} while(0)

/* ── Build account-base-offset table (replaces mul_var for multi-account) ─
 * Call once if a primitive operates on more than 2 accounts:
 *     build_account_offsets(g_aoff, n_accounts);
 * Then access account i's bytes at arr[g_aoff[i] + j].
 */
__attribute__((noinline, optnone))
static void build_account_offsets(int *aoff, int n) {
    int off = 0;
    int i = 0;
    while (i <= n) {
        aoff[i] = off;
        off = off + ACCOUNT_BYTES;
        i = i + 1;
    }
}

/* ── u128 add: dst[0..16) += src[0..16) (little-endian) ────────────
 * Returns final carry (0 or 1). Wrapping if carry remains.
 */
__attribute__((noinline, optnone))
static int u128_add_inplace(char *dst, const char *src) {
    int carry = 0;
    int i = 0;
    while (i < BALANCE_BYTES) {
        int a = (int)(unsigned char)dst[i];
        int b = (int)(unsigned char)src[i];
        int sum = a + b + carry;
        if (sum >= 256) { carry = 1; sum = sum - 256; } else { carry = 0; }
        dst[i] = (char)sum;
        i = i + 1;
    }
    return carry;
}

/* ── u128 subtract: dst[0..16) -= src[0..16) (little-endian) ──────
 * Returns final borrow (0 or 1). Caller must verify dst >= src first.
 */
__attribute__((noinline, optnone))
static int u128_sub_inplace(char *dst, const char *src) {
    int borrow = 0;
    int i = 0;
    while (i < BALANCE_BYTES) {
        int a = (int)(unsigned char)dst[i];
        int b = (int)(unsigned char)src[i];
        int diff = a - b - borrow;
        if (diff < 0) { borrow = 1; diff = diff + 256; } else { borrow = 0; }
        dst[i] = (char)diff;
        i = i + 1;
    }
    return borrow;
}

/* ── u128 compare: returns 1 if a >= b, else 0 (little-endian) ──── */
__attribute__((noinline, optnone))
static int u128_geq(const char *a, const char *b) {
    int i = BALANCE_BYTES - 1;
    while (i >= 0) {
        int av = (int)(unsigned char)a[i];
        int bv = (int)(unsigned char)b[i];
        if (av > bv) return 1;
        if (av < bv) return 0;
        i = i - 1;
    }
    return 1;  /* equal */
}

/* ── u64 increment: bytes[0..8) += 1 (little-endian) ──────────────── */
__attribute__((noinline, optnone))
static void u64_inc_inplace(char *bytes) {
    int carry = 1;
    int i = 0;
    while (i < NONCE_BYTES && carry) {
        int v = (int)(unsigned char)bytes[i] + carry;
        if (v >= 256) { v = v - 256; carry = 1; } else { carry = 0; }
        bytes[i] = (char)v;
        i = i + 1;
    }
}

/* ── Set u64 bytes[0..8) from a u32 epoch value ─────────────────── */
__attribute__((noinline, optnone))
static void u64_set(char *bytes, int epoch) {
    bytes[0] = (char)(epoch & 0xff);
    bytes[1] = (char)((epoch >> 8) & 0xff);
    bytes[2] = (char)((epoch >> 16) & 0xff);
    bytes[3] = (char)((epoch >> 24) & 0xff);
    bytes[4] = (char)0;
    bytes[5] = (char)0;
    bytes[6] = (char)0;
    bytes[7] = (char)0;
}

#endif /* PSL_COMMON_H */
