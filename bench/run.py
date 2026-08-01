"""Benchmark the port against the original textdistance.

Runs the same workload through both implementations in fresh subprocesses
(bench/worker.py), compares the results, prints a table, and writes the raw
numbers to bench/results.json. See bench/methodology.md for how each metric is
measured and what "honest" means here.

Usage:
    .venv/Scripts/python bench/run.py
"""

import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
WORKER = os.path.join(HERE, "worker.py")
REFERENCE = os.path.join(ROOT, "reference", "textdistance")

WORDS = [
    ("kitten", "sitting"), ("flaw", "lawn"), ("intention", "execution"),
    ("gumbo", "gambol"), ("test", "text"), ("nelson", "neilsen"),
    ("saturday", "sunday"), ("christmas", "xmas"), ("book", "back"),
    ("algorithm", "logarithm"), ("precision", "prison"), ("robert", "rupert"),
    ("dwayne", "duane"), ("martha", "marhta"), ("sam", "sana"),
    ("tim", "tam"), ("ruby", "raby"), ("python", "pythons"),
    ("levenshtein", "damerau"), ("jaro", "winkler"), ("lcs", "lcs"),
    ("sequence", "sequent"), ("tokenize", "tokenize"), ("distance", "dist"),
    ("fuzzy", "fizzy"), ("matching", "machining"), ("string", "sting"),
    ("character", "chara"), ("comparing", "compression"), ("pairwise", "ways"),
    ("edit", "delta"), ("opossum", "possum"), ("synonym", "antonym"),
    ("circumstances", "circumstances"), ("perpendicular", "parallel"),
    ("conspicuous", "conscious"), ("rhythm", "rhythms"), ("phlegm", "phlem"),
    ("queueing", "queuing"), ("accommodate", "accommodation"),
]

LONG = [
    ("the quick brown fox jumps over the lazy dog "
     "and runs through the forest at breakneck speed while birds circle above",
     "the quick brown fox jumps over the lazy cat "
     "and runs into the forest at breakneck pace while birds circle below"),
    ("pack my box with five dozen liquor jugs that sit upon the table "
     "in the corner of the kitchen next to the window with the blue curtains",
     "pack my box with five dozen liquor jugs that sit under the table "
     "in the corner of the kitchen beside the window with the red curtains"),
    ("a brave new world of sparkling machines and gentle electric whispers "
     "fills the quiet library where old paper books dream in the evening light",
     "a brave new world of shining machines and gentle electric murmurs "
     "fills the quiet library where old paper books dream in the evening glow"),
]

ALGORITHMS = ["levenshtein", "damerau_levenshtein", "jaro_winkler",
              "ratcliff_obershelp", "lcsseq", "jaccard", "arith_ncd",
              "bz2_ncd"]
LONG_ALGORITHMS = ["levenshtein", "lcsseq", "jaccard", "ratcliff_obershelp"]

WORD_SPEC = {"algorithms": ALGORITHMS, "corpus": WORDS,
             "warmup": 200, "calls": 8000}
LONG_SPEC = {"algorithms": LONG_ALGORITHMS, "corpus": LONG,
             "warmup": 5, "calls": 50}


def run_worker(impl, spec):
    result = subprocess.run([sys.executable, WORKER, impl],
                            input=json.dumps(spec), capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(f"{impl} worker failed: {result.stderr}")
    return json.loads(result.stdout)


def main():
    port_words = run_worker("port", WORD_SPEC)
    port_long = run_worker("port", LONG_SPEC)
    ref_words = run_worker(REFERENCE, WORD_SPEC)
    ref_long = run_worker(REFERENCE, LONG_SPEC)

    for data in (port_long, ref_long):
        for algo in LONG_ALGORITHMS:
            data["algorithms"][algo + "__long"] = data["algorithms"].pop(algo)

    port = {**port_words, "algorithms": {**port_words["algorithms"],
                                         **port_long["algorithms"]}}
    reference = {**ref_words, "algorithms": {**ref_words["algorithms"],
                                             **ref_long["algorithms"]}}

    order = ALGORITHMS + [a + "__long" for a in LONG_ALGORITHMS]
    lines = []
    lines.append("Benchmark: port vs original textdistance (same workload, fresh processes)")
    lines.append("  port       textdistance " + port["version"])
    lines.append("  reference  textdistance " + reference["version"])
    lines.append("")
    lines.append(f"  startup import: port {port['import_ms']:.1f} ms, "
                 f"reference {reference['import_ms']:.1f} ms")
    lines.append("")
    lines.append(f"  {'algorithm':<22} {'impl':<10} {'calls/s':>12} {'p50 us':>9} "
                 f"{'p99 us':>9} {'max us':>9} {'RSS MB':>8}")
    lines.append("  " + "-" * 82)

    speedups = {}
    for name in order:
        display = name.replace("__long", " (long)")
        p = port["algorithms"][name]
        r = reference["algorithms"][name]
        speedups[name] = p["throughput"] / r["throughput"]
        for impl, data in (("port", p), ("reference", r)):
            rss = "-" if data["rss_mb"] is None else f"{data['rss_mb']:.1f}"
            lines.append(
                f"  {display:<22} {impl:<10} {data['throughput']:>12,.0f} "
                f"{data['p50_us']:>9,.2f} {data['p99_us']:>9,.2f} "
                f"{data['max_us']:>9,.2f} {rss:>8}")
        lines.append(f"  {'':<22} {'speedup':<10} {speedups[name]:>12,.0f}x")

    report = {"port": port, "reference": reference, "speedups": speedups,
              "workload": {"word_pairs": WORDS, "long_pairs": LONG,
                           "calls": WORD_SPEC["calls"],
                           "long_calls": LONG_SPEC["calls"]}}
    with open(os.path.join(HERE, "results.json"), "w") as fh:
        json.dump(report, fh, indent=2, sort_keys=True)

    print("\n".join(lines))
    print("\nraw results written to bench/results.json")


if __name__ == "__main__":
    main()
