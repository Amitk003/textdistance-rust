#!/usr/bin/env bash
set -euo pipefail

PY=${PY:-python3}
"$PY" -m venv .venv
.venv/bin/pip install --quiet --upgrade pip
.venv/bin/pip install --quiet "maturin>=1.4,<2.0" pytest hypothesis
.venv/bin/maturin develop --release

echo "Extension built and installed into .venv."
echo "Run the original suite with:"
echo "  .venv/bin/python -m pytest -m 'not external' tests/original"
