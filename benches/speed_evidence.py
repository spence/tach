"""Shared evidence rules for tach's three steady-state timing contracts."""

from __future__ import annotations

import re
import statistics


LOCAL_COMPETITORS = ("quanta", "fastant", "minstant", "std")
METRICS = ("now", "elapsed")
SELECTOR_SAMPLES = 9
SELECTOR_REQUIRED_WINS = 8
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


def equivalence_allowance(reference: float) -> float:
  """Predeclared material-equivalence band for per-call nanoseconds."""
  return max(1.0, reference * 0.05)


def validate_ci(context: str, clock: str, entry: dict, failures: list[str]) -> None:
  for metric in METRICS:
    interval = entry.get(f"{metric}_ci95")
    point = entry.get(metric)
    if (
      not isinstance(point, (int, float))
      or point < 0
      or not isinstance(interval, list)
      or len(interval) != 2
      or not all(isinstance(bound, (int, float)) for bound in interval)
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


def selector_decision(selection: dict, failures: list[str], context: str) -> str:
  initializations = selection.get("initializations")
  if initializations is not None:
    if not isinstance(initializations, list) or not initializations:
      failures.append(f"{context}: malformed selector initialization evidence")
    else:
      decisions = {
        selector_decision(initialization, failures, f"{context} selector init {index}")
        for index, initialization in enumerate(initializations)
      }
      decisions.discard("invalid")
      if len(decisions) > 1:
        failures.append(f"{context}: selector decision changed across initializations")

  if not selection.get("available"):
    if selection.get("decision") != "syscall":
      failures.append(f"{context}: unavailable selector did not choose syscall")
    return "syscall"

  iterations = selection.get("iterations")
  perf = selection.get("perf_total_ns_samples")
  syscall = selection.get("syscall_total_ns_samples")
  if (
    not isinstance(iterations, int)
    or iterations <= 0
    or not isinstance(perf, list)
    or not isinstance(syscall, list)
    or len(perf) != SELECTOR_SAMPLES
    or len(syscall) != SELECTOR_SAMPLES
    or not all(isinstance(value, int) and value > 0 for value in perf + syscall)
  ):
    failures.append(f"{context}: malformed selector sample evidence")
    return "invalid"

  perf_median = statistics.median(perf)
  syscall_median = statistics.median(syscall)
  allowance = max(iterations, syscall_median // 20)
  wins = sum(
    perf_sample + allowance < syscall_sample
    for perf_sample, syscall_sample in zip(perf, syscall, strict=True)
  )
  expected = (
    "perf"
    if perf_median + allowance < syscall_median and wins >= SELECTOR_REQUIRED_WINS
    else "syscall"
  )

  persisted = {
    "perf_median_total_ns": perf_median,
    "syscall_median_total_ns": syscall_median,
    "equivalence_allowance_total_ns": allowance,
    "decisive_paired_wins": wins,
    "required_decisive_paired_wins": SELECTOR_REQUIRED_WINS,
  }
  for field, expected_value in persisted.items():
    if selection.get(field) != expected_value:
      failures.append(
        f"{context}: selector {field}={selection.get(field)!r}, expected {expected_value!r}"
      )
  if selection.get("decision") != expected:
    failures.append(f"{context}: selector decision does not reproduce from its samples")
  return expected


def validate_cell(context: str, clocks: dict) -> tuple[list[str], dict]:
  failures: list[str] = []
  required = {
    "tach", "tach_ordered", "quanta", "fastant", "minstant", "std",
    "tach_thread_cpu", "native_thread_cpu",
  }
  missing = sorted(required - clocks.keys())
  if missing:
    return [f"{context}: missing clocks: {', '.join(missing)}"], {}

  for clock in sorted(required | ({"direct_thread_cpu"} & clocks.keys())):
    validate_ci(context, clock, clocks[clock], failures)
  if failures:
    return failures, {}

  same_thread = {}
  for metric in METRICS:
    comparisons = {}
    for reference_name in LOCAL_COMPETITORS:
      passed, allowance = equivalent_or_faster(
        clocks["tach"], clocks[reference_name], metric
      )
      comparisons[reference_name] = {
        "reference_ns": clocks[reference_name][metric],
        "equivalence_allowance_ns": allowance,
        "passed": passed,
      }
      if not passed:
        failures.append(
          f"{context}: Instant {metric} is materially slower than {reference_name}"
        )
    same_thread[metric] = {
      "tach_ns": clocks["tach"][metric],
      "comparisons": comparisons,
      "passed": all(item["passed"] for item in comparisons.values()),
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

  tach_thread = clocks["tach_thread_cpu"]
  native = clocks["native_thread_cpu"]
  for name, entry in (("tach_thread_cpu", tach_thread), ("native_thread_cpu", native)):
    if entry.get("time_domain") != "thread CPU":
      failures.append(f"{context}: {name} did not measure current-thread CPU time")

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

  selection = tach_thread.get("selection")
  if selection is not None:
    expected = selector_decision(selection, failures, context)
    if selection.get("selected_provider") != expected:
      failures.append(f"{context}: selected provider disagrees with selector decision")
    provider_expected = "perf" if tach_thread.get("provider") == "Linux perf mmap" else "syscall"
    if expected != "invalid" and expected != provider_expected:
      failures.append(f"{context}: provider label disagrees with selector evidence")

  direct = clocks.get("direct_thread_cpu")
  if tach_thread.get("provider") == "Linux perf mmap" and direct is None:
    failures.append(f"{context}: selected perf without a direct cached-handle baseline")
  if direct is not None:
    if direct.get("time_domain") != "thread CPU":
      failures.append(f"{context}: direct perf baseline is not thread CPU time")
    current_thread["direct_perf"] = {}
    for metric in METRICS:
      passed, allowance = equivalent_or_faster(tach_thread, direct, metric)
      current_thread["direct_perf"][metric] = {
        "tach_ns": tach_thread[metric],
        "direct_ns": direct[metric],
        "equivalence_allowance_ns": allowance,
        "passed": passed,
      }
      if not passed:
        failures.append(
          f"{context}: ThreadCpuInstant {metric} is materially slower than direct perf"
        )

  return failures, {
    "same_thread_elapsed": same_thread,
    "cross_thread_elapsed": cross_thread,
    "current_thread_cpu": current_thread,
    "provider": tach_thread.get("provider"),
    "selector": selection,
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
      if sorted(provenance.get("features", [])) != [
        "bench-internal", "thread-cpu-inline"
      ]:
        failures.append(f"{context}: benchmark feature set changed")

    clocks = document.get("clocks")
    if not isinstance(clocks, dict):
      failures.append(f"{context}: missing clocks object")
      continue
    triple = str(document.get("triple", ""))
    if (
      "linux" in triple
      and triple.startswith(("x86_64", "aarch64"))
      and clocks.get("tach_thread_cpu", {}).get("selection") is None
    ):
      failures.append(f"{context}: Linux inline target is missing selector evidence")

    cell_failures, result = validate_cell(context, clocks)
    failures.extend(cell_failures)
    results.append({"environment": context, **result})

  if len(revisions) > 1:
    failures.append(f"campaign mixed source revisions: {sorted(revisions)}")

  return {
    "schema": "tach-speed-evidence-v2",
    "claim": "no tach clock is materially slower than an eligible reference",
    "equivalence_rule": (
      "tach point estimate and conservative 95% CI comparison fit within "
      "max(1 ns, 5%) of every eligible reference"
    ),
    "selector_rule": (
      "perf median advantage > max(1 ns/read, 5%) with >=8/9 decisive paired wins"
    ),
    "source_revision": next(iter(revisions)) if len(revisions) == 1 else None,
    "passed": not failures,
    "failures": failures,
    "environments": results,
  }
