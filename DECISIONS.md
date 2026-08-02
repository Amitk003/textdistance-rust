# Decisions

Every non-trivial architectural divergence from the original `textdistance`, with rationale.
This file is read as a record of intent. Entries are honest, including where a tradeoff was
forced by time or by the target language.

## D1. Rust kernels behind a thin Python adapter

The original is a pure-Python package whose classes are the API. Porting literally would mean
either (a) rebuilding every class in Rust via PyO3 pyclass, including Python-object plumbing
like `collections.Counter`, word splitting, and `itertools` permutations, or (b) keeping the
class layer and moving the math into Rust. We chose (b).

Rationale: the class machinery is generic object glue, not the algorithm. The algorithms are
the DP matrices, n-gram counts, and coding schemes, and those are the substance of the port.
Moving that substance to Rust keeps the port idiomatic and fast, while the adapter preserves
exact Python class semantics (`isinstance`, mutable attributes, callable instances) that
reimplementing in PyO3 would be fragile to reproduce. The adapter is small, clearly
delimited, and documented. This is the "thin adapter" boundary the porting brief for this
kind of work expects, and it is the only Python in the shipped package.

## D2. External library delegation is not ported

The original can delegate to installed optional libraries (jellyfish, abydos, and others)
through its `external_answer` path. We do not reproduce that delegation.

Rationale: in an environment with none of those libraries installed, the original's
`external_answer` returns None and the pure-Python implementation runs. That is exactly the
environment this port targets and tests in. Porting the delegation would mean importing
third-party Python modules from Rust, which adds surface for no behavioral gain in the tested
environment. The `external`-marked tests that exercise delegation are excluded from the suite
with the same marker the upstream project uses in its own CI.

## D3. Test suite definition mirrors upstream

The suite is run as `pytest -m "not external"`, matching the upstream `pytest-pure` task.
The `external` marker is registered (via the same `--strict-markers` setting) but not
filtered at the project level, so the marker split is reproduced explicitly.

Rationale: the upstream project itself separates pure tests from external-library tests. Our
parity claim is defined against the same set, so the comparison is apples to apples.

## D4. Compression NCD links the same C libraries as CPython

bz2, zlib, and lzma NCD algorithms depend on compressed length. CPython's `bz2`, `zlib`, and
`lzma` modules wrap libbz2, libz, and liblzma. To match compressed lengths bit for bit, the
port calls the same C libraries through `bzip2-sys`, `libz-sys`, and `lzma-sys` with the same
defaults (bz2 blocksize 9, zlib Z_DEFAULT_COMPRESSION, lzma preset 6).

Rationale: an alternative Rust compressor (for example flate2) is a different codec version
and can produce a different compressed length for the same input, which would change the NCD
value and break exact differential matching. Using the identical underlying codecs makes the
port's NCD outputs numerically identical to the original. This trades a pure-Rust dependency
graph for exact equivalence, which is the priority for a behavioral port.

## D5. Pure-Python coders are ported directly to Rust

ArithNCD, EntropyNCD, RLE, BWT+RLE, and SqrtNCD have no C dependency in the original; they
are deterministic pure-Python implementations. Those are ported line for line into Rust
arithmetic/entropy encoders so the byte output matches exactly.

Rationale: deterministic codecs are the easiest exact match, and there is no reason to accept
divergence. Order of operations in the entropy/arithmetic loops is preserved to keep floating
point identical.

## D6. Float arithmetic aims for bit-exactness

Python floats and Rust f64 are both IEEE-754 doubles, so identical operation order yields
identical bits. The port preserves the original's expression shapes (for example
`self.distance(...) / maximum` and the guard `if maximum == 0: return 0`) to keep normalized
outputs bit-identical where possible.

Rationale: the upstream property tests already compare with `isclose`, so exactness is not
required to pass them. But the differential fuzz harness compares raw outputs, and bit-exact
outputs let that comparison use exact equality with a tiny documented tolerance as a safety
valve for rare expression reshuffles. Where exactness cannot be guaranteed, the fuzz harness
uses a tolerance of 1e-9 and the divergence count is reported honestly.

## D7. Integer policy

Python integers are arbitrary precision. The port uses u64/usize, which is sufficient for
every reachable input in the test suite and for realistic workloads (edit distances and
counts grow linearly with input length, far below u64 bounds).

