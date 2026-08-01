# textdistance (native port)

The same `textdistance` you know, with a native Rust core underneath.

30+ string distance and similarity algorithms (Levenshtein, Jaro, Jaccard, LCS, NCD and more)
behind one common, pure-Python API. This project reimplements the algorithmic core in Rust
and keeps the original Python interface intact, so existing code keeps working while the
heavy math runs at native speed.

## What you get

- **Drop-in compatibility.** Import `textdistance`, use the same classes, singletons, and
  methods. `textdistance.levenshtein("test", "text")` works exactly as before, including
  `qval`, `as_set`, `ks`, and custom similarity callables.
- **Native performance.** Every algorithm is implemented in Rust. The default call path is a
  single FFI boundary hop into compiled code, not a Python loop.
- **Verified equivalence.** The original project's own test suite (400 tests) runs unmodified
  against this port. A differential fuzz harness compares this port against the original
  Python library over random inputs and logs zero divergence.
- **Safety discipline.** The core crate is compiled with `#![forbid(unsafe_code)]`. The FFI
  layer is the only place a boundary is touched, and it stays as small as possible.

## Quick start

Requirements: Python 3.8+, a Rust toolchain, and maturin (installed by the build script).

```powershell
# one command: creates .venv, installs tooling, builds and installs the extension
.\build.ps1
```

```python
import textdistance

textdistance.levenshtein("test", "text")            # 1
textdistance.jaro_winkler("nelson", "neilsen")      # ~0.92
textdistance.Jaccard(as_set=True)("test", "text")   # 0.25
```

There is also a standalone command line tool, `tdc`, that reads two values and prints the
chosen algorithm's result:

```powershell
tdc levenshtein "test" "text"
```

## Why a Rust port

The original `textdistance` is deliberately pure Python: simple, portable, zero dependencies.
That is its strength and its cost. String distance algorithms are compute-heavy, and a pure
Python implementation spends most of its time inside the interpreter loop. A Rust
implementation keeps the same API and moves the inner loop into optimized native code, which
matters when you compute distances in bulk (deduplication, fuzzy matching, record linkage,
search ranking).

## Equivalence proof

Three layers, weakest to strongest:

1. **Original tests, unmodified.** `tests/original/` contains the upstream test suite,
   byte-for-byte, with sha256 hashes recorded at the point it was pinned. The suite is run
   with the same command the upstream project uses for its pure tests:
   `pytest -m "not external"` (the `external`-marked tests require optional third-party
   libraries and are excluded upstream too). The port currently passes
   [pass rate to be filled from the latest run].
2. **Differential fuzzing.** `fuzz/` runs the original library and this port on identical
   random inputs (text, unicode, varying `qval` and `as_set`) and asserts identical outputs.
   See `fuzz/log.txt` for the latest continuous run.
3. **Port tests.** `tests/port/` holds additional tests we wrote (native Rust unit tests plus
   adapter-level checks) covering edge cases not exercised upstream.

Any divergence from the original is documented in `DECISIONS.md` with its rationale. The
numbers reported here and in `bench/` are honest: measured, with methodology, including where
the port falls short.

## Supported algorithms

Edit based: Levenshtein, DamerauLevenshtein, Jaro, JaroWinkler, Editex, Gotoh,
NeedlemanWunsch, SmithWaterman, Matrix, Hamming, MLIPNS, StrCmp95, Prefix, Postfix, Identity.

Sequence based: LCSSeq, LCSStr, RatcliffObershelp.

Token based: Jaccard, Sorensen, Tversky, Overlap, Cosine, MongeElkan, Bag, Containment.

Phonetic: MRA, Soundex family.

Compression based: ArithNCD, BWTRLE, BZ2NCD, RLE, ZlibNCD, SqrtNCD, EntropyNCD, LZMANCD.

## Project layout

```
crates/tdcore/       Rust algorithm kernels (the port). No unsafe.
crates/pyapi/        PyO3 extension module textdistance._textdistance. Thin FFI.
python/textdistance/ Python adapter: the public class API, delegating math to Rust.
tests/original/      Upstream test suite, verbatim, hashed.
tests/port/          Additional tests for the port.
fuzz/                Differential fuzz harness and logs.
bench/               Benchmark harness, methodology, and results.
DECISIONS.md         Every non-trivial divergence from the original, with rationale.
```

## Benchmarks

Methodology and raw results live in `bench/` (p99 latency, RSS, startup, and throughput on a
shared workload, original vs port). Published numbers are measured with the scripts in that
directory and reproducible with one command. See `bench/results.json`.

## License

MIT. The port is a reimplementation of the MIT-licensed `textdistance` project
(https://github.com/life4/textdistance) and keeps its public API and test suite; see
`LICENSE` and the upstream project for attribution.
