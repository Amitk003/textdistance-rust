#!/usr/bin/env bash
# Restore the pinned reference checkout used by fuzz/ and bench/.
# reference/ is gitignored (it is the upstream original, not part of the port),
# so this script re-creates it at the exact commit the port was proven against.
# See .port-mortem.toml and tests/port/test_surface.py for the pin.
set -euo pipefail

COMMIT="d6a68d61088a40eef5c88191ccf79323dbf34850"
TARGET="reference/textdistance"

if [ -d "$TARGET" ]; then
    echo "$TARGET already present; leaving it alone."
    exit 0
fi

git clone https://github.com/life4/textdistance.git "$TARGET"
git -C "$TARGET" checkout "$COMMIT"
echo "Reference pinned at $COMMIT."
