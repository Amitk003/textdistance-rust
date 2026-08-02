"""Honest-numbers report for the submission.

Two things the brief asks for explicitly:
  1. an unsafe block count,
  2. test pass rate per file.

Both are machine-verifiable:
  * scans every .rs file for ``unsafe {`` blocks and bare ``unsafe fn``, per
    crate; asserts 0 blocks in tdcore/pyapi/tdc and prints the real codec count;
  * runs the original suite with --junitxml and groups pass/fail per test file.

Usage:
    python scripts/honest_report.py [--out bench/honest_report.txt]
"""

import argparse
import collections
import os
import re
import subprocess
import sys
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
CRATES = os.path.join(ROOT, "crates")
DEFAULT_OUT = os.path.join(ROOT, "bench", "honest_report.txt")

# tdcore, pyapi, tdc must be safe; codec is the only crate that touches C.
CRATES_CHECKED = ["tdcore", "pyapi", "tdc", "codec"]
SAFE_CRATES = {"tdcore", "pyapi", "tdc"}
BLOCK_RE = re.compile(r"\bunsafe\s*\{")
FN_RE = re.compile(r"\bunsafe\s+fn\b")

SUITE = os.path.join(ROOT, "tests", "original")


def unsafe_counts():
    counts = {}
    for crate in CRATES_CHECKED:
        src = os.path.join(CRATES, crate, "src")
        blocks = fns = 0
        for dirpath, _, files in os.walk(src):
            for fn in files:
                if not fn.endswith(".rs"):
                    continue
                with open(os.path.join(dirpath, fn), "r",
                          encoding="utf-8", errors="replace") as fh:
                    text = fh.read()
                blocks += len(BLOCK_RE.findall(text))
                fns += len(FN_RE.findall(text))
        counts[crate] = (blocks, fns)
    return counts


def pytest_junit():
    xml = os.path.join(ROOT, "bench", "honest_junit.xml")
    if os.path.exists(xml):
        os.remove(xml)
    cmd = [sys.executable, "-m", "pytest", "-q", "--tb=no",
           "--junitxml", xml, SUITE, "-m", "not external"]
    subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    return xml


def pass_per_file(xml):
    root = ET.parse(xml).getroot()
    counts = collections.OrderedDict()
    for tc in root.iter("testcase"):
        cls = tc.get("classname") or ""
        # e.g. "tests.original.test_edit.test_levenshtein" -> "test_edit/test_levenshtein"
        parts = cls.split(".")
        if "original" in parts:
            sub = parts[parts.index("original") + 1:]
            mod = ("/".join(sub[:-1]) + "/" + sub[-1]) if sub else "unknown"
        else:
            mod = parts[0] or "unknown"
        failed = tc.find("failure") is not None or tc.find("error") is not None
        cur = counts.setdefault(mod, [0, 0])
        cur[1] += 1 if failed else 0
        cur[0] += 0 if failed else 1
    return counts


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=DEFAULT_OUT)
    args = ap.parse_args()

    lines = ["honest-report", "=" * 74, ""]

    # unsafe --------------------------------------------------------------
    lines.append("unsafe blocks / unsafe fn per crate (scan of crates/*/src):")
    lines.append("  %-12s %10s %10s   %s" % ("crate", "unsafe{}", "unsafe fn", "note"))
    counts = unsafe_counts()
    total = 0
    safe_ok = True
    for crate in CRATES_CHECKED:
        blocks, fns = counts[crate]
        total += blocks
        if crate in SAFE_CRATES and (blocks or fns):
            safe_ok = False
        note = ""
        if crate == "codec":
            note = "C/C++ wrapper crate (the only unsafe in the workspace)"
        lines.append("  %-10s %-7d %-10d   %s" % (crate, blocks, fns, note))
    lines.append("")
    lines.append("  total unsafe{ blocks in workspace : %d" % total)
    lines.append("  SAFE core (tdcore/pyapi/tdc)       : %s"
                 % ("PASS (0 blocks, 0 unsafe fn)" if safe_ok else "FAIL"))
    lines.append("")

    # test pass rate per file --------------------------------------------
    lines.append("original suite: python -m pytest -m 'not external' tests/original")
    xml = pytest_junit()

    per = pass_per_file(xml)
    lines.append("  %-30s %-8s %-8s %-7s" % ("test file (module)", "passed", "failed", "rate"))
    pass_tot = fail_tot = 0
    for mod, (p, f) in sorted(per.items()):
        pass_tot += p
        fail_tot += f
        rate = "%.1f%%" % (100.0 * p / (p + f)) if (p + f) else "n/a"
        lines.append("  %-30s %-8d %-7d %-7s" % (mod, p, f, rate))
    lines.append("  " + "-" * 58)
    lines.append("  %-30s %-8d %-7d" % ("TOTAL", pass_tot, fail_tot))

    with open(args.out, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    print("\n".join(lines))
    print("\nwrote %s" % args.out)


if __name__ == "__main__":
    main()