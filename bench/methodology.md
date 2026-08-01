# Benchmark methodology

Every number in `results.json` is produced by `run.py` and `worker.py` on this
machine, on one core, with no other load. Nothing is published here that cannot
be reproduced with `python bench/run.py`. Performance is secondary to behavior:
the port must first pass the original test suite and the differential fuzz
harness; speed is only a consequence.

## What is measured

For each algorithm, both implementations run the identical workload in a
**fresh subprocess** and report:

- **Throughput** (`calls/s`): how many `distance(s1, s2)` calls complete in one
  second. Derived from the total wall time of `N` calls over the corpus,
  including all per-call overhead.
- **Latency** (`p50`, `p99`, `max`, microseconds): per-call wall time sampled
  with `time.perf_counter_ns()`, sorted and read at the 50th, 99th percentile
  and the maximum.
- **Startup** (`import_ms`): wall time of `import textdistance` in a cold
  process, timed around the import itself.
- **Peak RSS** (`rss_mb`): peak working set read via the Windows psapi
  `GetProcessMemoryInfo` API at the end of each algorithm's timing loop. Not
  measured on non-Windows platforms.

## The workload

- **Word pairs** (`bench/run.py`, `WORDS`): 40 hand-written word pairs, 4 to 16
  characters, mixing real misspellings ("kitten"/"sitting", "martha"/"marhta")
  and unrelated words. Each algorithm runs 200 warmup calls then 8,000 timed
  calls, cycling the corpus.
- **Long strings** (`LONG`): 3 pairs of two-sentence strings, roughly 120 to
  200 characters each, with a few words changed. 5 warmup calls then 50 timed
  calls. Only the algorithms that are dominated by the `O(n*m)` dynamic
  programs or the tokenizer run here, because that is where interpreter cost is
  highest.

Algorithms were chosen to cover every family: edit (levenshtein,
damerau_levenshtein, jaro_winkler), sequence (lcsseq, ratcliff_obershelp),
token (jaccard), and compression (arith_ncd, bz2_ncd). The compression pair is
the honest baseline: both sides call the same C compression library, so the
speedup should be near 1x.

## Fairness

- Both sides run the same `worker.py` protocol in a fresh interpreter, so each
  side pays its own import and interpreter startup.
- The original runs with `reference/textdistance` (the pinned upstream clone)
  on `sys.path`; the port runs as the installed package. Both report
  `__version__ == "4.6.2"`; `results.json` records the version for each side.
- Timed calls alternate deterministically over the corpus, so both sides see
  the same inputs in the same order.
- No result is filtered or adjusted. If the port is slower on a case (as with
  bz2_ncd and jaccard on long strings), that number is reported as measured.

## Notes on interpretation

- Short-string edit distances are dominated by per-call overhead (the FFI
  boundary on the port side, the interpreter loop on the original). The
  speedups on 4-16 character words are real but modest.
- The long-string rows show the structural advantage: the quadratic dynamic
  programs (levenshtein, lcsseq) are the original's worst case, and the port
  spends that time in compiled code instead of the interpreter loop.
- RSS is the process working set after importing textdistance and running the
  workload; it includes the interpreter and, on the port side, the loaded Rust
  extension.
- Machine: Windows 11, x86_64, Python 3.11.9, Rust 1.97.1, single process, no
  other load during the run. Numbers vary between machines and runs; treat the
  ratios, not the absolute microseconds, as the signal.

## Reproduce

```
.venv/Scripts/python bench/run.py
```

`run.py` prints the table and overwrites `bench/results.json`.
