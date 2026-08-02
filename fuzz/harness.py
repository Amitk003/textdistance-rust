"""Differential fuzz harness: original textdistance vs this port.

Generates random inputs (text, unicode, varying qval and as_set, list/tuple and
numeric sequences, and lone-surrogate strings across every family), runs
the same case through the port in-process and through the original in a
separate subprocess, and compares every result.

Comparison rules:
  - Values are compared by their repr string (exact, type-sensitive, and
    bit-exact for floats). When reprs differ but both parse as numbers, the
    pair is a "near miss" and is accepted only within a documented tolerance
    (1e-9). Near misses are reported, never hidden.
  - Exception behavior is compared: a case that raises on one side and not the
    other is a divergence.

Run:
    .venv/Scripts/python fuzz/harness.py --duration 75 [--seed N]
    .venv/Scripts/python fuzz/harness.py --duration 65 --long [--seed N]
    .venv/Scripts/python fuzz/harness.py --duration 25 --no-logs  # CI smoke

Each run's summary is written to fuzz/log-{std,long}.txt and divergences to
fuzz/divergences-{std,long}.txt, so the committed artifacts for both modes
survive and cover every exported algorithm. --no-logs skips writing for CI.
"""

import argparse
import json
import math
import os
import random
import subprocess
import sys
import time

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REFERENCE_PATH = os.path.join(REPO_ROOT, "reference", "textdistance")
LOG_PATH = os.path.join(REPO_ROOT, "fuzz", "log-{mode}.txt")
DIVERGENCES_PATH = os.path.join(REPO_ROOT, "fuzz", "divergences-{mode}.txt")

TOLERANCE = 1e-9

# Every algorithm exported by the port (and by the pinned original), so the
# differential proof covers the full surface.
ALGORITHMS = [
    "arith_ncd", "bag", "bwtrle_ncd", "bz2_ncd", "cosine",
    "damerau_levenshtein", "editex", "entropy_ncd", "gotoh", "hamming",
    "identity", "jaccard", "jaro", "jaro_winkler", "lcsseq", "lcsstr",
    "length", "levenshtein", "lzma_ncd", "matrix", "mlipns", "monge_elkan",
    "mra", "needleman_wunsch", "overlap", "postfix", "prefix",
    "ratcliff_obershelp", "rle_ncd", "smith_waterman", "sorensen",
    "sorensen_dice", "sqrt_ncd", "strcmp95", "tanimoto", "tversky",
    "zlib_ncd",
]

ASCII = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 .,"
UNICODE = "a\u00e9\u00ea\u00fc\u00f1\u03b1\u03b2\u4e00\u65e5\u672c\u0928\u00f6\u00e8\u00e2"

# Every algorithm accepts strings; the edit, sequence, simple, and phonetic
# kernels now process Python code points too (see DECISIONS D20 and D21), so a
# lone surrogate flows through any of them like any other unit. The binary
# compressors (bz2/zlib/lzma) encode to UTF-8 first and raise
# UnicodeEncodeError on both sides, which the harness treats as an exact match.


def make_pool(unicode_chance):
    if random.random() < unicode_chance:
        return UNICODE + random.choice(ASCII)
    return ASCII


def random_sequence():
    pool = make_pool(0.35)
    length = random.choice([0, 1, 2, 3, 5, 8, 12])
    if length == 0:
        return ""
    return "".join(random.choice(pool) for _ in range(length))


def random_sequence_numeric(rng):
    length = rng.choice([0, 1, 2, 4, 8, 16])
    kind = rng.random()
    if kind < 0.4:
        pool = [0, 1, 2, 3, 5, 10]
    elif kind < 0.7:
        pool = [0.0, 0.5, 1.5, 2.0, 10.25, -3.0]
    else:
        pool = [0, 1, "a", 2.5, True, None]
    return [rng.choice(pool) for _ in range(length)]


def random_sequence_deep():
    pool = make_pool(0.5)
    kind = random.random()
    length = random.choice([0, 1, 2, 4, 8, 16, 24])
    chars = "".join(random.choice(pool) for _ in range(length))
    if kind < 0.7:
        return chars
    items = [c for c in chars]
    if kind < 0.85:
        return items
    return tuple(items)


