#!/usr/bin/env python3
"""Extract per-clock now()/elapsed() medians from a criterion output tree.

Usage: extract_speed.py <path-to-target/criterion>

Prints JSON {clock: {"now": ns, "elapsed": ns}} for the speed-bench clocks.
Thread-CPU entries additionally carry the provider and read-cost labels encoded
by the Rust benchmark ID.
Runtime-selected ordered clocks include their selected provider and every
eligible exact direct-candidate `now()` row so dispatch overhead is explicit.
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
CRITERION_GROUP_IDS = {
    WALL_GROUPS["now"]: "Instant::now()",
    WALL_GROUPS["elapsed"]: "Instant::now() + elapsed()",
    THREAD_CPU_GROUPS["now"]: "ThreadCpuInstant::now()",
    THREAD_CPU_GROUPS["elapsed"]: "ThreadCpuInstant::now() + elapsed()",
}

TACH_PROVIDER_LABELS = {
    "linux_perf_mmap": "Linux perf task-clock mmap",
    "linux_perf_read": "Linux perf task-clock read",
    "posix_thread_cpu_clock": "POSIX thread CPU clock",
    "windows_thread_times": "Windows GetThreadTimes",
    "wasi_thread_cpu_clock": "WASI thread CPU clock",
    "node_thread_cpu_usage": "Node thread CPU usage",
    "performance_now": "performance.now",
    "node_hrtime": "process.hrtime.bigint",
    "monotonic_wall_clock": "monotonic wall clock",
    "unavailable": "unavailable",
    "other": "other",
}
TACH_CPU_PROVIDERS = {
    "linux_perf_mmap",
    "linux_perf_read",
    "posix_thread_cpu_clock",
    "windows_thread_times",
    "wasi_thread_cpu_clock",
    "node_thread_cpu_usage",
}
NATIVE_PROVIDER_LABELS = {
    "clock_gettime_clock_thread_cputime_id": "clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
    "inline_syscall_clock_thread_cputime_id": (
        "inline syscall(CLOCK_THREAD_CPUTIME_ID)"
    ),
    "clock_gettime_nsec_np_clock_thread_cputime_id": (
        "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)"
    ),
    "get_thread_times_current_thread_pseudohandle": (
        "GetThreadTimes(current-thread pseudo-handle)"
    ),
}


def criterion_benchmarks(
    criterion_dir: Path, group_dir: str
) -> list[tuple[int, str, Path]]:
    group = criterion_dir / group_dir
    if not group.is_dir():
        return []

    expected_group_id = CRITERION_GROUP_IDS.get(group_dir)
    benchmarks = []
    for directory in group.iterdir():
        estimates = directory / "new" / "estimates.json"
        if not directory.is_dir() or not estimates.exists():
            continue

        identity = directory.name
        metadata_path = directory / "new" / "benchmark.json"
        if metadata_path.exists():
            metadata = json.loads(metadata_path.read_text())
            group_id = metadata.get("group_id")
            function_id = metadata.get("function_id")
            full_id = metadata.get("full_id")
            if (
                not isinstance(group_id, str)
                or not isinstance(function_id, str)
                or full_id != f"{group_id}/{function_id}"
                or (expected_group_id is not None and group_id != expected_group_id)
            ):
                raise RuntimeError(
                    f"malformed Criterion benchmark identity in {metadata_path}"
                )
            identity = function_id
        benchmarks.append((estimates.stat().st_mtime_ns, identity, directory))
    return benchmarks


def find_exact_benchmark(
    criterion_dir: Path, group_dir: str, fn: str
) -> Path:
    group = criterion_dir / group_dir
    matches = [
        (modified, directory)
        for modified, identity, directory in criterion_benchmarks(
            criterion_dir, group_dir
        )
        if identity == fn
    ]
    if not matches:
        raise RuntimeError(f"expected benchmark {fn!r} under {group}, found none")
    newest_time = max(modified for modified, _ in matches)
    newest = [directory for modified, directory in matches if modified == newest_time]
    if len(newest) != 1:
        raise RuntimeError(
            f"ambiguous newest benchmark {fn!r} under {group}: "
            f"{[directory.name for directory in newest]}"
        )
    return newest[0]


def median_estimate(criterion_dir: Path, group_dir: str, fn: str) -> dict:
    directory = find_exact_benchmark(criterion_dir, group_dir, fn)
    with (directory / "new" / "estimates.json").open() as f:
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
    (modified, identity)
    for modified, identity, _ in criterion_benchmarks(criterion_dir, group_dir)
    if identity == prefix or identity.startswith(f"{prefix}__")
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


def has_benchmark(criterion_dir: Path, group_dir: str, prefix: str) -> bool:
  return any(
    identity == prefix or identity.startswith(f"{prefix}__")
    for _, identity, _ in criterion_benchmarks(criterion_dir, group_dir)
  )


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
    else:
        entry["provider"] = NATIVE_PROVIDER_LABELS.get(
            suffix, suffix.replace("_", " ")
        )
        entry["read_cost"] = "system call"
        entry["time_domain"] = "thread CPU"
    return entry


def add_thread_cpu_selector_evidence(criterion_dir: Path, out: dict) -> None:
    path = criterion_dir / "thread-cpu-selection.json"
    if not path.exists():
        return

    selection = json.loads(path.read_text())
    out["tach_thread_cpu"]["selection"] = selection
    if selection.get("selection_kind") == "fixed_native":
        add_fixed_native_thread_cpu_selector_evidence(
            criterion_dir, out, selection, path
        )
        return
    if selection.get("selection_kind") == "fixed_windows_thread_times":
        add_windows_thread_cpu_selector_evidence(criterion_dir, out, selection, path)
        return

    candidates = selection.get("eligible_direct_candidates")
    if not isinstance(candidates, list):
        raise RuntimeError(f"malformed thread-CPU candidates in {path}")
    native_probe = selection.get("native_entry_probe")
    perf = selection.get("perf")
    if not isinstance(native_probe, dict) or not isinstance(perf, dict):
        raise RuntimeError(f"malformed thread-CPU selector layers in {path}")
    mmap = perf.get("mmap")
    perf_read = perf.get("read")
    if not isinstance(mmap, dict) or not isinstance(perf_read, dict):
        raise RuntimeError(f"malformed perf thread-CPU provider layers in {path}")
    mmap_read_cost = mmap.get("read_cost")
    read_read_cost = perf_read.get("read_cost")
    if mmap_read_cost not in ("inline", "system call") or read_read_cost != "system call":
        raise RuntimeError(f"malformed perf thread-CPU read costs in {path}")
    native_candidates = []
    if "candidate_names" in native_probe:
        names = native_probe.get("candidate_names")
        eligible = native_probe.get("candidate_eligible")
        if (
            not isinstance(names, list)
            or not isinstance(eligible, list)
            or len(names) != len(eligible)
            or not all(isinstance(name, str) for name in names)
            or not all(type(available) is bool for available in eligible)
        ):
            raise RuntimeError(f"malformed generic native thread-CPU candidates in {path}")
        native_candidates.extend(
            f"direct_thread_cpu__{provider}"
            for provider, available in zip(names, eligible, strict=True)
            if available
        )
    else:
        for available, provider in (
            (native_probe.get("libc_available"), native_probe.get("libc_provider")),
            (native_probe.get("raw_available"), native_probe.get("raw_provider")),
        ):
            if type(available) is not bool or not isinstance(provider, str):
                raise RuntimeError(f"malformed native thread-CPU candidates in {path}")
            if available:
                native_candidates.append(f"direct_thread_cpu__{provider}")
    mmap_candidates = mmap.get("eligible_benchmarks")
    read_candidates = perf_read.get("eligible_benchmarks")
    if not all(
        isinstance(layer, list)
        and all(isinstance(candidate, str) for candidate in layer)
        for layer in (mmap_candidates, read_candidates)
    ):
        raise RuntimeError(f"malformed perf thread-CPU candidates in {path}")
    if candidates != [*native_candidates, *mmap_candidates, *read_candidates]:
        raise RuntimeError(f"thread-CPU candidate union disagrees with selector layers in {path}")

    selected_benchmark = selection.get("selected_native_benchmark")
    if not isinstance(selected_benchmark, str):
        raise RuntimeError(f"malformed selected thread-CPU benchmark in {path}")
    selected_provider = selection.get("selected_provider")
    selected_mechanism = selection.get("selected_mechanism")
    selected_cost = selection.get("selected_read_cost")
    if (
        selected_provider not in TACH_PROVIDER_LABELS
        or not isinstance(selected_mechanism, str)
        or selected_cost not in ("inline", "system call", "host call")
        or selected_benchmark != f"direct_selected_thread_cpu__{selected_mechanism}"
    ):
        raise RuntimeError(f"malformed selected thread-CPU identity in {path}")
    if (
        out["tach_thread_cpu"].get("provider") != TACH_PROVIDER_LABELS[selected_provider]
        or out["tach_thread_cpu"].get("read_cost") != selected_cost
    ):
        raise RuntimeError("thread-CPU introspection disagrees with selector metadata")

    fallback_benchmark = selection.get("fallback_native_benchmark")
    fallback_mechanism = selection.get("fallback_mechanism")
    fallback_cost = selection.get("fallback_read_cost")
    if fallback_benchmark is not None and (
        not isinstance(fallback_mechanism, str)
        or fallback_cost not in ("inline", "system call")
        or fallback_benchmark != f"direct_fallback_thread_cpu__{fallback_mechanism}"
    ):
        raise RuntimeError(f"malformed fallback thread-CPU identity in {path}")

    benchmarks = list(
        dict.fromkeys(
            [*candidates, selected_benchmark]
            + ([fallback_benchmark] if fallback_benchmark is not None else [])
        )
    )
    for benchmark in benchmarks:
        provider = benchmark.removeprefix("direct_selected_thread_cpu__").removeprefix(
            "direct_fallback_thread_cpu__"
        ).removeprefix(
            "direct_thread_cpu__"
        )
        if benchmark == selected_benchmark:
            read_cost = selected_cost
        elif benchmark == fallback_benchmark:
            read_cost = fallback_cost
        elif provider.startswith("linux_perf_mmap__"):
            read_cost = mmap_read_cost
        elif provider.startswith("linux_perf_read__"):
            read_cost = read_read_cost
        else:
            read_cost = "system call"
        entry = {
            "provider": provider,
            "read_cost": read_cost,
            "time_domain": "thread CPU",
            "benchmark": benchmark,
        }
        for metric, group in THREAD_CPU_GROUPS.items():
            add_estimate(
                entry,
                metric,
                median_estimate(criterion_dir, group, benchmark),
            )
        if benchmark == selected_benchmark:
            key = "direct_selected_thread_cpu"
        elif benchmark == fallback_benchmark:
            key = "direct_fallback_thread_cpu"
        else:
            key = benchmark
        out[key] = entry

    if out["direct_selected_thread_cpu"]["provider"] != selected_mechanism:
        raise RuntimeError("selected thread-CPU benchmark disagrees with selector metadata")
    if fallback_benchmark is not None and (
        out["direct_fallback_thread_cpu"]["provider"] != fallback_mechanism
    ):
        raise RuntimeError("fallback thread-CPU benchmark disagrees with selector metadata")


def add_fixed_native_thread_cpu_selector_evidence(
    criterion_dir: Path,
    out: dict,
    selection: dict,
    path: Path,
) -> None:
    mechanism = "macos_clock_gettime_nsec_np_thread_cpu"
    candidate = f"direct_thread_cpu__{mechanism}"
    selected_benchmark = f"direct_selected_thread_cpu__{mechanism}"
    expected_identity = {
        "selected_provider": "posix_thread_cpu_clock",
        "selected_mechanism": mechanism,
        "selected_read_cost": "system call",
        "selected_native_benchmark": selected_benchmark,
        "fallback_provider": None,
        "fallback_mechanism": None,
        "fallback_read_cost": None,
        "fallback_native_benchmark": None,
        "eligible_direct_candidates": [candidate],
    }
    if any(selection.get(key) != value for key, value in expected_identity.items()):
        raise RuntimeError(f"malformed fixed-native thread-CPU identity in {path}")

    fixed = selection.get("fixed_provider")
    expected_fixed = {
        "candidate": mechanism,
        "supported_architectures": ["x86_64", "aarch64"],
        "native_primitive": "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)",
        "time_domain": "thread CPU",
    }
    if (
        not isinstance(fixed, dict)
        or any(fixed.get(key) != value for key, value in expected_fixed.items())
        or not isinstance(fixed.get("selection_basis"), str)
        or not fixed["selection_basis"].strip()
        or not isinstance(selection.get("read_cost_basis"), str)
        or not selection["read_cost_basis"].strip()
        or selection.get("perf") is not None
        or selection.get("native_entry_probe") is not None
        or selection.get("failure_fallback") is not None
    ):
        raise RuntimeError(f"malformed fixed-native thread-CPU basis in {path}")

    public = out.get("tach_thread_cpu")
    if (
        not isinstance(public, dict)
        or public.get("provider") != TACH_PROVIDER_LABELS["posix_thread_cpu_clock"]
        or public.get("read_cost") != "system call"
        or public.get("time_domain") != "thread CPU"
    ):
        raise RuntimeError(
            "macOS thread-CPU introspection disagrees with fixed selector metadata"
        )

    for benchmark in (candidate, selected_benchmark):
        entry = {
            "provider": mechanism,
            "read_cost": "system call",
            "time_domain": "thread CPU",
            "benchmark": benchmark,
        }
        for metric, group in THREAD_CPU_GROUPS.items():
            add_estimate(
                entry,
                metric,
                median_estimate(criterion_dir, group, benchmark),
            )
        key = (
            "direct_selected_thread_cpu"
            if benchmark == selected_benchmark
            else benchmark
        )
        out[key] = entry


def add_windows_thread_cpu_selector_evidence(
    criterion_dir: Path,
    out: dict,
    selection: dict,
    path: Path,
) -> None:
    mechanism = "get_thread_times_current_thread_pseudohandle"
    candidate = f"direct_thread_cpu__{mechanism}"
    selected_benchmark = f"direct_selected_thread_cpu__{mechanism}"
    expected_identity = {
        "selected_provider": "windows_thread_times",
        "selected_mechanism": mechanism,
        "selected_read_cost": "system call",
        "selected_native_benchmark": selected_benchmark,
        "fallback_provider": None,
        "fallback_mechanism": None,
        "fallback_read_cost": None,
        "fallback_native_benchmark": None,
        "eligible_direct_candidates": [candidate],
    }
    if any(selection.get(key) != value for key, value in expected_identity.items()):
        raise RuntimeError(f"malformed fixed Windows thread-CPU identity in {path}")

    expected_guard = {
        "required_provider": "windows_thread_times",
        "required_read_cost": "system call",
        "stale_selection_removed_before_guard": True,
        "on_mismatch": "panic before thread-cpu-selection.json is written",
    }
    if selection.get("native_campaign_guard") != expected_guard:
        raise RuntimeError(f"malformed Windows native campaign guard in {path}")

    fixed = selection.get("fixed_provider")
    if (
        not isinstance(fixed, dict)
        or fixed.get("candidate") != mechanism
        or fixed.get("supported_architectures") != ["x86", "x86_64", "aarch64"]
        or not isinstance(fixed.get("selection_basis"), str)
        or not fixed["selection_basis"].strip()
        or not isinstance(fixed.get("authority"), str)
        or not fixed["authority"].startswith("https://learn.microsoft.com/")
    ):
        raise RuntimeError(f"malformed fixed Windows thread-CPU basis in {path}")

    failure_fallback = selection.get("failure_fallback")
    fallback_mechanism = "windows_selected_monotonic_wall_fallback"
    fallback_benchmark = f"direct_fallback_thread_cpu__{fallback_mechanism}"
    expected_fallback = {
        "provider": "monotonic_wall_clock",
        "mechanism": fallback_mechanism,
        "read_cost": "system call",
        "time_domain": "monotonic wall fallback",
        "trigger": "GetThreadTimes(current-thread pseudo-handle) returns zero",
        "state_transition": "sticky process-wide fallback",
        "eligible_for_thread_cpu_speed_claim": False,
        "exact_route_measured": True,
        "exact_benchmark": fallback_benchmark,
        "observed_as_public_provider_during_campaign": False,
        "campaign_behavior": (
            "an observed fallback aborts the native benchmark before extraction "
            "instead of emitting thread-CPU parity evidence"
        ),
    }
    if not isinstance(failure_fallback, dict):
        raise RuntimeError(f"malformed Windows thread-CPU failure fallback in {path}")
    if failure_fallback.get("observed_as_public_provider_during_campaign") is not False:
        raise RuntimeError(
            "observed Windows wall fallback cannot be extracted as thread-CPU speed evidence"
        )
    if failure_fallback != expected_fallback:
        raise RuntimeError(f"malformed Windows thread-CPU failure fallback in {path}")

    exclusions = selection.get("ineligible_direct_candidates")
    if not isinstance(exclusions, dict) or set(exclusions) != {
        "query_thread_cycle_time",
        "nt_query_information_thread",
    }:
        raise RuntimeError(f"malformed Windows thread-CPU exclusions in {path}")
    for exclusion in exclusions.values():
        if (
            not isinstance(exclusion, dict)
            or exclusion.get("eligibility") != "ineligible"
            or not isinstance(exclusion.get("reason"), str)
            or not exclusion["reason"].strip()
            or not isinstance(exclusion.get("authority"), str)
            or not exclusion["authority"].startswith("https://learn.microsoft.com/")
        ):
            raise RuntimeError(f"malformed Windows thread-CPU exclusion in {path}")

    public = out.get("tach_thread_cpu")
    if (
        not isinstance(public, dict)
        or public.get("provider") != TACH_PROVIDER_LABELS["windows_thread_times"]
        or public.get("read_cost") != "system call"
        or public.get("time_domain") != "thread CPU"
    ):
        raise RuntimeError("Windows thread-CPU introspection disagrees with fixed selector metadata")

    for benchmark in (candidate, selected_benchmark):
        entry = {
            "provider": mechanism,
            "read_cost": "system call",
            "time_domain": "thread CPU",
            "benchmark": benchmark,
        }
        for metric, group in THREAD_CPU_GROUPS.items():
            add_estimate(
                entry,
                metric,
                median_estimate(criterion_dir, group, benchmark),
            )
        key = "direct_selected_thread_cpu" if benchmark == selected_benchmark else benchmark
        out[key] = entry

    fallback_entry = {
        "provider": fallback_mechanism,
        "read_cost": "system call",
        "time_domain": "monotonic wall fallback",
        "benchmark": fallback_benchmark,
        "eligible_for_thread_cpu_speed_claim": False,
    }
    for metric, group in THREAD_CPU_GROUPS.items():
        add_estimate(
            fallback_entry,
            metric,
            median_estimate(criterion_dir, group, fallback_benchmark),
        )
    out["direct_failure_fallback_thread_cpu"] = fallback_entry


def wall_candidate_read_cost(benchmark: str) -> str:
    if "vdso_direct" in benchmark or "vdso_time64_direct" in benchmark:
        return "direct vDSO call"
    if "syscall" in benchmark:
        return "system call"
    if "clock_monotonic" in benchmark:
        return "vDSO or system call"
    return "inline"


def add_wall_selector_evidence(criterion_dir: Path, out: dict) -> None:
    for filename in (
        "linux-x86-wall-selection.json",
        "linux-aarch64-wall-selection.json",
        "residual-wall-selection.json",
        "apple-wall-selection.json",
        "windows-wall-selection.json",
    ):
        path = criterion_dir / filename
        if not path.exists():
            continue

        selection = json.loads(path.read_text())
        out["tach"]["selection"] = selection
        # `tach_ordered.selection` retains the architecture-protocol evidence
        # emitted by ordered-selection.json; this record proves the complete
        # OS/direct provider choice for the OrderedInstant domain.
        out["tach_ordered"]["wall_selection"] = selection
        candidates = selection.get("eligible_direct_candidates", {})
        if isinstance(candidates, list):
            candidates = {"instant": candidates, "ordered": []}
        if not isinstance(candidates, dict):
            raise RuntimeError(f"malformed eligible candidates in {path}")

        for domain in ("instant", "ordered"):
            domain_candidates = candidates.get(domain, [])
            if not isinstance(domain_candidates, list):
                raise RuntimeError(f"malformed {domain} candidates in {path}")
            for candidate in domain_candidates:
                if candidate in out:
                    entry = out[candidate]
                    entry["benchmark"] = candidate
                    if "now" not in entry:
                        add_estimate(
                            entry,
                            "now",
                            median_estimate(
                                criterion_dir, WALL_GROUPS["now"], candidate
                            ),
                        )
                    if "elapsed" not in entry:
                        add_estimate(
                            entry,
                            "elapsed",
                            median_estimate(
                                criterion_dir, WALL_GROUPS["elapsed"], candidate
                            ),
                        )
                    continue
                estimate = median_estimate(criterion_dir, WALL_GROUPS["now"], candidate)
                entry = {
                    "provider": candidate.removeprefix("direct_wall__").removeprefix(
                        "direct_ordered_wall__"
                    ).removeprefix("direct_ordered__"),
                    "read_cost": wall_candidate_read_cost(candidate),
                    "time_domain": f"{domain} wall",
                    "benchmark": candidate,
                }
                add_estimate(entry, "now", estimate)
                add_estimate(
                    entry,
                    "elapsed",
                    median_estimate(
                        criterion_dir, WALL_GROUPS["elapsed"], candidate
                    ),
                )
                out[candidate] = entry

        return


def add_selected_wall_evidence(criterion_dir: Path, out: dict) -> None:
    for prefix, domain in (
        ("direct_selected_wall", "instant"),
        ("direct_selected_ordered_wall", "ordered"),
    ):
        present = {
            metric: has_benchmark(criterion_dir, group, prefix)
            for metric, group in WALL_GROUPS.items()
        }
        if not any(present.values()):
            continue
        if not all(present.values()):
            raise RuntimeError(
                f"selected native {prefix!r} must exist in both wall groups"
            )
        benchmarks = {
            metric: find_benchmark(criterion_dir, group, prefix)
            for metric, group in WALL_GROUPS.items()
        }
        if benchmarks["now"] != benchmarks["elapsed"]:
            raise RuntimeError(
                "selected wall provider changed between groups: "
                f"{benchmarks['now']} vs {benchmarks['elapsed']}"
            )
        benchmark = benchmarks["now"]
        provider = benchmark.removeprefix(f"{prefix}__")
        if "vdso_direct" in provider or "vdso_time64_direct" in provider:
            read_cost = "direct vDSO call"
        elif "syscall" in provider:
            read_cost = "system call"
        elif "clock_monotonic" in provider or provider == "windows_qpc":
            read_cost = "platform call"
        else:
            read_cost = "inline"
        entry = {
            "provider": provider,
            "read_cost": read_cost,
            "time_domain": f"{domain} wall",
            "benchmark": benchmark,
        }
        for metric, group in WALL_GROUPS.items():
            add_estimate(
                entry,
                metric,
                median_estimate(criterion_dir, group, benchmark),
            )
        out[prefix] = entry


def main() -> None:
    criterion_dir = Path(sys.argv[1])
    out = {}
    for fn in WALL_FUNS:
        entry = {}
        for kind, group_dir in WALL_GROUPS.items():
            add_estimate(entry, kind, median_estimate(criterion_dir, group_dir, fn))
        out[fn] = entry
    out["tach"]["time_domain"] = "instant wall"
    out["tach_ordered"]["time_domain"] = "ordered wall"
    ordered_selection = criterion_dir / "ordered-selection.json"
    if ordered_selection.exists():
        selection_data = json.loads(ordered_selection.read_text())
        out["tach_ordered"]["selection"] = selection_data
        for candidate in selection_data.get("eligible_direct_candidates", []):
            estimate = median_estimate(
                criterion_dir, WALL_GROUPS["now"], candidate
            )
            entry = {
                "provider": candidate.removeprefix("direct_ordered__"),
                "read_cost": "inline",
                "time_domain": "ordered wall",
                "benchmark": candidate,
            }
            add_estimate(entry, "now", estimate)
            out[candidate] = entry
    add_wall_selector_evidence(criterion_dir, out)
    add_selected_wall_evidence(criterion_dir, out)
    out["tach_thread_cpu"] = thread_cpu_entry(criterion_dir, "tach_thread_cpu")
    out["native_thread_cpu"] = thread_cpu_entry(criterion_dir, "native_thread_cpu")
    add_thread_cpu_selector_evidence(criterion_dir, out)
    json.dump(out, sys.stdout, indent=2)
    print()


if __name__ == "__main__":
    main()