Rationale: arbitrary-precision integers in Rust would force a big-int dependency for a case
that cannot occur within the documented input envelope. The limit is stated here and in the
README rather than silently accepted. Guard functions assert on unreachable overflow rather
than wrapping.

## D8. Unicode and word splitting

`qval=None` semantics split text by words. Python's `str.split()` with no argument splits on
any run of Unicode whitespace and drops empty tokens. The port reimplements that exact rule
over the input characters rather than using ASCII-only splitting.

Rationale: `bag` and other word-mode algorithms are behaviorally defined by this rule, and a
naive whitespace split changes results for Unicode whitespace and multiple runs of spaces.

## D9. Custom similarity callables are wrapped at the FFI boundary

Some algorithms accept Python callables (`sim_func` for Gotoh and SmithWaterman, `sim_test`
for Prefix). The DP kernels live in Rust and invoke the callable through PyO3 when one is
supplied.

Rationale: honoring arbitrary Python callables inside a Rust kernel requires a callback
bridge; there is no way around it without reimplementing the DP in Python. The default path
(no custom callable) uses a pure-Rust predicate and never crosses back, which keeps the hot
path fast and the benchmark story honest.

## D10. quick_answer and external_answer behavior

The generic `quick_answer` short-circuits (empty sequences, identical sequences, single
sequence) before the kernel runs. That logic is reproduced in the adapter so the return
values, including `maximum` for empty inputs in the similarity base, match the original.

Rationale: these short-circuits are observable API behavior (the property tests depend on
`distance('', '') == 0` and the unequal-distance guard), so they are part of the contract,
not an optimization detail.

## D11. The module surface matches the original exactly

`textdistance/__init__.py` in the port exposes the same names as upstream: all algorithm
singletons, all classes, `utils.find_ngrams` and `utils.words_combinations`, plus the module
metadata (`__title__`, `__version__`, `VERSION`). The adapter's `__all__` mirrors the
upstream files.

Rationale: the original tests reference module-level names directly
(`textdistance.levenshtein`, `textdistance.Tversky`, `textdistance.bag`). A port that passes
the original tests must present the same surface, and doing so is also what makes the port a
drop-in replacement for downstream users.

## D12. Distribution naming

The built wheel is distributed as `textdistance-rust`, while the importable package remains
`textdistance`.

Rationale: keeping the import name `textdistance` is required for drop-in compatibility and
for the original tests to run unmodified. A distinct distribution name avoids confusion with
the upstream package on index servers while keeping the source package name exact.

## D13. Build reproducibility and pinned reference

The reference original is pinned to commit d6a68d61088a40eef5c88191ccf79323dbf34850, and the
test suite hashes are recorded at that pin. The port's CI and local build install a pinned
toolchain and the exact test dependencies.

Rationale: a behavioral port is only meaningful against a fixed original. Pinning makes the
hashes in `tests/original/SHA256SUMS.txt` reproducible and lets anyone re-verify the
equivalence claim byte for byte.

## D14. Safety boundary

`crates/tdcore` is compiled with `#![forbid(unsafe_code)]`, and `crates/pyapi` (the PyO3
layer) is written without any `unsafe` block. All `unsafe` in the project is confined to one
small crate: the C-compression bindings (`crates/codec`), each `unsafe` block carrying a
SAFETY comment.

Rationale: the core is where correctness lives and where a memory error would be a port bug.
Keeping it mechanically unsafe-free is a checkable property, and it keeps the FFI surface
small and auditable. The compression crate is the single exception: it calls the same C
libraries as CPython so that compressed lengths match bit for bit, and no unsafe escapes
into `tdcore`.

## D15. Benchmark scope is declared up front

Benchmarks cover throughput, p99 latency, RSS, and startup on a shared workload, with the
methodology in `bench/methodology.md`. No number is published without the script that
produced it.

Rationale: throughput-only numbers are easy to make look good and easy to distrust. The
behavioral claims come first; performance numbers are reported with their distribution and
their measurement method, including cases where the port is slower than the original.

## D16. The surface parity claim is a test, not a note

`tests/port/test_surface.py` pins the exact public name sets of `textdistance` and
`textdistance.utils`, captured from the pinned upstream release, and asserts the port
exposes the same names and `__version__`.

Rationale: surface parity was previously only documented. Making it an executable test means
any future edit that adds, removes, or renames an exported name fails CI instead of silently
drifting from the original API.

## D17. Draft and untested upstream modules are not ported