def random_surrogate_case(rng):
    alg = rng.choice(ALGORITHMS)
    qval = rng.choice([None, 1, 1, 2, 3])
    as_set = rng.choice([False, False, True])
    s1 = random_surrogate_string(rng)
    s2 = random_surrogate_string(rng)
    if rng.random() < 0.3:
        s2 = s1
    return {
        "alg": alg,
        "kwargs": {"qval": qval, "as_set": as_set},
        "s1": s1,
        "s2": s2,
    }


def random_surrogate_string(rng):
    pool = make_pool(0.5) + "\ud800"
    length = rng.choice([0, 1, 2, 3, 5, 8])
    s = "".join(rng.choice(pool) for _ in range(length))
    if s and rng.random() < 0.7:
        pos = rng.randrange(len(s) + 1)
        s = s[:pos] + "\ud800" + s[pos:]
    return s


def random_case(rng):
    if rng.random() < 0.12:
        return random_surrogate_case(rng)
    alg = rng.choice(ALGORITHMS)
    qval = rng.choice([None, 0, 1, 1, 2, 3, 5])
    as_set = rng.choice([False, False, True])
    s1 = random_sequence()
    s2 = random_sequence()
    if rng.random() < 0.15:
        s2 = s1
    if rng.random() < 0.1:
        s2 = ""
    return {
        "alg": alg,
        "kwargs": {"qval": qval, "as_set": as_set},
        "s1": s1,
        "s2": s2,
    }


def random_case_deep(rng):
    if rng.random() < 0.12:
        return random_surrogate_case(rng)
    alg = rng.choice(ALGORITHMS)
    qval = rng.choice([None, None, 0, 1, 1, 2, 3, 4, 6])
    as_set = rng.choice([False, False, True])
    if rng.random() < 0.15:
        s1 = random_sequence_numeric(rng)
        s2 = random_sequence_numeric(rng)
        if rng.random() < 0.2:
            s2 = s1
    else:
        s1 = random_sequence_deep()
        s2 = random_sequence_deep()
        if rng.random() < 0.2:
            s2 = s1
        if rng.random() < 0.1:
            s2 = "" if isinstance(s1, str) else type(s1)()
    return {
        "alg": alg,
        "kwargs": {"qval": qval, "as_set": as_set},
        "s1": s1,
        "s2": s2,
    }


def parse_value(raw, path):
    if raw.startswith("__RAISED__:"):
        return ("raised", raw.split(":", 1)[1])
    if raw in ("None", "True", "False", "inf", "-inf"):
        return ("num", float(raw.replace("inf", "1e309"))) if raw != "None" else ("none", None)
    if raw.startswith("Fraction("):
        return ("frac", raw)
    try:
        if raw == "nan":
            return ("num", float("nan"))
        return ("num", float(raw))
    except ValueError:
        return ("other", raw)


