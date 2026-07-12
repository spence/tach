"""Shared evidence rules for tach's three steady-state timing contracts."""

from __future__ import annotations

import math
from pathlib import Path
import random
import re
import statistics
import subprocess


LOCAL_COMPETITORS = ("quanta", "fastant", "minstant", "std")
METRICS = ("now", "elapsed")
BENCHMARK_FEATURES = ("bench-internal", "thread-cpu-inline")
STRICT_THREAD_CPU_PROVIDER_COSTS = {
  "Linux perf task-clock read": "system call",
  "POSIX thread CPU clock": "system call",
  "Windows GetThreadTimes": "system call",
  "WASI thread CPU clock": "host call",
  "Node thread CPU usage": "host call",
}
THREAD_CPU_PROVIDER_KEY_METADATA = {
  "linux_perf_read": ("Linux perf task-clock read", "system call"),
  "posix_thread_cpu_clock": ("POSIX thread CPU clock", "system call"),
  "windows_thread_times": ("Windows GetThreadTimes", "system call"),
  "wasi_thread_cpu_clock": ("WASI thread CPU clock", "host call"),
  "node_thread_cpu_usage": ("Node thread CPU usage", "host call"),
}


def perf_thread_cpu_read_cost(target_triple: str | None) -> str:
  """Classify a perf mmap read by the calls tach issues on its hot path."""
  return "inline"


def strict_thread_cpu_provider_cost(
  provider: str | None,
  target_triple: str | None,
) -> str | None:
  if provider == "Linux perf task-clock mmap":
    return perf_thread_cpu_read_cost(target_triple)
  return STRICT_THREAD_CPU_PROVIDER_COSTS.get(provider)


def thread_cpu_provider_key_metadata(
  provider_key: str | None,
  target_triple: str | None,
) -> tuple[str, str] | None:
  if provider_key == "linux_perf_mmap":
    return ("Linux perf task-clock mmap", perf_thread_cpu_read_cost(target_triple))
  return THREAD_CPU_PROVIDER_KEY_METADATA.get(provider_key)
THREAD_CPU_PERF_COUNTER_CANDIDATE_SETS = {
  (
    "x86_cpuid_rdtsc_cpuid",
    "x86_lfence_rdtsc_lfence",
    "x86_mfence_rdtsc_mfence",
    "x86_rdtscp_lfence",
    "x86_serialize_rdtsc_serialize",
  ),
  ("aarch64_isb_cntvct_isb", "aarch64_cntvctss_isb"),
  ("arm_isb_mrrc_cntvct_isb",),
  ("riscv_fence_rdtime_fence",),
}
THREAD_CPU_PERF_READ_ENTRY_CANDIDATE_SETS = {
  ("raw_read_syscall",),
  ("raw_read_svc",),
  ("raw_read_ecall",),
  ("raw_read_svc_3",),
  ("raw_read_syscall_0",),
  ("raw_read_int80", "libc_syscall_read", "libc_read"),
  ("raw_read_sc", "raw_read_scv", "libc_read"),
}
LAMBDA_INVOCATIONS = 5
LAMBDA_SAMPLES_PER_INVOCATION = 31
LAMBDA_SAMPLE_COUNT = LAMBDA_INVOCATIONS * LAMBDA_SAMPLES_PER_INVOCATION
LAMBDA_BOOTSTRAP_SAMPLES = 5_000
BENCHMARK_SOURCE_PATHS = (
  ".github/workflows/bench-speed-windows.yml",
  "Cargo.lock",
  "Cargo.toml",
  "src",
  "tests",
  "benches/bench_data.py",
  "benches/compose-speed.py",
  "benches/extract_speed.py",
  "benches/instant.rs",
  "benches/lambda-speed/Cargo.toml",
  "benches/lambda-speed/Cargo.lock",
  "benches/lambda-speed/src",
  "benches/require-clean-benchmark-source.sh",
  "benches/probes/aarch64-thread-pmu.c",
  "benches/route-coverage.toml",
  "benches/run-speed-aws.sh",
  "benches/run-speed-freebsd-aws.sh",
  "benches/run-speed-lambda.sh",
  "benches/run-speed-local.sh",
  "benches/run-thread-pmu-aws.sh",
  "benches/speed_evidence.py",
  "benches/summary.py",
  "benches/summary-thread-cpu.py",
  "benches/summary-use-cases.py",
  "benches/test_extract_speed.py",
  "benches/test_speed_evidence.py",
  "benches/validate-speed-evidence.py",
  "benches/validate-release-evidence.py",
  "benches/validate-supplemental-thread-cpu.py",
  "benches/verify-target-providers.py",
)
EXPECTED_ENVIRONMENTS = (
  (0, "Apple Silicon", "M1 Max MacBook Pro", "aarch64-apple-darwin", "criterion", "bench"),
  (1, "AWS Graviton 3", "c7g.large", "aarch64-unknown-linux-gnu", "criterion", "bench"),
  (2, "AWS Intel", "c7i.large", "x86_64-unknown-linux-gnu", "criterion", "bench"),
  (
    3,
    "AWS Intel (musl)",
    "c7i.large + Alpine",
    "x86_64-unknown-linux-musl",
    "criterion",
    "bench",
  ),
  (4, "GitHub Windows", "windows-2025", "x86_64-pc-windows-msvc", "criterion", "bench"),
  (
    5,
    "AWS Lambda",
    "provided.al2023 1024MB",
    "x86_64-unknown-linux-gnu",
    "lambda",
    "release",
  ),
)

SUPPLEMENTAL_SPEED_CELLS = {
  "speed-supplemental-macos-x86_64.json": (
    "x86_64-apple-darwin", "criterion", "full_speed_cell"
  ),
  "speed-supplemental-windows-i686.json": (
    "i686-pc-windows-msvc", "criterion", "full_speed_cell"
  ),
  "speed-supplemental-windows-aarch64.json": (
    "aarch64-pc-windows-msvc", "criterion", "full_speed_cell"
  ),
  "speed-supplemental-linux-i686.json": (
    "i686-unknown-linux-gnu", "criterion", "full_speed_cell"
  ),
  "speed-supplemental-freebsd-x86_64.json": (
    "x86_64-unknown-freebsd", "criterion", "full_speed_cell"
  ),
  "speed-supplemental-wasm-node.json": (
    "wasm32-unknown-unknown", "node-wasm-bindgen", "full_speed_cell"
  ),
  "speed-supplemental-emscripten-node.json": (
    "wasm32-unknown-emscripten", "emcc-node", "full_speed_cell"
  ),
  "speed-supplemental-wasi-p1-node.json": (
    "wasm32-wasip1", "node-uvwasi", "full_speed_cell"
  ),
  "speed-supplemental-wasi-p1-wasmtime.json": (
    "wasm32-wasip1", "wasmtime", "tagged_wall_fallback"
  ),
  "speed-supplemental-wasi-p2-wasmtime.json": (
    "wasm32-wasip2", "wasmtime-component", "tagged_wall_fallback"
  ),
  "speed-supplemental-browser-negative.json": (
    "wasm32-unknown-unknown", "browser", "tagged_wall_fallback"
  ),
  "speed-supplemental-wasip1-threads-smoke.json": (
    "wasm32-wasip1-threads", "wasi-threads-smoke", "runtime_smoke"
  ),
  "speed-supplemental-wasm32v1-none-smoke.json": (
    "wasm32v1-none", "wasm32v1-none-smoke", "runtime_smoke"
  ),
}

WINDOWS_QPC_AUTHORITY = (
  "https://learn.microsoft.com/en-us/windows/win32/sysinfo/"
  "acquiring-high-resolution-time-stamps"
)
WINDOWS_GET_THREAD_TIMES_AUTHORITY = (
  "https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/"
  "nf-processthreadsapi-getthreadtimes"
)
WINDOWS_QUERY_THREAD_CYCLE_TIME_AUTHORITY = (
  "https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/"
  "nf-realtimeapiset-querythreadcycletime"
)
WINDOWS_NT_QUERY_INFORMATION_THREAD_AUTHORITY = (
  "https://learn.microsoft.com/en-us/windows/win32/api/winternl/"
  "nf-winternl-ntqueryinformationthread"
)


def local_reference_eligibility(target_triple: str | None) -> dict[str, dict]:
  """Classify public competitors against Instant's target-specific contract."""
  eligibility = {
    name: {
      "eligible": True,
      "reason": None,
      "implementation": None,
    }
    for name in LOCAL_COMPETITORS
  }
  if not isinstance(target_triple, str) or "-windows-" not in target_triple:
    return eligibility

  for name in ("fastant", "minstant"):
    eligibility[name] = {
      "eligible": False,
      "reason": "windows_system_time_fallback_is_not_monotonic_qpc",
      "implementation": (
        f"{name} falls back to std::time::SystemTime on non-Linux-x86 targets"
      ),
    }

  architecture = target_triple.split("-", 1)[0]
  if architecture in ("x86_64", "aarch64"):
    raw_counter = "RDTSC" if architecture == "x86_64" else "CNTVCT_EL0"
    eligibility["quanta"] = {
      "eligible": False,
      "reason": "windows_raw_counter_does_not_meet_qpc_reliability_contract",
      "implementation": f"quanta selects calibrated {raw_counter} on this target",
    }

  return eligibility


def equivalence_allowance(reference: float) -> float:
  """Predeclared material-equivalence band for per-call nanoseconds."""
  return max(1.0, reference * 0.05)


def finite_number(value) -> bool:
  return type(value) in (int, float) and math.isfinite(value)


def exact_wall_candidate_identity(domain: str, benchmark: str) -> dict | None:
  prefix = "direct_wall__" if domain == "instant" else "direct_ordered_wall__"
  if (
    domain not in ("instant", "ordered")
    or not isinstance(benchmark, str)
    or not benchmark.startswith(prefix)
  ):
    return None
  if "vdso_direct" in benchmark or "vdso_time64_direct" in benchmark:
    read_cost = "direct vDSO call"
  elif "syscall" in benchmark:
    read_cost = "system call"
  elif "clock_monotonic" in benchmark:
    read_cost = "vDSO or system call"
  else:
    read_cost = "inline"
  return {
    "benchmark": benchmark,
    "provider": benchmark.removeprefix(prefix),
    "time_domain": f"{domain} wall",
    "read_cost": read_cost,
  }


def validate_ci(context: str, clock: str, entry: dict, failures: list[str]) -> None:
  for metric in METRICS:
    interval = entry.get(f"{metric}_ci95")
    point = entry.get(metric)
    if (
      not finite_number(point)
      or point < 0
      or not isinstance(interval, list)
      or len(interval) != 2
      or not all(finite_number(bound) for bound in interval)
      or not interval[0] <= point <= interval[1]
    ):
      failures.append(f"{context} {clock}: invalid {metric} estimate or 95% CI")


def equivalent_or_faster(subject: dict, reference: dict, metric: str) -> tuple[bool, float]:
  allowance = equivalence_allowance(reference[metric])
  point_passed = subject[metric] <= reference[metric] + allowance
  # This conservative comparison uses the slow edge of tach's interval and
  # the fast edge of the reference interval. Passing means the entire 95% CI
  # uncertainty still fits inside the predeclared material-equivalence band.
  ci_passed = subject[f"{metric}_ci95"][1] <= (
    reference[f"{metric}_ci95"][0] + allowance
  )
  return point_passed and ci_passed, allowance


