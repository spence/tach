#!/usr/bin/env python3
"""Extract per-clock now()/elapsed() medians from a criterion output tree.

Usage: extract_speed.py <path-to-target/criterion>

Prints JSON {clock: {"now": ns, "elapsed": ns}} for the speed-bench clocks.
Thread-CPU entries additionally carry the provider label encoded by the Rust
benchmark ID, so a result records whether tach selected its inline or syscall
tier on that machine.
Every cell of the campaign (local, EC2, Docker-Alpine musl, Windows) funnels
through this so the extraction arithmetic is identical everywhere.
"""

import json
import sys
from pathlib import Path

WALL_FUNS = ["tach", "tach_ordered", "quanta", "fastant", "minstant", "std"]
# Criterion sanitizes the group label "Instant::now()" -> dir "Instant__now()"
# ("::" -> "__"); spaces / "+" / "()" are kept verbatim.
WALL_GROUPS = {"now": "Instant__now()", "elapsed": "Instant__now() + elapsed()"}
THREAD_CPU_GROUPS = {
    "now": "ThreadCpuInstant__now()",
    "elapsed": "ThreadCpuInstant__now() + elapsed()",
}

TACH_PROVIDER_LABELS = {
    "linux_perf_mmap": "Linux perf mmap",
    "posix_thread_cpu_clock": "POSIX thread CPU clock",
    "windows_thread_times": "Windows GetThreadTimes",
    "wasi_thread_cpu_clock": "WASI thread CPU clock",
    "performance_now": "Performance.now()",
    "monotonic_wall_clock": "monotonic wall clock",
    "other": "other",
}
TACH_CPU_PROVIDERS = {
    "linux_perf_mmap",
    "posix_thread_cpu_clock",
    "windows_thread_times",
    "wasi_thread_cpu_clock",
}
NATIVE_PROVIDER_LABELS = {
    "clock_gettime_clock_thread_cputime_id": "clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
    "clock_gettime_nsec_np_clock_thread_cputime_id": (
        "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)"
    ),
    "get_thread_times": "GetThreadTimes",
}


def median_estimate(criterion_dir: Path, group_dir: str, fn: str) -> dict:
    p = criterion_dir / group_dir / fn / "new" / "estimates.json"
    with open(p) as f:
        median = json.load(f)["median"]
    return {
        "point": median["point_estimate"],
        "ci95": [
            median["confidence_interval"]["lower_bound"],
            median["confidence_interval"]["upper_bound"],
        ],
    }


def add_estimate(entry: dict, kind: str, estimate: dict) -> None:
    entry[kind] = estimate["point"]
    entry[f"{kind}_ci95"] = estimate["ci95"]


def find_benchmark(criterion_dir: Path, group_dir: str, prefix: str) -> str:
  group = criterion_dir / group_dir
  matches = sorted(
    (
      (p / "new" / "estimates.json").stat().st_mtime_ns,
      p.name,
    )
    for p in group.iterdir()
    if p.is_dir()
    and (p.name == prefix or p.name.startswith(f"{prefix}__"))
    and (p / "new" / "estimates.json").exists()
  )
  if not matches:
    raise RuntimeError(
      f"expected a {prefix!r} benchmark under {group}, found none"
    )
  newest_time = matches[-1][0]
  newest = [name for modified, name in matches if modified == newest_time]
  if len(newest) != 1:
    raise RuntimeError(f"ambiguous newest {prefix!r} benchmark under {group}: {newest}")
  return newest[0]


def thread_cpu_entry(criterion_dir: Path, prefix: str) -> dict:
    benchmark = find_benchmark(criterion_dir, THREAD_CPU_GROUPS["now"], prefix)
    elapsed_benchmark = find_benchmark(
        criterion_dir, THREAD_CPU_GROUPS["elapsed"], prefix
    )
    if benchmark != elapsed_benchmark:
        raise RuntimeError(
            f"provider changed between thread-CPU groups: {benchmark} vs {elapsed_benchmark}"
        )

    suffix = benchmark.removeprefix(f"{prefix}__")
    entry = {}
    for kind, group in THREAD_CPU_GROUPS.items():
        add_estimate(entry, kind, median_estimate(criterion_dir, group, benchmark))
    if prefix == "tach_thread_cpu":
        provider_key, separator, cost = suffix.rpartition("__")
        if not separator:
            provider_key, cost = suffix, "unknown_cost"
        entry["provider"] = TACH_PROVIDER_LABELS.get(
            provider_key, provider_key.replace("_", " ")
        )
        entry["read_cost"] = cost.replace("_", " ")
        if provider_key in TACH_CPU_PROVIDERS:
            entry["time_domain"] = "thread CPU"
        else:
            entry["time_domain"] = "monotonic wall fallback"
    elif prefix == "direct_thread_cpu":
        entry["provider"] = "Linux perf mmap (cached direct handle)"
        entry["read_cost"] = "inline"
        entry["time_domain"] = "thread CPU"
    else:
        entry["provider"] = NATIVE_PROVIDER_LABELS.get(
            suffix, suffix.replace("_", " ")
        )
        entry["read_cost"] = "system call"
        entry["time_domain"] = "thread CPU"
    return entry


def main() -> None:
    criterion_dir = Path(sys.argv[1])
    out = {}
    for fn in WALL_FUNS:
        entry = {}
        for kind, group_dir in WALL_GROUPS.items():
            add_estimate(entry, kind, median_estimate(criterion_dir, group_dir, fn))
        out[fn] = entry
    out["tach_thread_cpu"] = thread_cpu_entry(criterion_dir, "tach_thread_cpu")
    out["native_thread_cpu"] = thread_cpu_entry(criterion_dir, "native_thread_cpu")
    selection = criterion_dir / "thread-cpu-selection.json"
    if selection.exists():
        selection_data = json.loads(selection.read_text())
        out["tach_thread_cpu"]["selection"] = selection_data
        if selection_data.get("decision") == "perf":
            out["direct_thread_cpu"] = thread_cpu_entry(
                criterion_dir, "direct_thread_cpu"
            )
    json.dump(out, sys.stdout, indent=2)
    print()


if __name__ == "__main__":
    main()