def close_enough(ref_raw, port_raw):
    if ref_raw == port_raw:
        return 0
    ref = parse_value(ref_raw, "ref")
    port = parse_value(port_raw, "port")
    if ref[0] == port[0] == "num":
        a, b = ref[1], port[1]
        if math.isnan(a) and math.isnan(b):
            return 0
        if abs(a - b) <= TOLERANCE:
            return 1
    return 2


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--duration", type=float, default=60)
    parser.add_argument("--seed", type=int, default=20260801)
    parser.add_argument("--max-cases", type=int, default=0)
    parser.add_argument("--batch", type=int, default=200)
    parser.add_argument(
        "--long",
        action="store_true",
        help="deeper mode: longer strings, list and tuple sequences, more unicode",
    )
    parser.add_argument(
        "--no-logs",
        action="store_true",
        help="do not write log/divergence files (for CI smoke runs that must not clobber committed artifacts)",
    )
    args = parser.parse_args()

    mode = "long" if args.long else "std"
    log_path = LOG_PATH.format(mode=mode)
    divergences_path = DIVERGENCES_PATH.format(mode=mode)
    rng = random.Random(args.seed)
    gen_case = random_case_deep if args.long else random_case
    os.makedirs(os.path.dirname(log_path), exist_ok=True)

    if not os.path.isdir(REFERENCE_PATH):
        print("reference checkout not found at", REFERENCE_PATH)
        sys.exit(2)

    # The port lives in this process; the original runs in a subprocess.
    import textdistance as td_port

    worker = subprocess.Popen(
        [sys.executable, os.path.join(REPO_ROOT, "fuzz", "reference_worker.py"), REFERENCE_PATH],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
    )

    start = time.time()
    cases_run = 0
    divergences = []
    near_misses = []

    try:
        while True:
            batch = [gen_case(rng) for _ in range(args.batch)]
            # Round-trip through JSON so both sides operate on byte-identical
            # deserialized inputs (JSON collapses () and [] into [], so the
            # port must not keep the pre-serialization Python objects).
            batch_json = json.dumps(batch)
            worker.stdin.write(batch_json + "\n")
            worker.stdin.flush()
            ref_results = json.loads(worker.stdout.readline())
            batch = json.loads(batch_json)

            for case, ref in zip(batch, ref_results):
                cases_run += 1
                port = {}
                port_error = None
                cls = type(getattr(td_port, case["alg"]))
                try:
                    inst = cls(**case["kwargs"])
                except Exception as exc:  # noQA
                    port_error = type(exc).__name__
                if "error" in ref:
                    if port_error != ref["error"]:
                        divergences.append(
                            (case, {"reference_error": ref["error"], "port_error": port_error}, "error path")
                        )
                    continue
                if port_error is not None:
                    divergences.append(
                        (case, {"port_error": port_error, "reference_error": None}, "error path")
                    )
                    continue
                for name in ("distance", "similarity", "normalized_distance", "normalized_similarity"):
                    try:
                        port[name] = repr(getattr(inst, name)(case["s1"], case["s2"]))
                    except Exception as exc:  # noQA
                        port[name] = "__RAISED__:" + type(exc).__name__
                try:
                    port["maximum"] = repr(inst.maximum)
                except Exception as exc:  # noQA
                    port["maximum"] = "__RAISED__:" + type(exc).__name__

                for name in ("distance", "similarity", "normalized_distance", "normalized_similarity", "maximum"):
                    status = close_enough(ref[name], port[name])
                    if status == 2:
                        divergences.append((case, {name: {"ref": ref[name], "port": port[name]}}, "value"))
                        break
                    if status == 1:
                        near_misses.append((case, name, ref[name], port[name]))
                        break
            if args.max_cases and cases_run >= args.max_cases:
                break
            if time.time() - start >= args.duration:
                break
    finally:
        worker.stdin.close()
        worker.terminate()
        worker.wait()

    elapsed = time.time() - start

    if not args.no_logs:
        with open(divergences_path, "w", encoding="utf8", errors="replace") as f:
            f.write(f"divergences: {len(divergences)}\n")
            for case, detail, kind in divergences[:200]:
                f.write(json.dumps({"kind": kind, "case": case, "detail": detail}) + "\n")

        summary = (
            "differential fuzz run\n"
            f"mode: {mode}\n"
            f"seed: {args.seed}\n"
            f"duration: {elapsed:.1f}s (requested {args.duration}s)\n"
            f"cases: {cases_run}\n"
            f"divergences: {len(divergences)}\n"
            f"near_misses_within_1e-9: {len(near_misses)}\n"
            f"algorithms: {len(ALGORITHMS)}\n"
            f"tolerance: {TOLERANCE}\n"
        )
        with open(log_path, "w", encoding="utf8", errors="replace") as f:
            f.write(summary)
            if divergences:
                f.write("first divergences:\n")
                for case, detail, kind in divergences[:10]:
                    f.write(json.dumps({"kind": kind, "case": case, "detail": detail}) + "\n")

    print(
        "differential fuzz run\n"
        f"mode: {mode}\n"
        f"seed: {args.seed}\n"
        f"duration: {elapsed:.1f}s (requested {args.duration}s)\n"
        f"cases: {cases_run}\n"
        f"divergences: {len(divergences)}\n"
        f"near_misses_within_1e-9: {len(near_misses)}\n"
        f"algorithms: {len(ALGORITHMS)}\n"
        f"tolerance: {TOLERANCE}\n"
    )
    if divergences:
        print(f"divergences found: {len(divergences)}, see {divergences_path}")
        sys.exit(1)
    if near_misses:
        print(f"note: {len(near_misses)} near misses within tolerance, see details above")
    else:
        print("all outputs bit-identical")


if __name__ == "__main__":
    main()