Upstream ships `textdistance/algorithms/vector_based.py` (Chebyshev, Minkowski, Euclidean,
and friends) but it is explicitly marked as a draft, is not imported by
`textdistance/algorithms/__init__.py`, requires numpy, and is not exercised by any test.

Rationale: the port targets the public surface and the original pure suite. Shipping a
verbatim draft module whose classes raise `NotImplementedError` by design would add dead
code without adding provable behavior. The module is documented as an intentional omission
here rather than silently dropped.

## D18. External-library speed registry is not bundled

Upstream `libraries.py` reads `libraries.json` in `optimize()` to sort third-party
implementations of an algorithm by speed. The port keeps the `LibrariesManager` API
(`register`, `get_libs`, `clone`) but makes `optimize()` a no-op and does not ship the json.

Rationale: the port does not bundle pyxdameraulevenshtein, jellyfish, rapidfuzz, or the
other external providers, so there is nothing to load or reorder. Keeping the manager API
preserves compatibility for downstream code that registers libraries; optimizing an empty
registry would only add a broken file read.

## D19. Hypothesis deadline is relaxed for the verbatim upstream suite

The repo-root `conftest.py` registers and loads a hypothesis profile with a 2000 ms
per-example deadline. The default is 200 ms.

Rationale: the upstream compression tests generate very long unicode strings on which the
arithmetic NCD is inherently slow on both sides of the port. Measured on the failing input,
the port takes ~280 ms and the pinned original ~300 ms, both above the 200 ms default, and
both compute bit-identical output (1.0118632). The upstream project has the same problem and
excludes `lzma_ncd` from CI as "too slow, makes CI flaky". The deadline is an anti-hang guard,
not a correctness assertion, so relaxing it does not weaken what the tests verify. This makes
the port's suite deterministic on this class of tests instead of flaky.

## D20. Strings are processed as Python code points

Python strings are sequences of code points, and a lone surrogate (U+D800..DFFF) is a valid
one-element code point even though it is not valid Unicode text. The pure compression coders
(arithmetic coding, RLE, BWT, sqrt, entropy) process their input as code points, so a string
containing a lone surrogate matches the original bit for bit; the extension extracts code
points directly (with a Python-semantics fallback when UTF-8 would reject a surrogate). The
binary compressors (bz2, zlib, lzma) encode to UTF-8 first, exactly as the original does, so
a lone surrogate raises `UnicodeEncodeError` on both sides. Behavior is pinned by
`tests/port/test_surrogates.py`.

Rationale: the upstream compression suite draws lone surrogates via `characters()` and
expects the pure coders to handle them, so this is exercised behavior, not a corner. At the
time this entry was written, the edit, sequence, and phonetic kernels compared Rust `char`
scalar values, which cannot hold a lone surrogate, so such an input raised
`UnicodeEncodeError` there where the original computes. That boundary was a deliberate,
documented tradeoff. It has since been closed: every family now processes code points just
like the compression coders do, so the boundary is gone (see D21).

## D21. The code-point model now spans every family; MLIPNS returns int; verified deviations

Building on D20, the edit, sequence, simple, and phonetic kernels were migrated from Rust
`&[char]` to `&[u32]` code points. The kernels were already generic over their element type, so
the change is confined to the FFI layer and the non-generic kernels (StrCmp95 and Editex were
already `&[u32]`): every string fast path now extracts Unicode code points via
`seq_to_codepoints` (fast UTF-8, with a Python-semantics fallback for lone surrogates). A lone
surrogate is now a distinct unit compared unequal to every ASCII character, exactly as in the
original, and no kernel raises `UnicodeEncodeError` on it. Behavior is pinned by
`tests/port/test_surrogates_edit_sequence.py`, and the differential fuzz pool was widened to
draw lone surrogates across every family. See D20.

The migration surfaced one latent, pre-existing divergence unrelated to surrogates: the
original MLIPNS returns `1` or `0` as an `int` on every path, while the port's two-sequence
path returned the f64 kernel value. The earlier differential comparisons matched `1.0 == 1`
numerically, so a type-sensitive check never flagged it. The adapter now narrows the kernel's
result to `int`, restoring exact return-type parity.

