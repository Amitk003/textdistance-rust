"""One side of the benchmark harness.

Runs inside a subprocess and measures a single textdistance implementation,
either the port (installed package) or the original (reference clone put on
sys.path). Reads a JSON workload spec from stdin, writes a JSON result to
stdout. Both sides are measured with the exact same protocol in a fresh
process, so the numbers are comparable.

Spec (stdin): {"algorithms": [name, ...], "corpus": [[s1, s2], ...],
"warmup": int, "calls": int}

Result (stdout): {"implementation": str, "version": str, "import_ms": float,
"algorithms": {name: {"calls": int, "throughput": float (calls/s),
"p50_us": float, "p99_us": float, "max_us": float, "rss_mb": float}}}
"""

import ctypes
import json
import os
import sys
import time


def peak_rss_mb():
    """Peak working set in MB, Windows via the psapi API; None elsewhere."""
    if os.name != "nt":
        return None

    class PROCESS_MEMORY_COUNTERS(ctypes.Structure):
        _fields_ = [
            ("cb", ctypes.c_ulong),
            ("PageFaultCount", ctypes.c_ulong),
            ("PeakWorkingSetSize", ctypes.c_size_t),
            ("WorkingSetSize", ctypes.c_size_t),
            ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
            ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
            ("PagefileUsage", ctypes.c_size_t),
            ("PeakPagefileUsage", ctypes.c_size_t),
        ]

    counters = PROCESS_MEMORY_COUNTERS()
    counters.cb = ctypes.sizeof(PROCESS_MEMORY_COUNTERS)
    ctypes.windll.kernel32.GetCurrentProcess.restype = ctypes.c_void_p
    ctypes.windll.psapi.GetProcessMemoryInfo.argtypes = [
        ctypes.c_void_p, ctypes.POINTER(PROCESS_MEMORY_COUNTERS), ctypes.c_size_t]
    ctypes.windll.psapi.GetProcessMemoryInfo.restype = ctypes.c_int
    handle = ctypes.windll.kernel32.GetCurrentProcess()
    ctypes.windll.psapi.GetProcessMemoryInfo(
        handle, ctypes.byref(counters), counters.cb)
    return counters.PeakWorkingSetSize / (1024 * 1024)


def measure(algo_name, corpus, warmup, calls, td):
    algo = getattr(td, algo_name)
    n = len(corpus)
    for _ in range(warmup):
        for s1, s2 in corpus:
            algo.distance(s1, s2)

    samples = []
    start = time.perf_counter()
    for i in range(calls):
        s1, s2 = corpus[i % n]
        t0 = time.perf_counter_ns()
        algo.distance(s1, s2)
        t1 = time.perf_counter_ns()
        samples.append(t1 - t0)
    elapsed = time.perf_counter() - start

    samples.sort()
    k = len(samples)
    p50 = samples[k // 2] / 1000.0
    p99 = samples[min(k - 1, int(k * 0.99))] / 1000.0
    worst = samples[-1] / 1000.0
    return {
        "calls": k,
        "throughput": k / elapsed,
        "p50_us": p50,
        "p99_us": p99,
        "max_us": worst,
        "rss_mb": peak_rss_mb(),
    }


def main():
    impl = sys.argv[1]
    spec = json.load(sys.stdin)
    import_start = time.perf_counter()
    if impl != "port":
        sys.path.insert(0, impl)
    import textdistance as td

    version = td.__version__
    import_ms = (time.perf_counter() - import_start) * 1000.0

    results = {"implementation": "port" if impl == "port" else "reference",
               "version": version, "import_ms": import_ms, "algorithms": {}}
    for algo_name in spec["algorithms"]:
        results["algorithms"][algo_name] = measure(
            algo_name, spec["corpus"], spec["warmup"], spec["calls"], td)
    json.dump(results, sys.stdout)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
