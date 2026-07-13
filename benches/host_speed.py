"""Extraction rules for retained non-Criterion speed observations."""

from __future__ import annotations

import math
from pathlib import Path

import speed_evidence


LAMBDA_RUNS = range(1, speed_evidence.LAMBDA_INVOCATIONS + 1)


def _load_json(path: Path, description: str) -> dict:
  # Import lazily so extract_speed can dispatch to this module without a
  # module-initialization cycle.
  import extract_speed

  return extract_speed.load_json_object(path, description)


def _lambda_paths(host_dir: Path) -> tuple[list[Path], list[Path]]:
  payloads = [host_dir / f"run-{run}.json" for run in LAMBDA_RUNS]
  metadata = [host_dir / f"invoke-{run}.json" for run in LAMBDA_RUNS]
  return payloads, metadata


def _aggregate_sampled_rows(runs: list[dict], source: str) -> dict:
  wall_selections = [run.get("wall_selection") for run in runs]
  if not all(isinstance(selection, dict) for selection in wall_selections):
    raise RuntimeError(f"{source} wall selector metadata is malformed")
  wall_selection = wall_selections[0]
  if any(selection != wall_selection for selection in wall_selections[1:]):
    stable = [
      {key: value for key, value in selection.items() if key != "probe"}
      for selection in wall_selections
    ]
    probes = [selection.get("probe") for selection in wall_selections]
    if (
      source != "host-runtime"
      or any(value != stable[0] for value in stable[1:])
      or not all(isinstance(probe, dict) for probe in probes)
    ):
      raise RuntimeError(f"{source} wall selector metadata changed across samples")
    wall_selection = dict(wall_selection)
    wall_selection["probe_observations"] = probes
  if not isinstance(wall_selection, dict):
    raise RuntimeError(f"{source} wall selector metadata changed across samples")
  wall_candidates = wall_selection.get("eligible_direct_candidates")
  if not isinstance(wall_candidates, dict):
    raise RuntimeError(f"{source} wall selector omitted eligible candidates")

  thread_selections = [run.get("thread_cpu_selection") for run in runs]
  if not isinstance(thread_selections[0], dict) or any(
    selection != thread_selections[0] for selection in thread_selections[1:]
  ):
    raise RuntimeError(f"{source} thread-CPU selector metadata changed across samples")
  thread_selection = thread_selections[0]
  thread_candidates = thread_selection.get("eligible_direct_candidates")
  if (
    not isinstance(thread_candidates, list)
    or not thread_candidates
    or not all(isinstance(candidate, str) and candidate for candidate in thread_candidates)
  ):
    raise RuntimeError(f"{source} thread-CPU selector has malformed eligible candidates")

  clock_keys = [
    "tach",
    "tach_ordered",
    "tach_thread_cpu",
    "native_thread_cpu",
    "direct_selected_wall",
    "direct_selected_ordered_wall",
    "direct_selected_thread_cpu",
  ]
  for reference in speed_evidence.LOCAL_COMPETITORS:
    present = [isinstance(run.get(reference), dict) for run in runs]
    if any(present) and not all(present):
      raise RuntimeError(f"{source} reference row {reference} changed availability")
    if all(present):
      clock_keys.append(reference)
  for domain in ("instant", "ordered"):
    candidates = wall_candidates.get(domain)
    if (
      not isinstance(candidates, list)
      or not candidates
      or not all(isinstance(candidate, str) and candidate for candidate in candidates)
    ):
      raise RuntimeError(f"{source} wall selector has malformed {domain} candidates")
    clock_keys.extend(candidates)
  clock_keys.extend(thread_candidates)
  if thread_selection.get("fallback_native_benchmark") is not None:
    clock_keys.append("direct_fallback_thread_cpu")

  clocks = {}
  for key in dict.fromkeys(clock_keys):
    rows = [run.get(key) for run in runs]
    if not all(isinstance(row, dict) for row in rows):
      raise RuntimeError(f"{source} samples omitted clock row {key}")
    entry = {}
    for metric in speed_evidence.METRICS:
      sample_lists = [row.get(f"{metric}_samples") for row in rows]
      if not all(
        isinstance(samples, list)
        and len(samples) == speed_evidence.LAMBDA_SAMPLES_PER_INVOCATION
        for samples in sample_lists
      ):
        raise RuntimeError(f"{source} samples omitted {key}.{metric}_samples")
      samples = [sample for values in sample_lists for sample in values]
      if not all(
        type(sample) in (int, float) and math.isfinite(sample) and sample >= 0
        for sample in samples
      ):
        raise RuntimeError(f"{source} samples contained invalid {key}.{metric} values")
      point, interval = speed_evidence.lambda_median_with_ci(
        samples,
        f"{key}:{metric}",
      )
      entry[metric] = point
      entry[f"{metric}_ci95"] = interval
      entry[f"{metric}_samples"] = samples

    exact_wall = key.startswith(("direct_wall__", "direct_ordered_wall__"))
    carries_identity = (
      key.startswith("direct_thread_cpu__")
      or exact_wall
      or key
      in {
        "tach",
        "tach_ordered",
        "tach_thread_cpu",
        "native_thread_cpu",
        "direct_selected_wall",
        "direct_selected_ordered_wall",
        "direct_selected_thread_cpu",
        "direct_fallback_thread_cpu",
      }
    )
    if carries_identity:
      for field in ("provider", "read_cost", "time_domain"):
        values = {row.get(field) for row in rows}
        if len(values) != 1:
          raise RuntimeError(f"{key} {field} changed across {source} samples")
        value = values.pop()
        if not isinstance(value, str) or not value:
          raise RuntimeError(f"{key} omitted {field}")
        entry[field] = value
      if key.startswith("direct_") or key == "native_thread_cpu":
        benchmarks = {row.get("benchmark") for row in rows}
        if len(benchmarks) != 1:
          raise RuntimeError(f"{key} benchmark identity changed across {source} samples")
        benchmark = benchmarks.pop()
        if not isinstance(benchmark, str) or not benchmark:
          raise RuntimeError(f"{key} omitted its benchmark identity")
        entry["benchmark"] = benchmark
    clocks[key] = entry

  clocks["tach"]["selection"] = wall_selection
  clocks["tach_ordered"]["wall_selection"] = wall_selection
  clocks["tach_thread_cpu"]["selection"] = thread_selection
  return clocks


