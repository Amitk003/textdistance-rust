"""Reference side of the differential fuzz harness.

Runs inside a subprocess with the ORIGINAL textdistance package on sys.path
(given as the first argument). Reads JSON batches from stdin, one case per
line, and writes one JSON result line per batch. Keeping the original in a
separate process avoids any module clash with the port, which lives in the
parent process.

Each case is {"alg": str, "kwargs": dict, "s1": str, "s2": str}. Each result
is {"distance": repr, "similarity": repr, "normalized_distance": repr,
"normalized_similarity": repr, "maximum": repr} with an "error" key replacing
the value when a call raises.
"""

import json
import sys


def run_case(cls, kwargs, s1, s2):
    inst = cls(**kwargs)
    out = {}
    for name in ("distance", "similarity", "normalized_distance", "normalized_similarity"):
        try:
            out[name] = repr(getattr(inst, name)(s1, s2))
        except Exception as exc:  # noQA
            out[name] = "__RAISED__:" + type(exc).__name__
    try:
        out["maximum"] = repr(inst.maximum)
    except Exception as exc:  # noQA
        out["maximum"] = "__RAISED__:" + type(exc).__name__
    return out


def main():
    reference_path = sys.argv[1]
    sys.path.insert(0, reference_path)
    import textdistance as td

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        cases = json.loads(line)
        results = []
        for case in cases:
            cls = type(getattr(td, case["alg"]))
            try:
                results.append(run_case(cls, case["kwargs"], case["s1"], case["s2"]))
            except Exception as exc:  # noQA
                results.append({"error": type(exc).__name__})
        sys.stdout.write(json.dumps(results) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
