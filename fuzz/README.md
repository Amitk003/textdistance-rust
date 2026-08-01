# Differential fuzz

A harness that runs the original `textdistance` and this port on identical random inputs and
asserts identical outputs.

Layout:

- `harness.py`          the differential harness (generates inputs, runs both sides, diffs).
- `reference_worker.py` reference side, runs in a subprocess with the original on sys.path.
- `log.txt`             output of the latest continuous run (mode, duration, divergence count).
- `divergences.txt`     details of any divergences from the latest run.
- `corpus/`             any saved seeds or interesting inputs for reproducibility.

How the comparison works:

- The original runs in a separate subprocess (via `reference_worker.py`) so both sides can be
  imported without a module clash; the port lives in the parent process.
- Inputs: random text, unicode, varying `qval`, `as_set`, and list and tuple sequences.
- Outputs compared: `distance`, `similarity`, `normalized_distance`,
  `normalized_similarity`, `maximum`, plus exception behavior (same inputs raise or not).
- Every batch is serialized to JSON and deserialized identically on both sides, so the port
  and the reference always see byte-identical inputs (JSON collapses `()` and `[]`, so the
  port must not keep the pre-serialization objects).
- Float comparison uses a documented tolerance (1e-9); exact-bit equality is the target and
  reprs are compared first, so any near miss within tolerance is reported, never hidden.

Run:

```
.venv/Scripts/python fuzz/harness.py --duration 60
.venv/Scripts/python fuzz/harness.py --duration 60 --long
```
