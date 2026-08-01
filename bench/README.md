# Benchmarks

Original vs port on a shared workload, measured honestly.

- `worker.py`       the measurement protocol (run once per implementation, in a fresh process).
- `run.py`          the harness: builds the workload, runs both sides, prints the table.
- `methodology.md`  how each number was produced (hardware, workload, warmup, repetition).
- `results.json`    the measured numbers (throughput, p99 latency, RSS, startup).

No number is published here without the script and methodology that produced it. Performance
is secondary to behavior; if the port is slower on a case, that is reported too.

Run:

```
.venv/Scripts/python bench/run.py
```