A second, independent finding is the verified Smith-Waterman deviation. The original returns
the bottom-right cell of its DP matrix (`dist_mat[-1, -1]`) rather than the matrix maximum,
so it is end-anchored rather than true local alignment: `smith_waterman('ax','xb')` is 0.0 where
the textbook answer is 1.0. Every one of the five upstream Smith-Waterman tests pins the
bottom-right values, so this is deliberate, test-pinned behavior, not an accidental bug. The
port reproduces it bit-for-bit, and because it is faithful to the original (and every
principle of a behavioral port), it is documented here rather than "fixed": porting means
preserving the original's behavior, not correcting it.

### Bug Catcher outcome

The completed migration was re-verified by widening the differential fuzz harness to draw lone
surrogates across every exported algorithm and re-running both mode sets (std 1,746,200 cases
and long 1,365,400 cases, zero divergences and zero near-misses, all outputs bit-identical),
plus the earlier 4.69M-case broad hunt with the full 37-algorithm surface. At the time, no
genuine, defensible bug in the original was found. The two candidate findings investigated (the
Smith-Waterman end-anchoring above, and `jaccard((), []) == 1` versus `jaccard([], []) == 0`
which follows the documented `quick_answer` ordering) are both faithful to the original and to
its own tests, so neither is filed as a bug.

A later repair pass on the fuzz harness (see D23) surfaced a genuine upstream defect that had
been hidden by the earlier harness bug: `gotoh` raises `IndexError` on single-empty input. The
port reproduces it byte-for-byte and the finding is filed separately against upstream (D24).
This outcome is reported honestly rather than fabricated: a behavioral port should preserve the
original, not manufacture defects in it — and when a real one is found, it is documented with
the exact failing input rather than silently "fixed".

### D22 Honest numbers artifacts (unsafe count, pass rate per file, coverage, CLI diff)

Deliverable verifiers, all committed and machine-runnable.

- **Unsafe blocks** - `scripts/honest_report.py` scans every `.rs` file: **0** `unsafe {`
  blocks and **0** `unsafe fn` in `tdcore`, `pyapi`, and `tdc`; **10** blocks in `codec`
  (the bzip2/zlib/lzma wrapper crate, the only C in the workspace; each with a
  `// SAFETY:` comment). `tdcore` still carries `#![forbid(unsafe_code)]`. The count is of
  `unsafe {` blocks (plus `unsafe fn`), not of the word `unsafe` wherever it appears.
- **Test pass rate per file** (`scripts/honest_report.py`) — original suite via `--junitxml`:
  **400/400, 100%**, every test file green, 0 failed.
- **Coverage diff** (`scripts/coverage_diff.py`): statement coverage of
  `textdistance.algorithms` for the reference clone vs the port under the same suite, to
  `bench/coverage.json`. Port adapter modules are covered at least as well as the
  reference's. Honest caveat: coverage counts *Python* lines only; the port's math is Rust,
  so % understates proven behaviour — `fuzz/log-*.txt` are the equivalence proof.
- **CLI output diff on a shared input set** (`scripts/cli_diff.py`): 792 cases (11 algos × 4
  metrics × 19 pairs), **0 numeric diffs** vs the reference clone. Four cases are
  `REF_RAISED`: upstream raises (its empty-input `gotoh` indexing bug) where the CLI answers.

D22 also caught and fixed a real **CLI-only** bug: `tdc` returned 0.0 for `jaro('','')`
similarity, but upstream `BaseSimilarity.quick_answer` returns the maximum (1.0) for
identical (both empty) sequences. `crates/tdc/src/main.rs` now mirrors that ordering
(both-empty → maximum; single-empty → 0) with a regression test. The Python API was already
correct; only the previously-unexercised Rust CLI had the bug.

## D23. Fuzz-harness repair and the parity bugs it surfaced

The differential harness was value-comparing only a fraction of the surface. It passed the same
constructor kwargs to all 37 algorithms, but 28 of them reject `as_set`, so those cases died of
`TypeError` at construction and were never compared. The harness now filters kwargs through
`inspect.signature` (identically for the port and the reference subprocess), strips numpy repr
wrappers (`np.float64(3.0)`, `np.int64(19)`) in `parse_value`, and compares `maximum` as a
called value rather than a bound-method repr. This repaired harness is the source of truth for
the counts below, and it is what surfaced the gotoh defect in D21.

Each repair below was verified with a targeted probe and by re-running both fuzz modes to a
final state of **0 divergences** (std 59,000 cases, long 101,400 cases; near misses within the
documented 1e-9 tolerance are numpy-cosmetic and still reported, never hidden).

