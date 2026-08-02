"""CLI-vs-reference output diff on a shared input set.

For every algorithm exposed by the ``tdc`` CLI, computes all four metrics
(distance, similarity, normalized_distance, normalized_similarity) two ways:

  * CLI path: shell out to  ``target/release/tdc <metric> <algo> <s1> <s2>``,
  * reference path: the verified original textdistance clone
    (reference/textdistance) as a library.

Any value that does not match (within 1e-9) is a DIFF and makes the script
exit non-zero. Output is also written to ``bench/cli_diff.txt`` so the result
is reproducible without re-running.

Requires: the release CLI built (cargo build --release -p tdc) and pytest deps.
Usage:
    python scripts/cli_diff.py [--out bench/cli_diff.txt]
"""

import argparse
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
REFERENCE_PKG = os.path.join(ROOT, "reference", "textdistance")
TDC = os.path.join(ROOT, "target", "release", "tdc.exe")
DEFAULT_OUT = os.path.join(ROOT, "bench", "cli_diff.txt")

ALGOS = [
    "levenshtein", "damerau_levenshtein", "hamming", "gotoh",
    "needleman_wunsch", "smith_waterman", "length", "jaro",
    "jaro_winkler", "strcmp95", "mlipns",
]
METRICS = ["distance", "similarity", "normalized_distance", "normalized_similarity"]

# A shared input set spanning typical phone/word data and the edge cases that
# trip over quick-answer/empty guards, identical for both paths.
PAIRS = [
    ("test", "text"),
    ("MARTHA", "MARHTA"),
    ("nelson", "neilsen"),
    ("nelson", "neilsen"),
    ("example", "samples"),
    ("", "abc"),
    ("abc", ""),
    ("", ""),
    ("a b", "a  b"),
    ("M A RHTA", "MARTHA"),
    ("kitten", "sitting"),
    ("flaw", "lawn"),
    ("  MARTHA  ", "MARTHA"),
    ("\u3053\u3093\u306b\u3061\u306f", "\u3053\u3093\u306b\u3061\u306f"),
    ("\u00e9\u00e8\u00ea", "\u00e9\u00ea"),
    ("quick", "quikc"),
    ("Saturday", "Sunday"),
    ("bookkeeper", "bookkeeping"),
]


def _tdc(algo, metric, s1, s2):
    sub = subprocess.run([TDC, metric, algo, s1, s2],
                         capture_output=True, text=True)
    if sub.returncode != 0:
        return None, sub.stderr.strip().splitlines()
    return float(sub.stdout.strip()), None


def reference_matrix():
    """One subprocess computes every reference value for the whole matrix,
    keyed by [(algo_idx, metric_idx, pair_idx)]."""
    env = dict(os.environ)
    env["PYTHONPATH"] = (REFERENCE_PKG + os.pathsep
                         + env.get("PYTHONPATH", ""))
    import json as _json
    code = (
        "import json, textdistance as t\n"
        "algos=%r\n" % (ALGOS,) +
        "metrics=%r\n" % (METRICS,) +
        "pairs=%r\n" % (PAIRS,) +
        "out=[]\n"
        "for a in algos:\n"
        "  row_a=[]\n"
        "  for m in metrics:\n"
        "    row=[]\n"
        "    for s1,s2 in pairs:\n"
        "      try:\n"
        "        row.append(float(getattr(getattr(t,a),m)(s1,s2)))\n"
        "      except Exception:\n"
        "        row.append(None)\n"
        "    row_a.append(row)\n"
        "  out.append(row_a)\n"
        "print(json.dumps(out))"
    )
    sub = subprocess.run([sys.executable, "-c", code], env=env,
                         capture_output=True, text=True)
    if sub.returncode != 0:
        raise SystemExit("reference batch failed:\n%s" % sub.stderr[-1500:])
    return _json.loads(sub.stdout)  # [algo][metric][pair]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=DEFAULT_OUT)
    args = ap.parse_args()

    ref = reference_matrix()

    rows = []
    diffs = 0
    ref_errors = 0
    for ai, algo in enumerate(ALGOS):
        for mi, metric in enumerate(METRICS):
            for pi, (s1, s2) in enumerate(PAIRS):
                tdc_v, tdc_err = _tdc(algo, metric, s1, s2)
                ref_v = ref[ai][mi][pi]
                if tdc_v is None:
                    diffs += 1
                    rows.append((s1, s2, (algo, metric), "CLI_ERR",
                                 tdc_err, ref_v))
                    continue
                if ref_v is None:
                    # The original raises here (known edge-case bugs, D21);
                    # our CLI answers cleanly. Not a divergence.
                    ref_errors += 1
                    rows.append((s1, s2, (algo, metric), "REF_RAISED",
                                 tdc_v, "upstream raises"))
                    continue
                ok = abs(tdc_v - ref_v) < 1e-9
                if not ok:
                    diffs += 1
                rows.append((s1, s2, (algo, metric),
                             "OK" if ok else "DIFF", tdc_v, ref_v))

    with open(args.out, "w", encoding="utf-8") as fh:
        fh.write("# cli_diff: tdc CLI vs reference clone (v4.6.2) on a shared\n")
        fh.write("# input pair set. REF_RAISED = original raises on this edge\n")
        fh.write("# case (upstream bugs, see DECISIONS D21); CLI answers.\n")
        fh.write("# algorithm metric s1 s2 status tdc_value reference_value\n")
        for s1, s2, key, status, tdc_v, ref_v in rows:
            fh.write("%-22s %-22s %-12s %-10s %-6s %r %r\n"
                     % (key[0], key[1], s1, s2, status, tdc_v, ref_v))
        fh.write("\n# total: %d, numeric DIFFs: %d, reference-raised edge: %d\n"
                 % (len(rows), diffs, ref_errors))

    print("cases=%d  numeric_diffs=%d  ref_raised=%d"
          % (len(rows), diffs, ref_errors))
    for s1, s2, key, status, tdc_v, ref_v in rows:
        if status == "DIFF":
            print("  DIFF %-22s %-22s %r %r -> cli=%r ref=%r"
                  % (key[0], key[1], s1, s2, tdc_v, ref_v))
    print("wrote %s" % args.out)
    sys.exit(1 if diffs else 0)


if __name__ == "__main__":
    main()