# Differential fuzz

A harness that runs the original `textdistance` and this port on identical random inputs and
asserts identical outputs.

Layout:

- `harness.py`       the differential harness (generates inputs, runs both sides, diffs).
- `log.txt`          output of the latest continuous run (start, duration, divergence count).
- `corpus/`          any saved seeds or interesting inputs for reproducibility.

How the comparison works:

- The original is installed in a separate reference virtualenv (`reference/.venv`) so both
  sides can be imported in-process under distinct names, or the original is driven via
  subprocess batches when in-process dual import is not possible.
- Inputs: random text, unicode, varying `qval`, `as_set`, and list-of-token inputs.
- Outputs compared: `distance`, `similarity`, `normalized_distance`,
  `normalized_similarity`, `maximum`, plus exception behavior (same inputs raise or not).
- Float comparison uses a documented tolerance (1e-9); exact-bit equality is the target.

Run:

```
.venv/Scripts/python fuzz/harness.py --duration 60
```
