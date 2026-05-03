"""Reference trace verifier for PSL.

Computes the canonical PSL trace_hash:

    trace_hash := BLAKE3(utf8(" ".join(predicted_tokens)))

This is the third-party verification reference cited by docs/ARCHITECTURE.md
section 0.2. The sequencer's `sequencer/src/trace.rs::hash_trace` must
produce identical hashes.

Usage:
    python tools/verify_trace.py path/to/predicted_trace.txt
    python tools/verify_trace.py path/to/predicted_trace.txt --expected <hex32>
"""

import argparse
import sys
from pathlib import Path

try:
    import blake3 as _blake3
except ImportError:
    print("ERROR: install blake3 (uv add blake3 / pip install blake3)", file=sys.stderr)
    sys.exit(2)


def hash_trace(tokens: list[str]) -> bytes:
    canon = " ".join(tokens)
    return _blake3.blake3(canon.encode("utf-8")).digest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("trace_file", help="Whitespace-separated token list file")
    parser.add_argument("--expected", help="Expected trace_hash in hex (64 chars)")
    args = parser.parse_args()

    path = Path(args.trace_file)
    if not path.is_file():
        print(f"ERROR: {path} not found", file=sys.stderr)
        sys.exit(2)

    tokens = path.read_text().split()
    digest = hash_trace(tokens)
    print(digest.hex())

    if args.expected:
        if digest.hex().lower() == args.expected.lower():
            print(f"[verify_trace] MATCH ({len(tokens)} tokens)", file=sys.stderr)
            sys.exit(0)
        else:
            print(
                f"[verify_trace] MISMATCH: got {digest.hex()}, expected {args.expected}",
                file=sys.stderr,
            )
            sys.exit(1)


if __name__ == "__main__":
    main()