def reproduce_material_decision(
  challenger: list[int],
  incumbent: list[int],
  reads_per_batch: int,
  required_wins: int,
  relative_denominator: int = 20,
  floor_ns_per_read: int = 1,
) -> dict:
  if (
    len(challenger) != 9
    or len(incumbent) != 9
    or not all(type(value) is int and value > 0 for value in challenger + incumbent)
    or type(reads_per_batch) is not int
    or reads_per_batch <= 0
    or type(relative_denominator) is not int
    or relative_denominator <= 0
    or type(floor_ns_per_read) is not int
    or floor_ns_per_read <= 0
  ):
    raise ValueError("malformed selector samples")
  challenger_median = int(statistics.median(challenger))
  incumbent_median = int(statistics.median(incumbent))
  allowance = max(reads_per_batch * floor_ns_per_read, incumbent_median // relative_denominator)
  decisive_wins = sum(
    candidate + allowance < baseline
    for candidate, baseline in zip(challenger, incumbent, strict=True)
  )
  selected = (
    challenger_median + allowance < incumbent_median
    and decisive_wins >= required_wins
  )
  return {
    "challenger_median_ns": challenger_median,
    "incumbent_median_ns": incumbent_median,
    "allowance_ns": allowance,
    "decisive_wins": decisive_wins,
    "selected": selected,
  }


def validate_windows_wall_selector(
  context: str,
  selection: dict,
  failures: list[str],
) -> dict:
  selected = selection.get("selected_provider")
  candidates = selection.get("eligible_direct_candidates")
  probe = selection.get("ordered_probe")
  if not isinstance(selected, dict) or not isinstance(candidates, dict):
    failures.append(f"{context}: malformed Windows wall selector metadata")
    return {}
  if selected.get("instant") != "windows_qpc":
    failures.append(f"{context}: Windows Instant did not select QPC")
  if candidates.get("instant") != ["direct_wall__windows_qpc"]:
    failures.append(f"{context}: Windows Instant eligible-candidate set is incomplete")
  if not isinstance(probe, dict):
    failures.append(f"{context}: malformed Windows Ordered probe")
    return {}

  expected_exclusion = (
    "windows_raw_tsc" if "cpuid_batches_ns" in probe else "windows_raw_cntvct_el0"
  )
  exclusions = selection.get("ineligible_direct_candidates")
  validated_exclusions = {}
  if not isinstance(exclusions, dict) or set(exclusions) != {expected_exclusion}:
    failures.append(f"{context}: Windows wall selector lacks exact raw-counter exclusions")
  else:
    exclusion = exclusions[expected_exclusion]
    if not isinstance(exclusion, dict):
      failures.append(f"{context}: malformed Windows {expected_exclusion} exclusion")
    else:
      if exclusion.get("contracts") != ["instant", "ordered"]:
        failures.append(
          f"{context}: Windows {expected_exclusion} exclusion has wrong contracts"
        )
      if exclusion.get("eligibility") != "ineligible":
        failures.append(
          f"{context}: Windows {expected_exclusion} exclusion is not ineligible"
        )
      if not isinstance(exclusion.get("reason"), str) or not exclusion["reason"].strip():
        failures.append(
          f"{context}: Windows {expected_exclusion} exclusion lacks a reason"
        )
      if exclusion.get("authority") != WINDOWS_QPC_AUTHORITY:
        failures.append(
          f"{context}: Windows {expected_exclusion} exclusion lacks QPC authority"
        )
      validated_exclusions[expected_exclusion] = exclusion

  if "cpuid_batches_ns" not in probe:
    expected = "windows_qpc_arm64_dmb_ishld_isb"
    if selected.get("ordered") != expected:
      failures.append(f"{context}: Windows Arm64 Ordered provider is not {expected}")
    if candidates.get("ordered") != [f"direct_ordered_wall__{expected}"]:
      failures.append(f"{context}: Windows Arm64 Ordered candidate set is incomplete")
    return {
      "winner": expected,
      "decisions": [],
      "ineligible_direct_candidates": validated_exclusions,
    }

  reads = probe.get("reads_per_batch")
  required = probe.get("required_decisive_wins")
  if type(required) is not int or required != 8:
    failures.append(f"{context}: Windows Ordered required-win rule changed")
    return {}
  incumbent_name = "windows_qpc_x86_cpuid"
  incumbent = probe.get("cpuid_batches_ns")
  expected_candidates = ["direct_ordered_wall__windows_qpc_x86_cpuid"]
  decisions = []
  for key, eligible_key, provider in (
    ("lfence_batches_ns", "lfence_eligible", "windows_qpc_x86_lfence"),
    ("rdtscp_batches_ns", "rdtscp_eligible", "windows_qpc_x86_rdtscp_lfence"),
    ("mfence_batches_ns", "mfence_eligible", "windows_qpc_x86_mfence"),
    ("serialize_batches_ns", "serialize_eligible", "windows_qpc_x86_serialize"),
  ):
    if probe.get(eligible_key) is True:
      expected_candidates.append(f"direct_ordered_wall__{provider}")
      try:
        decision = reproduce_material_decision(probe.get(key), incumbent, reads, required)
      except (TypeError, ValueError):
        failures.append(f"{context}: malformed Windows Ordered {provider} samples")
        continue
      decision["challenger"] = provider
      decision["incumbent"] = incumbent_name
      decisions.append(decision)
      if decision["selected"]:
        incumbent_name = provider
        incumbent = probe.get(key)
    elif probe.get(eligible_key) is not False:
      failures.append(f"{context}: malformed Windows Ordered {eligible_key}")

  if candidates.get("ordered") != expected_candidates:
    failures.append(f"{context}: Windows Ordered eligible-candidate set is incomplete")
  if selected.get("ordered") != incumbent_name or probe.get("selected_provider") != incumbent_name:
    failures.append(f"{context}: Windows Ordered selected provider does not reproduce")
  return {
    "winner": incumbent_name,
    "decisions": decisions,
    "ineligible_direct_candidates": validated_exclusions,
  }


def validate_apple_wall_selector(
  context: str,
  selection: dict,
  failures: list[str],
) -> dict:
  selected = selection.get("selected_provider")
  candidates = selection.get("eligible_direct_candidates")
  selected_benchmarks = selection.get("selected_native_benchmark")
  if not all(isinstance(value, dict) for value in (selected, candidates, selected_benchmarks)):
    failures.append(f"{context}: malformed Apple wall selector metadata")
    return {}
  probe = selection.get("probe")
  if isinstance(probe, dict) and all(
    isinstance(probe.get(domain), dict) for domain in ("instant", "ordered")
  ):
    results = {}
    for domain, direct_prefix, selected_prefix in (
      ("instant", "direct_wall", "direct_selected_wall"),
      ("ordered", "direct_ordered_wall", "direct_selected_ordered_wall"),
    ):
      domain_probe = probe[domain]
      mode = domain_probe.get("user_timebase_mode")
      continuous = domain_probe.get("continuous_hwclock")
      if type(mode) is not int or type(continuous) is not bool:
        failures.append(f"{context}: malformed Apple aarch64 {domain} eligibility")
        continue
      absolute_direct = {
        1: (
          "apple_commpage_cntvct_offset"
          if domain == "instant"
          else "apple_commpage_isb_cntvct_offset"
        ),
        2: "apple_commpage_cntvctss_offset",
        3: "apple_commpage_acntvct_offset",
      }.get(mode)
      continuous_direct = {
        2: "apple_continuous_hw_cntvctss_base",
        3: "apple_continuous_hw_acntvct_base",
      }.get(mode)
      if continuous_direct is None:
        continuous_direct = (
          "apple_continuous_hw_cntvct_base"
          if domain == "instant"
          else "apple_continuous_hw_isb_cntvct_base"
        )
      expected_providers = []
      if absolute_direct is not None:
        expected_providers.append(absolute_direct)
      expected_providers.append("apple_mach_absolute_time")
      if continuous:
        expected_providers.append(continuous_direct)
      expected_providers.append("apple_mach_continuous_time")
      declared = candidates.get(domain)
      expected_rows = [f"{direct_prefix}__{provider}" for provider in expected_providers]
      if declared != expected_rows:
        failures.append(f"{context}: Apple aarch64 {domain} candidate set is incomplete")

      count = domain_probe.get("candidate_count")
      evidence = domain_probe.get("candidates")
      reads = domain_probe.get("reads_per_batch")
      required = domain_probe.get("required_decisive_wins")
      floor_ticks = domain_probe.get("equivalence_floor_ticks_per_batch")
      relative = domain_probe.get("equivalence_relative_denominator")
      if (
        domain_probe.get("ready") is not True
        or count != len(expected_providers)
        or not isinstance(evidence, list)
        or len(evidence) < count
        or reads != 4_096
        or required != 8
        or type(floor_ticks) is not int
        or floor_ticks <= 0
        or relative != 20
      ):
        failures.append(f"{context}: malformed Apple aarch64 {domain} selector evidence")
        continue

      names = []
      batches = []
      valid = True
      for candidate in evidence[:count]:
        if not isinstance(candidate, dict):
          valid = False
          break
        name = candidate.get("provider")
        samples = candidate.get("batches_ticks")
        if (
          not isinstance(name, str)
          or not isinstance(samples, list)
          or len(samples) != 9
          or not all(type(sample) is int and sample > 0 for sample in samples)
          or candidate.get("median_ticks") != int(statistics.median(samples))
        ):
          valid = False
          break
        names.append(name)
        batches.append(samples)
      if not valid or names != expected_providers:
        failures.append(f"{context}: malformed Apple aarch64 {domain} candidate samples")
        continue

      incumbent_index = 0
      decisions = []
      for challenger_index in range(1, count):
        challenger = batches[challenger_index]
        incumbent = batches[incumbent_index]
        challenger_median = int(statistics.median(challenger))
        incumbent_median = int(statistics.median(incumbent))
        allowance = max(floor_ticks, incumbent_median // relative)
        decisive_wins = sum(
          challenger_sample + allowance < incumbent_sample
          for challenger_sample, incumbent_sample in zip(
            challenger, incumbent, strict=True
          )
        )
        challenger_selected = (
          challenger_median + allowance < incumbent_median
          and decisive_wins >= required
        )
        decisions.append({
          "challenger": names[challenger_index],
          "incumbent": names[incumbent_index],
          "allowance_ticks": allowance,
          "decisive_wins": decisive_wins,
          "challenger_selected": challenger_selected,
        })
        if challenger_selected:
          incumbent_index = challenger_index
      measured_winner = names[incumbent_index]
      if domain_probe.get("measured_winner") != measured_winner:
        failures.append(f"{context}: Apple aarch64 {domain} winner does not reproduce")
      selected_provider = domain_probe.get("selected_provider")
      basis = domain_probe.get("selection_basis")
      if basis == "runtime_measured_complete_public_path":
        if selected_provider != measured_winner:
          failures.append(f"{context}: Apple aarch64 {domain} selected a non-winner")
      elif basis == "same_thread_reentry_or_fork_safe_absolute_fallback":
        if selected_provider != "apple_mach_absolute_time":
          failures.append(f"{context}: Apple aarch64 {domain} safe fallback changed")
      else:
        failures.append(f"{context}: Apple aarch64 {domain} selection basis changed")
      if selected.get(domain) != selected_provider:
        failures.append(f"{context}: Apple aarch64 {domain} selected providers disagree")
      if selected_benchmarks.get(domain) != f"{selected_prefix}__{selected_provider}":
        failures.append(f"{context}: Apple {domain} selected benchmark is mislabeled")
      results[domain] = {
        "winner": selected_provider,
        "measured_winner": measured_winner,
        "decisions": decisions,
      }
    return results

  for domain, prefix in (
    ("instant", "direct_selected_wall"),
    ("ordered", "direct_selected_ordered_wall"),
  ):
    provider = selected.get(domain)
    expected_selected = f"{prefix}__{provider}"
    if selected_benchmarks.get(domain) != expected_selected:
      failures.append(f"{context}: Apple {domain} selected benchmark is mislabeled")
    declared = candidates.get(domain)
    if not isinstance(declared, list) or "native_wall__mach_absolute_time" not in declared:
      failures.append(f"{context}: Apple {domain} candidate set omits mach_absolute_time")
      continue
    if isinstance(provider, str) and "commpage" in provider:
      direct_prefix = "direct_wall" if domain == "instant" else "direct_ordered_wall"
      if f"{direct_prefix}__{provider}" not in declared:
        failures.append(f"{context}: Apple {domain} candidate set omits selected commpage path")
  return {"winner": selected, "decision": "XNU USER_TIMEBASE mode"}


def validate_legacy_native_thread_cpu_entry_probe(
  context: str,
  probe: dict,
  failures: list[str],
) -> dict:
  libc_provider = probe.get("libc_provider")
  raw_provider = probe.get("raw_provider")
  libc_available = probe.get("libc_available")
  raw_available = probe.get("raw_available")
  if (
    not isinstance(libc_provider, str)
    or not isinstance(raw_provider, str)
    or type(libc_available) is not bool
    or type(raw_available) is not bool
  ):
    failures.append(f"{context}: malformed native thread-CPU candidate identities")
    return {}

  candidates = []
  if libc_available:
    candidates.append(f"direct_thread_cpu__{libc_provider}")
  if raw_available:
    candidates.append(f"direct_thread_cpu__{raw_provider}")
  if not candidates:
    failures.append(f"{context}: thread-CPU selector found no strict native provider")
    return {}

  reads_per_batch = probe.get("reads_per_batch")
  required_wins = probe.get("required_decisive_wins")
  if reads_per_batch != 4096 or required_wins != 8:
    failures.append(f"{context}: native thread-CPU decision rule changed")

  raw_decision = None
  libc_decision = None
  if libc_available and raw_available:
    try:
      raw_decision = reproduce_material_decision(
        probe.get("raw_batches_ns"),
        probe.get("libc_batches_ns"),
        reads_per_batch,
        required_wins,
      )
      libc_decision = reproduce_material_decision(
        probe.get("libc_batches_ns"),
        probe.get("raw_batches_ns"),
        reads_per_batch,
        required_wins,
      )
    except (TypeError, ValueError):
      failures.append(f"{context}: malformed native thread-CPU selector samples")
      return {}
    expected = raw_provider if raw_decision["selected"] else libc_provider
    for prefix, decision in (("raw", raw_decision), ("libc", libc_decision)):
      expected_fields = {
        f"{prefix}_allowance_ns": decision["allowance_ns"],
        f"{prefix}_decisive_wins": decision["decisive_wins"],
      }
      if any(probe.get(key) != value for key, value in expected_fields.items()):
        failures.append(f"{context}: native thread-CPU {prefix} decision does not reproduce")
    if probe.get("raw_selected") != raw_decision["selected"]:
      failures.append(f"{context}: native thread-CPU raw winner flag does not reproduce")
    if probe.get("libc_materially_faster") != libc_decision["selected"]:
      failures.append(f"{context}: native thread-CPU libc reverse decision does not reproduce")
    if probe.get("libc_median_ns") != libc_decision["challenger_median_ns"]:
      failures.append(f"{context}: native thread-CPU libc median does not reproduce")
    if probe.get("raw_median_ns") != raw_decision["challenger_median_ns"]:
      failures.append(f"{context}: native thread-CPU raw median does not reproduce")
  elif libc_available:
    expected = libc_provider
  else:
    expected = raw_provider

  if not (libc_available and raw_available):
    for field in ("libc_batches_ns", "raw_batches_ns"):
      if probe.get(field) != [0] * 9:
        failures.append(f"{context}: unavailable native tournament has stale samples")
    for field in (
      "libc_median_ns",
      "raw_median_ns",
      "raw_allowance_ns",
      "raw_decisive_wins",
      "libc_allowance_ns",
      "libc_decisive_wins",
    ):
      if probe.get(field) != 0:
        failures.append(f"{context}: unavailable native tournament has stale {field}")
    if probe.get("raw_selected") is not False or probe.get("libc_materially_faster") is not False:
      failures.append(f"{context}: unavailable native tournament reports a measured winner")

  if probe.get("selected_provider") != expected:
    failures.append(f"{context}: native thread-CPU selected provider does not reproduce")
  if probe.get("selected_read_cost") != "system call":
    failures.append(f"{context}: native thread-CPU read-cost tier does not reproduce")
  return {
    "winner": expected,
    "candidates": candidates,
    "selected_read_cost": "system call",
    "raw_decision": raw_decision,
    "libc_reverse_decision": libc_decision,
  }


def validate_generic_native_thread_cpu_entry_probe(
  context: str,
  probe: dict,
  failures: list[str],
) -> dict:
  names = probe.get("candidate_names")
  eligible = probe.get("candidate_eligible")
  measured = probe.get("candidate_measured")
  if (
    not isinstance(names, list)
    or not names
    or not all(isinstance(name, str) and name for name in names)
    or len(set(names)) != len(names)
    or not isinstance(eligible, list)
    or len(eligible) != len(names)
    or not all(type(value) is bool for value in eligible)
    or not all(eligible)
    or not isinstance(measured, list)
    or len(measured) != len(names)
    or not all(type(value) is bool for value in measured)
  ):
    failures.append(f"{context}: malformed generic native thread-CPU candidates")
    return {}
  reads_per_batch = probe.get("reads_per_batch")
  required_wins = probe.get("required_decisive_wins")
  band = probe.get("equivalence_band")
  if (
    reads_per_batch != 4096
    or required_wins != 8
    or band != {"floor_ns_per_read": 1, "relative_denominator": 20}
  ):
    failures.append(f"{context}: generic native decision rule changed")
  expected_candidates = [
    f"direct_thread_cpu__{name}" for name in names
  ]

  batches = probe.get("candidate_batches_ns")
  decisions = []
  incumbent_index = 0
  if len(names) == 1:
    if probe.get("selection_kind") != "fixed_candidate" or batches is not None:
      failures.append(f"{context}: single available native route contains tournament samples")
  else:
    if not all(measured):
      failures.append(f"{context}: native tournament contains an unmeasured candidate")
    if probe.get("selection_kind") != "tournament":
      failures.append(f"{context}: multi-candidate native selector is not a tournament")
    if not isinstance(batches, list) or len(batches) != len(names):
      failures.append(f"{context}: malformed generic native tournament samples")
      return {}
    for index, samples in enumerate(batches):
      if not isinstance(samples, list) or len(samples) != 9 or not all(
        type(value) is int and value >= 0 for value in samples
      ):
        failures.append(f"{context}: malformed generic native samples for {names[index]}")
        return {}
      if 0 in samples:
        failures.append(f"{context}: eligible native route {names[index]} has zero samples")
    for challenger_index in range(1, len(names)):
      try:
        decision = reproduce_material_decision(
          batches[challenger_index],
          batches[incumbent_index],
          reads_per_batch,
          required_wins,
          relative_denominator=band.get("relative_denominator"),
          floor_ns_per_read=band.get("floor_ns_per_read"),
        )
      except (AttributeError, TypeError, ValueError):
        failures.append(f"{context}: malformed generic native tournament decision")
        return {}
      decision["challenger"] = names[challenger_index]
      decision["incumbent"] = names[incumbent_index]
      decisions.append(decision)
      if decision["selected"]:
        incumbent_index = challenger_index

  winner = names[incumbent_index]
  if probe.get("selected_candidate") != winner:
    failures.append(f"{context}: generic native winner does not reproduce")
  return {
    "winner": winner,
    "candidates": expected_candidates,
    "selected_read_cost": "system call",
    "decisions": decisions,
    "selection_kind": probe.get("selection_kind"),
  }


def validate_native_thread_cpu_entry_probe(
  context: str,
  probe: dict,
  failures: list[str],
) -> dict:
  if "candidate_names" in probe:
    return validate_generic_native_thread_cpu_entry_probe(context, probe, failures)
  return validate_legacy_native_thread_cpu_entry_probe(context, probe, failures)


def validate_thread_cpu_perf_counter_probe(
  context: str,
  probe: dict,
  failures: list[str],
) -> dict:
  names = probe.get("candidate_names")
  eligible = probe.get("candidate_eligible")
  batches = probe.get("candidate_batches_ns")
  if (
    not isinstance(names, list)
    or not isinstance(eligible, list)
    or len(names) not in (1, 2, 5)
    or len(eligible) != len(names)
    or not all(isinstance(name, str) and name for name in names)
    or not all(type(value) is bool for value in eligible)
  ):
    failures.append(f"{context}: malformed perf counter candidate metadata")
    return {}
  if tuple(names) not in THREAD_CPU_PERF_COUNTER_CANDIDATE_SETS:
    failures.append(f"{context}: perf counter candidate identities changed")
  selection_kind = probe.get("selection_kind", "tournament")
  if len(names) == 1:
    if (
      selection_kind != "fixed_candidate"
      or eligible != [True]
      or batches not in (None, [])
      or probe.get("reads_per_batch") is not None
      or probe.get("required_decisive_wins") is not None
    ):
      failures.append(f"{context}: malformed fixed perf counter evidence")
    winner = names[0]
    if probe.get("selected_candidate") != winner:
      failures.append(f"{context}: fixed perf counter identity disagrees")
    return {
      "winner": winner,
      "candidates": [f"direct_thread_cpu__linux_perf_mmap__{winner}"],
      "decisions": [],
      "selection_kind": "fixed_candidate",
    }
  if selection_kind != "tournament" or not isinstance(batches, list) or len(batches) != len(names):
    failures.append(f"{context}: malformed perf counter tournament metadata")
    return {}
  if probe.get("reads_per_batch") != 4096 or probe.get("required_decisive_wins") != 8:
    failures.append(f"{context}: perf counter decision rule changed")
  if probe.get("equivalence_band") != {"floor_ns_per_read": 1, "relative_denominator": 20}:
    failures.append(f"{context}: perf counter equivalence band changed")
  if eligible[0] is not True:
    failures.append(f"{context}: perf counter baseline is unavailable")

  expected_candidates = []
  decisions = []
  incumbent_index = None
  eligible_names = []
  for index, (name, is_eligible, samples) in enumerate(
    zip(names, eligible, batches, strict=True)
  ):
    if not isinstance(samples, list) or len(samples) != 9 or not all(
      type(value) is int and value >= 0 for value in samples
    ):
      failures.append(f"{context}: malformed perf counter samples for {name}")
      continue
    if not is_eligible:
      continue
    if name in eligible_names:
      failures.append(f"{context}: duplicate eligible perf counter identity {name}")
      continue
    eligible_names.append(name)
    expected_candidates.append(f"direct_thread_cpu__linux_perf_mmap__{name}")
    if incumbent_index is None:
      if 0 in samples:
        failures.append(f"{context}: malformed perf counter samples for {name}")
      incumbent_index = index
      continue
    try:
      decision = reproduce_material_decision(
        samples,
        batches[incumbent_index],
        probe.get("reads_per_batch"),
        probe.get("required_decisive_wins"),
      )
    except (TypeError, ValueError):
      failures.append(f"{context}: malformed perf counter tournament for {name}")
      continue
    decision["challenger"] = name
    decision["incumbent"] = names[incumbent_index]
    decisions.append(decision)
    if decision["selected"]:
      incumbent_index = index

  if incumbent_index is None:
    failures.append(f"{context}: perf counter tournament has no eligible candidate")
    return {}
  winner = names[incumbent_index]
  if probe.get("selected_candidate") != winner:
    failures.append(f"{context}: perf counter winner does not reproduce")
  return {
    "winner": winner,
    "candidates": expected_candidates,
    "decisions": decisions,
    "selection_kind": "tournament",
  }


def validate_thread_cpu_perf_read_entry_probe(
  context: str,
  probe: dict,
  failures: list[str],
) -> dict:
  names = probe.get("candidate_names")
  eligible = probe.get("candidate_eligible")
  measured = probe.get("candidate_measured")
  batches = probe.get("candidate_batches_ns")
  if (
    not isinstance(names, list)
    or tuple(names) not in THREAD_CPU_PERF_READ_ENTRY_CANDIDATE_SETS
    or not isinstance(eligible, list)
    or not isinstance(measured, list)
    or len(eligible) != len(names)
    or len(measured) != len(names)
    or not all(type(value) is bool for value in eligible + measured)
  ):
    failures.append(f"{context}: malformed perf-read entry metadata")
    return {}
  expected_candidates = [
    f"direct_thread_cpu__linux_perf_read__{name}"
    for name, available in zip(names, eligible, strict=True)
    if available
  ]
  if len(names) == 1:
    if (
      probe.get("selection_kind") != "fixed_candidate"
      or eligible != [True]
      or measured != [False]
      or batches is not None
    ):
      failures.append(f"{context}: malformed fixed perf-read ABI evidence")
    winner = names[0]
    decisions = []
  else:
    if (
      probe.get("selection_kind") != "tournament"
      or probe.get("reads_per_batch") != 4096
      or probe.get("required_decisive_wins") != 8
      or probe.get("equivalence_band")
      != {"floor_ns_per_read": 1, "relative_denominator": 20}
      or not isinstance(batches, list)
      or len(batches) != len(names)
      or measured != eligible
    ):
      failures.append(f"{context}: malformed perf-read ABI tournament")
      return {}
    incumbent_index = None
    decisions = []
    for index, (name, available, samples) in enumerate(
      zip(names, eligible, batches, strict=True)
    ):
      if not isinstance(samples, list) or len(samples) != 9 or not all(
        type(value) is int and value >= 0 for value in samples
      ):
        failures.append(f"{context}: malformed perf-read samples for {name}")
        return {}
      if not available:
        if any(samples):
          failures.append(f"{context}: unavailable perf-read ABI {name} has samples")
        continue
      if 0 in samples:
        failures.append(f"{context}: eligible perf-read ABI {name} has zero samples")
      if incumbent_index is None:
        incumbent_index = index
        continue
      try:
        decision = reproduce_material_decision(
          samples,
          batches[incumbent_index],
          probe.get("reads_per_batch"),
          probe.get("required_decisive_wins"),
        )
      except (TypeError, ValueError):
        failures.append(f"{context}: malformed perf-read ABI decision for {name}")
        return {}
      decision.update({"challenger": name, "incumbent": names[incumbent_index]})
      decisions.append(decision)
      if decision["selected"]:
        incumbent_index = index
    if incumbent_index is None:
      failures.append(f"{context}: perf-read ABI tournament has no eligible candidate")
      return {}
    winner = names[incumbent_index]
  if probe.get("selected_candidate") != winner:
    failures.append(f"{context}: perf-read ABI winner does not reproduce")
  return {
    "winner": winner,
    "candidates": expected_candidates,
    "decisions": decisions,
    "selection_kind": probe.get("selection_kind"),
  }


def replay_thread_cpu_paths(
  names: list[str],
  eligible: list[bool],
  batches: list[list[int]],
  reads_per_batch: int,
  required_wins: int,
  excluded: str | None = None,
) -> dict:
  incumbent_index = None
  decisions = []
  for index, (name, available, samples) in enumerate(
    zip(names, eligible, batches, strict=True)
  ):
    if not available or name == excluded:
      continue
    if incumbent_index is None:
      incumbent_index = index
      continue
    decision = reproduce_material_decision(
      samples,
      batches[incumbent_index],
      reads_per_batch,
      required_wins,
    )
    decision.update({"challenger": name, "incumbent": names[incumbent_index]})
    decisions.append(decision)
    if decision["selected"]:
      incumbent_index = index
  if incumbent_index is None:
    raise ValueError("thread-CPU path tournament has no eligible candidate")
  return {"winner": names[incumbent_index], "decisions": decisions}


def validate_thread_cpu_path_probe(
  context: str,
  probe: dict,
  failures: list[str],
) -> dict:
  names = probe.get("candidate_names")
  eligible = probe.get("candidate_eligible")
  batches = probe.get("candidate_batches_ns")
  if (
    probe.get("selection_kind") != "tournament_with_measured_runner_up"
    or names != ["posix_thread_cpu", "linux_perf_mmap", "linux_perf_read"]
    or not isinstance(eligible, list)
    or len(eligible) != 3
    or eligible[0] is not True
    or eligible[2] is not True
    or not all(type(value) is bool for value in eligible)
    or not isinstance(batches, list)
    or len(batches) != 3
    or probe.get("reads_per_batch") != 4096
    or probe.get("required_decisive_wins") != 8
    or probe.get("equivalence_band")
    != {"floor_ns_per_read": 1, "relative_denominator": 20}
  ):
    failures.append(f"{context}: malformed three-way thread-CPU path tournament")
    return {}
  for name, available, samples in zip(names, eligible, batches, strict=True):
    if not isinstance(samples, list) or len(samples) != 9 or not all(
      type(value) is int and value >= 0 for value in samples
    ):
      failures.append(f"{context}: malformed thread-CPU path samples for {name}")
      return {}
    if available and 0 in samples:
      failures.append(f"{context}: eligible thread-CPU path {name} has zero samples")
    if not available and any(samples):
      failures.append(f"{context}: unavailable thread-CPU path {name} has samples")
  try:
    selected = replay_thread_cpu_paths(
      names,
      eligible,
      batches,
      probe.get("reads_per_batch"),
      probe.get("required_decisive_wins"),
    )
    fallback = replay_thread_cpu_paths(
      names,
      eligible,
      batches,
      probe.get("reads_per_batch"),
      probe.get("required_decisive_wins"),
      excluded=selected["winner"],
    )
  except (TypeError, ValueError):
    failures.append(f"{context}: malformed three-way thread-CPU path decisions")
    return {}
  if probe.get("selected_candidate") != selected["winner"]:
    failures.append(f"{context}: thread-CPU path winner does not reproduce")
  if probe.get("fallback_candidate") != fallback["winner"]:
    failures.append(f"{context}: thread-CPU measured fallback does not reproduce")
  expected_capability_loss = eligible[1] and selected["winner"] != "linux_perf_mmap"
  if probe.get("capability_was_not_profitable") is not expected_capability_loss:
    failures.append(f"{context}: capability-vs-profitability evidence does not reproduce")
  return {"selected": selected, "fallback": fallback}


def validate_thread_cpu_selector(
  context: str,
  selection: dict,
  failures: list[str],
  target_triple: str | None = None,
) -> dict:
  if selection.get("selection_kind") == "fixed_windows_thread_times":
    return validate_windows_thread_cpu_selector(
      context, selection, failures, target_triple
    )
  if selection.get("selection_kind") == "fixed_native":
    return validate_fixed_native_thread_cpu_selector(
      context, selection, failures, target_triple
    )

  native_probe = selection.get("native_entry_probe")
  perf = selection.get("perf")
  candidates = selection.get("eligible_direct_candidates")
  if (
    not isinstance(native_probe, dict)
    or not isinstance(perf, dict)
    or not isinstance(candidates, list)
  ):
    failures.append(f"{context}: malformed thread-CPU selector metadata")
    return {}
  native = validate_native_thread_cpu_entry_probe(context, native_probe, failures)
  if not native:
    return {}

  event_available = perf.get("event_available")
  mmap = perf.get("mmap")
  read = perf.get("read")
  if type(event_available) is not bool or not isinstance(mmap, dict) or not isinstance(read, dict):
    failures.append(f"{context}: malformed perf provider evidence")
    return {}
  architecture = target_triple.split("-", 1)[0] if isinstance(target_triple, str) else None
  expected_mmap_support = architecture in (
    "x86_64",
    "i686",
    "aarch64",
    "arm",
    "armv7",
    "riscv64gc",
    "riscv64",
  ) if architecture is not None else mmap.get("supported_on_target")
  expected_read_support = (
    target_triple is not None
    and (
      "-unknown-linux-" in target_triple
      or target_triple.endswith("-linux-android")
      or target_triple.endswith("-linux-gnu")
      or target_triple.endswith("-linux-musl")
    )
    and architecture in (
      "x86_64", "i686", "aarch64", "arm", "armv7", "riscv64gc", "riscv64",
      "s390x", "loongarch64", "powerpc64", "powerpc64le",
    )
  ) if target_triple is not None else read.get("supported_on_target")
  if mmap.get("supported_on_target") is not expected_mmap_support:
    failures.append(f"{context}: perf-mmap target support classification changed")
  if read.get("supported_on_target") is not expected_read_support:
    failures.append(f"{context}: persistent perf-read target support classification changed")
  expected_mmap_cost = perf_thread_cpu_read_cost(target_triple)
  if mmap.get("read_cost") != expected_mmap_cost or read.get("read_cost") != "system call":
    failures.append(f"{context}: perf provider read-cost classification changed")

  counter = {}
  read_entry = {}
  path_replay = {}
  expected_mmap_candidates = []
  expected_read_candidates = []
  mmap_mechanism = None
  read_mechanism = None
  selected_path = "posix_thread_cpu"
  fallback_path = None
  if event_available:
    path_probe = perf.get("path_probe")
    read_probe = read.get("entry_probe")
    if not isinstance(path_probe, dict) or not isinstance(read_probe, dict):
      failures.append(f"{context}: available perf selector lacks path/read probes")
      return {}
    path_replay = validate_thread_cpu_path_probe(context, path_probe, failures)
    read_entry = validate_thread_cpu_perf_read_entry_probe(context, read_probe, failures)
    if not path_replay or not read_entry:
      return {}
    selected_path = path_replay["selected"]["winner"]
    fallback_path = path_replay["fallback"]["winner"]
    read_mechanism = f"linux_perf_read__{read_entry['winner']}"
    expected_read_candidates = read_entry["candidates"]
    mmap_available = path_probe["candidate_eligible"][1]
    if mmap.get("available") is not mmap_available:
      failures.append(f"{context}: perf-mmap capability evidence disagrees with path probe")
    if read.get("available") is not True:
      failures.append(f"{context}: open perf event lacks persistent read evidence")
    if mmap_available:
      counter_probe = mmap.get("counter_probe")
      if not isinstance(counter_probe, dict):
        failures.append(f"{context}: available perf-mmap path lacks counter evidence")
        return {}
      counter = validate_thread_cpu_perf_counter_probe(context, counter_probe, failures)
      if not counter:
        return {}
      mmap_mechanism = f"linux_perf_mmap__{counter['winner']}"
      expected_mmap_candidates = counter["candidates"]
    elif any(
      value not in (None, [], False)
      for value in (
        mmap.get("counter_probe"),
        mmap.get("selected_mechanism"),
        mmap.get("selected_candidate_benchmark"),
        mmap.get("eligible_benchmarks"),
      )
    ):
      failures.append(f"{context}: unavailable perf-mmap path contains candidate evidence")
  else:
    if (
      perf.get("path_probe") is not None
      or mmap.get("available") is not False
      or read.get("available") is not False
      or mmap.get("counter_probe") is not None
      or read.get("entry_probe") is not None
    ):
      failures.append(f"{context}: unavailable perf event contains measured provider evidence")

  expected_mmap_benchmark = (
    f"direct_thread_cpu__{mmap_mechanism}" if mmap_mechanism is not None else None
  )
  expected_read_benchmark = (
    f"direct_thread_cpu__{read_mechanism}" if read_mechanism is not None else None
  )
  expected_mmap_fields = {
    "selected_mechanism": mmap_mechanism,
    "selected_candidate_benchmark": expected_mmap_benchmark,
    "eligible_benchmarks": expected_mmap_candidates,
  }
  expected_read_fields = {
    "selected_mechanism": read_mechanism,
    "selected_candidate_benchmark": expected_read_benchmark,
    "eligible_benchmarks": expected_read_candidates,
  }
  if any(mmap.get(key) != value for key, value in expected_mmap_fields.items()):
    failures.append(f"{context}: perf-mmap exact candidate coverage does not reproduce")
  if any(read.get(key) != value for key, value in expected_read_fields.items()):
    failures.append(f"{context}: perf-read exact candidate coverage does not reproduce")

  expected_candidates = [
    *native["candidates"],
    *expected_mmap_candidates,
    *expected_read_candidates,
  ]
  if candidates != expected_candidates or len(candidates) != len(set(candidates)):
    failures.append(f"{context}: thread-CPU eligible candidate coverage is not exhaustive")

  def identity(path: str) -> tuple[str, str, str]:
    if path == "linux_perf_mmap":
      if mmap_mechanism is None:
        raise ValueError("selected unavailable perf-mmap path")
      return ("linux_perf_mmap", mmap_mechanism, expected_mmap_cost)
    if path == "linux_perf_read":
      if read_mechanism is None:
        raise ValueError("selected unavailable perf-read path")
      return ("linux_perf_read", read_mechanism, "system call")
    if path == "posix_thread_cpu":
      return ("posix_thread_cpu_clock", native["winner"], "system call")
    raise ValueError("unknown thread-CPU path")

  try:
    selected_provider, selected_mechanism, selected_cost = identity(selected_path)
    fallback_identity = identity(fallback_path) if fallback_path is not None else None
  except ValueError:
    failures.append(f"{context}: selected thread-CPU path has no exact mechanism")
    return {}
  expected_selected = {
    "selected_provider": selected_provider,
    "selected_mechanism": selected_mechanism,
    "selected_read_cost": selected_cost,
    "selected_native_benchmark": f"direct_selected_thread_cpu__{selected_mechanism}",
  }
  if any(selection.get(key) != value for key, value in expected_selected.items()):
    failures.append(f"{context}: thread-CPU public/exact winner does not reproduce")
  if fallback_identity is None:
    expected_fallback = {
      "fallback_provider": None,
      "fallback_mechanism": None,
      "fallback_read_cost": None,
      "fallback_native_benchmark": None,
    }
  else:
    fallback_provider, fallback_mechanism, fallback_cost = fallback_identity
    expected_fallback = {
      "fallback_provider": fallback_provider,
      "fallback_mechanism": fallback_mechanism,
      "fallback_read_cost": fallback_cost,
      "fallback_native_benchmark": f"direct_fallback_thread_cpu__{fallback_mechanism}",
    }
  if any(selection.get(key) != value for key, value in expected_fallback.items()):
    failures.append(f"{context}: thread-CPU measured fallback does not reproduce")

  expected_clock = (
    "raw SYS_clock_gettime(CLOCK_MONOTONIC_RAW), never libc/vDSO or the candidate under test"
  )
  if perf.get("measurement_clock") != expected_clock:
    failures.append(f"{context}: perf selector measurement clock changed")
  if not isinstance(perf.get("decision_rule"), str) or not perf["decision_rule"]:
    failures.append(f"{context}: perf selector decision rule is missing")
  return {
    "winner": selected_provider,
    "selected_mechanism": selected_mechanism,
    "selected_read_cost": selected_cost,
    "fallback": fallback_identity,
    "native_entry": native,
    "perf_counter": counter,
    "perf_read_entry": read_entry,
    "perf_path": path_replay,
  }


def validate_fixed_native_thread_cpu_selector(
  context: str,
  selection: dict,
  failures: list[str],
  target_triple: str | None = None,
) -> dict:
  """Validate a fixed native current-thread CPU clock without a tournament."""
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
    failures.append(f"{context}: fixed-native thread-CPU identity changed")

  expected_fixed = {
    "candidate": mechanism,
    "supported_architectures": ["x86_64", "aarch64"],
    "native_primitive": "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)",
    "time_domain": "thread CPU",
  }
  fixed = selection.get("fixed_provider")
  if not isinstance(fixed, dict) or any(
    fixed.get(key) != value for key, value in expected_fixed.items()
  ) or not isinstance(fixed.get("selection_basis"), str) or not fixed["selection_basis"].strip():
    failures.append(f"{context}: fixed-native thread-CPU basis is incomplete")

  if not isinstance(selection.get("read_cost_basis"), str) or not selection["read_cost_basis"].strip():
    failures.append(f"{context}: fixed-native thread-CPU read-cost basis is missing")

  if selection.get("perf") is not None or selection.get("native_entry_probe") is not None:
    failures.append(f"{context}: fixed-native thread-CPU selector must not declare a perf tournament")
  if selection.get("failure_fallback") is not None:
    failures.append(f"{context}: fixed-native thread-CPU selector must not declare a fallback")

  if target_triple is not None:
    architecture = target_triple.split("-", 1)[0]
    if target_triple not in {
      "x86_64-apple-darwin",
      "aarch64-apple-darwin",
    } or architecture not in expected_fixed["supported_architectures"]:
      failures.append(f"{context}: fixed-native thread-CPU selector used for unsupported target")

  return {
    "winner": "posix_thread_cpu_clock",
    "selected_mechanism": mechanism,
    "selected_read_cost": "system call",
    "fallback": None,
    "eligible_direct_candidates": [candidate],
  }


def validate_windows_thread_cpu_selector(
  context: str,
  selection: dict,
  failures: list[str],
  target_triple: str | None = None,
) -> dict:
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
    failures.append(f"{context}: fixed Windows thread-CPU identity changed")

  expected_guard = {
    "required_provider": "windows_thread_times",
    "required_read_cost": "system call",
    "stale_selection_removed_before_guard": True,
    "on_mismatch": "panic before thread-cpu-selection.json is written",
  }
  if selection.get("native_campaign_guard") != expected_guard:
    failures.append(f"{context}: Windows native campaign guard is incomplete")

  supported_architectures = ["x86", "x86_64", "aarch64"]
  expected_fixed = {
    "candidate": mechanism,
    "supported_architectures": supported_architectures,
    "selection_basis": (
      "GetThreadTimes is Windows' documented elapsed current-thread CPU timeline"
    ),
    "authority": WINDOWS_GET_THREAD_TIMES_AUTHORITY,
  }
  fixed = selection.get("fixed_provider")
  if fixed != expected_fixed:
    failures.append(f"{context}: malformed fixed Windows thread-CPU basis")

  if target_triple is not None:
    architecture = target_triple.split("-", 1)[0]
    normalized_architecture = "x86" if architecture == "i686" else architecture
    if (
      "-windows-" not in target_triple
      or normalized_architecture not in supported_architectures
    ):
      failures.append(
        f"{context}: fixed Windows thread-CPU selector used for unsupported target"
      )

  expected_failure_fallback = {
    "provider": "monotonic_wall_clock",
    "mechanism": "windows_selected_monotonic_wall_fallback",
    "read_cost": "system call",
    "time_domain": "monotonic wall fallback",
    "trigger": "GetThreadTimes(current-thread pseudo-handle) returns zero",
    "state_transition": "sticky process-wide fallback",
    "eligible_for_thread_cpu_speed_claim": False,
    "exact_route_measured": True,
    "exact_benchmark": (
      "direct_fallback_thread_cpu__windows_selected_monotonic_wall_fallback"
    ),
    "observed_as_public_provider_during_campaign": False,
    "campaign_behavior": (
      "an observed fallback aborts the native benchmark before extraction "
      "instead of emitting thread-CPU parity evidence"
    ),
  }
  failure_fallback = selection.get("failure_fallback")
  if (
    isinstance(failure_fallback, dict)
    and failure_fallback.get("observed_as_public_provider_during_campaign") is not False
  ):
    failures.append(
      f"{context}: observed Windows wall fallback cannot be thread-CPU speed evidence"
    )
  if failure_fallback != expected_failure_fallback:
    failures.append(f"{context}: Windows thread-CPU failure fallback is not explicit")

  expected_exclusions = {
    "query_thread_cycle_time": {
      "eligibility": "ineligible",
      "reason": (
        "implementation-dependent cycles cannot be converted to elapsed thread CPU time"
      ),
      "authority": WINDOWS_QUERY_THREAD_CYCLE_TIME_AUTHORITY,
    },
    "nt_query_information_thread": {
      "eligibility": "ineligible",
      "reason": (
        "the documented THREADINFOCLASS contract exposes no stable ThreadTimes class"
      ),
      "authority": WINDOWS_NT_QUERY_INFORMATION_THREAD_AUTHORITY,
    },
  }
  exclusions = selection.get("ineligible_direct_candidates")
  if exclusions != expected_exclusions:
    failures.append(f"{context}: Windows thread-CPU exclusions are incomplete")

  return {
    "winner": "windows_thread_times",
    "selected_mechanism": mechanism,
    "selected_read_cost": "system call",
    "fallback": failure_fallback,
    "eligible_direct_candidates": [candidate],
    "ineligible_direct_candidates": exclusions if isinstance(exclusions, dict) else {},
  }


def validate_tournament(
  context: str,
  candidate_names: list[str],
  candidate_batches: list[list[int]],
  steps: list[dict],
  reads_per_batch: int,
  required_wins: int,
  selected_provider: str,
  failures: list[str],
) -> dict:
  if (
    not candidate_names
    or len(candidate_names) != len(candidate_batches)
    or len(set(candidate_names)) != len(candidate_names)
    or len(steps) != len(candidate_names) - 1
  ):
    failures.append(f"{context}: malformed or incomplete tournament")
    return {}
  samples = dict(zip(candidate_names, candidate_batches, strict=True))
  incumbent = candidate_names[0]
  decisions = []
  for index, step in enumerate(steps):
    challenger = candidate_names[index + 1]
    if not isinstance(step, dict):
      failures.append(f"{context}: malformed tournament step {index}")
      continue
    if step.get("incumbent") != incumbent or step.get("challenger") != challenger:
      failures.append(f"{context}: tournament step {index} does not follow candidate order")
      continue
    try:
      decision = reproduce_material_decision(
        samples[challenger], samples[incumbent], reads_per_batch, required_wins
      )
    except (TypeError, ValueError):
      failures.append(f"{context}: malformed tournament samples at step {index}")
      continue
    winner = challenger if decision["selected"] else incumbent
    expected = {
      "allowance_ns": decision["allowance_ns"],
      "decisive_wins": decision["decisive_wins"],
      "challenger_selected": decision["selected"],
      "winner": winner,
    }
    if any(step.get(key) != value for key, value in expected.items()):
      failures.append(f"{context}: tournament decision {index} does not reproduce")
    decision.update({"challenger": challenger, "incumbent": incumbent, "winner": winner})
    decisions.append(decision)
    incumbent = winner
  if incumbent != selected_provider:
    failures.append(
      f"{context}: reproduced tournament winner {incumbent!r} != {selected_provider!r}"
    )
  return {"winner": incumbent, "decisions": decisions}


def validate_linux_x86_wall_selector(
  context: str,
  selection: dict,
  failures: list[str],
) -> dict:
  selected = selection.get("selected_provider")
  candidates = selection.get("eligible_direct_candidates")
  selected_benchmarks = selection.get("selected_native_benchmark")
  probe = selection.get("probe")
  if not all(isinstance(value, dict) for value in (selected, candidates, selected_benchmarks, probe)):
    failures.append(f"{context}: malformed Linux x86 wall selector metadata")
    return {}
  reads = probe.get("reads_per_batch")
  required = probe.get("required_decisive_wins")
  if required != 8:
    failures.append(f"{context}: Linux x86 required-win rule changed")
  output = {}
  for domain, direct_prefix, selected_prefix in (
    ("instant", "direct_wall", "direct_selected_wall"),
    ("ordered", "direct_ordered_wall", "direct_selected_ordered_wall"),
  ):
    count = probe.get(f"{domain}_candidate_count")
    all_names = probe.get(f"{domain}_candidate_names")
    all_eligible = probe.get(f"{domain}_candidate_eligible")
    all_batches = probe.get(f"{domain}_candidate_batches_ns")
    all_medians = probe.get(f"{domain}_candidate_medians_ns")
    decision_count = probe.get(f"{domain}_tournament_decision_count")
    if (
      type(count) is not int
      or count <= 0
      or not all(isinstance(value, list) for value in (
        all_names, all_eligible, all_batches, all_medians,
      ))
      or count > min(len(all_names), len(all_eligible), len(all_batches), len(all_medians))
      or decision_count != count - 1
    ):
      failures.append(f"{context}: malformed Linux x86 {domain} candidate evidence")
      continue
    names = all_names[:count]
    batches = all_batches[:count]
    if not all(all_eligible[:count]):
      failures.append(f"{context}: Linux x86 {domain} published an ineligible candidate")
    for name, samples, recorded_median in zip(names, batches, all_medians[:count], strict=True):
      if isinstance(samples, list) and len(samples) == 9:
        if recorded_median != int(statistics.median(samples)):
          failures.append(f"{context}: Linux x86 {domain} median changed for {name}")
    declared = candidates.get(domain)
    expected_candidates = [f"{direct_prefix}__{name}" for name in names]
    if declared != expected_candidates:
      failures.append(f"{context}: Linux x86 {domain} candidate set is incomplete")
    provider = selected.get(domain)
    if selected_benchmarks.get(domain) != f"{selected_prefix}__{provider}":
      failures.append(f"{context}: Linux x86 {domain} selected benchmark is mislabeled")
    steps = [
      {
        "challenger": challenger,
        "incumbent": incumbent,
        "winner": winner,
        "allowance_ns": allowance,
        "decisive_wins": wins,
        "challenger_selected": challenger_selected,
      }
      for challenger, incumbent, winner, allowance, wins, challenger_selected in zip(
        probe.get(f"{domain}_tournament_challengers", [])[:decision_count],
        probe.get(f"{domain}_tournament_incumbents", [])[:decision_count],
        probe.get(f"{domain}_tournament_winners", [])[:decision_count],
        probe.get(f"{domain}_tournament_allowances_ns", [])[:decision_count],
        probe.get(f"{domain}_tournament_decisive_wins", [])[:decision_count],
        probe.get(f"{domain}_tournament_challenger_selected", [])[:decision_count],
        strict=True,
      )
    ]
    output[domain] = validate_tournament(
      f"{context}: Linux x86 {domain}", names, batches, steps, reads, required, provider, failures
    )

    eligibility = probe.get(f"{domain}_eligibility")
    if eligibility == "pr_get_tsc_not_enabled":
      if any("syscall" not in name or "rdtscp" in name for name in names):
        failures.append(f"{context}: Linux x86 denied-TSC {domain} retained a faulting path")
  return output


def validate_linux_aarch64_wall_selector(
  context: str,
  selection: dict,
  failures: list[str],
) -> dict:
  selected = selection.get("selected_provider")
  candidates = selection.get("eligible_direct_candidates")
  selected_benchmarks = selection.get("selected_native_benchmark")
  if not all(isinstance(value, dict) for value in (selected, candidates, selected_benchmarks)):
    failures.append(f"{context}: malformed Linux aarch64 wall selector metadata")
    return {}
  if not isinstance(selection.get("permission_rule"), str):
    failures.append(f"{context}: Linux aarch64 selector lacks its permission rule")
  output = {}
  permission_probes = {}
  specifications = {
    "instant": (
      selection.get("instant_probe"),
      "direct_wall",
      "direct_selected_wall",
      {
        "aarch64_cntvct": "cntvct_batches_ns",
        "linux_clock_monotonic": "clock_batches_ns",
        "linux_clock_monotonic_raw": "clock_raw_batches_ns",
        "linux_clock_monotonic_vdso_direct": "vdso_batches_ns",
        "linux_clock_monotonic_raw_vdso_direct": "vdso_raw_batches_ns",
        "linux_clock_monotonic_syscall": "syscall_batches_ns",
        "linux_clock_monotonic_raw_syscall": "syscall_raw_batches_ns",
      },
    ),
    "ordered": (
      selection.get("ordered_probe"),
      "direct_ordered_wall",
      "direct_selected_ordered_wall",
      {
        "aarch64_isb_cntvct": "isb_batches_ns",
        "aarch64_cntvctss": "cntvctss_batches_ns",
        "linux_clock_monotonic": "clock_batches_ns",
        "linux_clock_monotonic_raw": "clock_raw_batches_ns",
        "linux_clock_monotonic_vdso_direct": "vdso_batches_ns",
        "linux_clock_monotonic_raw_vdso_direct": "vdso_raw_batches_ns",
        "linux_clock_monotonic_syscall": "syscall_batches_ns",
        "linux_clock_monotonic_raw_syscall": "syscall_raw_batches_ns",
      },
    ),
  }
  for domain, (probe, direct_prefix, selected_prefix, sample_fields) in specifications.items():
    if not isinstance(probe, dict):
      failures.append(f"{context}: malformed Linux aarch64 {domain} probe")
      continue
    permission = {
      field: probe.get(field)
      for field in (
        "eligibility",
        "permission_basis",
        "pr_get_tsc_status",
        "kernel_version_known",
        "kernel_version_major",
        "kernel_version_minor",
      )
    }
    permission_probes[domain] = permission
    basis = permission["permission_basis"]
    status = permission["pr_get_tsc_status"]
    known = permission["kernel_version_known"]
    major = permission["kernel_version_major"]
    minor = permission["kernel_version_minor"]
    version_fields_valid = (
      type(known) is bool
      and type(major) is int
      and type(minor) is int
      and major >= 0
      and minor >= 0
    )
    if not version_fields_valid:
      failures.append(f"{context}: malformed Linux aarch64 {domain} kernel evidence")
    elif not known and (major != 0 or minor != 0):
      failures.append(f"{context}: Linux aarch64 {domain} unknown kernel has a version")

    if permission["eligibility"] == "eligible":
      if basis == "pr_get_tsc_enabled":
        if status != 0:
          failures.append(f"{context}: Linux aarch64 {domain} enabled query did not succeed")
      elif basis == "legacy_kernel_without_pr_get_tsc":
        predates_control = (
          version_fields_valid
          and known
          and major >= 3
          and (major < 6 or (major == 6 and minor < 12))
        )
        if status != -22 or not predates_control:
          failures.append(
            f"{context}: Linux aarch64 {domain} legacy inference lacks exact pre-6.12 proof"
          )
      else:
        failures.append(f"{context}: Linux aarch64 {domain} has an invalid eligible basis")
    elif permission["eligibility"] == "pr_get_tsc_not_enabled":
      if basis not in ("pr_get_tsc_not_enabled", "pr_get_tsc_unknown_mode") or status != 0:
        failures.append(f"{context}: Linux aarch64 {domain} has invalid denied evidence")
    elif permission["eligibility"] == "pr_get_tsc_unavailable":
      opaque_evidence_valid = status != 0 and basis in (
        "pr_get_tsc_failed",
        "new_kernel_without_observable_pr_get_tsc",
        "kernel_release_unknown",
      )
      if basis == "kernel_release_unknown":
        opaque_evidence_valid = opaque_evidence_valid and known is False
      elif basis == "new_kernel_without_observable_pr_get_tsc":
        opaque_evidence_valid = (
          opaque_evidence_valid
          and version_fields_valid
          and known
          and not (major >= 3 and (major < 6 or (major == 6 and minor < 12)))
        )
      elif basis == "pr_get_tsc_failed":
        opaque_evidence_valid = (
          opaque_evidence_valid
          and version_fields_valid
          and known
          and major >= 3
          and (major < 6 or (major == 6 and minor < 12))
          and status != -22
        )
      if not opaque_evidence_valid:
        failures.append(f"{context}: Linux aarch64 {domain} has invalid opaque evidence")
    else:
      failures.append(f"{context}: Linux aarch64 {domain} has unknown permission eligibility")
    declared = candidates.get(domain)
    if not isinstance(declared, list):
      failures.append(f"{context}: malformed Linux aarch64 {domain} candidate set")
      continue
    names = [candidate.removeprefix(f"{direct_prefix}__") for candidate in declared]
    if probe.get("candidate_count") != len(names) or len(set(names)) != len(names):
      failures.append(f"{context}: Linux aarch64 {domain} candidate set is incomplete")
      continue
    try:
      batches = [probe[sample_fields[name]] for name in names]
    except (KeyError, TypeError):
      failures.append(f"{context}: Linux aarch64 {domain} has an unknown candidate")
      continue
    provider = selected.get(domain)
    if probe.get("selected_provider") != provider:
      failures.append(f"{context}: Linux aarch64 {domain} selected providers disagree")
    if selected_benchmarks.get(domain) != f"{selected_prefix}__{provider}":
      failures.append(f"{context}: Linux aarch64 {domain} selected benchmark is mislabeled")
    step_count = probe.get("tournament_step_count")
    all_steps = probe.get("tournament_steps")
    if type(step_count) is not int or not isinstance(all_steps, list):
      failures.append(f"{context}: malformed Linux aarch64 {domain} tournament")
      continue
    output[domain] = validate_tournament(
      f"{context}: Linux aarch64 {domain}",
      names,
      batches,
      all_steps[:step_count],
      probe.get("reads_per_batch"),
      probe.get("required_decisive_wins"),
      provider,
      failures,
    )
    if probe.get("eligibility") != "eligible" and any("syscall" not in name for name in names):
      failures.append(f"{context}: Linux aarch64 denied-counter {domain} retained a faulting path")
  if set(permission_probes) == {"instant", "ordered"}:
    if permission_probes["instant"] != permission_probes["ordered"]:
      failures.append(f"{context}: Linux aarch64 wall selectors disagree on counter permission")
  return output


def validate_residual_wall_metadata(
  context: str,
  selection: dict,
  clocks: dict,
  failures: list[str],
) -> tuple[dict, dict, dict] | None:
  selected = selection.get("selected_provider")
  candidates = selection.get("eligible_direct_candidates")
  selected_benchmarks = selection.get("selected_native_benchmark")
  probe = selection.get("probe")
  if not all(
    isinstance(value, dict)
    for value in (selected, candidates, selected_benchmarks, probe)
  ):
    failures.append(f"{context}: malformed residual wall selector metadata")
    return None
  for domain, direct_prefix, selected_prefix in (
    ("instant", "direct_wall", "direct_selected_wall"),
    ("ordered", "direct_ordered_wall", "direct_selected_ordered_wall"),
  ):
    provider = selected.get(domain)
    declared = candidates.get(domain)
    if (
      not isinstance(provider, str)
      or not isinstance(declared, list)
      or not declared
      or len(set(declared)) != len(declared)
      or not all(
        isinstance(candidate, str)
        and candidate.startswith(f"{direct_prefix}__")
        for candidate in declared
      )
    ):
      failures.append(f"{context}: malformed residual {domain} candidate set")
      continue
    if f"{direct_prefix}__{provider}" not in declared:
      failures.append(f"{context}: residual {domain} winner is not an eligible candidate")
    expected_selected = f"{selected_prefix}__{provider}"
    if selected_benchmarks.get(domain) != expected_selected:
      failures.append(f"{context}: residual {domain} selected benchmark is mislabeled")
    for candidate in declared:
      row = clocks.get(candidate)
      if not isinstance(row, dict):
        failures.append(f"{context}: residual selector lacks exact row {candidate}")
        continue
      validate_ci(context, candidate, row, failures)
  return selected, candidates, probe


def replay_residual_staged_domain(
  context: str,
  probe: dict,
  baseline_provider: str,
  baseline_batches_field: str,
  stages: list[tuple[str, str, str, bool]],
  selected_provider: str,
  failures: list[str],
) -> dict:
  reads = probe.get("reads_per_batch")
  required = probe.get("required_decisive_wins")
  if reads != 4096 or required != 8:
    failures.append(f"{context}: residual wall decision rule changed")
    return {}
  incumbent = baseline_provider
  incumbent_batches = probe.get(baseline_batches_field)
  decisions = []
  for challenger, batches_field, decision_prefix, eligible in stages:
    if not eligible:
      continue
    challenger_batches = probe.get(batches_field)
    try:
      decision = reproduce_material_decision(
        challenger_batches, incumbent_batches, reads, required
      )
    except (TypeError, ValueError):
      failures.append(f"{context}: malformed residual samples for {challenger}")
      continue
    if (
      probe.get(f"{decision_prefix}_allowance_ns") != decision["allowance_ns"]
      or probe.get(f"{decision_prefix}_decisive_wins")
      != decision["decisive_wins"]
    ):
      failures.append(f"{context}: residual decision for {challenger} does not reproduce")
    decision.update({"challenger": challenger, "incumbent": incumbent})
    if decision["selected"]:
      incumbent = challenger
      incumbent_batches = challenger_batches
    decision["winner"] = incumbent
    decisions.append(decision)
  if probe.get("selected_provider") != selected_provider:
    failures.append(f"{context}: residual probe and envelope winners disagree")
  if incumbent != selected_provider:
    failures.append(
      f"{context}: reproduced residual winner {incumbent!r} != {selected_provider!r}"
    )
  return {"winner": incumbent, "decisions": decisions}


def residual_declared_providers(candidates: dict, domain: str) -> list[str]:
  prefix = "direct_wall__" if domain == "instant" else "direct_ordered_wall__"
  return [candidate.removeprefix(prefix) for candidate in candidates.get(domain, [])]


def validate_loongarch_wall_selector(
  context: str,
  selected: dict,
  candidates: dict,
  probe: dict,
  failures: list[str],
) -> dict:
  output = {}
  for domain in ("instant", "ordered"):
    domain_probe = probe.get(domain)
    if not isinstance(domain_probe, dict):
      failures.append(f"{context}: malformed LoongArch {domain} wall probe")
      continue
    expected = [
      "loongarch_stable_counter",
      "linux_clock_monotonic",
      "linux_clock_monotonic_syscall",
    ]
    if domain_probe.get("clock_raw_available") is True:
      expected.append("linux_clock_monotonic_raw")
    if domain_probe.get("syscall_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_syscall")
    if domain_probe.get("vdso_available") is True:
      expected.append("linux_clock_monotonic_vdso_direct")
    if domain_probe.get("vdso_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_vdso_direct")
    if domain_probe.get("clock_boottime_available") is True:
      expected.append("linux_clock_boottime")
    if domain_probe.get("syscall_boottime_available") is True:
      expected.append("linux_clock_boottime_syscall")
    if domain_probe.get("vdso_boottime_available") is True:
      expected.append("linux_clock_boottime_vdso_direct")
    if residual_declared_providers(candidates, domain) != expected:
      failures.append(f"{context}: LoongArch {domain} candidate set is incomplete")
    if domain_probe.get("candidate_count") != len(expected):
      failures.append(f"{context}: LoongArch {domain} probe candidate count changed")
    stages = [
      (
        "linux_clock_monotonic_syscall",
        "syscall_batches_ns",
        "fallback",
        True,
      ),
      (
        "linux_clock_monotonic_raw",
        "clock_raw_batches_ns",
        "clock_raw",
        domain_probe.get("clock_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_syscall",
        "syscall_raw_batches_ns",
        "syscall_raw",
        domain_probe.get("syscall_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_vdso_direct",
        "vdso_batches_ns",
        "vdso",
        domain_probe.get("vdso_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_vdso_direct",
        "vdso_raw_batches_ns",
        "vdso_raw",
        domain_probe.get("vdso_raw_available") is True,
      ),
      (
        "linux_clock_boottime",
        "clock_boottime_batches_ns",
        "clock_boottime",
        domain_probe.get("clock_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_syscall",
        "syscall_boottime_batches_ns",
        "syscall_boottime",
        domain_probe.get("syscall_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_vdso_direct",
        "vdso_boottime_batches_ns",
        "vdso_boottime",
        domain_probe.get("vdso_boottime_available") is True,
      ),
      (
        "loongarch_stable_counter",
        "direct_batches_ns",
        "direct",
        True,
      ),
    ]
    output[domain] = replay_residual_staged_domain(
      f"{context}: LoongArch {domain}",
      domain_probe,
      "linux_clock_monotonic",
      "clock_batches_ns",
      stages,
      selected.get(domain),
      failures,
    )
  return output


def validate_riscv_wall_selector(
  context: str,
  selected: dict,
  candidates: dict,
  probe: dict,
  failures: list[str],
) -> dict:
  output = {}
  for domain in ("instant", "ordered"):
    domain_probe = probe.get(domain)
    if not isinstance(domain_probe, dict):
      failures.append(f"{context}: malformed RISC-V {domain} wall probe")
      continue
    expected = []
    if domain_probe.get("direct_eligible") is True:
      expected.append("riscv_rdtime")
    expected.extend(("linux_clock_monotonic", "linux_clock_monotonic_syscall"))
    if domain == "ordered":
      expected.append("linux_clock_monotonic_syscall_os_ordered")
    if domain_probe.get("vdso_available") is True:
      expected.append("linux_clock_monotonic_vdso_direct")
    if domain_probe.get("vdso_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_vdso_direct")
    if domain_probe.get("clock_raw_available") is True:
      expected.append("linux_clock_monotonic_raw")
    if domain_probe.get("syscall_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_syscall")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_raw_syscall_os_ordered")
    if domain_probe.get("clock_boottime_available") is True:
      expected.append("linux_clock_boottime")
    if domain_probe.get("syscall_boottime_available") is True:
      expected.append("linux_clock_boottime_syscall")
      if domain == "ordered":
        expected.append("linux_clock_boottime_syscall_os_ordered")
    if domain_probe.get("vdso_boottime_available") is True:
      expected.append("linux_clock_boottime_vdso_direct")
    if residual_declared_providers(candidates, domain) != expected:
      failures.append(f"{context}: RISC-V {domain} candidate set is incomplete")
    if domain_probe.get("candidate_count") != len(expected):
      failures.append(f"{context}: RISC-V {domain} probe candidate count changed")
    stages = [
      (
        "linux_clock_monotonic_syscall",
        "syscall_batches_ns",
        "fallback",
        True,
      ),
      (
        "linux_clock_monotonic_syscall_os_ordered",
        "syscall_os_ordered_batches_ns",
        "syscall_os_ordered",
        domain == "ordered",
      ),
      (
        "linux_clock_monotonic_raw",
        "clock_raw_batches_ns",
        "clock_raw",
        domain_probe.get("clock_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_syscall",
        "syscall_raw_batches_ns",
        "syscall_raw",
        domain_probe.get("syscall_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_syscall_os_ordered",
        "syscall_raw_os_ordered_batches_ns",
        "syscall_raw_os_ordered",
        domain == "ordered" and domain_probe.get("syscall_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_vdso_direct",
        "vdso_batches_ns",
        "vdso",
        domain_probe.get("vdso_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_vdso_direct",
        "vdso_raw_batches_ns",
        "vdso_raw",
        domain_probe.get("vdso_raw_available") is True,
      ),
      (
        "linux_clock_boottime",
        "clock_boottime_batches_ns",
        "clock_boottime",
        domain_probe.get("clock_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_syscall",
        "syscall_boottime_batches_ns",
        "syscall_boottime",
        domain_probe.get("syscall_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_syscall_os_ordered",
        "syscall_boottime_os_ordered_batches_ns",
        "syscall_boottime_os_ordered",
        domain == "ordered"
        and domain_probe.get("syscall_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_vdso_direct",
        "vdso_boottime_batches_ns",
        "vdso_boottime",
        domain_probe.get("vdso_boottime_available") is True,
      ),
      (
        "riscv_rdtime",
        "direct_batches_ns",
        "direct",
        domain_probe.get("direct_eligible") is True,
      ),
    ]
    output[domain] = replay_residual_staged_domain(
      f"{context}: RISC-V {domain}",
      domain_probe,
      "linux_clock_monotonic",
      "clock_batches_ns",
      stages,
      selected.get(domain),
      failures,
    )
  return output


def validate_power_wall_selector(
  context: str,
  selected: dict,
  candidates: dict,
  probe: dict,
  failures: list[str],
) -> dict:
  output = {}
  for domain in ("instant", "ordered"):
    domain_probe = probe.get(domain)
    if not isinstance(domain_probe, dict):
      failures.append(f"{context}: malformed Power {domain} wall probe")
      continue
    expected = ["power_timebase", "linux_clock_monotonic", "linux_clock_monotonic_sc"]
    if domain == "ordered":
      expected.append("linux_clock_monotonic_sc_os_ordered")
    if domain_probe.get("scv_eligible") is True:
      expected.append("linux_clock_monotonic_scv")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_scv_os_ordered")
    if domain_probe.get("clock_raw_available") is True:
      expected.append("linux_clock_monotonic_raw")
    if domain_probe.get("sc_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_sc")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_raw_sc_os_ordered")
    if domain_probe.get("scv_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_scv")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_raw_scv_os_ordered")
    if domain_probe.get("vdso_available") is True:
      expected.append("linux_clock_monotonic_vdso_direct")
    if domain_probe.get("vdso_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_vdso_direct")
    if domain_probe.get("clock_boottime_available") is True:
      expected.append("linux_clock_boottime")
    if domain_probe.get("sc_boottime_available") is True:
      expected.append("linux_clock_boottime_sc")
      if domain == "ordered":
        expected.append("linux_clock_boottime_sc_os_ordered")
    if domain_probe.get("scv_boottime_available") is True:
      expected.append("linux_clock_boottime_scv")
      if domain == "ordered":
        expected.append("linux_clock_boottime_scv_os_ordered")
    if domain_probe.get("vdso_boottime_available") is True:
      expected.append("linux_clock_boottime_vdso_direct")
    if residual_declared_providers(candidates, domain) != expected:
      failures.append(f"{context}: Power {domain} candidate set is incomplete")
    if domain_probe.get("candidate_count") != len(expected):
      failures.append(f"{context}: Power {domain} probe candidate count changed")
    stages = [
      ("linux_clock_monotonic_sc", "sc_batches_ns", "sc", True),
      (
        "linux_clock_monotonic_sc_os_ordered",
        "sc_os_ordered_batches_ns",
        "sc_os_ordered",
        domain == "ordered",
      ),
      (
        "linux_clock_monotonic_scv",
        "scv_batches_ns",
        "scv",
        domain_probe.get("scv_eligible") is True,
      ),
      (
        "linux_clock_monotonic_scv_os_ordered",
        "scv_os_ordered_batches_ns",
        "scv_os_ordered",
        domain == "ordered" and domain_probe.get("scv_eligible") is True,
      ),
      (
        "linux_clock_monotonic_raw",
        "clock_raw_batches_ns",
        "clock_raw",
        domain_probe.get("clock_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_sc",
        "sc_raw_batches_ns",
        "sc_raw",
        domain_probe.get("sc_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_sc_os_ordered",
        "sc_raw_os_ordered_batches_ns",
        "sc_raw_os_ordered",
        domain == "ordered" and domain_probe.get("sc_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_scv",
        "scv_raw_batches_ns",
        "scv_raw",
        domain_probe.get("scv_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_scv_os_ordered",
        "scv_raw_os_ordered_batches_ns",
        "scv_raw_os_ordered",
        domain == "ordered" and domain_probe.get("scv_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_vdso_direct",
        "vdso_batches_ns",
        "vdso",
        domain_probe.get("vdso_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_vdso_direct",
        "vdso_raw_batches_ns",
        "vdso_raw",
        domain_probe.get("vdso_raw_available") is True,
      ),
      (
        "linux_clock_boottime",
        "clock_boottime_batches_ns",
        "clock_boottime",
        domain_probe.get("clock_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_sc",
        "sc_boottime_batches_ns",
        "sc_boottime",
        domain_probe.get("sc_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_sc_os_ordered",
        "sc_boottime_os_ordered_batches_ns",
        "sc_boottime_os_ordered",
        domain == "ordered" and domain_probe.get("sc_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_scv",
        "scv_boottime_batches_ns",
        "scv_boottime",
        domain_probe.get("scv_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_scv_os_ordered",
        "scv_boottime_os_ordered_batches_ns",
        "scv_boottime_os_ordered",
        domain == "ordered" and domain_probe.get("scv_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_vdso_direct",
        "vdso_boottime_batches_ns",
        "vdso_boottime",
        domain_probe.get("vdso_boottime_available") is True,
      ),
      ("power_timebase", "direct_batches_ns", "direct", True),
    ]
    output[domain] = replay_residual_staged_domain(
      f"{context}: Power {domain}",
      domain_probe,
      "linux_clock_monotonic",
      "clock_batches_ns",
      stages,
      selected.get(domain),
      failures,
    )
  return output


def validate_linux_clock_wall_selector(
  context: str,
  selected: dict,
  candidates: dict,
  probe: dict,
  failures: list[str],
) -> dict:
  output = {}
  for domain in ("instant", "ordered"):
    domain_probe = probe.get(domain)
    if not isinstance(domain_probe, dict):
      failures.append(f"{context}: malformed Linux clock {domain} probe")
      continue
    expected = ["linux_clock_monotonic"]
    if domain_probe.get("arm_cntvct_available") is True:
      expected.append(
        "linux_arm_cntvct"
        if domain == "instant"
        else "linux_arm_dmb_ish_isb_cntvct"
      )
    if domain_probe.get("raw_available") is True:
      expected.append("linux_clock_monotonic_syscall")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_syscall_os_ordered")
    if domain_probe.get("vdso_available") is True:
      expected.append("linux_clock_monotonic_vdso_direct")
    if domain_probe.get("vdso_monotonic_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_vdso_direct")
    if domain_probe.get("vdso_time64_available") is True:
      expected.append("linux_clock_monotonic_vdso_time64_direct")
    if domain_probe.get("vdso_time64_monotonic_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_vdso_time64_direct")
    if domain_probe.get("raw_time64_available") is True:
      expected.append("linux_clock_monotonic_time64_syscall")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_time64_syscall_os_ordered")
    if domain_probe.get("libc_monotonic_raw_available") is True:
      expected.append("linux_clock_monotonic_raw")
    if domain_probe.get("raw_monotonic_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_syscall")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_raw_syscall_os_ordered")
    if domain_probe.get("raw_time64_monotonic_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_time64_syscall")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_raw_time64_syscall_os_ordered")
    if domain_probe.get("libc_boottime_available") is True:
      expected.append("linux_clock_boottime")
    if domain_probe.get("raw_boottime_available") is True:
      expected.append("linux_clock_boottime_syscall")
      if domain == "ordered":
        expected.append("linux_clock_boottime_syscall_os_ordered")
    if domain_probe.get("raw_time64_boottime_available") is True:
      expected.append("linux_clock_boottime_time64_syscall")
      if domain == "ordered":
        expected.append("linux_clock_boottime_time64_syscall_os_ordered")
    if domain_probe.get("vdso_boottime_available") is True:
      expected.append("linux_clock_boottime_vdso_direct")
    if domain_probe.get("vdso_time64_boottime_available") is True:
      expected.append("linux_clock_boottime_vdso_time64_direct")
    if residual_declared_providers(candidates, domain) != expected:
      failures.append(f"{context}: Linux clock {domain} candidate set is incomplete")
    if domain_probe.get("candidate_count") != len(expected):
      failures.append(f"{context}: Linux clock {domain} probe candidate count changed")
    if domain_probe.get("s390_bare_stckf_eligible") is not False:
      failures.append(f"{context}: Linux clock {domain} exposed private bare STCKF")
    if not isinstance(domain_probe.get("s390_bare_stckf_exclusion"), str):
      failures.append(f"{context}: Linux clock {domain} lacks its STCKF exclusion")
    stages = [
      (
        "linux_clock_monotonic_syscall",
        "raw_batches_ns",
        "raw",
        domain_probe.get("raw_available") is True,
      ),
      (
        "linux_clock_monotonic_syscall_os_ordered",
        "raw_os_ordered_batches_ns",
        "raw_os_ordered",
        domain == "ordered" and domain_probe.get("raw_available") is True,
      ),
      (
        "linux_clock_monotonic_time64_syscall",
        "raw_time64_batches_ns",
        "raw_time64",
        domain_probe.get("raw_time64_available") is True,
      ),
      (
        "linux_clock_monotonic_time64_syscall_os_ordered",
        "raw_time64_os_ordered_batches_ns",
        "raw_time64_os_ordered",
        domain == "ordered" and domain_probe.get("raw_time64_available") is True,
      ),
      (
        "linux_clock_monotonic_raw",
        "libc_monotonic_raw_batches_ns",
        "libc_monotonic_raw",
        domain_probe.get("libc_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_syscall",
        "raw_monotonic_raw_batches_ns",
        "raw_monotonic_raw",
        domain_probe.get("raw_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_syscall_os_ordered",
        "raw_monotonic_raw_os_ordered_batches_ns",
        "raw_monotonic_raw_os_ordered",
        domain == "ordered"
        and domain_probe.get("raw_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_time64_syscall",
        "raw_time64_monotonic_raw_batches_ns",
        "raw_time64_monotonic_raw",
        domain_probe.get("raw_time64_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_time64_syscall_os_ordered",
        "raw_time64_monotonic_raw_os_ordered_batches_ns",
        "raw_time64_monotonic_raw_os_ordered",
        domain == "ordered"
        and domain_probe.get("raw_time64_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_vdso_direct",
        "vdso_batches_ns",
        "vdso",
        domain_probe.get("vdso_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_vdso_direct",
        "vdso_monotonic_raw_batches_ns",
        "vdso_monotonic_raw",
        domain_probe.get("vdso_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_vdso_time64_direct",
        "vdso_time64_batches_ns",
        "vdso_time64",
        domain_probe.get("vdso_time64_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_vdso_time64_direct",
        "vdso_time64_monotonic_raw_batches_ns",
        "vdso_time64_monotonic_raw",
        domain_probe.get("vdso_time64_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_boottime",
        "libc_boottime_batches_ns",
        "libc_boottime",
        domain_probe.get("libc_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_syscall",
        "raw_boottime_batches_ns",
        "raw_boottime",
        domain_probe.get("raw_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_syscall_os_ordered",
        "raw_boottime_os_ordered_batches_ns",
        "raw_boottime_os_ordered",
        domain == "ordered" and domain_probe.get("raw_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_time64_syscall",
        "raw_time64_boottime_batches_ns",
        "raw_time64_boottime",
        domain_probe.get("raw_time64_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_time64_syscall_os_ordered",
        "raw_time64_boottime_os_ordered_batches_ns",
        "raw_time64_boottime_os_ordered",
        domain == "ordered"
        and domain_probe.get("raw_time64_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_vdso_direct",
        "vdso_boottime_batches_ns",
        "vdso_boottime",
        domain_probe.get("vdso_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_vdso_time64_direct",
        "vdso_time64_boottime_batches_ns",
        "vdso_time64_boottime",
        domain_probe.get("vdso_time64_boottime_available") is True,
      ),
      (
        "linux_arm_cntvct"
        if domain == "instant"
        else "linux_arm_dmb_ish_isb_cntvct",
        "cntvct_batches_ns",
        "cntvct",
        domain_probe.get("arm_cntvct_available") is True,
      ),
    ]
    output[domain] = replay_residual_staged_domain(
      f"{context}: Linux clock {domain}",
      domain_probe,
      "linux_clock_monotonic",
      "libc_batches_ns",
      stages,
      selected.get(domain),
      failures,
    )
  return output


def validate_freebsd_barrier_tournament(
  context: str,
  probe: dict,
  kind: str,
  failures: list[str],
) -> dict:
  count_key = (
    "ordered_barrier_candidate_count"
    if kind == "clock"
    else "ordered_syscall_barrier_candidate_count"
  )
  names_key = (
    "ordered_barrier_candidate_names"
    if kind == "clock"
    else "ordered_syscall_barrier_candidate_names"
  )
  count = probe.get(count_key)
  names = probe.get(names_key)
  batches = probe.get(f"ordered_{kind}_barrier_candidate_batches_ns")
  medians = probe.get(f"ordered_{kind}_barrier_candidate_medians_ns")
  decision_count = probe.get(f"ordered_{kind}_barrier_decision_count")
  if (
    type(count) is not int
    or count <= 0
    or not isinstance(names, list)
    or not isinstance(batches, list)
    or not isinstance(medians, list)
    or count > min(len(names), len(batches), len(medians))
    or decision_count != count - 1
  ):
    failures.append(f"{context}: malformed FreeBSD {kind} barrier candidates")
    return {}
  names = names[:count]
  batches = batches[:count]
  for name, samples, recorded_median in zip(names, batches, medians[:count], strict=True):
    if isinstance(samples, list) and len(samples) == 9:
      if recorded_median != int(statistics.median(samples)):
        failures.append(f"{context}: FreeBSD {kind} barrier median changed for {name}")
  steps = [
    {
      "challenger": challenger,
      "incumbent": incumbent,
      "winner": winner,
      "allowance_ns": allowance,
      "decisive_wins": wins,
      "challenger_selected": challenger_selected,
    }
    for challenger, incumbent, winner, allowance, wins, challenger_selected in zip(
      probe.get(f"ordered_{kind}_barrier_challengers", [])[:decision_count],
      probe.get(f"ordered_{kind}_barrier_incumbents", [])[:decision_count],
      probe.get(f"ordered_{kind}_barrier_winners", [])[:decision_count],
      probe.get(f"ordered_{kind}_barrier_allowances_ns", [])[:decision_count],
      probe.get(f"ordered_{kind}_barrier_tournament_decisive_wins", [])[:decision_count],
      probe.get(f"ordered_{kind}_barrier_challenger_selected", [])[:decision_count],
      strict=True,
    )
  ]
  return validate_tournament(
    f"{context}: FreeBSD {kind} barrier",
    names,
    batches,
    steps,
    probe.get("ordered", {}).get("reads_per_batch"),
    probe.get("ordered", {}).get("required_decisive_wins"),
    steps[-1]["winner"] if steps else names[0],
    failures,
  )


def validate_freebsd_wall_selector(
  context: str,
  selected: dict,
  candidates: dict,
  probe: dict,
  failures: list[str],
) -> dict:
  instant = probe.get("instant")
  ordered = probe.get("ordered")
  if not isinstance(instant, dict) or not isinstance(ordered, dict):
    failures.append(f"{context}: malformed FreeBSD wall probe")
    return {}

  instant_expected = ["freebsd_clock_monotonic", "freebsd_clock_monotonic_syscall"]
  if instant.get("timekeep_available") is True:
    instant_expected.append("freebsd_at_timekeep")
  if instant.get("direct_eligible") is True:
    instant_expected.append("freebsd_kernel_eligible_tsc")
  if residual_declared_providers(candidates, "instant") != instant_expected:
    failures.append(f"{context}: FreeBSD Instant candidate set is incomplete")
  if instant.get("candidate_count") != len(instant_expected):
    failures.append(f"{context}: FreeBSD Instant probe candidate count changed")
  instant_result = replay_residual_staged_domain(
    f"{context}: FreeBSD Instant",
    instant,
    "freebsd_clock_monotonic",
    "clock_batches_ns",
    [
      (
        "freebsd_clock_monotonic_syscall",
        "syscall_batches_ns",
        "fallback",
        True,
      ),
      (
        "freebsd_at_timekeep",
        "timekeep_batches_ns",
        "timekeep",
        instant.get("timekeep_available") is True,
      ),
      (
        "freebsd_kernel_eligible_tsc",
        "direct_batches_ns",
        "direct",
        instant.get("direct_eligible") is True,
      ),
    ],
    selected.get("instant"),
    failures,
  )

  clock_barriers = validate_freebsd_barrier_tournament(
    context, probe, "clock", failures
  )
  syscall_barriers = validate_freebsd_barrier_tournament(
    context, probe, "syscall", failures
  )
  if not clock_barriers or not syscall_barriers:
    return {"instant": instant_result}
  clock_provider = f"freebsd_clock_monotonic_{clock_barriers['winner']}"
  syscall_provider = f"freebsd_clock_monotonic_syscall_{syscall_barriers['winner']}"
  declared_ordered = residual_declared_providers(candidates, "ordered")
  tsc_candidates = [
    provider
    for provider in declared_ordered
    if provider.startswith("freebsd_kernel_eligible_tsc_x86_")
  ]
  if ordered.get("direct_eligible") is True:
    if len(tsc_candidates) != 1:
      failures.append(f"{context}: FreeBSD Ordered TSC identity is incomplete")
      tsc_provider = "freebsd_kernel_eligible_tsc_x86_cpuid_rdtsc"
    else:
      tsc_provider = tsc_candidates[0]
  else:
    if tsc_candidates:
      failures.append(f"{context}: FreeBSD Ordered retained an ineligible TSC")
    tsc_provider = "freebsd_kernel_eligible_tsc_x86_cpuid_rdtsc"
  expected_ordered = ([] if ordered.get("direct_eligible") is not True else [tsc_provider])
  if ordered.get("timekeep_available") is True:
    expected_ordered.insert(0, "freebsd_at_timekeep_os_owned")
  expected_ordered.extend(
    f"freebsd_clock_monotonic_{barrier}"
    for barrier in probe.get("ordered_barrier_candidate_names", [])[
      : probe.get("ordered_barrier_candidate_count", 0)
    ]
  )
  expected_ordered.extend(
    f"freebsd_clock_monotonic_syscall_{barrier}"
    for barrier in probe.get("ordered_syscall_barrier_candidate_names", [])[
      : probe.get("ordered_syscall_barrier_candidate_count", 0)
    ]
  )
  if declared_ordered != expected_ordered:
    failures.append(f"{context}: FreeBSD Ordered candidate set is incomplete")
  expected_outer_count = (
    2
    + int(ordered.get("timekeep_available") is True)
    + int(ordered.get("direct_eligible") is True)
  )
  if ordered.get("candidate_count") != expected_outer_count:
    failures.append(f"{context}: FreeBSD Ordered probe candidate count changed")
  ordered_result = replay_residual_staged_domain(
    f"{context}: FreeBSD Ordered",
    ordered,
    clock_provider,
    "clock_batches_ns",
    [
      (syscall_provider, "syscall_batches_ns", "fallback", True),
      (
        "freebsd_at_timekeep_os_owned",
        "timekeep_batches_ns",
        "timekeep",
        ordered.get("timekeep_available") is True,
      ),
      (
        tsc_provider,
        "direct_batches_ns",
        "direct",
        ordered.get("direct_eligible") is True,
      ),
    ],
    selected.get("ordered"),
    failures,
  )
  outer_decisions = ordered_result.get("decisions", [])
  syscall_selected = bool(outer_decisions and outer_decisions[0].get("selected"))
  fallback_winner = (
    syscall_barriers["winner"] if syscall_selected else clock_barriers["winner"]
  )
  if probe.get("ordered_os_barrier") != fallback_winner:
    failures.append(f"{context}: FreeBSD complete Ordered fallback identity changed")
  return {
    "instant": instant_result,
    "ordered": ordered_result,
    "clock_barrier": clock_barriers,
    "syscall_barrier": syscall_barriers,
  }


def validate_residual_wall_selector(
  context: str,
  selection: dict,
  clocks: dict,
  failures: list[str],
) -> dict:
  metadata = validate_residual_wall_metadata(context, selection, clocks, failures)
  if metadata is None:
    return {}
  selected, candidates, probe = metadata
  architecture = selection.get("architecture")
  if architecture in ("armv7-linux", "s390x-linux"):
    return validate_linux_clock_wall_selector(
      context, selected, candidates, probe, failures
    )
  if architecture == "riscv64-linux":
    return validate_riscv_wall_selector(context, selected, candidates, probe, failures)
  if architecture == "loongarch64-linux":
    return validate_loongarch_wall_selector(context, selected, candidates, probe, failures)
  if architecture == "powerpc64-linux-gnu":
    return validate_power_wall_selector(context, selected, candidates, probe, failures)
  if architecture == "x86_64-freebsd":
    return validate_freebsd_wall_selector(context, selected, candidates, probe, failures)
  failures.append(f"{context}: unknown residual wall architecture {architecture!r}")
  return {}


def lambda_median_with_ci(samples: list[float], seed: str) -> tuple[float, list[float]]:
  point = statistics.median(samples)
  rng = random.Random(seed)
  bootstrap = sorted(
    statistics.median(rng.choices(samples, k=len(samples)))
    for _ in range(LAMBDA_BOOTSTRAP_SAMPLES)
  )
  return point, [bootstrap[124], bootstrap[4_874]]


def validate_lambda_samples(context: str, clocks: dict, failures: list[str]) -> None:
  for clock, entry in clocks.items():
    if clock not in {
      "tach", "tach_ordered", "quanta", "fastant", "minstant", "std",
      "tach_thread_cpu", "native_thread_cpu", "direct_selected_wall",
      "direct_selected_ordered_wall", "direct_selected_thread_cpu",
      "direct_fallback_thread_cpu",
    } and not clock.startswith((
      "direct_thread_cpu__",
      "direct_wall__",
      "direct_ordered_wall__",
    )):
      continue
    if not isinstance(entry, dict):
      failures.append(f"{context} {clock}: malformed clock evidence")
      continue
    for metric in METRICS:
      samples = entry.get(f"{metric}_samples")
      if (
        not isinstance(samples, list)
        or len(samples) != LAMBDA_SAMPLE_COUNT
        or not all(finite_number(sample) and sample >= 0 for sample in samples)
      ):
        failures.append(
          f"{context} {clock}: expected {LAMBDA_SAMPLE_COUNT} finite {metric} samples"
        )
        continue
      point, interval = lambda_median_with_ci(samples, f"{clock}:{metric}")
      if entry.get(metric) != point or entry.get(f"{metric}_ci95") != interval:
        failures.append(f"{context} {clock}: {metric} aggregate does not reproduce")


def git_command(root: Path, *args: str) -> subprocess.CompletedProcess[str]:
  return subprocess.run(
    ["git", *args],
    cwd=root,
    capture_output=True,
    text=True,
    check=False,
  )


def validate_checkout_binding(root: Path, revision: str) -> tuple[list[str], dict]:
  failures = []
  root = root.resolve()
  commit = git_command(root, "cat-file", "-e", f"{revision}^{{commit}}")
  if commit.returncode:
    return [f"campaign source commit is unavailable: {revision}"], {}

  ancestor = git_command(root, "merge-base", "--is-ancestor", revision, "HEAD")
  if ancestor.returncode:
    failures.append("campaign source revision is not an ancestor of HEAD")

  head_result = git_command(root, "rev-parse", "HEAD")
  head = head_result.stdout.strip() if head_result.returncode == 0 else None

  # First compare two committed trees. This keeps a later evidence-only commit
  # valid while requiring every executable, dependency lock, harness, runner,
  # extractor, and claim rule to be byte-identical to the measured revision.
  # A source file cannot pin its own eventual commit hash; the six generated
  # cell documents are the authority for `revision`.
  committed_diff = git_command(
    root,
    "diff",
    "--name-only",
    "--no-renames",
    revision,
    "HEAD",
    "--",
    *BENCHMARK_SOURCE_PATHS,
  )
  if committed_diff.returncode:
    committed_inputs_unchanged = False
    committed_changes = []
    failures.append("could not compare committed benchmark inputs with the campaign revision")
  else:
    committed_changes = [line for line in committed_diff.stdout.splitlines() if line]
    committed_inputs_unchanged = not committed_changes
    if committed_changes:
      failures.append(
        "committed benchmark inputs changed since the campaign: "
        + ", ".join(committed_changes)
      )

  # Then audit the index, worktree, and untracked files. Comparing only the
  # campaign commit with a dirty worktree can hide which side of the binding
  # failed; keeping the committed-tree and local-state checks separate makes
  # both failure modes explicit.
  working_status = git_command(
    root,
    "status",
    "--porcelain=v1",
    "--untracked-files=all",
    "--",
    *BENCHMARK_SOURCE_PATHS,
  )
  if working_status.returncode:
    working_tree_clean = False
    working_changes = []
    failures.append("could not inspect local benchmark inputs")
  else:
    working_changes = [line for line in working_status.stdout.splitlines() if line]
    working_tree_clean = not working_changes
    if working_changes:
      failures.append("local benchmark inputs differ from HEAD: " + ", ".join(working_changes))

  return failures, {
    "campaign_revision": revision,
    "checkout_head": head,
    "committed_tree_inputs_unchanged": committed_inputs_unchanged,
    "committed_tree_changes": committed_changes,
    "working_tree_inputs_clean": working_tree_clean,
    "working_tree_changes": working_changes,
  }


def validate_cell(
  context: str,
  clocks: dict,
  target_triple: str | None = None,
) -> tuple[list[str], dict]:
  failures: list[str] = []
  required = {
    "tach", "tach_ordered", "quanta", "fastant", "minstant", "std",
    "tach_thread_cpu", "native_thread_cpu",
  }
  missing = sorted(required - clocks.keys())
  if missing:
    return [f"{context}: missing clocks: {', '.join(missing)}"], {}

  for clock in sorted(required):
    validate_ci(context, clock, clocks[clock], failures)
  if failures:
    return failures, {}

  reference_eligibility = local_reference_eligibility(target_triple)
  same_thread = {}
  for metric in METRICS:
    comparisons = {}
    for reference_name in LOCAL_COMPETITORS:
      passed, allowance = equivalent_or_faster(
        clocks["tach"], clocks[reference_name], metric
      )
      policy = reference_eligibility[reference_name]
      comparisons[reference_name] = {
        "reference_ns": clocks[reference_name][metric],
        "equivalence_allowance_ns": allowance,
        "eligible_for_reliable_contract": policy["eligible"],
        "eligibility_reason": policy["reason"],
        "implementation": policy["implementation"],
        "passed": passed,
      }
      if not passed and policy["eligible"]:
        failures.append(
          f"{context}: Instant {metric} is materially slower than {reference_name}"
        )
    same_thread[metric] = {
      "tach_ns": clocks["tach"][metric],
      "comparisons": comparisons,
      "passed": all(
        item["passed"]
        for item in comparisons.values()
        if item["eligible_for_reliable_contract"]
      ),
    }

  cross_thread = {}
  for metric in METRICS:
    passed, allowance = equivalent_or_faster(
      clocks["tach_ordered"], clocks["std"], metric
    )
    cross_thread[metric] = {
      "tach_ns": clocks["tach_ordered"][metric],
      "std_ns": clocks["std"][metric],
      "equivalence_allowance_ns": allowance,
      "passed": passed,
    }
    if not passed:
      failures.append(f"{context}: OrderedInstant {metric} is materially slower than std")

  selected_wall_parity = {}
  wall_selection = clocks["tach"].get("selection")
  selector_reproduction = {}
  if isinstance(wall_selection, dict):
    selected_mapping = wall_selection.get("selected_provider")
    if isinstance(wall_selection.get("architecture"), str):
      selector_reproduction = validate_residual_wall_selector(
        context, wall_selection, clocks, failures
      )
    elif isinstance(wall_selection.get("probe"), dict) and "instant_candidate_names" in wall_selection["probe"]:
      selector_reproduction = validate_linux_x86_wall_selector(
        context, wall_selection, failures
      )
    elif isinstance(wall_selection.get("instant_probe"), dict):
      selector_reproduction = validate_linux_aarch64_wall_selector(
        context, wall_selection, failures
      )
    elif isinstance(selected_mapping, dict) and selected_mapping.get("instant") == "windows_qpc":
      selector_reproduction = validate_windows_wall_selector(
        context, wall_selection, failures
      )
    elif (
      isinstance(selected_mapping, dict)
      and isinstance(selected_mapping.get("instant"), str)
      and selected_mapping["instant"].startswith("apple_")
    ):
      selector_reproduction = validate_apple_wall_selector(
        context, wall_selection, failures
      )
  selected_pairs = (
    ("instant", "tach", "direct_selected_wall"),
    ("ordered", "tach_ordered", "direct_selected_ordered_wall"),
  )
  if wall_selection is not None:
    selected = wall_selection.get("selected_provider")
    if not isinstance(selected, dict):
      failures.append(f"{context}: wall selector lacks selected-provider mapping")
      selected = {}
    for domain, public_name, direct_name in selected_pairs:
      candidate_results = {}
      declared_candidates = wall_selection.get("eligible_direct_candidates", {})
      if isinstance(declared_candidates, dict):
        domain_candidates = declared_candidates.get(domain)
      else:
        domain_candidates = None
      if not isinstance(domain_candidates, list) or not domain_candidates:
        failures.append(f"{context}: {domain} wall selector lacks eligible candidates")
        domain_candidates = []
      for candidate in domain_candidates:
        expected_identity = exact_wall_candidate_identity(domain, candidate)
        if expected_identity is None:
          failures.append(
            f"{context}: {domain} wall selector declared invalid candidate key {candidate!r}"
          )
          continue
        candidate_row = clocks.get(candidate)
        if not isinstance(candidate_row, dict):
          failures.append(f"{context}: {domain} wall selector lacks exact row {candidate}")
          continue
        validate_ci(context, candidate, candidate_row, failures)
        identity_passed = all(
          candidate_row.get(field) == expected
          for field, expected in expected_identity.items()
        )
        if not identity_passed:
          failures.append(
            f"{context}: {domain} wall candidate {candidate} identity does not match its key"
          )
        metrics = {}
        for metric in METRICS:
          passed, allowance = equivalent_or_faster(
            clocks[public_name], candidate_row, metric
          )
          metrics[metric] = {
            "tach_ns": clocks[public_name][metric],
            "candidate_ns": candidate_row[metric],
            "equivalence_allowance_ns": allowance,
            "passed": passed,
          }
          if not passed:
            failures.append(
              f"{context}: {public_name} {metric} is materially slower than "
              f"eligible exact route {candidate}"
            )
        candidate_results[candidate] = {
          "metrics": metrics,
          "identity_passed": identity_passed,
          "passed": (
            identity_passed
            and all(result["passed"] for result in metrics.values())
          ),
        }

      direct_wall = clocks.get(direct_name)
      if direct_wall is None:
        failures.append(f"{context}: {domain} wall selector lacks selected-native evidence")
        continue
      validate_ci(context, direct_name, direct_wall, failures)
      expected_provider = selected.get(domain)
      if direct_wall.get("provider") != expected_provider:
        failures.append(
          f"{context}: {domain} selected-native provider "
          f"{direct_wall.get('provider')!r} != selector {expected_provider!r}"
        )
        continue
      metrics = {}
      for metric in METRICS:
        passed, allowance = equivalent_or_faster(
          clocks[public_name], direct_wall, metric
        )
        metrics[metric] = {
          "tach_ns": clocks[public_name][metric],
          "selected_native_ns": direct_wall[metric],
          "equivalence_allowance_ns": allowance,
          "passed": passed,
        }
        if not passed:
          failures.append(
            f"{context}: {public_name} {metric} is materially slower than its "
            "selected native path"
          )
      selected_wall_parity[domain] = {
        "selected_provider": direct_wall.get("provider"),
        "metrics": metrics,
        "eligible_candidates": candidate_results,
        "passed": (
          all(result["passed"] for result in metrics.values())
          and all(result["passed"] for result in candidate_results.values())
        ),
      }

  tach_thread = clocks["tach_thread_cpu"]
  native = clocks["native_thread_cpu"]
  for name, entry in (("tach_thread_cpu", tach_thread), ("native_thread_cpu", native)):
    if entry.get("time_domain") != "thread CPU":
      failures.append(f"{context}: {name} did not measure current-thread CPU time")
  provider = tach_thread.get("provider")
  read_cost = tach_thread.get("read_cost")
  if strict_thread_cpu_provider_cost(provider, target_triple) != read_cost:
    failures.append(
      f"{context}: ThreadCpuInstant provider/read-cost pair is not a strict "
      "native provider"
    )

  thread_selector_reproduction = {}
  selected_thread_parity = {}
  thread_selection = tach_thread.get("selection")
  if thread_selection is not None:
    if not isinstance(thread_selection, dict):
      failures.append(f"{context}: malformed thread-CPU selection evidence")
    else:
      thread_selector_reproduction = validate_thread_cpu_selector(
        context, thread_selection, failures, target_triple
      )
      declared_candidates = thread_selection.get("eligible_direct_candidates", [])
      candidate_parity = {}
      for candidate in declared_candidates:
        candidate_row = clocks.get(candidate)
        if not isinstance(candidate_row, dict):
          failures.append(f"{context}: thread-CPU selector lacks exact row {candidate}")
          continue
        validate_ci(context, candidate, candidate_row, failures)
        if candidate_row.get("benchmark") != candidate:
          failures.append(f"{context}: thread-CPU exact candidate {candidate} is mislabeled")
        if candidate_row.get("time_domain") != "thread CPU":
          failures.append(f"{context}: thread-CPU exact candidate {candidate} changed time domain")
        metrics = {}
        for metric in METRICS:
          passed, allowance = equivalent_or_faster(tach_thread, candidate_row, metric)
          metrics[metric] = {
            "tach_ns": tach_thread[metric],
            "candidate_ns": candidate_row[metric],
            "equivalence_allowance_ns": allowance,
            "passed": passed,
          }
          if not passed:
            failures.append(
              f"{context}: ThreadCpuInstant {metric} is materially slower than "
              f"eligible exact route {candidate}"
            )
        candidate_parity[candidate] = {
          "metrics": metrics,
          "passed": all(result["passed"] for result in metrics.values()),
        }
      selected_key = thread_selection.get("selected_provider")
      selected_metadata = thread_cpu_provider_key_metadata(selected_key, target_triple)
      if selected_metadata != (provider, read_cost):
        failures.append(
          f"{context}: thread-CPU selector public provider/read-cost disagrees "
          "with introspection"
        )
      selected_thread = clocks.get("direct_selected_thread_cpu")
      if not isinstance(selected_thread, dict):
        failures.append(f"{context}: thread-CPU selector lacks selected-native evidence")
      else:
        validate_ci(context, "direct_selected_thread_cpu", selected_thread, failures)
        expected_mechanism = thread_selection.get("selected_mechanism")
        if selected_thread.get("provider") != expected_mechanism:
          failures.append(
            f"{context}: selected thread-CPU mechanism "
            f"{selected_thread.get('provider')!r} != selector {expected_mechanism!r}"
          )
        if selected_thread.get("read_cost") != thread_selection.get("selected_read_cost"):
          failures.append(f"{context}: selected thread-CPU exact-row read cost disagrees")
        if selected_thread.get("benchmark") != thread_selection.get("selected_native_benchmark"):
          failures.append(f"{context}: selected thread-CPU exact row is mislabeled")
        if selected_thread.get("time_domain") != "thread CPU":
          failures.append(f"{context}: selected thread-CPU exact row changed time domain")
        metrics = {}
        for metric in METRICS:
          passed, allowance = equivalent_or_faster(tach_thread, selected_thread, metric)
          metrics[metric] = {
            "tach_ns": tach_thread[metric],
            "selected_native_ns": selected_thread[metric],
            "equivalence_allowance_ns": allowance,
            "passed": passed,
          }
          if not passed:
            failures.append(
              f"{context}: ThreadCpuInstant {metric} is materially slower than its "
              "selected native path"
            )
        selected_thread_parity = {
          "selected_provider": selected_key,
          "selected_mechanism": selected_thread.get("provider"),
          "selected_read_cost": selected_thread.get("read_cost"),
          "metrics": metrics,
          "eligible_candidates": candidate_parity,
          "passed": (
            all(result["passed"] for result in metrics.values())
            and all(result["passed"] for result in candidate_parity.values())
          ),
        }
        fallback_benchmark = thread_selection.get("fallback_native_benchmark")
        if fallback_benchmark is not None:
          fallback_thread = clocks.get("direct_fallback_thread_cpu")
          if not isinstance(fallback_thread, dict):
            failures.append(f"{context}: selector lacks measured fallback exact row")
          else:
            validate_ci(context, "direct_fallback_thread_cpu", fallback_thread, failures)
            if (
              fallback_thread.get("benchmark") != fallback_benchmark
              or fallback_thread.get("provider")
              != thread_selection.get("fallback_mechanism")
              or fallback_thread.get("read_cost")
              != thread_selection.get("fallback_read_cost")
              or fallback_thread.get("time_domain") != "thread CPU"
            ):
              failures.append(f"{context}: measured fallback exact row is mislabeled")
            runner_up_metrics = {}
            for metric in METRICS:
              passed, allowance = equivalent_or_faster(
                selected_thread, fallback_thread, metric
              )
              runner_up_metrics[metric] = {
                "selected_ns": selected_thread[metric],
                "fallback_ns": fallback_thread[metric],
                "equivalence_allowance_ns": allowance,
                "passed": passed,
              }
              if not passed:
                failures.append(
                  f"{context}: selected thread-CPU {metric} is materially slower "
                  "than its measured runner-up"
                )
            selected_thread_parity["measured_runner_up"] = {
              "provider": fallback_thread.get("provider"),
              "metrics": runner_up_metrics,
              "passed": all(result["passed"] for result in runner_up_metrics.values()),
            }

      if thread_selection.get("selection_kind") == "fixed_windows_thread_times":
        failure_fallback = thread_selection.get("failure_fallback")
        failure_row = clocks.get("direct_failure_fallback_thread_cpu")
        if not isinstance(failure_fallback, dict) or not isinstance(failure_row, dict):
          failures.append(f"{context}: Windows wall fallback lacks exact benchmark evidence")
        else:
          validate_ci(
            context,
            "direct_failure_fallback_thread_cpu",
            failure_row,
            failures,
          )
          if (
            failure_row.get("benchmark") != failure_fallback.get("exact_benchmark")
            or failure_row.get("provider") != failure_fallback.get("mechanism")
            or failure_row.get("read_cost") != failure_fallback.get("read_cost")
            or failure_row.get("time_domain") != "monotonic wall fallback"
            or failure_row.get("eligible_for_thread_cpu_speed_claim") is not False
          ):
            failures.append(
              f"{context}: Windows wall fallback exact row is mislabeled or eligible"
            )
          selected_thread_parity["failure_fallback"] = {
            "provider": failure_row.get("provider"),
            "read_cost": failure_row.get("read_cost"),
            "time_domain": failure_row.get("time_domain"),
            "eligible_for_thread_cpu_speed_claim": False,
            "now_ns": failure_row.get("now"),
            "elapsed_ns": failure_row.get("elapsed"),
            "measured": True,
          }

  current_thread = {}
  for metric in METRICS:
    passed, allowance = equivalent_or_faster(tach_thread, native, metric)
    current_thread[metric] = {
      "tach_ns": tach_thread[metric],
      "native_ns": native[metric],
      "equivalence_allowance_ns": allowance,
      "passed": passed,
    }
    if not passed:
      failures.append(
        f"{context}: ThreadCpuInstant {metric} is materially slower than native"
      )

  return failures, {
    "same_thread_elapsed": same_thread,
    "same_thread_reference_eligibility": reference_eligibility,
    "cross_thread_elapsed": cross_thread,
    "wall_selector_reproduction": selector_reproduction,
    "selected_wall_provider_parity": selected_wall_parity,
    "thread_cpu_selector_reproduction": thread_selector_reproduction,
    "selected_thread_cpu_provider_parity": selected_thread_parity,
    "current_thread_cpu": current_thread,
    "provider": tach_thread.get("provider"),
    "read_cost": tach_thread.get("read_cost"),
  }


def validate_campaign(documents: list[dict]) -> dict:
  failures = []
  results = []
  if len(documents) != len(EXPECTED_ENVIRONMENTS):
    failures.append(
      f"expected {len(EXPECTED_ENVIRONMENTS)} environments, found {len(documents)}"
    )

  identities = []
  for document in documents:
    provenance = document.get("provenance")
    provenance = provenance if isinstance(provenance, dict) else {}
    identities.append((
      document.get("order"),
      document.get("title"),
      document.get("instance"),
      document.get("triple"),
      provenance.get("harness"),
      provenance.get("cargo_profile"),
    ))
  if tuple(identities) != EXPECTED_ENVIRONMENTS:
    failures.append(
      "campaign environments differ from the exact six-platform contract: "
      f"{identities!r}"
    )

  revisions = set()
  for document in documents:
    context = document.get("title", "unnamed environment")
    provenance = document.get("provenance")
    if not isinstance(provenance, dict):
      failures.append(f"{context}: missing build provenance")
    else:
      revision = provenance.get("source_revision")
      rustc = provenance.get("rustc")
      if not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", revision):
        failures.append(f"{context}: invalid full source revision")
      else:
        revisions.add(revision)
      if not isinstance(rustc, str) or not rustc.startswith("rustc "):
        failures.append(f"{context}: invalid rustc provenance")
      expected_features = list(BENCHMARK_FEATURES)
      if sorted(provenance.get("features", [])) != expected_features:
        failures.append(f"{context}: benchmark feature set changed")

    clocks = document.get("clocks")
    if not isinstance(clocks, dict):
      failures.append(f"{context}: missing clocks object")
      continue
    if provenance.get("harness") == "lambda":
      validate_lambda_samples(context, clocks, failures)
    cell_failures, result = validate_cell(context, clocks, document.get("triple"))
    failures.extend(cell_failures)
    results.append({"environment": context, **result})

  if len(revisions) != 1:
    failures.append(f"campaign must use one source revision: {sorted(revisions)}")

  return {
    "schema": "tach-speed-evidence-v3",
    "claim": (
      "each tach timer selects the fastest audited eligible reliable steady-state provider "
      "for its contract in every measured environment"
    ),
    "equivalence_rule": (
      "tach is faster than or materially tied with every eligible reference: its point "
      "estimate and conservative 95% CI comparison fit within max(1 ns, 5%)"
    ),
    "source_revision": next(iter(revisions)) if len(revisions) == 1 else None,
    "passed": not failures,
    "failures": failures,
    "environments": results,
  }


def validate_supplemental_route_coverage(
  context: str,
  document: dict,
  failures: list[str],
) -> dict:
  clocks = document.get("clocks")
  coverage = document.get("route_coverage")
  if not isinstance(clocks, dict) or not isinstance(coverage, dict):
    failures.append(f"{context}: missing three-use-case route coverage")
    return {}

  mode = document.get("mode")
  expected = {
    "instant": ("tach", "direct_selected_wall", "instant"),
    "ordered": ("tach_ordered", "direct_selected_ordered_wall", "ordered"),
    "thread_cpu": ("tach_thread_cpu", "direct_selected_thread_cpu", "thread_cpu"),
  }
  if set(coverage) != set(expected):
    failures.append(f"{context}: route coverage must contain exactly all three public timers")
    return {}

  results = {}
  for use_case, (expected_public, default_selected, selection_domain) in expected.items():
    route = coverage.get(use_case)
    if not isinstance(route, dict):
      failures.append(f"{context}: malformed {use_case} route coverage")
      continue
    public_name = route.get("public_row")
    selected_name = route.get("selected_row")
    candidates = route.get("eligible_exact_rows")
    selection_kind = route.get("selection_kind")
    if (
      public_name != expected_public
      or not isinstance(selected_name, str)
      or not selected_name
      or not isinstance(candidates, list)
      or not candidates
      or len(set(candidates)) != len(candidates)
      or not all(isinstance(candidate, str) and candidate for candidate in candidates)
      or selection_kind not in ("runtime_tournament", "unique_provider", "fallback_only")
    ):
      failures.append(f"{context}: malformed {use_case} route identity")
      continue
    if len(candidates) > 1 and selection_kind != "runtime_tournament":
      failures.append(f"{context}: {use_case} has multiple routes without a runtime tournament")
    if selection_kind in ("unique_provider", "fallback_only") and len(candidates) != 1:
      failures.append(f"{context}: {use_case} non-tournament route is not unique")

    public = clocks.get(public_name)
    selected = clocks.get(selected_name)
    if not isinstance(public, dict) or not isinstance(selected, dict):
      failures.append(f"{context}: {use_case} public/selected rows are missing")
      continue
    validate_ci(context, public_name, public, failures)
    validate_ci(context, selected_name, selected, failures)

    candidate_rows = {}
    for candidate in candidates:
      row = clocks.get(candidate)
      if not isinstance(row, dict):
        failures.append(f"{context}: {use_case} lacks eligible exact row {candidate}")
        continue
      validate_ci(context, candidate, row, failures)
      benchmark = row.get("benchmark")
      if benchmark is not None and benchmark != candidate and not benchmark.startswith(
        f"{candidate}__"
      ):
        failures.append(f"{context}: {use_case} exact row {candidate} is mislabeled")
      candidate_rows[candidate] = row

    candidate_providers = {
      row.get("provider") for row in candidate_rows.values() if isinstance(row.get("provider"), str)
    }
    if (
      isinstance(selected.get("provider"), str)
      and candidate_providers
      and selected.get("provider") not in candidate_providers
    ):
      failures.append(f"{context}: {use_case} selected route is not an eligible exact provider")

    selection = None
    if selection_domain in ("instant", "ordered"):
      selection = clocks.get("tach", {}).get("selection")
      selected_mapping = selection.get("selected_provider") if isinstance(selection, dict) else None
      declared = (
        selection.get("eligible_direct_candidates", {}).get(selection_domain)
        if isinstance(selection, dict)
        and isinstance(selection.get("eligible_direct_candidates"), dict)
        else None
      )
      selected_provider = (
        selected_mapping.get(selection_domain) if isinstance(selected_mapping, dict) else None
      )
    else:
      selection = public.get("selection")
      declared = selection.get("eligible_direct_candidates") if isinstance(selection, dict) else None
      selected_provider = (
        selection.get("selected_mechanism") if isinstance(selection, dict) else None
      )
    if selection_kind == "runtime_tournament":
      if not isinstance(selection, dict):
        failures.append(f"{context}: {use_case} tournament lacks selector evidence")
      if declared != candidates:
        failures.append(f"{context}: {use_case} eligible route enumeration disagrees")
      if selected_provider != selected.get("provider"):
        failures.append(f"{context}: {use_case} selected route identity disagrees")

    route_results = {}
    for candidate, row in candidate_rows.items():
      metrics = {}
      for metric in METRICS:
        try:
          passed, allowance = equivalent_or_faster(public, row, metric)
        except (KeyError, TypeError):
          failures.append(f"{context}: malformed {use_case} estimate for {candidate}")
          continue
        metrics[metric] = {
          "public_ns": public[metric],
          "exact_ns": row[metric],
          "equivalence_allowance_ns": allowance,
          "passed": passed,
        }
        if not passed:
          failures.append(
            f"{context}: {use_case} public {metric} loses to eligible exact route {candidate}"
          )
      route_results[candidate] = {
        "metrics": metrics,
        "passed": len(metrics) == len(METRICS) and all(
          result["passed"] for result in metrics.values()
        ),
      }

    expected_cpu_domain = (
      "monotonic wall fallback" if mode == "tagged_wall_fallback" else "thread CPU"
    )
    if use_case == "thread_cpu":
      for row_name, row in ((public_name, public), (selected_name, selected)):
        if row.get("time_domain") != expected_cpu_domain:
          failures.append(f"{context}: {row_name} changed the declared thread timeline")

    results[use_case] = {
      "selection_kind": selection_kind,
      "selected_row": selected_name,
      "eligible_exact_routes": route_results,
      "passed": (
        len(route_results) == len(candidates)
        and all(result["passed"] for result in route_results.values())
      ),
    }
  return results


def validate_native_thread_cpu_behavior(
  context: str,
  behavior: object,
  failures: list[str],
) -> dict:
  if not isinstance(behavior, dict):
    failures.append(f"{context}: missing current-thread CPU semantic probes")
    return {}
  results = {}
  for phase in ("busy", "sleep", "sibling_isolation"):
    probe = behavior.get(phase)
    if not isinstance(probe, dict) or not all(
      finite_number(probe.get(field)) and probe[field] >= 0
      for field in ("wall_delta_ns", "public_delta_ns", "direct_delta_ns")
    ):
      failures.append(f"{context}: malformed {phase} semantic probe")
      continue
    wall_delta = probe["wall_delta_ns"]
    public_delta = probe["public_delta_ns"]
    direct_delta = probe["direct_delta_ns"]
    if phase == "busy":
      tolerance = max(100_000.0, direct_delta * 0.05)
      passed = (
        public_delta > 0
        and direct_delta > 0
        and abs(public_delta - direct_delta) <= tolerance
      )
    else:
      tolerance = max(100_000.0, wall_delta * 0.01)
      passed = public_delta <= tolerance and direct_delta <= tolerance
    results[phase] = {"passed": passed, "tolerance_ns": tolerance}
    if not passed:
      failures.append(f"{context}: {phase} thread-CPU semantic probe failed")
  return results


def validate_supplemental_speed_campaign(documents: dict[str, dict]) -> dict:
  failures = []
  results = []
  expected_names = set(SUPPLEMENTAL_SPEED_CELLS)
  actual_names = set(documents)
  if actual_names != expected_names:
    failures.append(
      "supplemental three-clock artifacts differ: "
      f"missing={sorted(expected_names - actual_names)!r}, "
      f"unexpected={sorted(actual_names - expected_names)!r}"
    )
  revisions = set()
  for name in sorted(expected_names & actual_names):
    document = documents[name]
    expected_triple, expected_harness, mode = SUPPLEMENTAL_SPEED_CELLS[name]
    context = f"supplemental {name}"
    provenance = document.get("provenance")
    if (
      document.get("schema") != "tach-speed-supplemental-v2"
      or document.get("triple") != expected_triple
      or document.get("mode") != mode
      or not isinstance(provenance, dict)
      or provenance.get("harness") != expected_harness
    ):
      failures.append(f"{context}: identity/provenance does not match the checked cell")
      continue
    revision = provenance.get("source_revision")
    if not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", revision):
      failures.append(f"{context}: invalid full source revision")
    else:
      revisions.add(revision)
    if mode == "runtime_smoke":
      assertions = document.get("assertions")
      if (
        document.get("evidence_class") != "runtime_smoke"
        or document.get("passed") is not True
        or not isinstance(assertions, list)
        or not assertions
        or not all(isinstance(assertion, str) and assertion for assertion in assertions)
      ):
        failures.append(f"{context}: malformed runtime smoke evidence")
      results.append({"artifact": name, "mode": mode, "passed": document.get("passed") is True})
      continue

    route_results = validate_supplemental_route_coverage(context, document, failures)
    clocks = document.get("clocks")
    public = clocks.get("tach_thread_cpu") if isinstance(clocks, dict) else None
    if mode == "tagged_wall_fallback":
      behavior = document.get("thread_cpu_behavior")
      passed = (
        document.get("evidence_class") == "measured_external_runtime"
        and isinstance(public, dict)
        and public.get("time_domain") == "monotonic wall fallback"
        and isinstance(behavior, dict)
        and behavior.get("tagged_wall_fallback") is True
        and behavior.get("wall_time_advanced_during_sleep") is True
        and set(route_results) == {"instant", "ordered", "thread_cpu"}
        and all(result.get("passed") is True for result in route_results.values())
      )
      if not passed:
        failures.append(f"{context}: fallback runtime was not explicitly tagged as wall time")
      results.append({"artifact": name, "mode": mode, "passed": passed})
      continue

    if document.get("evidence_class") != "measured_external_runtime" or not isinstance(clocks, dict):
      failures.append(f"{context}: malformed measured three-clock evidence")
      continue
    required_clocks = {
      "tach", "tach_ordered", "quanta", "fastant", "minstant", "std",
      "tach_thread_cpu", "native_thread_cpu",
    }
    if not required_clocks <= set(clocks):
      failures.append(f"{context}: full speed cell omits public/reference clocks")
      cell_report = {}
    else:
      cell_failures, cell_report = validate_cell(context, clocks, expected_triple)
      failures.extend(cell_failures)
    semantic_results = validate_native_thread_cpu_behavior(
      context, document.get("thread_cpu_behavior"), failures
    )
    results.append({
      "artifact": name,
      "mode": mode,
      "three_clock_speed": cell_report,
      "route_coverage": route_results,
      "semantics": semantic_results,
      "passed": not any(failure.startswith(context) for failure in failures),
    })
  if len(revisions) != 1:
    failures.append(f"supplemental campaign must use one source revision: {sorted(revisions)}")
  return {
    "schema": "tach-speed-supplemental-report-v2",
    "claim_scope": (
      "external runtime coverage for all three public timing contracts on hosted targets "
      "absent from the six-cell campaign"
    ),
    "evidence_class_invariant": (
      "measured runtime, tagged wall fallback, and runtime smoke evidence remain distinct; "
      "compile/codegen proof is never accepted as latency evidence"
    ),
    "source_revision": next(iter(revisions)) if len(revisions) == 1 else None,
    "passed": not failures,
    "failures": failures,
    "cells": results,
  }


def validate_campaign_for_checkout(documents: list[dict], root: Path) -> dict:
  report = validate_campaign(documents)
  revision = report.get("source_revision")
  if isinstance(revision, str):
    binding_failures, binding = validate_checkout_binding(root, revision)
  else:
    binding_failures = ["campaign has no single source revision to bind"]
    binding = {}
  report["checkout_binding"] = {"passed": not binding_failures, **binding}
  report["failures"].extend(binding_failures)
  report["passed"] = not report["failures"]
  return report
