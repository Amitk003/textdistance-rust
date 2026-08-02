# Latent bug documented: upstream `gotoh` crashes on single-empty input

**Upstream:** `life4/textdistance` (reference pinned at `d6a68d6`), `textdistance.gotoh`.

**Status:** reproduced faithfully by this port; filed upstream (filing text in `DECISIONS.md` D24).

## The bug

`Gotoh` raises `IndexError` when exactly **one** of the two inputs is empty:

```
>>> textdistance.gotoh('', 'x')
IndexError: index 1 is out of bounds for axis 0 with size 1

>>> textdistance.gotoh('x', '')
IndexError: index 1 is out of bounds for axis 0 with size 1

>>> textdistance.gotoh('', '')   # both empty: fine
0.0
```

The port reproduces this byte-for-byte:

```
$ .venv\Scripts\python.exe -c "import textdistance as td; td.gotoh('', 'x')"
IndexError: index 1 is out of bounds for axis 0 with size 1
```

## Root cause

In upstream `Gotoh.__call__` (`edit_based.py`) the DP matrices are built as
`numpy.zeros((len_s1 + 1, len_s2 + 1))`. When `len_s1 == 0` the matrix has a
single row, but the column-initialization loop writes `p_mat[1, j]` for every
`j` in `1..len_s2`, indexing row 1 of a one-row matrix. Symmetrically, when
`len_s2 == 0` the row loop writes `q_mat[i, 1]` on a one-column matrix.
Both-empty never runs either loop and returns `0.0`.

## Why it is latent

- It is hidden in the original's own code and test suite: upstream never passes
  exactly one empty string, so no upstream test reaches the crashing path.
- It was hidden in this port as well: the original differential fuzz harness
  value-compared only a fraction of the surface (it passed `as_set` to all 37
  algorithms and 28 reject it, so those cases died at construction). Once the
  harness was repaired (DECISIONS.md D23), the first genuinely-divergent input
  surfaced this defect. A real bug revealed by fixing our tooling.

## Why the port reproduces it instead of fixing it

The project's contract is behavioral parity: porting means preserving the
original's behavior, not correcting it (DECISIONS.md D21). A port that
silently "fixed" this would be *less* faithful. Reproducing it is pinned by
`tests/port/test_surface.py::test_gotoh_single_empty_matches_upstream_crash`.

## Cross-references

- DECISIONS.md D21 — "Bug Catcher outcome": the earlier 4.69M-case hunt found
  no defensible bug; the repaired harness then surfaced this one.
- DECISIONS.md D23 — harness repair and the parity bugs it surfaced (this
  defect, LZMA preset/length, StrCmp95, Hamming, BWTRLENCD).
- DECISIONS.md D24 — upstream filing text (`life4/textdistance`), ready to
  submit.
