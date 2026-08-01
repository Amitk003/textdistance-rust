# Port tests

Additional tests written for the port, on top of the unmodified upstream suite in
`tests/original/`.

Two kinds live here:

- Native Rust unit tests, colocated with the kernels in `crates/tdcore` (`cargo test`).
- Adapter-level tests in Python that exercise the public API beyond what upstream covers
  (edge cases, error paths, unicode, and property checks).

Run everything with:

```
cargo test
.venv/Scripts/python -m pytest -m "not external" tests/original tests/port
```