1. **Gotoh single-empty crash (faithful reproduction).** The original raises `IndexError`
   when exactly one input is empty: with `len_s1 == 0` the DP matrices have one row, and the
   column-initialization loop writes `p_mat[1, j]`; with `len_s2 == 0` the row loop writes
   `q_mat[i, 1]`. Both-empty is fine (no loop runs, result 0). The port previously returned a
   value; it now raises the identical `IndexError`, pinned by
   `tests/port/test_surface.py`. The upstream finding is filed separately (D24).

2. **LZMA preset panic.** `lzma_lzma_preset` returned 0 on this toolchain while still writing
   `dict_size = 8388608` (8 MiB, the preset-6 default), so an assert on the return value
   panicked at startup. The assert now checks the populated options (`dict_size != 0`) instead
   of the unreliable bool.

3. **LZMA length divergence (real port bug).** The port compressed through the one-shot
   `lzma_stream_buffer_encode`; CPython's `lzma.compress` drives the streaming
   `lzma_stream_encoder` + `lzma_code(LZMA_FINISH)`. Both call the same vendored liblzma
   5.2.5 (the version CPython 3.11 Windows bundles), but the one-shot API emits a different
   block header and LZMA2 payload, and for a repeated-string input produced a *different
   compressed length* (58 vs 62), which changes the NCD value. A Rust experiment proved the
   streaming path is byte-identical to CPython; the codec now uses it and the 16 fuzz
   divergences dropped to 0. The codec unit test pins CPython's header-trimmed lengths.

4. **StrCmp95 loops.** The port's matching, transposition, and similarity loops each deviated
   from the original quirk-for-quirks behavior: the similarity pass rewards only the `adjwt`
   phonetic pairs (not plain equality), and the transposition pass reuses the matching loop's
   final `j` and compares against `s2[len_s2 - 1]` when no flagged position is found. The
   loops were rewritten to mirror the Python line for line; the 12-pair probe went from 10/12
   to 12/12.

5. **Hamming `None`-padding parity.** For non-string sequences the reference pads with `None`
   via `zip_longest` and compares with Python `==`, so `None == None` matches. The port's
   column predicate treated a padded `None` column as an automatic mismatch and, worse,
   distinguished Rust `Option::None` (padding) from an actual `None` element. The predicate now
   maps padding to `None` and compares everything through Python `==`. This also fixed MLIPNS,
   whose `__call__` delegates to `Hamming()`.

6. **BWTRLENCD on list/tuple input.** The reference appends the `'\0'` terminator to a list
   and then calls `type(data)().join(...)`, which raises `AttributeError` (`list` has no
   `join`). The port's Rust BWT path returned a value. The port now mirrors the upstream body
   for non-string inputs and raises the identical `AttributeError`.

Final verification: `cargo test --workspace` green (codec, tdc, tdcore), full pytest
`428 passed, 30 deselected`.

## D24. Upstream issue: gotoh IndexError on single-empty input

A genuine, reproducible upstream defect. Filing text for `life4/textdistance` (no `gh` CLI on
the build box, so filed manually):

```
Title: Gotoh raises IndexError when exactly one input is empty

textdistance 4.5.0, `textdistance.gotoh`.

Repro:
    >>> textdistance.gotoh('', 'x')
    IndexError: index 1 is out of bounds for axis 0 with size 1

    >>> textdistance.gotoh('x', '')
    IndexError: index 1 is out of bounds for axis 0 with size 1

    >>> textdistance.gotoh('', '')   # both empty: fine
    0.0

Cause: in Gotoh.__call__ (edit_based.py), the DP matrices are
(numpy.zeros((len_s1+1, len_s2+1))). When len_s1 == 0 the matrix has a single
row, but the column-initialization loop writes `p_mat[1, j] = -self.gap_open`
for every j in 1..len_s2, indexing row 1 of a one-row matrix. Symmetrically,
when len_s2 == 0 the row loop writes `q_mat[i, 1] = -self.gap_open` on a
one-column matrix.

Expected: like every other metric (and like both-empty), single-empty input
should return a value rather than crash. `Gotoh.minimum` and `Gotoh.maximum`
already handle the lengths correctly, so returning `-gap_open - gap_ext *
len(s2)` (or `0`/`maximum`) would be consistent with the affine-gap semantics
and with the other edit metrics' `quick_answer` behavior.
```
