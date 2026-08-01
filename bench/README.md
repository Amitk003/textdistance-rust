# Benchmarks

Original vs port on a shared workload, measured honestly.

- `methodology.md`   how each number was produced (hardware, workload, warmup, repetition).
- `run.py`           the harness.
- `results.json`     the measured numbers (throughput, p99 latency, RSS, startup).

No number is published here without the script and methodology that produced it. Performance
is secondary to behavior; if the port is slower on a case, that is reported too.

Run:

```
.venv/Scripts/python bench/run.py
```
