"""Verify tests/original is byte-for-byte the pinned upstream suite.

Reads tests/original/SHA256SUMS.txt and hashes every listed file, failing on
any mismatch or missing file. Runs in CI so the "unmodified upstream tests"
claim cannot silently drift.
"""

import hashlib
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SUITE = os.path.join(ROOT, "tests", "original")
MANIFEST = os.path.join(SUITE, "SHA256SUMS.txt")


def main():
    failures = []
    entries = 0
    with open(MANIFEST, encoding="utf8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            expected, rel = line.split(" ", 1)
            rel = rel.strip()
            entries += 1
            path = os.path.join(SUITE, rel.replace("/", os.sep))
            if not os.path.isfile(path):
                failures.append(f"missing: {rel}")
                continue
            with open(path, "rb") as src:
                actual = hashlib.sha256(src.read()).hexdigest()
            if actual != expected:
                failures.append(f"hash mismatch: {rel}")
    print(f"verified {entries} files from SHA256SUMS.txt")
    if failures:
        for failure in failures:
            print(f"FAIL {failure}")
        sys.exit(1)


if __name__ == "__main__":
    main()
