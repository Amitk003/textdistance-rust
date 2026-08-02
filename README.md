# textdistance (native port)

[![CI](https://github.com/Amitk003/textdistance-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/Amitk003/textdistance-rust/actions/workflows/ci.yml)

The same `textdistance` you know, with a native Rust core underneath.

30+ string distance and similarity algorithms (Levenshtein, Jaro, Jaccard, LCS, NCD and more)
behind one common, pure-Python API. This project reimplements the algorithmic core in Rust
and keeps the original Python interface intact, so existing code keeps working while the
heavy math runs at native speed.

## What you get

- **Drop-in compatibility.** Import `textdistance`, use the same classes, singletons, and
  methods. `textdistance.levenshtein("test", "text")` works exactly as before, including
  `qval`, `as_set`, `ks`, and custom similarity callables.
- **Native performance.** Every algorithm's core math is implemented in Rust: the edit and
  sequence dynamic programs, the Jaro/StrCmp95/Editex scoring, the n-gram counters, and the
  compression coders all run on the native path. A default call crosses the FFI boundary once
  into a compiled kernel, not a Python loop. A few composition and ratio helpers stay in the
  thin Python adapter exactly as the original structures them (for example MongeElkan's
  word-pair matching and the token similarity ratios), because those are generic glue, not
  heavy math. See DECISIONS D1 and D21.
- **Verified equivalence.** The original project's own test suite (400 tests) runs unmodified
  against this port. A differential fuzz harness compares this port against the original
  Python library over random inputs, covering every exported algorithm (37, including `bag`
  and `lzma_ncd`); the latest continuous runs covered 1.75 million short and 1.37 million
  long cases, including lone-surrogate strings across every family, with zero divergence.
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

There is also a standalone command line tool, `tdc`, with no Python involved. It reads two
values, computes the chosen metric, and prints the result:

```powershell
tdc distance levenshtein "test" "text"          # 1
tdc similarity jaro_winkler "Robert" "Rupert"   # 0.8
tdc list                                        # supported algorithms
```

Build it (or run `.\build.ps1`, which builds it for you):

```powershell
cargo build --release -p tdc   # then use target\release\tdc.exe
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
   libraries and are excluded upstream too). The port passes the full pure suite:
   400 of 400 selected tests, with 30 deselected as `external`.
2. **Differential fuzzing.** `fuzz/` runs the original library and this port on identical
   random inputs (text, unicode, varying `qval` and `as_set`) and asserts identical outputs.
   Every exported algorithm is covered; see `fuzz/log-std.txt` and `fuzz/log-long.txt` for
   the latest continuous runs. The reference checkout (gitignored) is restored at its pinned
   commit with `scripts/fetch_reference.ps1` (or `.sh`).
3. **Port tests.** `tests/port/` holds additional tests we wrote (native Rust unit tests plus
   adapter-level checks) covering edge cases not exercised upstream.

Any divergence from the original is documented in `DECISIONS.md` with its rationale. The
numbers reported here and in `bench/` are honest: measured, with methodology, including where
the port falls short.

### Known boundaries

The port matches the original everywhere the original's own suite and the differential fuzz
exercise it, and a few deliberate boundaries are documented rather than silently accepted:

- **Lone surrogates.** A Python string can contain a lone surrogate (U+D800..DFFF), which is
  not valid Unicode text but is a valid one-element code point. The port processes strings as
  Python code points across every family, so a lone surrogate flows through the edit, sequence,
  simple, and phonetic kernels and the compression coders exactly as the original does, instead
  of raising. The binary compressors (bz2/zlib/lzma) encode to UTF-8 and raise
  `UnicodeEncodeError` on both sides, which matches the original. See DECISIONS D20/D21 and
  `tests/port/test_surrogates*.py`.
- **Unported drafts.** Upstream `vector_based.py` is a draft that needs numpy and is not part
  of the package surface; the external-library optimization registry is not bundled (see
  DECISIONS D17 and D18). Both are deliberately absent, matching the original's behavior in a
  clean environment.

## Supported algorithms

Edit based: Levenshtein, DamerauLevenshtein, Jaro, JaroWinkler, Editex, Gotoh,
NeedlemanWunsch, SmithWaterman, Matrix, Hamming, MLIPNS, StrCmp95, Prefix, Postfix, Identity.

Sequence based: LCSSeq, LCSStr, RatcliffObershelp.

Token based: Jaccard, Sorensen, Tversky, Overlap, Cosine, Tanimoto, MongeElkan, Bag.

Phonetic: MRA, Editex.

Compression based: ArithNCD, RLENCD, BWTRLENCD, SqrtNCD, EntropyNCD, BZ2NCD, ZLIBNCD,
LZMANCD.

## Project layout

```
crates/tdcore/       Rust algorithm kernels (the port). No unsafe.
crates/codec/        C compression bindings (bz2, zlib, lzma), the only crate containing unsafe.
crates/pyapi/        PyO3 extension module textdistance._textdistance. Thin FFI.
crates/tdc/          Standalone command line tool over the kernels.
python/textdistance/ Python adapter: the public class API, delegating math to Rust.
tests/original/      Upstream test suite, verbatim, hashed.
tests/port/          Additional tests for the port.
fuzz/                Differential fuzz harness and logs.
bench/               Benchmark harness, methodology, and results.
scripts/             Reference-fetch helpers (restore the pinned original for fuzz/bench).
DECISIONS.md         Every non-trivial divergence from the original, with rationale.
```

## Benchmarks

Measured on this machine (Windows 11, x86_64, Python 3.11.9, Rust 1.97.1). Both sides run the
identical workload in a fresh process: 40 word pairs, 8,000 timed `distance()` calls per
algorithm after 200 warmup calls. Throughput is calls per second; the full methodology is in
`bench/methodology.md` and the raw numbers in `bench/results.json`.

| algorithm             | port      | original  | speedup |
| --------------------- | --------- | --------- | ------- |
| levenshtein           | 225,575   | 37,441    | 6x      |
| damerau_levenshtein   | 272,012   | 25,702    | 11x     |
| jaro_winkler          | 292,283   | 119,125   | 2x      |
| lcsseq                | 393,526   | 32,685    | 12x     |
| ratcliff_obershelp    | 218,066   | 67,449    | 3x      |
| jaccard               | 159,426   | 100,343   | 2x      |
| arith_ncd             | 10,005    | 2,838     | 4x      |
| bz2_ncd               | 10,258    | 9,390     | 1x      |

The dynamic-programming algorithms widen the gap on longer strings: on ~200 character
sentence pairs, levenshtein runs 331x and lcsseq 199x faster than the original. Importing
`textdistance` takes 15 ms with the port versus 90 ms with the original, and peak RSS is
~17 MB versus ~27 MB.

Two rows are worth reading as the honest baseline: `bz2_ncd` is 1x because both sides call the
same C compression library, and `jaccard` is 1x on long strings because the token path is
dominated by the same counting logic. Nothing is filtered here; `bench/run.py` reproduces
every number.

```
.venv/Scripts/python bench/run.py
```

## License

MIT. The port is a reimplementation of the MIT-licensed `textdistance` project
(https://github.com/life4/textdistance) and keeps its public API and test suite; see
`LICENSE` and the upstream project for attribution.
