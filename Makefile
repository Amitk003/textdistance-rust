PY ?= .venv/bin/python

.PHONY: build test fuzz bench clean

build:
	./build.sh

test:
	$(PY) -m pytest -m "not external" tests/original

fuzz:
	$(PY) fuzz/harness.py

bench:
	$(PY) bench/run.py

clean:
	rm -rf target .venv build dist
