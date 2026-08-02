# Differential fuzz

A harness that runs the original `textdistance` and this port on identical random inputs and
asserts identical outputs. Every algorithm exported by the port is covered, including `bag`
and `lzma_ncd` (the latter is excluded from the upstream test suite as too slow, so the
fuzz is its only behavioral proof).

Layout:

- `harness.py`          the differential harness (generates inputs, runs both sides, diffs).
- `reference_worker.py` reference side, runs in a subprocess with the original on sys.path.
- `log-std.txt`         summary of the latest standard run (short strings).
- `log-long.txt`        summary of the latest long run (deeper strings, lists, tuples).
- `divergences-std.txt` details of any divergences from the standard run.
- `divergences-long.txt` details of any divergences from the long run.
- `corpus/`             any saved seeds or interesting inputs for reproducibility.

The reference checkout at `reference/textdistance` is gitignored (it is the upstream
original, not part of the port). Restore it at the pinned commit with
`scripts/fetch_reference.ps1` (or `.sh`).

How the comparison works:

- The original runs in a separate subprocess (via `reference_worker.py`) so both sides can be
  imported without a module clash; the port lives in the parent process.
- Inputs: random text, unicode, varying `qval` (including n-gram edge cases), `as_set`,
  list and tuple sequences, numeric and mixed-element sequences, and lone-surrogate strings
  across every family. Every family processes Python code points (edit, sequence, simple,
  phonetic, and the pure compression coders; see DECISIONS D20 and D21), so a lone
  surrogate is drawn into any algorithm; the binary compressors (bz2/zlib/lzma) encode to
  UTF-8 first and raise `UnicodeEncodeError` on both sides, which the harness treats as an
  exact match.
- Outputs compared: `distance`, `similarity`, `normalized_distance`,
  `normalized_similarity`, `maximum`, plus exception behavior (same inputs raise or not).
- Every batch is serialized to JSON and deserialized identically on both sides, so the port
  and the reference always see byte-identical inputs (JSON collapses `()` and `[]`, so the
  port must not keep the pre-serialization objects).
- Float comparison uses a documented tolerance (1e-9); exact-bit equality is the target and
  reprs are compared first, so any near miss within tolerance is reported, never hidden.

Run:

```
.venv/Scripts/python fuzz/harness.py --duration 75
.venv/Scripts/python fuzz/harness.py --duration 65 --long
.venv/Scripts/python fuzz/harness.py --duration 25 --no-logs   # CI smoke (no log writes)
```