def extract_lambda_observation(host_dir: Path, attestation: dict) -> dict:
  payload_paths, metadata_paths = _lambda_paths(host_dir)
  expected = {
    "runtime-attestation.json",
    *(path.name for path in payload_paths),
    *(path.name for path in metadata_paths),
  }
  actual = {path.name for path in host_dir.iterdir()}
  if actual != expected:
    raise RuntimeError(
      "Lambda host observation file set changed: "
      f"missing={sorted(expected - actual)!r}, unexpected={sorted(actual - expected)!r}"
    )

  runs = []
  for run, payload_path, metadata_path in zip(
    LAMBDA_RUNS,
    payload_paths,
    metadata_paths,
  ):
    metadata = _load_json(metadata_path, f"Lambda invocation {run} metadata")
    if metadata.get("StatusCode") != 200 or metadata.get("FunctionError") is not None:
      raise RuntimeError(f"Lambda invocation {run} failed: {metadata!r}")
    payload = _load_json(payload_path, f"Lambda invocation {run} payload")
    if payload.get("runtime_attestation") != attestation:
      raise RuntimeError(f"Lambda invocation {run} runtime attestation changed")
    runs.append(payload)

  behaviors = [run.get("thread_cpu_behavior") for run in runs]
  for run, behavior in enumerate(behaviors, start=1):
    if not isinstance(behavior, dict) or behavior.get("runtime_attestation") != attestation:
      raise RuntimeError(f"Lambda invocation {run} thread-CPU behavior is unbound")
    try:
      speed_evidence._validate_raw_thread_cpu_behavior(behavior)
    except ValueError as error:
      raise RuntimeError(
        f"Lambda invocation {run} thread-CPU behavior is malformed: {error}"
      ) from error

  return {
    "clocks": _aggregate_sampled_rows(runs, "Lambda"),
    "thread_cpu_behavior": behaviors[0],
  }


def extract_host_runtime_observation(host_dir: Path, attestation: dict) -> dict:
  payload_paths = [host_dir / f"run-{run}.json" for run in LAMBDA_RUNS]
  expected = {
    "runtime-attestation.json",
    *(path.name for path in payload_paths),
  }
  actual = {path.name for path in host_dir.iterdir()}
  if actual != expected:
    raise RuntimeError(
      "host-runtime observation file set changed: "
      f"missing={sorted(expected - actual)!r}, unexpected={sorted(actual - expected)!r}"
    )
  runs = []
  for run, payload_path in enumerate(payload_paths, start=1):
    payload = _load_json(payload_path, f"host-runtime observation {run}")
    if payload.get("runtime_attestation") != attestation:
      raise RuntimeError(f"host-runtime observation {run} attestation changed")
    behavior = payload.get("thread_cpu_behavior")
    if not isinstance(behavior, dict) or behavior.get("runtime_attestation") != attestation:
      raise RuntimeError(f"host-runtime observation {run} thread-CPU behavior is unbound")
    try:
      speed_evidence._validate_raw_thread_cpu_behavior(behavior)
    except ValueError as error:
      raise RuntimeError(
        f"host-runtime observation {run} thread-CPU behavior is malformed: {error}"
      ) from error
    runs.append(payload)
  return {
    "clocks": _aggregate_sampled_rows(runs, "host-runtime"),
    "thread_cpu_behavior": runs[0]["thread_cpu_behavior"],
  }


def extract_host_observation(host_dir: Path, attestation: dict) -> dict:
  harness = attestation.get("harness")
  if harness == "lambda":
    return extract_lambda_observation(host_dir, attestation)
  if harness in {
    "node-wasm-bindgen",
    "browser",
    "emcc-node",
    "node-uvwasi",
    "wasmtime",
    "wasmtime-component",
  }:
    return extract_host_runtime_observation(host_dir, attestation)
  raise RuntimeError(f"unsupported retained host harness {harness!r}")
