PY ?= .venv/bin/python

.PHONY: build test fuzz bench cli clean

build:
	./build.sh

cli:
	cargo build --release -p tdc

test:
	$(PY) -m pytest -m "not external" tests/original

fuzz:
	$(PY) fuzz/harness.py

bench:
	$(PY) bench/run.py

clean:
	rm -rf target .venv build dist
