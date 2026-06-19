"""Generate randomized witness vectors for PSL primitive bit-exact testing.

Each primitive consumes a different witness shape. This script writes per-primitive
JSON files under tests/vectors/ that the bit-exact harness consumes.

Usage:
    python tools/gen_vectors.py --count 10000 --seed 1
    python tools/gen_vectors.py --count 100 --seed 1 --quick    # smoke
"""

import argparse
import json
import random
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
VECTORS_DIR = REPO_ROOT / "tests" / "vectors"

ACCOUNT_BYTES = 64
BALANCE_OFFSET = 32
BALANCE_BYTES = 16
NONCE_OFFSET = 48
NONCE_BYTES = 8
LAST_ACTIVE_OFFSET = 56
FLAGS_BYTE = 47
FROZEN_MASK = 0x80


def rand_bytes(rng: random.Random, n: int) -> list[int]:
    return [rng.randint(0, 255) for _ in range(n)]


def rand_account(rng: random.Random, balance_max_bits: int = 120) -> list[int]:
    """Generate a random 64-byte account.

    balance_max_bits caps the balance below 2^120 to leave headroom for u128
    arithmetic without overflow during transfer/mint chains.
    """
    acc = rand_bytes(rng, ACCOUNT_BYTES)
    max_byte = balance_max_bits // 8
    for i in range(max_byte, BALANCE_BYTES):
        acc[BALANCE_OFFSET + i] = 0
    acc[FLAGS_BYTE] &= ~FROZEN_MASK
    return acc


def gen_freeze_vectors(rng: random.Random, count: int) -> list[dict]:
    out = []
    for _ in range(count):
        flag = rng.randint(0, 1)
        acc = rand_account(rng)
        out.append({
            "input": [flag] + acc,
            "kind": "freeze",
        })
    return out


def gen_transfer_vectors(rng: random.Random, count: int) -> list[dict]:
    out = []
    for _ in range(count):
        epoch = rng.randint(0, 0x7FFFFFFF)
        from_acc = rand_account(rng)
        to_acc = rand_account(rng)
        amount = rand_bytes(rng, BALANCE_BYTES)
        for i in range(13, BALANCE_BYTES):
            amount[i] = 0
        # 25% chance of frozen sender
        if rng.random() < 0.25:
            from_acc[FLAGS_BYTE] |= FROZEN_MASK
        # 25% chance of insufficient balance (force amount > balance)
        if rng.random() < 0.25:
            for i in range(BALANCE_BYTES):
                from_acc[BALANCE_OFFSET + i] = 0
            amount[0] = 1
        out.append({
            "input": [epoch] + from_acc + to_acc + amount,
            "kind": "transfer",
        })
    return out


def gen_mint_vectors(rng: random.Random, count: int) -> list[dict]:
    out = []
    for _ in range(count):
        epoch = rng.randint(0, 0x7FFFFFFF)
        to_acc = rand_account(rng)
        amount = rand_bytes(rng, BALANCE_BYTES)
        for i in range(13, BALANCE_BYTES):
            amount[i] = 0
        # 10% chance of overflow scenario
        if rng.random() < 0.1:
            for i in range(BALANCE_BYTES):
                to_acc[BALANCE_OFFSET + i] = 0xFF
            for i in range(BALANCE_BYTES):
                amount[i] = 0xFF
        out.append({
            "input": [epoch] + to_acc + amount,
            "kind": "mint",
        })
    return out


def gen_burn_vectors(rng: random.Random, count: int) -> list[dict]:
    out = []
    for _ in range(count):
        epoch = rng.randint(0, 0x7FFFFFFF)
        from_acc = rand_account(rng)
        amount = rand_bytes(rng, BALANCE_BYTES)
        for i in range(13, BALANCE_BYTES):
            amount[i] = 0
        # 25% chance of insufficient balance
        if rng.random() < 0.25:
            for i in range(BALANCE_BYTES):
                from_acc[BALANCE_OFFSET + i] = 0
            amount[0] = 1
        out.append({
            "input": [epoch] + from_acc + amount,
            "kind": "burn",
        })
    return out


def gen_multi_asset_vectors(rng: random.Random, count: int) -> list[dict]:
    out = []
    MAX_PAYLOADS = 4
    for _ in range(count):
        epoch = rng.randint(0, 0x7FFFFFFF)
        n = rng.randint(1, MAX_PAYLOADS)
        payload = []
        for _ in range(n):
            from_acc = rand_account(rng)
            to_acc = rand_account(rng)
            amount = rand_bytes(rng, BALANCE_BYTES)
            for i in range(13, BALANCE_BYTES):
                amount[i] = 0
            if rng.random() < 0.20:
                from_acc[FLAGS_BYTE] |= FROZEN_MASK
            payload += from_acc + to_acc + amount
        out.append({
            "input": [epoch, n] + payload,
            "kind": "multi_asset",
        })
    return out


def gen_mpt_vectors(rng: random.Random, count: int) -> list[dict]:
    out = []
    MAX_PAIRS = 8
    for _ in range(count):
        n = rng.randint(1, MAX_PAIRS)
        payload = []
        for _ in range(n):
            idx = rng.randint(0, 0x0FFFFFFF)
            payload.append(idx)
            payload += rand_account(rng)
        out.append({
            "input": [n] + payload,
            "kind": "mpt_apply_delta",
        })
    return out


GENERATORS = {
    "ledger_freeze": gen_freeze_vectors,
    "ledger_transfer": gen_transfer_vectors,
    "ledger_mint": gen_mint_vectors,
    "ledger_burn": gen_burn_vectors,
    "ledger_multi_asset": gen_multi_asset_vectors,
    "mpt_apply_delta": gen_mpt_vectors,
}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--count", type=int, default=10_000)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--quick", action="store_true", help="small count for smoke testing")
    parser.add_argument(
        "--primitive",
        choices=list(GENERATORS) + ["all"],
        default="all",
    )
    args = parser.parse_args()

    count = 100 if args.quick else args.count
    VECTORS_DIR.mkdir(parents=True, exist_ok=True)

    targets = list(GENERATORS) if args.primitive == "all" else [args.primitive]
    for name in targets:
        rng = random.Random(args.seed + hash(name) % (2**32))
        vectors = GENERATORS[name](rng, count)
        out_path = VECTORS_DIR / f"{name}.json"
        with out_path.open("w") as f:
            json.dump({"count": len(vectors), "seed": args.seed, "vectors": vectors}, f)
        print(f"[gen_vectors] {name}: {len(vectors)} vectors → {out_path}")


if __name__ == "__main__":
    main()
