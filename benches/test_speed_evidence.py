#!/usr/bin/env python3

from __future__ import annotations

import contextlib
import copy
import importlib.util
import io
import json
import re
import statistics
import subprocess
import sys
import tempfile
import tomllib
import unittest
from unittest import mock
from pathlib import Path

import extract_speed
import speed_evidence


ROOT = Path(__file__).resolve().parents[1]
ROUTE_COVERAGE_PATH = Path(__file__).with_name("route-coverage.toml")
ROUTE_TIMERS = ("instant", "ordered", "thread_cpu")
ROUTE_IDENTITIES = {
  "instant": {
    "public": "tach",
    "selected": "direct_selected_wall",
    "selection_metadata": "tach.selection",
    "metadata_fields": {
      "selected_provider",
      "selected_native_benchmark",
      "eligible_direct_candidates",
    },
  },
  "ordered": {
    "public": "tach_ordered",
    "selected": "direct_selected_ordered_wall",
    "selection_metadata": "tach_ordered.wall_selection",
    "metadata_fields": {
      "selected_provider",
      "selected_native_benchmark",
      "eligible_direct_candidates",
    },
  },
  "thread_cpu": {
    "public": "tach_thread_cpu",
    "selected": "direct_selected_thread_cpu",
    "selection_metadata": "tach_thread_cpu.selection",
    "metadata_fields": {
      "selected_provider",
      "selected_mechanism",
      "selected_read_cost",
      "selected_native_benchmark",
      "eligible_direct_candidates",
    },
  },
}
SELECTION_PROFILES = {
  "runtime_tournament",
  "fixed_native",
  "availability_fallback",
  "fallback_only",
}
PRODUCER_KINDS = {"criterion", "lambda", "host_runtime", "runtime_smoke"}
RUNTIME_PROOFS = {"not_collected", "runtime_smoke"}
NOW_ELAPSED_CODEGEN_ROUTES = {
  "instant_now_elapsed",
  "ordered_instant_now_elapsed",
  "thread_cpu_instant_now_elapsed",
}
MACOS_FIXED_NATIVE_SELECTOR = {
  "selection_kind": "fixed_native",
  "selected_provider": "posix_thread_cpu_clock",
  "selected_mechanism": "macos_clock_gettime_nsec_np_thread_cpu",
  "selected_read_cost": "system call",
  "selected_native_benchmark": (
    "direct_selected_thread_cpu__macos_clock_gettime_nsec_np_thread_cpu"
  ),
  "eligible_direct_candidates": [
    "direct_thread_cpu__macos_clock_gettime_nsec_np_thread_cpu"
  ],
}
MACOS_FIXED_NATIVE_PROVIDER = {
  "native_primitive": "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)",
  "time_domain": "thread CPU",
}
PROFILE_CONTRACT_DEFINITIONS = {
  "wall_runtime_tournament": {
    "selection_profile": "runtime_tournament",
    "validator": "wall_runtime_tournament_schema",
    "metadata_fields": [
      "selected_provider",
      "selected_native_benchmark",
      "eligible_direct_candidates",
      "probe",
    ],
  },
  "wall_fixed_native": {
    "selection_profile": "fixed_native",
    "validator": "wall_fixed_native_schema",
    "metadata_fields": [
      "selected_provider",
      "selected_native_benchmark",
      "eligible_direct_candidates",
      "fixed_provider",
    ],
  },
  "thread_cpu_runtime_tournament": {
    "selection_profile": "runtime_tournament",
    "validator": "validate_thread_cpu_selector",
    "selection_kinds": ["tournament_with_measured_runner_up"],
    "metadata_fields": [
      "selected_provider",
      "selected_mechanism",
      "selected_read_cost",
      "selected_native_benchmark",
      "eligible_direct_candidates",
      "native_entry_probe",
      "perf",
    ],
  },
  "thread_cpu_fixed_native": {
    "selection_profile": "fixed_native",
    "validator": "validate_fixed_native_thread_cpu_selector",
    "selection_kinds": ["fixed_native"],
    "metadata_fields": [
      "selection_kind",
      "selected_provider",
      "selected_mechanism",
      "selected_read_cost",
      "selected_native_benchmark",
      "eligible_direct_candidates",
      "fixed_provider",
      "read_cost_basis",
    ],
    "forbidden_metadata": ["perf", "native_entry_probe", "failure_fallback"],
  },
  "thread_cpu_availability_fallback": {
    "selection_profile": "availability_fallback",
    "validator": "availability_fallback_thread_cpu_schema",
    "selection_kinds": ["availability_fallback", "fixed_windows_thread_times"],
    "metadata_fields": [
      "selected_provider",
      "selected_mechanism",
      "selected_read_cost",
      "selected_native_benchmark",
      "eligible_direct_candidates",
      "failure_fallback",
    ],
  },
  "thread_cpu_capability_preferred": {
    "selection_profile": "availability_fallback",
    "validator": "validate_thread_cpu_selector",
    "selection_kinds": ["capability_preferred_with_failure_fallback"],
    "metadata_fields": [
      "selected_provider",
      "selected_mechanism",
      "selected_read_cost",
      "selected_native_benchmark",
      "eligible_direct_candidates",
      "native_entry_probe",
      "perf",
      "failure_fallback",
    ],
  },
  "thread_cpu_fallback_only": {
    "selection_profile": "fallback_only",
    "validator": "fallback_only_thread_cpu_schema",
    "selection_kinds": ["fallback_only"],
    "metadata_fields": [
      "selected_provider",
      "selected_mechanism",
      "selected_read_cost",
      "selected_native_benchmark",
      "eligible_direct_candidates",
      "time_domain",
    ],
  },
}
PROFILE_CONTRACT_BY_TIMER_PROFILE = {
  ("instant", "runtime_tournament"): "wall_runtime_tournament",
  ("ordered", "runtime_tournament"): "wall_runtime_tournament",
  ("instant", "fixed_native"): "wall_fixed_native",
  ("ordered", "fixed_native"): "wall_fixed_native",
  ("thread_cpu", "runtime_tournament"): "thread_cpu_runtime_tournament",
  ("thread_cpu", "fixed_native"): "thread_cpu_fixed_native",
  ("thread_cpu", "availability_fallback"): "thread_cpu_availability_fallback",
  ("thread_cpu", "fallback_only"): "thread_cpu_fallback_only",
}


def target_provider_module():
  path = Path(__file__).with_name("verify-target-providers.py")
  spec = importlib.util.spec_from_file_location("verify_target_providers", path)
  assert spec is not None and spec.loader is not None
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  return module


def supplemental_composer_module():
  path = Path(__file__).with_name("compose-supplemental-speed.py")
  spec = importlib.util.spec_from_file_location("compose_supplemental_speed", path)
  assert spec is not None and spec.loader is not None
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  return module


def primary_composer_module():
  path = Path(__file__).with_name("compose-speed.py")
  spec = importlib.util.spec_from_file_location("compose_speed", path)
  assert spec is not None and spec.loader is not None
  module = importlib.util.module_from_spec(spec)
  spec.loader.exec_module(module)
  return module


def load_route_coverage_manifest() -> dict:
  with ROUTE_COVERAGE_PATH.open("rb") as source:
    return tomllib.load(source)


def validate_route_contract(
  contract_id: object,
  contracts: object,
  failures: list[str],
) -> None:
  if not isinstance(contract_id, str) or not isinstance(contracts, dict):
    failures.append("route coverage case has no route contract")
    return
  contract = contracts.get(contract_id)
  if not isinstance(contract, dict):
    failures.append(f"route coverage references unknown contract {contract_id!r}")
    return
  routes = {timer: contract.get(timer) for timer in ROUTE_TIMERS}
  if set(contract) != {"description", *ROUTE_TIMERS} or not isinstance(
    contract.get("description"), str
  ):
    failures.append(f"route contract {contract_id!r} must describe exactly three timers")
  for timer, expected in ROUTE_IDENTITIES.items():
    route = routes[timer]
    if not isinstance(route, dict):
      failures.append(f"route contract {contract_id!r} lacks {timer} route")
      continue
    if route.get("public") != expected["public"]:
      failures.append(f"route contract {contract_id!r} changes {timer} public row")
    if route.get("selected") != expected["selected"]:
      failures.append(f"route contract {contract_id!r} changes {timer} selected direct row")
    if route.get("operations") != ["now", "elapsed"]:
      failures.append(
        f"route contract {contract_id!r} must declare {timer} now and elapsed"
      )
    if route.get("exact_candidate_identity") is not True:
      failures.append(
        f"route contract {contract_id!r} must bind {timer} exact candidate identity"
      )
    if route.get("selection_metadata") != expected["selection_metadata"]:
      failures.append(
        f"route contract {contract_id!r} changes {timer} selection metadata location"
      )
    fields = route.get("metadata_fields")
    if not isinstance(fields, list) or set(fields) != expected["metadata_fields"]:
      failures.append(
        f"route contract {contract_id!r} has incomplete {timer} selection metadata"
      )


def validate_declared_fixed_native_selector(
  case_id: str,
  profiles: object,
  selector: object,
  failures: list[str],
) -> None:
  if selector is None:
    if case_id == "macos_fixed_native":
      failures.append(f"case {case_id!r} lacks fixed-native thread-CPU metadata")
    return
  if not isinstance(selector, dict):
    failures.append(f"case {case_id!r} has malformed fixed-native thread-CPU metadata")
    return
  if not isinstance(profiles, dict) or profiles.get("thread_cpu") != "fixed_native":
    failures.append(f"case {case_id!r} declares a fixed-native selector for another profile")
    return
  if case_id != "macos_fixed_native":
    failures.append(f"case {case_id!r} has an undocumented fixed-native selector shape")
    return
  if any(selector.get(key) != value for key, value in MACOS_FIXED_NATIVE_SELECTOR.items()):
    failures.append(f"case {case_id!r} changes fixed-native thread-CPU identity")
  if selector.get("fixed_provider") != MACOS_FIXED_NATIVE_PROVIDER:
    failures.append(f"case {case_id!r} changes fixed-native native API metadata")
  if any(key in selector for key in ("perf", "native_entry_probe", "failure_fallback")):
    failures.append(f"case {case_id!r} fabricates a fixed-native perf or fallback path")


def validate_profile_contracts(
  case_id: str,
  profiles: object,
  profile_contracts: object,
  definitions: object,
  failures: list[str],
) -> None:
  if not isinstance(definitions, dict) or definitions != PROFILE_CONTRACT_DEFINITIONS:
    failures.append("route coverage profile-contract definitions changed")
    return
  if not isinstance(profiles, dict):
    return
  if not isinstance(profile_contracts, dict) or set(profile_contracts) != set(ROUTE_TIMERS):
    failures.append(f"case {case_id!r} lacks a concrete validator/schema for every profile")
    return
  for timer in ROUTE_TIMERS:
    profile = profiles.get(timer)
    if case_id == "linux_aarch64_capability_preferred_default" and timer == "thread_cpu":
      expected_contract = "thread_cpu_capability_preferred"
    else:
      expected_contract = PROFILE_CONTRACT_BY_TIMER_PROFILE.get((timer, profile))
    actual_contract = profile_contracts.get(timer)
    if expected_contract is None:
      failures.append(f"case {case_id!r} has no validator/schema for {timer} {profile!r}")
    elif actual_contract != expected_contract:
      failures.append(
        f"case {case_id!r} maps {timer} {profile!r} to the wrong validator/schema"
      )


def validate_route_coverage_manifest(document: dict, root: Path) -> list[str]:
  """Validate static route declarations without claiming a runnable host."""
  failures: list[str] = []
  if document.get("schema") != "tach-route-coverage-v1":
    failures.append("route coverage schema changed")
  claim_scope = document.get("claim_scope")
  if not isinstance(claim_scope, str) or "not runtime" not in claim_scope.lower():
    failures.append("route coverage must explicitly distinguish static declaration from runtime proof")

  producers = document.get("producer")
  producer_by_id: dict[str, dict] = {}
  if not isinstance(producers, list):
    failures.append("route coverage producers must be a list")
    producers = []
  for producer in producers:
    if not isinstance(producer, dict):
      failures.append("route coverage contains a malformed producer")
      continue
    producer_id = producer.get("id")
    if not isinstance(producer_id, str) or not producer_id:
      failures.append("route coverage producer lacks an id")
      continue
    if producer_id in producer_by_id:
      failures.append(f"route coverage duplicates producer {producer_id!r}")
      continue
    producer_by_id[producer_id] = producer
    if producer.get("kind") not in PRODUCER_KINDS:
      failures.append(f"producer {producer_id!r} has an unknown kind")
    entrypoints = producer.get("entrypoints")
    if not isinstance(entrypoints, list) or not entrypoints or not all(
      isinstance(path, str) and path for path in entrypoints
    ):
      failures.append(f"producer {producer_id!r} has no concrete entrypoint")
    else:
      for entrypoint in entrypoints:
        if not (root / entrypoint).is_file():
          failures.append(
            f"producer {producer_id!r} entrypoint does not exist: {entrypoint}"
          )
    if producer.get("state") != "ready":
      failures.append(f"producer {producer_id!r} is {producer.get('state')!r}, not ready")

  provider_matrix = target_provider_module()
  advertised = {
    (target, mode)
    for target in provider_matrix.TARGETS
    for mode in provider_matrix.target_modes(target)
  }
  cases = document.get("case")
  contracts = document.get("route_contracts")
  profile_contract_definitions = document.get("selection_profile_contracts")
  if not isinstance(cases, list):
    failures.append("route coverage cases must be a list")
    cases = []
  declared_build_identities: set[tuple[str, str]] = set()
  declared_runtime_identities: set[tuple[str, str, str]] = set()
  validated_contracts: set[str] = set()
  for case in cases:
    if not isinstance(case, dict):
      failures.append("route coverage contains a malformed case")
      continue
    case_id = case.get("id")
    if not isinstance(case_id, str) or not case_id:
      failures.append("route coverage case lacks an id")
      case_id = "<unnamed>"
    if case.get("static_contract") is not True:
      failures.append(f"case {case_id!r} does not declare a static contract")
    runtime_proof = case.get("runtime_proof")
    if runtime_proof not in RUNTIME_PROOFS:
      failures.append(f"case {case_id!r} falsely labels static declaration as runtime proof")
    if runtime_proof == "runtime_smoke":
      routes = case.get("codegen_now_elapsed_routes")
      if not isinstance(routes, list) or set(routes) != NOW_ELAPSED_CODEGEN_ROUTES:
        failures.append(f"smoke case {case_id!r} lacks all three now-plus-elapsed codegen routes")
    elif "codegen_now_elapsed_routes" in case:
      failures.append(f"non-smoke case {case_id!r} has an irrelevant smoke route list")

    producer_id = case.get("producer")
    if not isinstance(producer_id, str) or producer_id not in producer_by_id:
      failures.append(f"case {case_id!r} references a nonexistent planned producer")
    elif runtime_proof == "runtime_smoke" and producer_by_id[producer_id].get("kind") != "runtime_smoke":
      failures.append(f"smoke case {case_id!r} does not use a smoke producer")

    contract_id = case.get("route_contract")
    if isinstance(contract_id, str) and contract_id not in validated_contracts:
      validate_route_contract(contract_id, contracts, failures)
      validated_contracts.add(contract_id)
    elif not isinstance(contract_id, str):
      validate_route_contract(contract_id, contracts, failures)

    profiles = case.get("selection_profiles")
    if not isinstance(profiles, dict) or set(profiles) != set(ROUTE_TIMERS):
      failures.append(f"case {case_id!r} lacks typed selection profiles for all timers")
    elif any(profile not in SELECTION_PROFILES for profile in profiles.values()):
      failures.append(f"case {case_id!r} has an unknown selection profile")
    validate_profile_contracts(
      case_id,
      profiles,
      case.get("profile_contracts"),
      profile_contract_definitions,
      failures,
    )
    validate_declared_fixed_native_selector(
      case_id, profiles, case.get("thread_cpu_selector"), failures
    )

    targets = case.get("targets")
    modes = case.get("modes")
    runtime_profile = case.get("runtime_profile")
    if not isinstance(runtime_profile, str) or not runtime_profile:
      failures.append(f"case {case_id!r} lacks a runtime profile")
      continue
    if not isinstance(targets, list) or not targets or not all(
      isinstance(target, str) and target for target in targets
    ):
      failures.append(f"case {case_id!r} lacks advertised targets")
      continue
    if not isinstance(modes, list) or not modes or not all(
      isinstance(mode, str) and mode for mode in modes
    ):
      failures.append(f"case {case_id!r} lacks target modes")
      continue
    for target in targets:
      if target not in provider_matrix.TARGETS:
        failures.append(f"case {case_id!r} declares unknown target {target!r}")
        continue
      supported_modes = set(provider_matrix.target_modes(target))
      for mode in modes:
        if mode not in supported_modes:
          failures.append(f"case {case_id!r} declares unknown mode {target} {mode!r}")
          continue
        declared_build_identities.add((target, mode))
        runtime_identity = (target, mode, runtime_profile)
        if runtime_identity in declared_runtime_identities:
          failures.append(f"route coverage duplicates runtime identity {runtime_identity!r}")
        declared_runtime_identities.add(runtime_identity)

  missing = sorted(advertised - declared_build_identities)
  if missing:
    failures.append(f"route coverage misses advertised target/mode identities: {missing!r}")
  unexpected = sorted(declared_build_identities - advertised)
  if unexpected:
    failures.append(f"route coverage declares unknown target/mode identities: {unexpected!r}")
  return failures


def route_coverage_admission_failures() -> list[str]:
  """Return the M1 admission failures for the checked-in static manifest."""
  return validate_route_coverage_manifest(load_route_coverage_manifest(), ROOT)


def estimate(value: float) -> dict:
  return {
    "now": value,
    "now_ci95": [value, value],
    "elapsed": value,
    "elapsed_ci95": [value, value],
  }


def clocks(
  provider: str = "POSIX thread CPU clock", read_cost: str = "system call"
) -> dict:
  values = {
    name: estimate(10.0)
    for name in ("tach", "tach_ordered", "quanta", "fastant", "minstant", "std")
  }
  values["tach_thread_cpu"] = {
    **estimate(10.0),
    "provider": provider,
    "read_cost": read_cost,
    "time_domain": "thread CPU",
  }
  values["native_thread_cpu"] = {
    **estimate(10.0),
    "provider": "native thread clock",
    "read_cost": "system call",
    "time_domain": "thread CPU",
  }
  return values


def synthetic_runtime_attestation(
  triple: str,
  harness: str,
  revision: str,
  build_mode: str,
  *,
  runtime_smoke: bool = False,
) -> dict:
  features = (
    speed_evidence.runtime_smoke_features_for_build_mode(build_mode)
    if runtime_smoke
    else speed_evidence.benchmark_features_for_build_mode(build_mode)
  )
  return {
    "schema": speed_evidence.RUNTIME_ATTESTATION_SCHEMA,
    "invocation_id": f"synthetic-{triple.replace('-', '_')}",
    "harness": harness,
    "target": copy.deepcopy(speed_evidence.SUPPLEMENTAL_RUNTIME_TARGETS[triple]),
    "features": list(features or ()),
    "build_mode": build_mode,
    "build_profile": "optimized",
    "source_revision": revision,
    "runner": "synthetic-runtime",
    "output_isolated": True,
  }


def primary_speed_observation(
  artifact_id: str = "speed-0-apple.json",
  revision: str = "1" * 40,
) -> tuple[dict, dict]:
  _, _, _, triple, harness, build_mode = speed_evidence.PRIMARY_SPEED_CELLS[artifact_id]
  assert harness == "criterion"
  assert isinstance(build_mode, str)
  attestation = synthetic_runtime_attestation(triple, harness, revision, build_mode)
  collector = {
    "schema": speed_evidence.COLLECTOR_ATTESTATION_SCHEMA,
    "invocation_id": attestation["invocation_id"],
    "runtime_attestation": attestation,
    "manifest_sha256": "f" * 64,
  }
  return clocks(), collector


def primary_speed_document(
  artifact_id: str = "speed-0-apple.json",
  revision: str = "1" * 40,
) -> dict:
  order, title, instance, triple, _, _ = speed_evidence.PRIMARY_SPEED_CELLS[artifact_id]
  values, collector = primary_speed_observation(artifact_id, revision)
  return speed_evidence.compose_primary_speed_cell(
    artifact_id,
    title,
    instance,
    triple,
    order,
    values,
    collector,
    "collector.bundle",
  )


def adaptive_thread_cpu_selection() -> dict:
  libc = [100_000] * 9
  raw = [95_000] * 7 + [100_000] * 2
  counter_names = [
    "x86_cpuid_rdtsc_cpuid",
    "x86_lfence_rdtsc_lfence",
    "x86_mfence_rdtsc_mfence",
    "x86_rdtscp_lfence",
    "x86_serialize_rdtsc_serialize",
  ]
  perf_mechanism = "linux_perf_mmap__x86_lfence_rdtsc_lfence"
  read_mechanism = "linux_perf_read__raw_read_syscall"
  return {
    "selected_provider": "linux_perf_mmap",
    "selected_read_cost": "inline",
    "selected_mechanism": perf_mechanism,
    "selected_native_benchmark": f"direct_selected_thread_cpu__{perf_mechanism}",
    "fallback_provider": "linux_perf_read",
    "fallback_read_cost": "system call",
    "fallback_mechanism": read_mechanism,
    "fallback_native_benchmark": f"direct_fallback_thread_cpu__{read_mechanism}",
    "eligible_direct_candidates": [
      "direct_thread_cpu__libc_entry",
      "direct_thread_cpu__raw_entry",
      "direct_thread_cpu__linux_perf_mmap__x86_cpuid_rdtsc_cpuid",
      "direct_thread_cpu__linux_perf_mmap__x86_lfence_rdtsc_lfence",
      "direct_thread_cpu__linux_perf_read__raw_read_syscall",
    ],
    "native_entry_probe": {
      "selected_provider": "raw_entry",
      "selected_read_cost": "system call",
      "libc_provider": "libc_entry",
      "raw_provider": "raw_entry",
      "libc_available": True,
      "raw_available": True,
      "reads_per_batch": 4_096,
      "required_decisive_wins": 7,
      "floor_ns_per_read": 1,
      "relative_denominator": None,
      "libc_batches_ns": libc,
      "raw_batches_ns": raw,
      "libc_median_ns": 100_000,
      "raw_median_ns": 95_000,
      "raw_allowance_ns": 4_096,
      "raw_decisive_wins": 7,
      "raw_selected": True,
      "libc_allowance_ns": 4_096,
      "libc_decisive_wins": 0,
      "libc_materially_faster": False,
    },
    "perf": {
      "event_available": True,
      "path_probe": {
        "selection_kind": "tournament_with_measured_runner_up",
        "candidate_names": [
          "posix_thread_cpu",
          "linux_perf_mmap",
          "linux_perf_read",
        ],
        "candidate_eligible": [True, True, True],
        "candidate_batches_ns": [
          [100_000] * 9,
          [80_000] * 9,
          [90_000] * 9,
        ],
        "selected_candidate": "linux_perf_mmap",
        "fallback_candidate": "linux_perf_read",
        "reads_per_batch": 4_096,
        "required_decisive_wins": 8,
        "equivalence_band": {"floor_ns_per_read": 1, "relative_denominator": 20},
        "capability_was_not_profitable": False,
      },
      "mmap": {
        "supported_on_target": True,
        "available": True,
        "read_cost": "inline",
        "selected_mechanism": perf_mechanism,
        "selected_candidate_benchmark": f"direct_thread_cpu__{perf_mechanism}",
        "eligible_benchmarks": [
          "direct_thread_cpu__linux_perf_mmap__x86_cpuid_rdtsc_cpuid",
          "direct_thread_cpu__linux_perf_mmap__x86_lfence_rdtsc_lfence",
        ],
        "counter_probe": {
          "candidate_names": counter_names,
          "candidate_eligible": [True, True, False, False, False],
          "candidate_batches_ns": [
            [100_000] * 9,
            [90_000] * 9,
            [0] * 9,
            [0] * 9,
            [0] * 9,
          ],
          "selected_candidate": "x86_lfence_rdtsc_lfence",
          "reads_per_batch": 4_096,
          "required_decisive_wins": 8,
          "equivalence_band": {"floor_ns_per_read": 1, "relative_denominator": 20},
        },
      },
      "read": {
        "supported_on_target": True,
        "available": True,
        "read_cost": "system call",
        "selected_mechanism": read_mechanism,
        "selected_candidate_benchmark": f"direct_thread_cpu__{read_mechanism}",
        "eligible_benchmarks": [
          "direct_thread_cpu__linux_perf_read__raw_read_syscall",
        ],
        "entry_probe": {
          "selection_kind": "fixed_candidate",
          "candidate_names": ["raw_read_syscall"],
          "candidate_eligible": [True],
          "candidate_measured": [False],
          "candidate_batches_ns": None,
          "selected_candidate": "raw_read_syscall",
          "reads_per_batch": 4_096,
          "required_decisive_wins": 8,
          "equivalence_band": {"floor_ns_per_read": 1, "relative_denominator": 20},
        },
      },
      "measurement_clock": (
        "raw SYS_clock_gettime(CLOCK_MONOTONIC_RAW), never libc/vDSO or the "
        "candidate under test"
      ),
      "decision_rule": "synthetic material-win tournament",
    },
  }


def aarch64_capability_thread_cpu_selection() -> dict:
  selection = adaptive_thread_cpu_selection()
  perf_mechanism = "linux_perf_mmap__aarch64_isb_cntvct_isb"
  read_mechanism = "linux_perf_read__raw_read_svc"
  selection.update({
    "selection_kind": "capability_preferred_with_failure_fallback",
    "selected_mechanism": perf_mechanism,
    "selected_native_benchmark": f"direct_selected_thread_cpu__{perf_mechanism}",
    "fallback_provider": "posix_thread_cpu_clock",
    "fallback_read_cost": "system call",
    "fallback_mechanism": "raw_entry",
    "fallback_native_benchmark": "direct_fallback_thread_cpu__raw_entry",
    "eligible_direct_candidates": [
      "direct_thread_cpu__libc_entry",
      "direct_thread_cpu__raw_entry",
      "direct_thread_cpu__linux_perf_mmap__aarch64_isb_cntvct_isb",
      "direct_thread_cpu__linux_perf_read__raw_read_svc",
    ],
    "failure_fallback": {
      "preferred_provider": "linux_perf_mmap",
      "eligibility_gate": (
        "perf task-clock mmap exposes complete seqlock conversion metadata and a "
        "usable architectural counter"
      ),
      "fallback_provider": "posix_thread_cpu_clock",
      "fallback_mechanism": "raw_entry",
      "trigger": (
        "perf event or mmap capability is unavailable, or an inline mmap read fails"
      ),
    },
  })
  path = selection["perf"]["path_probe"]
  path.pop("fallback_candidate")
  path.pop("capability_was_not_profitable")
  path.update({
    "selection_kind": "capability_preferred_with_performance_audit",
    "preferred_candidate": "linux_perf_mmap",
    "failure_fallback_candidate": "posix_thread_cpu",
    "selection_basis": (
      "complete perf mmap metadata and architectural-counter capability; audit "
      "samples do not select the provider"
    ),
  })
  mmap = selection["perf"]["mmap"]
  mmap.update({
    "selected_mechanism": perf_mechanism,
    "selected_candidate_benchmark": f"direct_thread_cpu__{perf_mechanism}",
    "eligible_benchmarks": [
      "direct_thread_cpu__linux_perf_mmap__aarch64_isb_cntvct_isb"
    ],
    "counter_probe": {
      "selection_kind": "tournament",
      "candidate_names": ["aarch64_isb_cntvct_isb", "aarch64_cntvctss_isb"],
      "candidate_eligible": [True, False],
      "candidate_batches_ns": [[80_000] * 9, [0] * 9],
      "selected_candidate": "aarch64_isb_cntvct_isb",
      "reads_per_batch": 4_096,
      "required_decisive_wins": 8,
      "equivalence_band": {"floor_ns_per_read": 1, "relative_denominator": 20},
    },
  })
  read = selection["perf"]["read"]
  read.update({
    "selected_mechanism": read_mechanism,
    "selected_candidate_benchmark": f"direct_thread_cpu__{read_mechanism}",
    "eligible_benchmarks": [f"direct_thread_cpu__{read_mechanism}"],
    "entry_probe": {
      "selection_kind": "fixed_candidate",
      "candidate_names": ["raw_read_svc"],
      "candidate_eligible": [True],
      "candidate_measured": [False],
      "candidate_batches_ns": None,
      "selected_candidate": "raw_read_svc",
      "reads_per_batch": 4_096,
      "required_decisive_wins": 8,
      "equivalence_band": {"floor_ns_per_read": 1, "relative_denominator": 20},
    },
  })
  selection["perf"]["decision_rule"] = (
    "prefer complete perf mmap capability; paired path samples are an audit"
  )
  return selection


def freebsd_thread_cpu_selection() -> dict:
  selection = adaptive_thread_cpu_selection()
  mechanism = selection["native_entry_probe"]["selected_provider"]
  selection.update({
    "selection_kind": "tournament_with_measured_runner_up",
    "selected_provider": "posix_thread_cpu_clock",
    "selected_read_cost": "system call",
    "selected_mechanism": mechanism,
    "selected_native_benchmark": f"direct_selected_thread_cpu__{mechanism}",
    "fallback_provider": None,
    "fallback_read_cost": None,
    "fallback_mechanism": None,
    "fallback_native_benchmark": None,
    "eligible_direct_candidates": [
      "direct_thread_cpu__libc_entry",
      "direct_thread_cpu__raw_entry",
    ],
  })
  selection["perf"] = {
    "event_available": False,
    "path_probe": None,
    "mmap": {
      "supported_on_target": False,
      "available": False,
      "read_cost": "inline",
      "selected_mechanism": None,
      "selected_candidate_benchmark": None,
      "eligible_benchmarks": [],
      "counter_probe": None,
    },
    "read": {
      "supported_on_target": False,
      "available": False,
      "read_cost": "system call",
      "selected_mechanism": None,
      "selected_candidate_benchmark": None,
      "eligible_benchmarks": [],
      "entry_probe": None,
    },
    "measurement_clock": (
      "raw SYS_clock_gettime(CLOCK_MONOTONIC_RAW), never libc/vDSO or the "
      "candidate under test"
    ),
    "decision_rule": "no perf provider exists on FreeBSD",
  }
  return selection


def windows_thread_cpu_selection() -> dict:
  mechanism = "get_thread_times_current_thread_pseudohandle"
  return {
    "selection_kind": "fixed_windows_thread_times",
    "selected_provider": "windows_thread_times",
    "selected_mechanism": mechanism,
    "selected_read_cost": "system call",
    "selected_native_benchmark": f"direct_selected_thread_cpu__{mechanism}",
    "fallback_provider": None,
    "fallback_mechanism": None,
    "fallback_read_cost": None,
    "fallback_native_benchmark": None,
    "eligible_direct_candidates": [f"direct_thread_cpu__{mechanism}"],
    "native_campaign_guard": {
      "required_provider": "windows_thread_times",
      "required_read_cost": "system call",
      "stale_selection_removed_before_guard": True,
      "on_mismatch": "panic before thread-cpu-selection.json is written",
    },
    "fixed_provider": {
      "candidate": mechanism,
      "supported_architectures": ["x86", "x86_64", "aarch64"],
      "selection_basis": (
        "GetThreadTimes is Windows' documented elapsed current-thread CPU timeline"
      ),
      "authority": speed_evidence.WINDOWS_GET_THREAD_TIMES_AUTHORITY,
    },
    "failure_fallback": {
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
    },
    "ineligible_direct_candidates": {
      "query_thread_cycle_time": {
        "eligibility": "ineligible",
        "reason": (
          "implementation-dependent cycles cannot be converted to elapsed thread CPU time"
        ),
        "authority": speed_evidence.WINDOWS_QUERY_THREAD_CYCLE_TIME_AUTHORITY,
      },
      "nt_query_information_thread": {
        "eligibility": "ineligible",
        "reason": (
          "the documented THREADINFOCLASS contract exposes no stable ThreadTimes class"
        ),
        "authority": speed_evidence.WINDOWS_NT_QUERY_INFORMATION_THREAD_AUTHORITY,
      },
    },
  }


def macos_fixed_native_thread_cpu_selection() -> dict:
  mechanism = "macos_clock_gettime_nsec_np_thread_cpu"
  return {
    "selection_kind": "fixed_native",
    "selected_provider": "posix_thread_cpu_clock",
    "selected_mechanism": mechanism,
    "selected_read_cost": "system call",
    "selected_native_benchmark": f"direct_selected_thread_cpu__{mechanism}",
    "fallback_provider": None,
    "fallback_mechanism": None,
    "fallback_read_cost": None,
    "fallback_native_benchmark": None,
    "eligible_direct_candidates": [f"direct_thread_cpu__{mechanism}"],
    "fixed_provider": {
      "candidate": mechanism,
      "supported_architectures": ["x86_64", "aarch64"],
      "native_primitive": "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)",
      "selection_basis": (
        "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID) is macOS's direct "
        "current-thread CPU-time entry"
      ),
      "time_domain": "thread CPU",
    },
    "read_cost_basis": (
      "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID) is a native system-call "
      "tier for scheduled CPU time on the current macOS thread"
    ),
  }


def residual_rows(selection: dict) -> dict:
  rows = {}
  for domain in ("instant", "ordered"):
    for benchmark in selection["eligible_direct_candidates"][domain]:
      identity = speed_evidence.exact_wall_candidate_identity(domain, benchmark)
      assert identity is not None
      rows[benchmark] = {**estimate(10.0), **identity}
  return rows


def loongarch_wall_selection() -> dict:
  clock = [100_000] * 9
  syscall = [90_000] * 9
  direct = [110_000] * 9
  domain = {
    "candidate_count": 3,
    "direct_eligible": True,
    "clock_raw_available": False,
    "syscall_raw_available": False,
    "vdso_available": False,
    "vdso_raw_available": False,
    "clock_boottime_available": False,
    "syscall_boottime_available": False,
    "vdso_boottime_available": False,
    "selected_provider": "linux_clock_monotonic_syscall",
    "reads_per_batch": 4_096,
    "required_decisive_wins": 8,
    "direct_batches_ns": direct,
    "clock_batches_ns": clock,
    "syscall_batches_ns": syscall,
    "fallback_allowance_ns": 5_000,
    "fallback_decisive_wins": 9,
    "direct_allowance_ns": 4_500,
    "direct_decisive_wins": 0,
  }
  return {
    "architecture": "loongarch64-linux",
    "selected_provider": {
      "instant": "linux_clock_monotonic_syscall",
      "ordered": "linux_clock_monotonic_syscall",
    },
    "selected_native_benchmark": {
      "instant": "direct_selected_wall__linux_clock_monotonic_syscall",
      "ordered": "direct_selected_ordered_wall__linux_clock_monotonic_syscall",
    },
    "eligible_direct_candidates": {
      "instant": [
        "direct_wall__loongarch_stable_counter",
        "direct_wall__linux_clock_monotonic",
        "direct_wall__linux_clock_monotonic_syscall",
      ],
      "ordered": [
        "direct_ordered_wall__loongarch_stable_counter",
        "direct_ordered_wall__linux_clock_monotonic",
        "direct_ordered_wall__linux_clock_monotonic_syscall",
      ],
    },
    "probe": {"instant": copy.deepcopy(domain), "ordered": copy.deepcopy(domain)},
  }


def freebsd_wall_selection() -> dict:
  instant = {
    "candidate_count": 2,
    "direct_eligible": False,
    "timekeep_available": False,
    "selected_provider": "freebsd_clock_monotonic",
    "reads_per_batch": 4_096,
    "required_decisive_wins": 8,
    "clock_batches_ns": [100_000] * 9,
    "syscall_batches_ns": [110_000] * 9,
    "fallback_allowance_ns": 5_000,
    "fallback_decisive_wins": 0,
  }
  ordered = {
    "candidate_count": 2,
    "direct_eligible": False,
    "timekeep_available": False,
    "selected_provider": "freebsd_clock_monotonic_x86_lfence",
    "reads_per_batch": 4_096,
    "required_decisive_wins": 8,
    "clock_batches_ns": [90_000] * 9,
    "syscall_batches_ns": [120_000] * 9,
    "fallback_allowance_ns": 4_500,
    "fallback_decisive_wins": 0,
  }
  return {
    "architecture": "x86_64-freebsd",
    "selected_provider": {
      "instant": "freebsd_clock_monotonic",
      "ordered": "freebsd_clock_monotonic_x86_lfence",
    },
    "selected_native_benchmark": {
      "instant": "direct_selected_wall__freebsd_clock_monotonic",
      "ordered": "direct_selected_ordered_wall__freebsd_clock_monotonic_x86_lfence",
    },
    "eligible_direct_candidates": {
      "instant": [
        "direct_wall__freebsd_clock_monotonic",
        "direct_wall__freebsd_clock_monotonic_syscall",
      ],
      "ordered": [
        "direct_ordered_wall__freebsd_clock_monotonic_x86_cpuid",
        "direct_ordered_wall__freebsd_clock_monotonic_x86_lfence",
        "direct_ordered_wall__freebsd_clock_monotonic_syscall_x86_cpuid",
      ],
    },
    "probe": {
      "instant": instant,
      "ordered": ordered,
      "ordered_os_barrier": "x86_lfence",
      "ordered_barrier_candidate_count": 2,
      "ordered_barrier_candidate_names": ["x86_cpuid", "x86_lfence"],
      "ordered_clock_barrier_candidate_batches_ns": [
        [100_000] * 9,
        [90_000] * 9,
      ],
      "ordered_clock_barrier_candidate_medians_ns": [100_000, 90_000],
      "ordered_clock_barrier_decision_count": 1,
      "ordered_clock_barrier_challengers": ["x86_lfence"],
      "ordered_clock_barrier_incumbents": ["x86_cpuid"],
      "ordered_clock_barrier_winners": ["x86_lfence"],
      "ordered_clock_barrier_allowances_ns": [5_000],
      "ordered_clock_barrier_tournament_decisive_wins": [9],
      "ordered_clock_barrier_challenger_selected": [True],
      "ordered_syscall_barrier_candidate_count": 1,
      "ordered_syscall_barrier_candidate_names": ["x86_cpuid"],
      "ordered_syscall_barrier_candidate_batches_ns": [[120_000] * 9],
      "ordered_syscall_barrier_candidate_medians_ns": [120_000],
      "ordered_syscall_barrier_decision_count": 0,
      "ordered_syscall_barrier_challengers": [],
      "ordered_syscall_barrier_incumbents": [],
      "ordered_syscall_barrier_winners": [],
      "ordered_syscall_barrier_allowances_ns": [],
      "ordered_syscall_barrier_tournament_decisive_wins": [],
      "ordered_syscall_barrier_challenger_selected": [],
    },
  }


def apple_aarch64_wall_selection() -> dict:
  candidate_names = [
    "apple_continuous_hw_acntvct_base",
    "apple_commpage_acntvct_offset",
    "apple_mach_absolute_time",
    "apple_mach_continuous_time",
  ]
  candidate_batches = [
    [90_000] * 9,
    [100_000] * 9,
    [110_000] * 9,
    [95_000] * 9,
  ]
  domain_probe = {
    "ready": True,
    "user_timebase_mode": 3,
    "continuous_hwclock": True,
    "reads_per_batch": 4_096,
    "candidate_count": len(candidate_names),
    "candidates": [
      {
        "provider": provider,
        "batches_ticks": batches,
        "median_ticks": int(statistics.median(batches)),
      }
      for provider, batches in zip(candidate_names, candidate_batches, strict=True)
    ],
    "required_decisive_wins": 8,
    "equivalence_floor_ticks_per_batch": 4_096,
    "equivalence_relative_denominator": 20,
    "measured_winner": "apple_continuous_hw_acntvct_base",
    "selected_provider": "apple_continuous_hw_acntvct_base",
    "selection_basis": "runtime_measured_complete_public_path",
  }
  selected = {
    "instant": "apple_continuous_hw_acntvct_base",
    "ordered": "apple_continuous_hw_acntvct_base",
  }
  public_exact_probe = {
    "selection_kind": "paired_public_exact_parity",
    "reads_per_batch": 65_536,
    "required_decisive_losses": 8,
    "equivalence_band": {
      "floor_ns_per_read": 1,
      "relative_denominator": 20,
    },
    "batch_order": "public-first on even batches; exact-first on odd batches",
    "call_boundary": "symmetric dynamic FnMut boundary",
    "measurement_clock": "std::time::Instant outside the measured read loop",
    "public_batches_ns": [650_000] * 9,
    "exact_batches_ns": [600_000] * 9,
  }
  return {
    "selected_provider": selected,
    "selected_native_benchmark": {
      "instant": "direct_selected_wall__apple_continuous_hw_acntvct_base",
      "ordered": "direct_selected_ordered_wall__apple_continuous_hw_acntvct_base",
    },
    "eligible_direct_candidates": {
      "instant": [f"direct_wall__{provider}" for provider in candidate_names],
      "ordered": [f"direct_ordered_wall__{provider}" for provider in candidate_names],
    },
    "probe": {
      "instant": copy.deepcopy(domain_probe),
      "ordered": copy.deepcopy(domain_probe),
    },
    "public_exact_probe": {
      "instant": copy.deepcopy(public_exact_probe),
      "ordered": copy.deepcopy(public_exact_probe),
    },
  }


def linux_x86_wall_selection() -> dict:
  parity = {
    "selection_kind": "paired_public_exact_parity",
    "reads_per_batch": 65_536,
    "required_decisive_losses": 8,
    "equivalence_band": {
      "floor_ns_per_read": 1,
      "relative_denominator": 20,
    },
    "batch_order": "public-first on even batches; exact-first on odd batches",
    "call_boundary": "symmetric dynamic FnMut boundary",
    "measurement_clock": "std::time::Instant outside the measured read loop",
    "public_batches_ns": [100_000] * 9,
    "exact_batches_ns": [90_000] * 9,
  }
  probe = {
    "reads_per_batch": 4_096,
    "required_decisive_wins": 8,
  }
  for domain, provider in (
    ("instant", "linux_kernel_eligible_tsc"),
    ("ordered", "linux_kernel_eligible_tsc_x86_lfence_rdtsc"),
  ):
    probe.update({
      f"{domain}_candidate_count": 1,
      f"{domain}_candidate_names": [provider],
      f"{domain}_candidate_eligible": [True],
      f"{domain}_candidate_batches_ns": [[90_000] * 9],
      f"{domain}_candidate_medians_ns": [90_000],
      f"{domain}_tournament_decision_count": 0,
      f"{domain}_tournament_challengers": [],
      f"{domain}_tournament_incumbents": [],
      f"{domain}_tournament_winners": [],
      f"{domain}_tournament_allowances_ns": [],
      f"{domain}_tournament_decisive_wins": [],
      f"{domain}_tournament_challenger_selected": [],
      f"{domain}_eligibility": "eligible",
    })
  return {
    "selected_provider": {
      "instant": "linux_kernel_eligible_tsc",
      "ordered": "linux_kernel_eligible_tsc_x86_lfence_rdtsc",
    },
    "selected_native_benchmark": {
      "instant": "direct_selected_wall__linux_kernel_eligible_tsc",
      "ordered": (
        "direct_selected_ordered_wall__"
        "linux_kernel_eligible_tsc_x86_lfence_rdtsc"
      ),
    },
    "eligible_direct_candidates": {
      "instant": ["direct_wall__linux_kernel_eligible_tsc"],
      "ordered": [
        "direct_ordered_wall__linux_kernel_eligible_tsc_x86_lfence_rdtsc"
      ],
    },
    "probe": probe,
    "public_exact_probe": {
      domain: {
        metric: copy.deepcopy(parity)
        for metric in speed_evidence.METRICS
      }
      for domain in ("instant", "ordered")
    },
  }


def apple_aarch64_complete_cell() -> tuple[dict, dict]:
  selection = apple_aarch64_wall_selection()
  values = clocks()
  values["tach"]["selection"] = selection
  for domain, prefix in (
    ("instant", "direct_wall__"),
    ("ordered", "direct_ordered_wall__"),
  ):
    for benchmark in selection["eligible_direct_candidates"][domain]:
      values[benchmark] = {
        **estimate(10.0),
        "provider": benchmark.removeprefix(prefix),
        "read_cost": "inline",
        "time_domain": f"{domain} wall",
        "benchmark": benchmark,
      }
  for domain, benchmark, public_key in (
    ("instant", "direct_selected_wall", "tach"),
    ("ordered", "direct_selected_ordered_wall", "tach_ordered"),
  ):
    selected_benchmark = selection["selected_native_benchmark"][domain]
    values[benchmark] = {
      **estimate(10.0),
      "provider": selection["selected_provider"][domain],
      "read_cost": "inline",
      "time_domain": f"{domain} wall",
      "benchmark": selected_benchmark,
    }
    assert values[public_key]["now"] == values[benchmark]["now"]
  return selection, values


def supplemental_speed_documents() -> dict[str, dict]:
  revision = "1" * 40
  documents = {}
  for artifact, (triple, harness, mode, build_mode) in speed_evidence.SUPPLEMENTAL_SPEED_CELLS.items():
    attestation = synthetic_runtime_attestation(
      triple,
      harness,
      revision,
      build_mode,
      runtime_smoke=mode == "runtime_smoke",
    )
    document = {
      "schema": speed_evidence.SUPPLEMENTAL_SPEED_SCHEMA,
      "triple": triple,
      "mode": mode,
      "build_mode": build_mode,
      "provenance": speed_evidence.runtime_identity_provenance(attestation),
    }
    if mode == "runtime_smoke":
      document.update({
        "evidence_class": "runtime_smoke",
        "passed": True,
        "smoke_schema": speed_evidence.RUNTIME_SMOKE_ATTESTATION_SCHEMA,
        "runtime_attestation": attestation,
        "assertions": ["provider constructed and returned monotonic values"],
      })
    else:
      values = clocks()
      values["tach"]["time_domain"] = "instant wall"
      values["tach_ordered"]["time_domain"] = "ordered wall"
      native_identity = speed_evidence.SUPPLEMENTAL_NATIVE_THREAD_CPU_IDENTITIES.get(
        (triple, harness, build_mode)
      )
      if native_identity is None:
        values["native_thread_cpu"]["benchmark"] = "native_thread_cpu__native_thread_clock"
      else:
        benchmark, provider, read_cost = native_identity
        values["native_thread_cpu"].update({
          "benchmark": benchmark,
          "provider": provider,
          "read_cost": read_cost,
          "time_domain": "thread CPU",
        })
      values["collector_attestation"] = {
        "schema": speed_evidence.COLLECTOR_ATTESTATION_SCHEMA,
        "invocation_id": attestation["invocation_id"],
        "runtime_attestation": copy.deepcopy(attestation),
        "manifest_sha256": "f" * 64,
      }
      values["direct_selected_wall"] = {
        **estimate(10.0),
        "provider": "exact_instant_wall",
        "read_cost": "inline",
        "time_domain": "instant wall",
        "benchmark": "direct_selected_wall__exact_instant_wall",
      }
      values["direct_wall__exact_instant_wall"] = {
        **values["direct_selected_wall"],
        "benchmark": "direct_wall__exact_instant_wall",
      }
      values["direct_selected_ordered_wall"] = {
        **estimate(10.0),
        "provider": "exact_ordered_wall",
        "read_cost": "inline",
        "time_domain": "ordered wall",
        "benchmark": "direct_selected_ordered_wall__exact_ordered_wall",
      }
      values["direct_ordered_wall__exact_ordered_wall"] = {
        **values["direct_selected_ordered_wall"],
        "benchmark": "direct_ordered_wall__exact_ordered_wall",
      }
      wall_selection = {
        "selection_kind": "runtime_tournament",
        "selected_provider": {
          "instant": "exact_instant_wall",
          "ordered": "exact_ordered_wall",
        },
        "selected_native_benchmark": {
          "instant": "direct_selected_wall__exact_instant_wall",
          "ordered": "direct_selected_ordered_wall__exact_ordered_wall",
        },
        "eligible_direct_candidates": {
          "instant": ["direct_wall__exact_instant_wall"],
          "ordered": ["direct_ordered_wall__exact_ordered_wall"],
        },
        "probe": {"selection_basis": "synthetic complete-path tournament"},
      }
      values["tach"]["selection"] = copy.deepcopy(wall_selection)
      values["tach_ordered"]["wall_selection"] = copy.deepcopy(wall_selection)
      if mode == "tagged_wall_fallback":
        values["tach_thread_cpu"] = {
          **estimate(10.0),
          "provider": "monotonic wall clock",
          "read_cost": "host call",
          "time_domain": "monotonic wall fallback",
        }
        values["native_thread_cpu"] = {
          **estimate(10.0),
          "provider": "native wall fallback",
          "read_cost": "host call",
          "time_domain": "monotonic wall fallback",
          "benchmark": "native_thread_cpu__monotonic_wall_fallback",
        }
        values["direct_selected_thread_cpu"] = {
          **estimate(10.0),
          "provider": "exact_thread_wall_fallback",
          "read_cost": "host call",
          "time_domain": "monotonic wall fallback",
          "benchmark": "direct_selected_thread_cpu__exact_thread_wall_fallback",
        }
        values["direct_thread_cpu__exact_thread_wall_fallback"] = {
          **estimate(10.0),
          "provider": "exact_thread_wall_fallback",
          "read_cost": "host call",
          "time_domain": "monotonic wall fallback",
          "benchmark": "direct_thread_cpu__exact_thread_wall_fallback",
        }
        thread_selection = {
          "selection_kind": "fallback_only",
          "selected_provider": "monotonic_wall_clock",
          "selected_mechanism": "exact_thread_wall_fallback",
          "selected_read_cost": "host call",
          "selected_native_benchmark": "direct_selected_thread_cpu__exact_thread_wall_fallback",
          "eligible_direct_candidates": ["direct_thread_cpu__exact_thread_wall_fallback"],
          "time_domain": "monotonic wall fallback",
        }
        phase_samples = {
          phase: [
            {
              "wall_delta_ns": 50_000_000,
              "public_delta_ns": 49_900_000,
              "direct_delta_ns": 49_900_000,
            }
            for _ in range(3)
          ]
          for phase in ("busy", "sleep", "sibling_isolation")
        }
        profiles = {
          "instant": "runtime_tournament",
          "ordered": "runtime_tournament",
          "thread_cpu": "fallback_only",
        }
      else:
        values["direct_thread_cpu__native_thread_clock"] = {
          **estimate(10.0),
          "provider": "native_thread_clock",
          "read_cost": "system call",
          "time_domain": "thread CPU",
          "benchmark": "direct_thread_cpu__native_thread_clock",
        }
        values["direct_selected_thread_cpu"] = {
          **estimate(10.0),
          "provider": "native_thread_clock",
          "read_cost": "system call",
          "time_domain": "thread CPU",
          "benchmark": "direct_selected_thread_cpu__native_thread_clock",
        }
        thread_selection = {
          "selection_kind": "runtime_tournament",
          "selected_provider": "posix_thread_cpu_clock",
          "selected_mechanism": "native_thread_clock",
          "selected_read_cost": "system call",
          "selected_native_benchmark": "direct_selected_thread_cpu__native_thread_clock",
          "eligible_direct_candidates": ["direct_thread_cpu__native_thread_clock"],
        }
        phase_samples = {
          "busy": [
            {
              "wall_delta_ns": 50_000_000,
              "public_delta_ns": 49_900_000,
              "direct_delta_ns": 49_900_000,
            }
            for _ in range(3)
          ],
          "sleep": [
            {
              "wall_delta_ns": 50_000_000,
              "public_delta_ns": 10_000,
              "direct_delta_ns": 10_000,
            }
            for _ in range(3)
          ],
          "sibling_isolation": [
            {
              "wall_delta_ns": 50_000_000,
              "public_delta_ns": 10_000,
              "direct_delta_ns": 10_000,
            }
            for _ in range(3)
          ],
        }
        profiles = {
          "instant": "runtime_tournament",
          "ordered": "runtime_tournament",
          "thread_cpu": "runtime_tournament",
        }
      values["tach_thread_cpu"]["selection"] = thread_selection
      behavior_sidecar = {
        "schema": speed_evidence.THREAD_CPU_BEHAVIOR_SCHEMA,
        "direct_benchmark": values["native_thread_cpu"]["benchmark"],
        "sample_count": 3,
        "runtime_attestation": copy.deepcopy(
          values["collector_attestation"]["runtime_attestation"]
        ),
      }
      for phase, samples in phase_samples.items():
        behavior_sidecar[phase] = {
          **speed_evidence._semantic_summary(samples),
          "samples": samples,
        }
      document.update({
        "evidence_class": "measured_external_runtime",
        "collector_bundle": {
          "schema": speed_evidence.COLLECTOR_BUNDLE_DESCRIPTOR_SCHEMA,
          "path": "synthetic.bundle",
          "manifest_sha256": "f" * 64,
        },
        "clocks": values,
        "selection_profiles": profiles,
        "route_coverage": speed_evidence.supplemental_route_coverage_from_clocks(
          values, profiles
        ),
        "thread_cpu_behavior": speed_evidence.build_thread_cpu_behavior(
          behavior_sidecar, values["tach_thread_cpu"]["time_domain"]
        ),
      })
    documents[artifact] = document
  return documents


def write_criterion_estimate(root: Path, group: str, benchmark: str) -> None:
  directory = root / group / benchmark / "new"
  directory.mkdir(parents=True)
  (directory / "estimates.json").write_text(json.dumps({
    "median": {
      "point_estimate": 10.0,
      "confidence_interval": {"lower_bound": 9.0, "upper_bound": 11.0},
    }
  }))


def git(root: Path, *args: str) -> str:
  return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


class SpeedEvidenceTests(unittest.TestCase):
  def test_linux_aarch64_boottime_candidates_reproduce_from_complete_samples(self) -> None:
    reads_per_batch = 4_096
    required_wins = 8

    def tournament_steps(names: list[str], samples: dict[str, list[int]]) -> list[dict]:
      incumbent = names[0]
      steps = []
      for challenger in names[1:]:
        decision = speed_evidence.reproduce_material_decision(
          samples[challenger],
          samples[incumbent],
          reads_per_batch,
          required_wins,
        )
        winner = challenger if decision["selected"] else incumbent
        steps.append({
          "challenger": challenger,
          "incumbent": incumbent,
          "winner": winner,
          "allowance_ns": decision["allowance_ns"],
          "decisive_wins": decision["decisive_wins"],
          "challenger_selected": decision["selected"],
        })
        incumbent = winner
      return steps

    specifications = {
      "instant": {
        "prefix": "direct_wall",
        "selected_prefix": "direct_selected_wall",
        "names": [
          "linux_clock_boottime",
          "linux_clock_boottime_vdso_direct",
          "linux_clock_boottime_syscall",
          "aarch64_cntvct",
        ],
        "fields": {
          "linux_clock_boottime": "clock_boottime_batches_ns",
          "linux_clock_boottime_vdso_direct": "vdso_boottime_batches_ns",
          "linux_clock_boottime_syscall": "syscall_boottime_batches_ns",
          "aarch64_cntvct": "cntvct_batches_ns",
        },
      },
      "ordered": {
        "prefix": "direct_ordered_wall",
        "selected_prefix": "direct_selected_ordered_wall",
        "names": [
          "linux_clock_boottime",
          "linux_clock_boottime_vdso_direct",
          "linux_clock_boottime_syscall",
          "aarch64_isb_cntvct",
        ],
        "fields": {
          "linux_clock_boottime": "clock_boottime_batches_ns",
          "linux_clock_boottime_vdso_direct": "vdso_boottime_batches_ns",
          "linux_clock_boottime_syscall": "syscall_boottime_batches_ns",
          "aarch64_isb_cntvct": "isb_batches_ns",
        },
      },
    }
    samples_by_cost = {
      "linux_clock_boottime": [100_000] * 9,
      "linux_clock_boottime_vdso_direct": [90_000] * 9,
      "linux_clock_boottime_syscall": [200_000] * 9,
      "aarch64_cntvct": [50_000] * 9,
      "aarch64_isb_cntvct": [50_000] * 9,
    }
    selection = {
      "permission_rule": "PR_GET_TSC permission evidence",
      "selected_provider": {},
      "selected_native_benchmark": {},
      "eligible_direct_candidates": {},
    }
    for domain, specification in specifications.items():
      names = specification["names"]
      selected = names[-1]
      samples = {name: samples_by_cost[name] for name in names}
      probe = {
        "eligibility": "eligible",
        "permission_basis": "pr_get_tsc_enabled",
        "pr_get_tsc_status": 0,
        "kernel_version_known": False,
        "kernel_version_major": 0,
        "kernel_version_minor": 0,
        "candidate_count": len(names),
        "reads_per_batch": reads_per_batch,
        "required_decisive_wins": required_wins,
        "selected_provider": selected,
        "tournament_step_count": len(names) - 1,
        "tournament_steps": tournament_steps(names, samples),
      }
      for name, field in specification["fields"].items():
        probe[field] = samples[name]
      selection[f"{domain}_probe"] = probe
      selection["selected_provider"][domain] = selected
      selection["selected_native_benchmark"][domain] = (
        f"{specification['selected_prefix']}__{selected}"
      )
      selection["eligible_direct_candidates"][domain] = [
        f"{specification['prefix']}__{name}" for name in names
      ]

    failures: list[str] = []
    result = speed_evidence.validate_linux_aarch64_wall_selector(
      "synthetic", selection, failures
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["instant"]["winner"], "aarch64_cntvct")
    self.assertEqual(result["ordered"]["winner"], "aarch64_isb_cntvct")

    for domain, field in (
      ("instant", "clock_boottime_batches_ns"),
      ("instant", "syscall_boottime_batches_ns"),
      ("instant", "vdso_boottime_batches_ns"),
      ("ordered", "clock_boottime_batches_ns"),
      ("ordered", "syscall_boottime_batches_ns"),
      ("ordered", "vdso_boottime_batches_ns"),
    ):
      with self.subTest(domain=domain, field=field):
        incomplete = copy.deepcopy(selection)
        del incomplete[f"{domain}_probe"][field]
        failures = []
        speed_evidence.validate_linux_aarch64_wall_selector(
          "incomplete", incomplete, failures
        )
        self.assertTrue(any("unknown candidate" in failure for failure in failures))

  def test_route_coverage_admission_reports_the_declared_gap_inventory(self) -> None:
    matrix = target_provider_module()
    advertised = {
      (target, mode)
      for target in matrix.TARGETS
      for mode in matrix.target_modes(target)
    }
    self.assertEqual(len(advertised), 49)

    failures = route_coverage_admission_failures()
    planned = {
      match.group(1)
      for failure in failures
      if (match := re.fullmatch(r"producer '([^']+)' is 'planned', not ready", failure))
    }
    self.assertEqual(
      planned,
      set(),
    )
    self.assertEqual(
      [failure for failure in failures if " is 'planned', not ready" not in failure],
      [],
    )

  def test_route_coverage_validator_rejects_missing_or_unready_contracts(self) -> None:
    ready = copy.deepcopy(load_route_coverage_manifest())
    for producer in ready["producer"]:
      producer["state"] = "ready"
    self.assertEqual(validate_route_coverage_manifest(ready, ROOT), [])

    missing = copy.deepcopy(ready)
    missing["case"] = [
      case for case in missing["case"] if case["id"] != "wasm32v1_none_smoke"
    ]
    self.assertTrue(
      any(
        "misses advertised target/mode identities" in failure
        for failure in validate_route_coverage_manifest(missing, ROOT)
      )
    )

    unknown = copy.deepcopy(ready)
    unknown["case"][0]["targets"] = ["made-up-unknown-target"]
    self.assertTrue(
      any(
        "declares unknown target" in failure
        for failure in validate_route_coverage_manifest(unknown, ROOT)
      )
    )

    duplicate = copy.deepcopy(ready)
    duplicate["case"].append(copy.deepcopy(duplicate["case"][0]))
    self.assertTrue(
      any(
        "duplicates runtime identity" in failure
        for failure in validate_route_coverage_manifest(duplicate, ROOT)
      )
    )

    nonexistent = copy.deepcopy(ready)
    nonexistent["producer"][0]["entrypoints"].append("benches/not-a-real-producer")
    self.assertTrue(
      any(
        "entrypoint does not exist" in failure
        for failure in validate_route_coverage_manifest(nonexistent, ROOT)
      )
    )

    incomplete = copy.deepcopy(ready)
    incomplete["route_contracts"]["three_timer_direct"]["thread_cpu"]["operations"] = [
      "now"
    ]
    incomplete["case"][0]["selection_profiles"]["thread_cpu"] = "untyped"
    incomplete["case"][0]["runtime_proof"] = "external_runtime"
    macos_case = next(case for case in incomplete["case"] if case["id"] == "macos_fixed_native")
    macos_case["thread_cpu_selector"]["perf"] = {}
    failures = validate_route_coverage_manifest(incomplete, ROOT)
    self.assertTrue(any("now and elapsed" in failure for failure in failures))
    self.assertTrue(any("unknown selection profile" in failure for failure in failures))
    self.assertTrue(any("falsely labels static declaration" in failure for failure in failures))
    self.assertTrue(any("fabricates a fixed-native perf" in failure for failure in failures))

    missing_macos_metadata = copy.deepcopy(ready)
    macos_case = next(
      case for case in missing_macos_metadata["case"] if case["id"] == "macos_fixed_native"
    )
    macos_case.pop("thread_cpu_selector")
    self.assertTrue(
      any(
        "lacks fixed-native thread-CPU metadata" in failure
        for failure in validate_route_coverage_manifest(missing_macos_metadata, ROOT)
      )
    )

  def test_target_proof_keeps_runtime_and_codegen_evidence_classes_distinct(self) -> None:
    path = Path(__file__).with_name("verify-target-providers.py")
    spec = importlib.util.spec_from_file_location("verify_target_providers", path)
    self.assertIsNotNone(spec)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    self.assertEqual(
      module.runtime_evidence_class("x86_64-unknown-linux-gnu"),
      "measured_external_runtime",
    )
    self.assertEqual(
      module.runtime_evidence_class("s390x-unknown-linux-gnu"),
      "runtime_self_selecting_codegen_proven",
    )
    self.assertEqual(
      module.runtime_evidence_class("i686-pc-windows-msvc"),
      "unique_provider_source_codegen_proven",
    )
    self.assertEqual(
      module.runtime_evidence_class("wasm32-wasip2"),
      "fallback_only_source_codegen_proven",
    )
    self.assertIn(
      "i686-pc-windows-msvc", module.RELEASE_REQUIRED_HOSTED_SPEED_TARGETS
    )
    self.assertIn(
      "i686-unknown-linux-gnu", module.RELEASE_REQUIRED_HOSTED_SPEED_TARGETS
    )

  def test_target_proof_requires_distinct_now_and_elapsed_phase_roots(self) -> None:
    path = Path(__file__).with_name("verify-target-providers.py")
    spec = importlib.util.spec_from_file_location("verify_target_providers", path)
    self.assertIsNotNone(spec)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)

    routes = module.route_specs("x86_64-unknown-linux-gnu", "default")
    self.assertEqual(
      set(routes),
      {
        "instant_now",
        "instant_now_elapsed",
        "ordered_instant_now",
        "ordered_instant_now_elapsed",
        "thread_cpu_instant_now",
        "thread_cpu_instant_now_elapsed",
      },
    )
    self.assertNotIn("ordered_instant_now_elapsed_unordered", routes)
    self.assertIn("weaker end-read ordering", module.ORDERED_UNORDERED_ELAPSED_EXCLUSION)

    complete = """
define void @tach_probe_instant_now() {
  ret void
}
define void @tach_probe_instant_elapsed() {
  ret void
}
define void @tach_probe_instant_now_elapsed() {
  call void @tach_probe_instant_now()
  call void @tach_probe_instant_elapsed()
  ret void
}
"""
    phases = module.require_composed_phase_roots(
      complete,
      "instant_now_elapsed",
      "tach_probe_instant_now_elapsed",
      {
        "now": "tach_probe_instant_now",
        "elapsed": "tach_probe_instant_elapsed",
      },
    )
    self.assertEqual(
      phases,
      {
        "now": "tach_probe_instant_now",
        "elapsed": "tach_probe_instant_elapsed",
      },
    )

    false_green = complete.replace("  call void @tach_probe_instant_elapsed()\n", "")
    with self.assertRaisesRegex(RuntimeError, r"missing direct phase roots \['elapsed'\]"):
      module.require_composed_phase_roots(
        false_green,
        "instant_now_elapsed",
        "tach_probe_instant_now_elapsed",
        {
          "now": "tach_probe_instant_now",
          "elapsed": "tach_probe_instant_elapsed",
        },
      )

  def test_target_proof_follows_outlined_implementation_routes(self) -> None:
    module = target_provider_module()
    probe = """
define void @tach_probe() {
  call void @_RNvCsPROBE_4tach22tach_outlined_provider()
  ret void
}
declare void @_RNvCsPROBE_4tach22tach_outlined_provider()
"""
    implementation = """
define void @_RNvCsIMPLEMENTATION_4tach22tach_outlined_provider() {
  call void @clock_gettime()
  ret void
}
declare void @clock_gettime()
"""

    self.assertNotIn("call void @clock_gettime()", module.reachable_ir(probe, "tach_probe"))
    self.assertIn(
      "call void @clock_gettime()",
      module.reachable_ir(
        module.normalize_tach_ir_symbols("\n".join((probe, implementation))),
        "tach_probe",
      ),
    )
    specialized_probe = """
define void @tach_probe_specialized() {
  call void @tach_shared_route()
  ret void
}
define void @tach_shared_route() {
  call void @probe_specialization()
  ret void
}
declare void @probe_specialization()
"""
    generic_implementation = """
define void @tach_shared_route() {
  call void @generic_implementation()
  ret void
}
declare void @generic_implementation()
"""
    specialized_closure = module.reachable_ir(
      "\n".join((generic_implementation, specialized_probe)),
      "tach_probe_specialized",
    )
    self.assertIn("@probe_specialization()", specialized_closure)
    self.assertNotIn("@generic_implementation()", specialized_closure)
    module.validate_route_patterns(
      "test-target",
      "default",
      "test-route",
      {
        "required_patterns": ["@generic_implementation"],
        "forbidden_patterns": ["@generic_implementation"],
      },
      generic_implementation,
      specialized_probe,
    )
    with self.assertRaisesRegex(RuntimeError, "unexpected"):
      module.validate_route_patterns(
        "test-target",
        "default",
        "test-route",
        {
          "required_patterns": ["@generic_implementation"],
          "forbidden_patterns": ["@probe_specialization"],
        },
        generic_implementation,
        specialized_probe,
      )
    inlined_vdso_call = """
%function = inttoptr i64 %address to ptr
%status = call noundef i32 %function(i32 noundef %clock_id, ptr noundef %value)
"""
    self.assertRegex(
      inlined_vdso_call,
      module.direct_vdso_hot_patterns("x86_64-unknown-linux-gnu")[-1],
    )
    self.assertRegex(
      inlined_vdso_call.replace("i64", "i32"),
      module.direct_vdso_hot_patterns("i686-unknown-linux-gnu")[-1],
    )
    windows_ordered_pattern = module.ordered_instant_route(
      "x86_64-pc-windows-msvc", "default"
    )["required_patterns"][0]
    self.assertRegex("qpc_ticks_ordered_after_selection", windows_ordered_pattern)
    self.assertRegex("windows_ticks_ordered_after_selection", windows_ordered_pattern)

  def test_emscripten_target_proof_uses_guarded_host_imports(self) -> None:
    path = Path(__file__).with_name("verify-target-providers.py")
    spec = importlib.util.spec_from_file_location("verify_target_providers", path)
    self.assertIsNotNone(spec)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)

    local_imports = list(module.EMSCRIPTEN_LOCAL_WALL_IMPORTS)
    self.assertEqual(
      module.instant_route("wasm32-unknown-emscripten")["required_patterns"],
      local_imports,
    )
    self.assertEqual(
      module.ordered_instant_route(
        "wasm32-unknown-emscripten", "default"
      )["required_patterns"],
      local_imports,
    )
    pthread_ordered = module.ordered_instant_route(
      "wasm32-unknown-emscripten", "emscripten-pthreads"
    )
    self.assertTrue(
      set(module.EMSCRIPTEN_ORDERED_PTHREAD_IMPORTS)
      <= set(pthread_ordered["required_patterns"])
    )
    self.assertEqual(
      module.thread_cpu_route(
        "wasm32-unknown-emscripten", "default"
      )["required_patterns"],
      [module.EMSCRIPTEN_NODE_THREAD_CPU_IMPORT, *local_imports],
    )

    source = (Path(__file__).resolve().parents[1] / "src/arch/emscripten.rs").read_text()
    for import_name in (
      *module.EMSCRIPTEN_LOCAL_WALL_IMPORTS,
      *module.EMSCRIPTEN_ORDERED_PTHREAD_IMPORTS,
      module.EMSCRIPTEN_NODE_THREAD_CPU_IMPORT,
    ):
      self.assertIn(f"fn {import_name}", source)
    self.assertIn("globalThis.performance", source)
    self.assertIn("h.bigint", source)
    self.assertIn("_emscripten_get_now", source)

  def test_supplemental_runtime_schema_never_accepts_codegen_as_speed(self) -> None:
    documents = supplemental_speed_documents()
    report = speed_evidence.validate_supplemental_speed_campaign(
      documents, require_bound_observations=False
    )
    self.assertTrue(report["passed"], report["failures"])

    missing = copy.deepcopy(documents)
    missing.pop("speed-supplemental-freebsd-x86_64.json")
    report = speed_evidence.validate_supplemental_speed_campaign(
      missing, require_bound_observations=False
    )
    self.assertFalse(report["passed"])
    self.assertTrue(any("missing=" in failure for failure in report["failures"]))

    codegen_only = copy.deepcopy(documents)
    codegen_only["speed-supplemental-macos-x86_64.json"]["evidence_class"] = (
      "unique_provider_source_codegen_proven"
    )
    report = speed_evidence.validate_supplemental_speed_campaign(
      codegen_only, require_bound_observations=False
    )
    self.assertFalse(report["passed"])
    self.assertTrue(
      any("malformed measured three-clock" in failure for failure in report["failures"])
    )

  def test_supplemental_release_validation_requires_retained_observations(self) -> None:
    report = speed_evidence.validate_supplemental_speed_campaign(
      supplemental_speed_documents()
    )
    self.assertFalse(report["passed"])
    self.assertTrue(any(
      "missing retained collector bundle path" in failure
      for failure in report["failures"]
    ))
    self.assertFalse(any(
      "wasip1-threads-smoke" in failure and "bundle" in failure
      for failure in report["failures"]
    ))

  def test_supplemental_cell_reextracts_its_retained_bundle(self) -> None:
    artifact = "speed-supplemental-macos-x86_64.json"
    document = supplemental_speed_documents()[artifact]
    raw_behavior = {
      key: copy.deepcopy(document["thread_cpu_behavior"][key])
      for key in (
        "schema",
        "direct_benchmark",
        "sample_count",
        "runtime_attestation",
        *speed_evidence.THREAD_CPU_BEHAVIOR_PHASES,
      )
    }
    observed_clocks = copy.deepcopy(document["clocks"])
    collector = observed_clocks.pop("collector_attestation")
    observation = {
      "clocks": observed_clocks,
      "thread_cpu_behavior": raw_behavior,
      "collector_attestation": collector,
    }
    with mock.patch.object(
      extract_speed,
      "extract_collector_bundle_observation",
      return_value=observation,
    ):
      report = speed_evidence.validate_supplemental_speed_cell_from_bundle(
        artifact, document, Path("/retained/collector.bundle")
      )
      self.assertTrue(report["passed"], report["failures"])
      self.assertTrue(report["bundle_binding"]["passed"])

      changed_clock = copy.deepcopy(document)
      changed_clock["clocks"]["tach"]["now"] = 999.0
      report = speed_evidence.validate_supplemental_speed_cell_from_bundle(
        artifact, changed_clock, Path("/retained/collector.bundle")
      )
      self.assertFalse(report["passed"])
      self.assertTrue(any(
        "serialized clocks differ from retained collector bundle" in failure
        for failure in report["failures"]
      ))

      changed_digest = copy.deepcopy(document)
      changed_digest["collector_bundle"]["manifest_sha256"] = "0" * 64
      report = speed_evidence.validate_supplemental_speed_cell_from_bundle(
        artifact, changed_digest, Path("/retained/collector.bundle")
      )
      self.assertFalse(report["passed"])
      self.assertTrue(any(
        "retained collector bundle digest changed" in failure
        for failure in report["failures"]
      ))

  def test_supplemental_composer_accepts_criterion_collector_cell(self) -> None:
    fixture = supplemental_speed_documents()["speed-supplemental-macos-x86_64.json"]
    raw_behavior = {
      key: copy.deepcopy(fixture["thread_cpu_behavior"][key])
      for key in (
        "schema",
        "direct_benchmark",
        "sample_count",
        "runtime_attestation",
        "busy",
        "sleep",
        "sibling_isolation",
      )
    }
    composer = supplemental_composer_module()
    document = composer.compose(
      "speed-supplemental-macos-x86_64.json",
      copy.deepcopy(fixture["clocks"]),
      raw_behavior,
      "1" * 40,
      copy.deepcopy(fixture["selection_profiles"]),
      {"runtime": "synthetic"},
      [],
    )
    report = speed_evidence.validate_supplemental_speed_cell(
      "speed-supplemental-macos-x86_64.json", document
    )
    self.assertTrue(report["passed"], report["failures"])
    self.assertEqual(
      document["thread_cpu_behavior"]["direct_benchmark"],
      document["clocks"]["native_thread_cpu"]["benchmark"],
    )
    self.assertEqual(document["provenance"]["runtime"], "synthetic")

  def test_primary_composer_derives_identity_from_a_collector_bundle(self) -> None:
    artifact_id = "speed-0-apple.json"
    values, collector = primary_speed_observation(artifact_id)
    observation = {
      "clocks": copy.deepcopy(values),
      "collector_attestation": copy.deepcopy(collector),
    }
    composer = primary_composer_module()
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      bundle = root / "collector.bundle"
      bundle.mkdir()
      output = root / artifact_id
      with mock.patch.object(
        sys,
        "argv",
        [
          "compose-speed.py",
          str(output),
          "--collector-bundle",
          str(bundle),
        ],
      ), mock.patch.object(
        composer.extract_speed,
        "extract_collector_bundle_observation",
        return_value=observation,
      ) as extract:
        composer.main()

      extract.assert_called_once_with(bundle)
      document = json.loads(output.read_text())

    self.assertEqual(document["artifact_id"], artifact_id)
    self.assertEqual(document["build_mode"], "default")
    self.assertEqual(document["evidence_kind"], "full_speed")
    self.assertEqual(document["collector_bundle"]["path"], "collector.bundle")
    self.assertEqual(
      document["provenance"],
      speed_evidence.runtime_identity_provenance(collector["runtime_attestation"]),
    )
    self.assertNotIn("rustc", document["provenance"])
    report = speed_evidence.validate_primary_speed_cell(artifact_id, document)
    self.assertTrue(report["passed"], report["failures"])

  def test_primary_composer_accepts_retained_lambda_observation(self) -> None:
    composer = primary_composer_module()
    artifact_id = "speed-5-lambda.json"
    values = clocks()
    for clock, row in values.items():
      for metric in speed_evidence.METRICS:
        samples = [row[metric]] * speed_evidence.LAMBDA_SAMPLE_COUNT
        point, interval = speed_evidence.lambda_median_with_ci(
          samples,
          f"{clock}:{metric}",
        )
        row[metric] = point
        row[f"{metric}_ci95"] = interval
        row[f"{metric}_samples"] = samples
    attestation = synthetic_runtime_attestation(
      "x86_64-unknown-linux-gnu",
      "lambda",
      "1" * 40,
      "default",
    )
    collector = {
      "schema": speed_evidence.COLLECTOR_ATTESTATION_SCHEMA,
      "invocation_id": attestation["invocation_id"],
      "runtime_attestation": attestation,
      "manifest_sha256": "f" * 64,
    }
    observation = {
      "clocks": values,
      "collector_attestation": collector,
    }
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      bundle = root / "speed-5-lambda.collector.bundle"
      bundle.mkdir()
      output = root / artifact_id
      with (
        mock.patch.object(
          sys,
          "argv",
          [
            "compose-speed.py",
            str(output),
            "--collector-bundle",
            str(bundle),
          ],
        ),
        mock.patch.object(
          composer.extract_speed,
          "extract_collector_bundle_observation",
          return_value=observation,
        ) as extract,
      ):
        composer.main()

      document = json.loads(output.read_text())
    extract.assert_called_once_with(bundle)
    self.assertEqual(document["provenance"]["harness"], "lambda")
    self.assertEqual(document["build_mode"], "default")
    report = speed_evidence.validate_primary_speed_cell(artifact_id, document)
    self.assertTrue(report["passed"], report["failures"])

  def test_primary_bound_cell_reextracts_the_retained_collector_bundle(self) -> None:
    artifact_id = "speed-0-apple.json"
    document = primary_speed_document(artifact_id)
    values, collector = primary_speed_observation(artifact_id)
    observation = {
      "clocks": copy.deepcopy(values),
      "collector_attestation": copy.deepcopy(collector),
    }
    with tempfile.TemporaryDirectory() as directory, mock.patch.object(
      extract_speed,
      "extract_collector_bundle_observation",
      return_value=observation,
    ):
      bundle = Path(directory) / "collector.bundle"
      bundle.mkdir()
      report = speed_evidence.validate_primary_speed_cell_from_bundle(
        artifact_id, document, bundle
      )
      self.assertTrue(report["passed"], report["failures"])
      self.assertTrue(report["bound_observation"])
      self.assertEqual(report["artifact_id"], artifact_id)
      self.assertEqual(report["source_revision"], "1" * 40)
      self.assertEqual(report["triple"], "aarch64-apple-darwin")
      self.assertEqual(report["build_mode"], "default")
      self.assertEqual(report["evidence_kind"], "full_speed")

      changed = copy.deepcopy(document)
      changed["clocks"]["tach"]["now"] = 9.0
      report = speed_evidence.validate_primary_speed_cell_from_bundle(
        artifact_id, changed, bundle
      )
      self.assertFalse(report["passed"])
      self.assertFalse(report["bound_observation"])
      self.assertTrue(any(
        "serialized clocks differ from retained collector bundle" in failure
        for failure in report["failures"]
      ))

  def test_primary_campaign_rejects_missing_or_legacy_bundle_evidence(self) -> None:
    artifact_id = "speed-0-apple.json"
    document = primary_speed_document(artifact_id)
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      cell_path = root / artifact_id
      cell_path.write_text(json.dumps(document))
      report = speed_evidence.validate_primary_speed_campaign(
        {artifact_id: document}, {artifact_id: cell_path}
      )
    self.assertFalse(report["passed"])
    self.assertTrue(any(
      "retained collector bundle is missing" in failure
      for failure in report["failures"]
    ))

    legacy = json.loads((ROOT / "benches" / artifact_id).read_text())
    legacy_report = speed_evidence.validate_primary_speed_cell(artifact_id, legacy)
    self.assertFalse(legacy_report["passed"])
    self.assertTrue(any(
      "document schema changed" in failure for failure in legacy_report["failures"]
    ))

  def test_primary_validator_rejects_provenance_and_build_mode_drift(self) -> None:
    artifact_id = "speed-0-apple.json"
    document = primary_speed_document(artifact_id)
    runner_drift = copy.deepcopy(document)
    runner_drift["provenance"]["runner"] = "wrong-runner"
    report = speed_evidence.validate_primary_speed_cell(artifact_id, runner_drift)
    self.assertFalse(report["passed"])
    self.assertTrue(any(
      "document provenance disagrees with runtime identity" in failure
      for failure in report["failures"]
    ))

    build_drift = copy.deepcopy(document)
    runtime = build_drift["collector_attestation"]["runtime_attestation"]
    runtime["build_mode"] = "no-default"
    runtime["features"] = ["bench-internal"]
    build_drift["build_mode"] = "no-default"
    build_drift["provenance"]["build_mode"] = "no-default"
    build_drift["provenance"]["features"] = ["bench-internal"]
    report = speed_evidence.validate_primary_speed_cell(artifact_id, build_drift)
    self.assertFalse(report["passed"])
    self.assertTrue(any(
      "build mode does not match the primary campaign" in failure
      for failure in report["failures"]
    ))

  def test_supplemental_composer_accepts_host_collector_bundle(self) -> None:
    artifact = "speed-supplemental-lambda-aarch64.json"
    fixture = supplemental_speed_documents()[artifact]
    raw_behavior = {
      key: copy.deepcopy(fixture["thread_cpu_behavior"][key])
      for key in (
        "schema",
        "direct_benchmark",
        "sample_count",
        "runtime_attestation",
        "busy",
        "sleep",
        "sibling_isolation",
      )
    }
    composer = supplemental_composer_module()
    composed = composer.compose(
      artifact,
      copy.deepcopy(fixture["clocks"]),
      raw_behavior,
      "1" * 40,
      copy.deepcopy(fixture["selection_profiles"]),
      {},
      None,
    )
    self.assertEqual(composed["provenance"]["harness"], "lambda")
    self.assertEqual(composed["triple"], "aarch64-unknown-linux-gnu")

    with tempfile.TemporaryDirectory() as temporary_directory:
      root = Path(temporary_directory)
      output = root / artifact
      bundle = root / "collector.bundle"
      bundle.mkdir()
      extracted_clocks = copy.deepcopy(fixture["clocks"])
      collector = extracted_clocks.pop("collector_attestation")
      observation = {
        "clocks": extracted_clocks,
        "collector_attestation": collector,
        "thread_cpu_behavior": raw_behavior,
      }
      with (
        mock.patch.object(
          sys,
          "argv",
          [
            "compose-supplemental-speed.py",
            "--artifact",
            artifact,
            "--output",
            str(output),
            "--source-revision",
            "1" * 40,
            "--collector-bundle",
            str(bundle),
            "--instant-profile",
            "runtime_tournament",
            "--ordered-profile",
            "runtime_tournament",
            "--thread-cpu-profile",
            "runtime_tournament",
          ],
        ),
        mock.patch.object(
          composer.extract_speed,
          "extract_collector_bundle_observation",
          return_value=observation,
        ) as extract,
      ):
        composer.main()

      document = json.loads(output.read_text())
    extract.assert_called_once_with(bundle)
    report = speed_evidence.validate_supplemental_speed_cell(artifact, document)
    self.assertTrue(report["passed"], report["failures"])

  def test_supplemental_composer_accepts_tagged_fallback_collector_bundle(self) -> None:
    artifact = "speed-supplemental-wasi-p1-wasmtime.json"
    fixture = supplemental_speed_documents()[artifact]
    raw_behavior = {
      key: copy.deepcopy(fixture["thread_cpu_behavior"][key])
      for key in (
        "schema",
        "direct_benchmark",
        "sample_count",
        "runtime_attestation",
        "busy",
        "sleep",
        "sibling_isolation",
      )
    }
    composer = supplemental_composer_module()
    with tempfile.TemporaryDirectory() as temporary_directory:
      root = Path(temporary_directory)
      output = root / artifact
      bundle = root / "collector.bundle"
      bundle.mkdir()
      extracted_clocks = copy.deepcopy(fixture["clocks"])
      collector = extracted_clocks.pop("collector_attestation")
      observation = {
        "clocks": extracted_clocks,
        "collector_attestation": collector,
        "thread_cpu_behavior": raw_behavior,
      }
      with (
        mock.patch.object(
          sys,
          "argv",
          [
            "compose-supplemental-speed.py",
            "--artifact",
            artifact,
            "--output",
            str(output),
            "--source-revision",
            "1" * 40,
            "--collector-bundle",
            str(bundle),
            "--instant-profile",
            "runtime_tournament",
            "--ordered-profile",
            "runtime_tournament",
            "--thread-cpu-profile",
            "fallback_only",
          ],
        ),
        mock.patch.object(
          composer.extract_speed,
          "extract_collector_bundle_observation",
          return_value=observation,
        ) as extract,
      ):
        composer.main()

      document = json.loads(output.read_text())
    extract.assert_called_once_with(bundle)
    report = speed_evidence.validate_supplemental_speed_cell(artifact, document)
    self.assertTrue(report["passed"], report["failures"])

  def test_supplemental_composer_rejects_mislabeled_rows_and_semantic_summaries(self) -> None:
    fixture = supplemental_speed_documents()["speed-supplemental-macos-x86_64.json"]
    raw_behavior = {
      key: copy.deepcopy(fixture["thread_cpu_behavior"][key])
      for key in (
        "schema",
        "direct_benchmark",
        "sample_count",
        "runtime_attestation",
        "busy",
        "sleep",
        "sibling_isolation",
      )
    }
    composer = supplemental_composer_module()
    mislabeled = copy.deepcopy(fixture["clocks"])
    mislabeled["direct_selected_wall"]["benchmark"] = "direct_selected_wall__wrong"
    with self.assertRaisesRegex(ValueError, "selected row does not match"):
      composer.compose(
        "speed-supplemental-macos-x86_64.json",
        mislabeled,
        copy.deepcopy(raw_behavior),
        "1" * 40,
        copy.deepcopy(fixture["selection_profiles"]),
        {},
        [],
      )

    mislabeled_profile = copy.deepcopy(fixture["selection_profiles"])
    mislabeled_profile["thread_cpu"] = "fixed_native"
    with self.assertRaisesRegex(ValueError, "profiles do not match observed selectors"):
      composer.compose(
        "speed-supplemental-macos-x86_64.json",
        copy.deepcopy(fixture["clocks"]),
        copy.deepcopy(raw_behavior),
        "1" * 40,
        mislabeled_profile,
        {},
        [],
      )

    summary_tampered = copy.deepcopy(raw_behavior)
    summary_tampered["sleep"]["public_delta_ns"] += 1
    with self.assertRaisesRegex(ValueError, "summary does not reproduce"):
      composer.compose(
        "speed-supplemental-macos-x86_64.json",
        copy.deepcopy(fixture["clocks"]),
        summary_tampered,
        "1" * 40,
        copy.deepcopy(fixture["selection_profiles"]),
        {},
        [],
      )

  def test_supplemental_validator_rejects_selector_or_direct_binding_drift(self) -> None:
    document = supplemental_speed_documents()["speed-supplemental-macos-x86_64.json"]
    selector_drift = copy.deepcopy(document)
    selector_drift["route_coverage"]["thread_cpu"]["eligible_exact_rows"] = []
    report = speed_evidence.validate_supplemental_speed_cell(
      "speed-supplemental-macos-x86_64.json", selector_drift
    )
    self.assertFalse(report["passed"])
    self.assertTrue(any("malformed thread_cpu route identity" in item for item in report["failures"]))

    direct_drift = copy.deepcopy(document)
    direct_drift["thread_cpu_behavior"]["direct_benchmark"] = "native_thread_cpu__other"
    report = speed_evidence.validate_supplemental_speed_cell(
      "speed-supplemental-macos-x86_64.json", direct_drift
    )
    self.assertFalse(report["passed"])
    self.assertTrue(any("not bound to native_thread_cpu" in item for item in report["failures"]))

  def test_supplemental_route_comparison_decomposes_public_parity_and_selection(self) -> None:
    artifact = "speed-supplemental-macos-x86_64.json"
    document = supplemental_speed_documents()[artifact]
    clocks = document["clocks"]
    slower = "direct_thread_cpu__slower_thread_clock"
    clocks[slower] = {
      **estimate(20.0),
      "provider": "slower_thread_clock",
      "read_cost": "system call",
      "time_domain": "thread CPU",
      "benchmark": slower,
    }
    clocks["tach_thread_cpu"]["selection"]["eligible_direct_candidates"].append(slower)
    document["route_coverage"] = speed_evidence.supplemental_route_coverage_from_clocks(
      clocks, document["selection_profiles"]
    )

    report = speed_evidence.validate_supplemental_speed_cell(artifact, document)

    self.assertTrue(report["passed"], report["failures"])
    routes = report["route_coverage"]["thread_cpu"]["eligible_exact_routes"]
    selected = routes["direct_thread_cpu__native_thread_clock"]["metrics"]["now"]
    self.assertEqual(selected["comparison_basis"], "public versus its selected exact route")
    nonselected = routes[slower]["metrics"]["now"]
    self.assertEqual(
      nonselected["comparison_basis"],
      "selected exact route versus another eligible route",
    )
    self.assertEqual(nonselected["comparison_ns"], 10.0)

  def test_supplemental_validator_rejects_cross_target_or_cross_run_inputs(self) -> None:
    macos = supplemental_speed_documents()["speed-supplemental-macos-x86_64.json"]
    relabeled = copy.deepcopy(macos)
    relabeled["triple"] = "i686-pc-windows-msvc"
    report = speed_evidence.validate_supplemental_speed_cell(
      "speed-supplemental-windows-i686.json", relabeled
    )
    self.assertFalse(report["passed"])
    self.assertTrue(any("attestation target disagrees" in item for item in report["failures"]))

    cross_run = copy.deepcopy(macos)
    cross_run["thread_cpu_behavior"]["runtime_attestation"]["invocation_id"] = "other-run"
    report = speed_evidence.validate_supplemental_speed_cell(
      "speed-supplemental-macos-x86_64.json", cross_run
    )
    self.assertFalse(report["passed"])
    self.assertTrue(any("another runtime invocation" in item for item in report["failures"]))

  def test_supplemental_validator_binds_runtime_provenance_and_features(self) -> None:
    artifact = "speed-supplemental-macos-x86_64.json"
    document = supplemental_speed_documents()[artifact]

    runner_drift = copy.deepcopy(document)
    runner_drift["provenance"]["runner"] = "wrong-runner"
    report = speed_evidence.validate_supplemental_speed_cell(artifact, runner_drift)
    self.assertFalse(report["passed"])
    self.assertTrue(any(
      "document provenance disagrees with runtime identity" in failure
      for failure in report["failures"]
    ))

    for features in (
      ["bench-internal"],
      ["bench-internal", "recalibrate-background", "thread-cpu-inline"],
    ):
      with self.subTest(features=features):
        feature_drift = copy.deepcopy(document)
        feature_drift["provenance"]["features"] = features
        feature_drift["clocks"]["collector_attestation"]["runtime_attestation"][
          "features"
        ] = features
        feature_drift["thread_cpu_behavior"]["runtime_attestation"][
          "features"
        ] = features
        report = speed_evidence.validate_supplemental_speed_cell(artifact, feature_drift)
        self.assertFalse(report["passed"])
        self.assertTrue(any(
          "feature set differs from build mode" in failure
          for failure in report["failures"]
        ))

  def test_build_modes_have_exact_noninterchangeable_attestation_signatures(self) -> None:
    self.assertEqual(
      extract_speed.RUNTIME_BUILD_MODE_FEATURES,
      speed_evidence.BENCHMARK_FEATURES_BY_BUILD_MODE,
    )
    triple = "x86_64-apple-darwin"
    revision = "1" * 40
    no_default = synthetic_runtime_attestation(
      triple, "criterion", revision, "no-default"
    )
    failures: list[str] = []
    speed_evidence.validate_runtime_attestation(
      "no-default fixture",
      no_default,
      triple,
      "criterion",
      "no-default",
      revision,
      failures,
    )
    self.assertEqual(failures, [])
    self.assertEqual(
      no_default["features"],
      list(speed_evidence.BENCHMARK_FEATURES_BY_BUILD_MODE["no-default"]),
    )

    default_failures: list[str] = []
    speed_evidence.validate_runtime_attestation(
      "default fixture",
      no_default,
      triple,
      "criterion",
      "default",
      revision,
      default_failures,
    )
    self.assertTrue(any(
      "feature set differs from build mode 'default'" in failure
      for failure in default_failures
    ))

  def test_supplemental_validator_rejects_weak_native_or_exact_thread_cpu_rows(self) -> None:
    document = supplemental_speed_documents()["speed-supplemental-macos-x86_64.json"]
    weak_native = copy.deepcopy(document)
    weak_native["clocks"]["native_thread_cpu"]["benchmark"] = "native_thread_cpu"
    weak_native["thread_cpu_behavior"]["direct_benchmark"] = "native_thread_cpu"
    report = speed_evidence.validate_supplemental_speed_cell(
      "speed-supplemental-macos-x86_64.json", weak_native
    )
    self.assertFalse(report["passed"])
    self.assertTrue(any("native thread-CPU benchmark identity changed" in item for item in report["failures"]))

    exact_drift = copy.deepcopy(document)
    candidate = exact_drift["route_coverage"]["thread_cpu"]["eligible_exact_rows"][0]
    exact_drift["clocks"][candidate]["provider"] = "wrong_direct_provider"
    report = speed_evidence.validate_supplemental_speed_cell(
      "speed-supplemental-macos-x86_64.json", exact_drift
    )
    self.assertFalse(report["passed"])
    self.assertTrue(any("changed provider identity" in item for item in report["failures"]))

  def test_linux_aarch64_native_identity_tracks_harness_and_build_mode(self) -> None:
    documents = supplemental_speed_documents()
    no_default = documents["speed-supplemental-linux-aarch64-no-default.json"]
    lambda_default = documents["speed-supplemental-lambda-aarch64.json"]

    self.assertEqual(
      no_default["clocks"]["native_thread_cpu"]["benchmark"],
      "native_thread_cpu__libc_clock_gettime_clock_thread_cputime_id",
    )
    self.assertEqual(
      lambda_default["clocks"]["native_thread_cpu"]["benchmark"],
      "native_thread_cpu__raw_syscall_clock_thread_cputime_id",
    )
    self.assertTrue(speed_evidence.validate_supplemental_speed_cell(
      "speed-supplemental-linux-aarch64-no-default.json", no_default
    )["passed"])
    self.assertTrue(speed_evidence.validate_supplemental_speed_cell(
      "speed-supplemental-lambda-aarch64.json", lambda_default
    )["passed"])

  def test_supplemental_validator_requires_exact_integer_sidecar_samples(self) -> None:
    document = supplemental_speed_documents()["speed-supplemental-macos-x86_64.json"]
    wrong_count = copy.deepcopy(document)
    wrong_count["thread_cpu_behavior"]["sample_count"] = 4
    for phase in speed_evidence.THREAD_CPU_BEHAVIOR_PHASES:
      samples = wrong_count["thread_cpu_behavior"][phase]["samples"]
      samples.append(copy.deepcopy(samples[-1]))
    report = speed_evidence.validate_supplemental_speed_cell(
      "speed-supplemental-macos-x86_64.json", wrong_count
    )
    self.assertFalse(report["passed"])
    self.assertTrue(any("invalid sample count" in item for item in report["failures"]))

    float_sample = copy.deepcopy(document)
    float_sample["thread_cpu_behavior"]["sleep"]["samples"][0]["public_delta_ns"] = 10.0
    report = speed_evidence.validate_supplemental_speed_cell(
      "speed-supplemental-macos-x86_64.json", float_sample
    )
    self.assertFalse(report["passed"])
    self.assertTrue(any("integer deltas" in item for item in report["failures"]))

  def test_supplemental_runtime_smoke_requires_producer_attestation(self) -> None:
    document = supplemental_speed_documents()["speed-supplemental-wasip1-threads-smoke.json"]
    document.pop("runtime_attestation")
    report = speed_evidence.validate_supplemental_speed_cell(
      "speed-supplemental-wasip1-threads-smoke.json", document
    )
    self.assertFalse(report["passed"])
    self.assertTrue(any("runtime attestation" in item for item in report["failures"]))

  def test_supplemental_runtime_smoke_cannot_carry_a_collector_bundle(self) -> None:
    artifact = "speed-supplemental-wasip1-threads-smoke.json"
    document = supplemental_speed_documents()[artifact]
    document["collector_bundle"] = {
      "schema": speed_evidence.COLLECTOR_BUNDLE_DESCRIPTOR_SCHEMA,
      "path": "not-a-runtime-smoke.bundle",
      "manifest_sha256": "0" * 64,
    }
    report = speed_evidence.validate_supplemental_speed_cell(artifact, document)
    self.assertFalse(report["passed"])
    self.assertTrue(any(
      "document schema changed" in failure for failure in report["failures"]
    ))

  def test_paired_equivalence_uses_the_difference_distribution(self) -> None:
    reference_samples = [100.0, 200.0] * 15 + [150.0]
    subject_samples = [value + 4.0 for value in reference_samples]
    reference = {
      "now": 150.0,
      "now_ci95": [100.0, 200.0],
      "now_samples": reference_samples,
      "paired_sample_id": "paired-host-route",
    }
    subject = {
      "now": 154.0,
      "now_ci95": [104.0, 204.0],
      "now_samples": subject_samples,
      "paired_sample_id": "paired-host-route",
    }
    passed, allowance = speed_evidence.equivalent_or_faster(subject, reference, "now")
    self.assertTrue(passed)
    self.assertEqual(allowance, 7.5)

    subject["paired_sample_id"] = "different-route"
    with self.assertRaises(TypeError):
      speed_evidence.equivalent_or_faster(subject, reference, "now")

    subject["paired_sample_id"] = "paired-host-route"
    del reference["paired_sample_id"]
    passed, _ = speed_evidence.equivalent_or_faster(subject, reference, "now")
    self.assertFalse(passed)

  def test_aarch64_pmccntr_negative_evidence_is_reproducible_and_ineligible(self) -> None:
    root = Path(__file__).resolve().parents[1]
    evidence_path = root / "benches/evidence/thread-cpu-aarch64-pmccntr-negative.json"
    evidence = json.loads(evidence_path.read_text())
    self.assertEqual(evidence["status"], "ineligible-production-candidate")
    cells = {cell["instance"]: cell for cell in evidence["measurements"]}
    virtualized = cells["c7g.large"]
    metal = cells["c7g.metal"]
    self.assertGreater(
      virtualized["pmccntr_ns_per_read"],
      virtualized["task_clock_cntvct_ns_per_read"],
    )
    self.assertAlmostEqual(
      virtualized["pmccntr_over_task_ratio"],
      virtualized["pmccntr_ns_per_read"] / virtualized["task_clock_cntvct_ns_per_read"],
      places=3,
    )
    material_allowance = speed_evidence.equivalence_allowance(
      metal["task_clock_cntvct_ns_per_read"]
    )
    self.assertLessEqual(metal["pmccntr_advantage_ns"], material_allowance)
    self.assertEqual(metal["material_result"], "tie")
    probe = root / evidence["reproduction"]["probe"]
    runner = root / evidence["reproduction"]["runner"]
    probe_source = probe.read_text()
    runner_source = runner.read_text()
    for invariant in (
      "PERF_COUNT_HW_CPU_CYCLES, 3",
      "index != 32",
      "width != 64",
      "enabled != running",
      "read_pmccntr_ns",
      "pin_to_cpu(1)",
    ):
      self.assertIn(invariant, probe_source)
    self.assertIn("-Wall -Wextra -Werror", runner_source)
    self.assertIn("kernel.perf_event_paranoid=-1", runner_source)
    self.assertIn("kernel.perf_user_access=1", runner_source)

    x86 = json.loads(
      (root / "benches/evidence/thread-cpu-x86-reference-cycles-negative.json").read_text()
    )
    self.assertEqual(x86["status"], "ineligible-production-candidate")
    self.assertEqual(x86["primary_source"]["revision"], "092")
    self.assertTrue(
      any("thermal-throttling" in mismatch for mismatch in x86["semantic_mismatch"])
    )
    self.assertIn("different quantity", x86["conclusion"])

  def test_residual_wall_route_tables_cover_every_exact_reader_and_identity(self) -> None:
    root = Path(__file__).resolve().parents[1]
    public_source = (root / "src/bench.rs").read_text()
    harness_source = (root / "benches/instant.rs").read_text()
    arch_sources = [
      (root / path).read_text()
      for path in (
        "src/arch/linux_clock_wall.rs",
        "src/arch/riscv64.rs",
        "src/arch/loongarch64.rs",
        "src/arch/powerpc64.rs",
        "src/arch/freebsd_x86_64.rs",
      )
    ]
    for source in arch_sources:
      exact_readers = set(
        re.findall(
          r"exact_bench_reader!\(\s*(bench_exact_[a-z0-9_]+)", source
        )
      )
      for reader in exact_readers:
        self.assertIn(reader, public_source, f"{reader} lacks a public exact route")

    public_routes = set(
      re.findall(
        r"expose_[a-z0-9_]+_exact_read!\(\s*(residual_exact_[a-z0-9_]+)",
        public_source,
      )
    )
    for route in public_routes:
      self.assertIn(route, harness_source, f"{route} is absent from the static route table")

    provider_pattern = r'"((?:linux_clock|riscv_|loongarch_|power_|freebsd_)[a-z0-9_]+)"'
    provider_identities = set().union(
      *(re.findall(provider_pattern, source) for source in arch_sources)
    )
    provider_identities.discard("freebsd_timehands_protocol")
    for provider in provider_identities:
      self.assertIn(
        f'"{provider}"',
        harness_source,
        f"{provider} is absent from the static route table",
      )

  def test_apple_aarch64_harness_uses_every_selector_primitive(self) -> None:
    root = Path(__file__).resolve().parents[1]
    arch_source = (root / "src/arch/apple_aarch64.rs").read_text()
    public_source = (root / "src/bench.rs").read_text()
    harness_source = (root / "benches/instant.rs").read_text()
    for source_name in (
      "bench_instant_candidate_primitives",
      "bench_ordered_candidate_primitives",
      "bench_selected_instant_primitive",
      "bench_selected_ordered_primitive",
      "bench_instant_evidence",
      "bench_ordered_evidence",
    ):
      self.assertIn(f"fn {source_name}", arch_source)
    for route_name in (
      "apple_aarch64_instant_candidate_primitives",
      "apple_aarch64_ordered_candidate_primitives",
      "apple_aarch64_selected_instant_primitive",
      "apple_aarch64_selected_ordered_primitive",
      "apple_aarch64_instant_selection_measurements",
      "apple_aarch64_ordered_selection_measurements",
    ):
      self.assertIn(f"fn {route_name}", public_source)
      self.assertGreaterEqual(
        harness_source.count(route_name),
        1 if route_name.endswith("selection_measurements") else 2,
      )
    for stale_provider in (
      '"apple_commpage_isb_cntvct"',
      '"apple_commpage_cntvctss"',
      '"apple_commpage_acntvct"',
    ):
      self.assertNotIn(stale_provider, harness_source)

  def test_apple_aarch64_selector_requires_every_now_and_elapsed_route(self) -> None:
    selection, values = apple_aarch64_complete_cell()

    failures, report = speed_evidence.validate_cell(
      "Apple aarch64", values, "aarch64-apple-darwin"
    )
    self.assertEqual(failures, [])
    for domain in ("instant", "ordered"):
      self.assertEqual(
        report["wall_selector_reproduction"][domain]["measured_winner"],
        "apple_continuous_hw_acntvct_base",
      )
      self.assertTrue(report["selected_wall_provider_parity"][domain]["passed"])
      self.assertEqual(
        report["selected_wall_provider_parity"][domain]["metrics"]["now"][
          "comparison_basis"
        ],
        "alternating paired public/exact probe",
      )

    missing = copy.deepcopy(values)
    missing.pop(selection["eligible_direct_candidates"]["instant"][-1])
    failures, _ = speed_evidence.validate_cell(
      "Apple aarch64 missing route", missing, "aarch64-apple-darwin"
    )
    self.assertTrue(any("lacks exact row" in failure for failure in failures))

    tampered = copy.deepcopy(selection)
    tampered["probe"]["ordered"]["measured_winner"] = "apple_mach_absolute_time"
    failures = []
    speed_evidence.validate_apple_wall_selector("tampered", tampered, failures)
    self.assertTrue(any("winner does not reproduce" in failure for failure in failures))

    slower_public = copy.deepcopy(selection)
    slower_public["public_exact_probe"]["ordered"]["public_batches_ns"] = [800_000] * 9
    failures = []
    speed_evidence.validate_apple_wall_selector("slower public", slower_public, failures)
    self.assertTrue(
      any("ordered public read is repeatably slower" in failure for failure in failures)
    )

  def test_apple_aarch64_extractor_completes_duplicate_ordered_elapsed_rows(self) -> None:
    selection = apple_aarch64_wall_selection()
    with tempfile.TemporaryDirectory() as directory:
      criterion = Path(directory)
      (criterion / "apple-wall-selection.json").write_text(json.dumps(selection))
      for group in extract_speed.WALL_GROUPS.values():
        for domain in ("instant", "ordered"):
          for benchmark in selection["eligible_direct_candidates"][domain]:
            write_criterion_estimate(criterion, group, benchmark)
      out = {"tach": {}, "tach_ordered": {}}
      for benchmark in selection["eligible_direct_candidates"]["ordered"]:
        entry = {
          "provider": benchmark.removeprefix("direct_ordered_wall__"),
          "read_cost": "inline",
          "time_domain": "ordered wall",
        }
        extract_speed.add_estimate(
          entry,
          "now",
          extract_speed.median_estimate(
            criterion, extract_speed.WALL_GROUPS["now"], benchmark
          ),
        )
        out[benchmark] = entry
      extract_speed.add_wall_selector_evidence(criterion, out)
    for benchmark in selection["eligible_direct_candidates"]["ordered"]:
      self.assertIn("now", out[benchmark])
      self.assertIn("elapsed", out[benchmark])
      self.assertIn("elapsed_ci95", out[benchmark])
      self.assertEqual(out[benchmark]["benchmark"], benchmark)

  def test_lambda_samples_cover_both_exact_wall_candidate_domains(self) -> None:
    rows = {}
    for domain, benchmark in (
      ("instant", "direct_wall__exact_instant_route"),
      ("ordered", "direct_ordered_wall__exact_ordered_route"),
    ):
      identity = speed_evidence.exact_wall_candidate_identity(domain, benchmark)
      assert identity is not None
      rows[benchmark] = {**estimate(10.0), **identity}

    failures: list[str] = []
    speed_evidence.validate_lambda_samples("Lambda", rows, failures)
    for benchmark in rows:
      self.assertTrue(
        any(
          f"Lambda {benchmark}: expected {speed_evidence.LAMBDA_SAMPLE_COUNT} finite now samples"
          in failure
          for failure in failures
        )
      )
      self.assertTrue(
        any(
          f"Lambda {benchmark}: expected {speed_evidence.LAMBDA_SAMPLE_COUNT} finite elapsed samples"
          in failure
          for failure in failures
        )
      )

    for benchmark, entry in rows.items():
      for metric in speed_evidence.METRICS:
        samples = [10.0] * speed_evidence.LAMBDA_SAMPLE_COUNT
        point, interval = speed_evidence.lambda_median_with_ci(
          samples, f"{benchmark}:{metric}"
        )
        entry[f"{metric}_samples"] = samples
        entry[metric] = point
        entry[f"{metric}_ci95"] = interval
    failures = []
    speed_evidence.validate_lambda_samples("Lambda", rows, failures)
    self.assertEqual(failures, [])

    tampered = copy.deepcopy(rows)
    tampered["direct_ordered_wall__exact_ordered_route"]["elapsed"] = 11.0
    failures = []
    speed_evidence.validate_lambda_samples("Lambda", tampered, failures)
    self.assertTrue(any("elapsed aggregate does not reproduce" in item for item in failures))

  def test_hosted_evidence_is_sha_frozen_and_artifact_only(self) -> None:
    root = Path(__file__).resolve().parents[1]
    workflow = (root / ".github/workflows/bench-speed-windows.yml").read_text()
    freeze_script = (root / "benches/require-clean-benchmark-source.sh").read_text()
    self.assertIn("ref: ${{ env.TACH_FREEZE_SHA }}", workflow)
    self.assertIn("runs-on: macos-15-intel", workflow)
    self.assertIn("windows-i686-supplemental:", workflow)
    self.assertIn("rustup target add i686-pc-windows-msvc", workflow)
    self.assertIn("--target i686-pc-windows-msvc", workflow)
    self.assertIn("windows-arm64-supplemental:", workflow)
    self.assertIn("runs-on: windows-11-arm", workflow)
    self.assertIn("--target aarch64-pc-windows-msvc", workflow)
    self.assertIn("linux-i686-supplemental:", workflow)
    self.assertIn("rustup target add i686-unknown-linux-gnu", workflow)
    self.assertIn("--target i686-unknown-linux-gnu", workflow)
    self.assertIn("RUSTFLAGS: -D warnings", workflow)
    self.assertNotIn("CRITERION_HOME", workflow)
    self.assertIn("actions/checkout@v6.0.2", workflow)
    self.assertIn("actions/upload-artifact@v7.0.1", workflow)
    self.assertNotIn("contents: write", workflow)
    self.assertNotIn("git push", workflow)
    freeze_path_block = freeze_script.split("paths=(", 1)[1].split("\n)", 1)[0]
    freeze_paths = {
      line.strip().strip("'\"")
      for line in freeze_path_block.splitlines()
      if line.strip()
    }
    self.assertEqual(freeze_paths, set(speed_evidence.BENCHMARK_SOURCE_PATHS))
    self.assertFalse(
      any(path.endswith(".json") for path in speed_evidence.BENCHMARK_SOURCE_PATHS),
      "generated evidence binds itself through source_revision, not the benchmark source list",
    )

  def test_every_declared_same_thread_reference_is_gated(self) -> None:
    values = clocks()
    values["quanta"] = estimate(1.0)
    failures, report = speed_evidence.validate_cell(
      "synthetic", values, "x86_64-unknown-linux-gnu"
    )
    self.assertTrue(any("Instant now is materially slower than quanta" in item for item in failures))
    self.assertFalse(report["same_thread_elapsed"]["now"]["passed"])

  def test_apple_aarch64_quanta_bare_counter_is_ineligible_only_on_that_target(self) -> None:
    apple = speed_evidence.local_reference_eligibility("aarch64-apple-darwin")
    quanta = apple["quanta"]
    self.assertFalse(quanta["eligible"])
    self.assertEqual(
      quanta["reason"],
      speed_evidence.APPLE_AARCH64_QUANTA_INELIGIBILITY_REASON,
    )
    self.assertEqual(
      quanta["implementation"],
      speed_evidence.APPLE_AARCH64_QUANTA_IMPLEMENTATION,
    )
    self.assertIn("bare CNTVCT_EL0", quanta["implementation"])
    self.assertIn("wake-corrected", quanta["implementation"])
    self.assertIn("suspend", quanta["implementation"])

    for target in ("aarch64-unknown-linux-gnu", "x86_64-apple-darwin"):
      with self.subTest(target=target):
        self.assertTrue(
          speed_evidence.local_reference_eligibility(target)["quanta"]["eligible"]
        )

  def test_emscripten_quanta_compile_gap_is_explicit_and_the_row_may_be_absent(self) -> None:
    target = "wasm32-unknown-emscripten"
    policies = speed_evidence.local_reference_eligibility(target)
    quanta = policies["quanta"]
    self.assertFalse(quanta["eligible"])
    self.assertEqual(
      quanta["reason"],
      speed_evidence.EMSCRIPTEN_QUANTA_INELIGIBILITY_REASON,
    )
    self.assertIn("target_os=wasi", quanta["implementation"])
    for reference in ("fastant", "minstant"):
      self.assertFalse(policies[reference]["eligible"])
      self.assertEqual(
        policies[reference]["reason"],
        speed_evidence.WASM_UNKNOWN_SYSTEM_TIME_INELIGIBILITY_REASON,
      )
    self.assertTrue(policies["std"]["eligible"])

    values = clocks()
    values.pop("quanta")
    failures, report = speed_evidence.validate_cell("Emscripten", values, target)
    self.assertEqual(failures, [])
    comparison = report["same_thread_elapsed"]["now"]["comparisons"]["quanta"]
    self.assertFalse(comparison["eligible_for_reliable_contract"])
    self.assertFalse(comparison["measured"])
    self.assertIsNone(comparison["reference_ns"])

  def test_emscripten_wall_selector_reproduces_from_paired_samples(self) -> None:
    performance = [100_000] * 9
    hrtime = [80_000] * 9
    decision = speed_evidence.reproduce_material_decision(
      hrtime, performance, 4_096, 8
    )
    domain_probe = {
      "performance_eligible": True,
      "hrtime_eligible": True,
      "performance_batches_ns": performance,
      "hrtime_batches_ns": hrtime,
      "allowance_ns": decision["allowance_ns"],
      "hrtime_decisive_wins": decision["decisive_wins"],
    }
    selected = {
      "instant": "process.hrtime.bigint",
      "ordered": "process.hrtime.bigint",
    }
    candidates = {
      "instant": [
        "direct_wall__performance.now",
        "direct_wall__process.hrtime.bigint",
      ],
      "ordered": [
        "direct_ordered_wall__performance.now",
        "direct_ordered_wall__process.hrtime.bigint",
      ],
    }
    probe = {
      "reads_per_batch": 4_096,
      "required_decisive_wins": 8,
      "instant": copy.deepcopy(domain_probe),
      "ordered": copy.deepcopy(domain_probe),
    }
    failures = []
    reproduction = speed_evidence.validate_emscripten_wall_selector(
      "Emscripten", selected, candidates, probe, failures
    )
    self.assertEqual(failures, [])
    self.assertEqual(reproduction["instant"]["winner"], "process.hrtime.bigint")

    probe["ordered"]["allowance_ns"] += 1
    speed_evidence.validate_emscripten_wall_selector(
      "Emscripten", selected, candidates, probe, failures
    )
    self.assertTrue(any(
      "Emscripten ordered allowance does not reproduce" in failure
      for failure in failures
    ))

  def test_wasm_unknown_ineligible_references_are_explicit(self) -> None:
    target = "wasm32-unknown-unknown"
    policies = speed_evidence.local_reference_eligibility(target)
    self.assertFalse(policies["quanta"]["eligible"])
    self.assertEqual(
      policies["quanta"]["reason"],
      speed_evidence.WASM_UNKNOWN_QUANTA_INELIGIBILITY_REASON,
    )
    self.assertIn("never-crash contract", policies["quanta"]["implementation"])
    for reference in ("fastant", "minstant"):
      self.assertFalse(policies[reference]["eligible"])
      self.assertEqual(
        policies[reference]["reason"],
        speed_evidence.WASM_UNKNOWN_SYSTEM_TIME_INELIGIBILITY_REASON,
      )
      self.assertIn("Date.now", policies[reference]["implementation"])
    self.assertFalse(policies["std"]["eligible"])
    self.assertEqual(
      policies["std"]["reason"],
      speed_evidence.WASM_UNKNOWN_STD_INELIGIBILITY_REASON,
    )
    values = clocks()
    values.pop("std")
    failures, report = speed_evidence.validate_cell("Node Wasm", values, target)
    self.assertEqual(failures, [])
    comparisons = report["same_thread_elapsed"]["now"]["comparisons"]
    self.assertTrue(all(
      not comparison["eligible_for_reliable_contract"]
      for comparison in comparisons.values()
    ))
    ordered = report["cross_thread_elapsed"]["now"]
    self.assertFalse(ordered["measured"])
    self.assertFalse(ordered["eligible_for_reliable_contract"])
    self.assertTrue(ordered["passed"])

  def test_wasi_system_time_references_are_not_treated_as_monotonic(self) -> None:
    for target in ("wasm32-wasip1", "wasm32-wasip2", "wasm32-wasip1-threads"):
      policies = speed_evidence.local_reference_eligibility(target)
      for reference in ("fastant", "minstant"):
        self.assertFalse(policies[reference]["eligible"])
        self.assertEqual(
          policies[reference]["reason"],
          speed_evidence.WASI_SYSTEM_TIME_INELIGIBILITY_REASON,
        )
      self.assertTrue(policies["quanta"]["eligible"])
      self.assertTrue(policies["std"]["eligible"])

  def test_wasm_wall_selector_reproduces_from_paired_samples(self) -> None:
    performance = [100_000] * 9
    hrtime = [80_000] * 9
    decision = speed_evidence.reproduce_material_decision(
      hrtime, performance, 4_096, 8
    )
    domain_probe = {
      "performance_median_ns": decision["incumbent_median_ns"],
      "hrtime_median_ns": decision["challenger_median_ns"],
      "performance_batches_ns": performance,
      "hrtime_batches_ns": hrtime,
      "allowance_ns": decision["allowance_ns"],
      "hrtime_decisive_wins": decision["decisive_wins"],
    }
    selected = {
      "instant": "process.hrtime.bigint",
      "ordered": "process.hrtime.bigint",
    }
    candidates = {
      "instant": [
        "direct_wall__performance.now",
        "direct_wall__process.hrtime.bigint",
      ],
      "ordered": [
        "direct_ordered_wall__performance.now",
        "direct_ordered_wall__process.hrtime.bigint",
      ],
    }
    probe = {
      "reads_per_batch": 4_096,
      "required_decisive_wins": 8,
      "instant": copy.deepcopy(domain_probe),
      "ordered": copy.deepcopy(domain_probe),
    }
    failures = []
    reproduction = speed_evidence.validate_wasm_wall_selector(
      "Node Wasm", selected, candidates, probe, failures
    )
    self.assertEqual(failures, [])
    self.assertEqual(reproduction["instant"]["winner"], "process.hrtime.bigint")

    repeated = [copy.deepcopy(probe) for _ in range(5)]
    failures = []
    reproduction = speed_evidence.validate_wasm_wall_selector(
      "Node Wasm", selected, candidates, probe, failures, repeated
    )
    self.assertEqual(failures, [])
    self.assertEqual(len(reproduction["instant"]["observations"]), 5)

    selected["instant"] = "performance.now"
    speed_evidence.validate_wasm_wall_selector(
      "Node Wasm", selected, candidates, probe, failures
    )
    self.assertTrue(any(
      "Wasm instant selected provider does not reproduce" in failure
      for failure in failures
    ))

  def test_apple_aarch64_quanta_row_is_reported_but_does_not_gate_a_valid_cell(self) -> None:
    _, values = apple_aarch64_complete_cell()
    values["quanta"] = estimate(1.0)

    failures, report = speed_evidence.validate_cell(
      "Apple aarch64", values, "aarch64-apple-darwin"
    )

    self.assertEqual(failures, [])
    comparison = report["same_thread_elapsed"]["now"]["comparisons"]["quanta"]
    self.assertEqual(comparison["reference_ns"], 1.0)
    self.assertFalse(comparison["eligible_for_reliable_contract"])
    self.assertEqual(
      comparison["eligibility_reason"],
      speed_evidence.APPLE_AARCH64_QUANTA_INELIGIBILITY_REASON,
    )
    self.assertFalse(comparison["passed"])
    self.assertTrue(report["same_thread_elapsed"]["now"]["passed"])

  def test_windows_same_thread_gate_excludes_non_qpc_contracts_but_keeps_rows(self) -> None:
    values = clocks()
    for reference in ("quanta", "fastant", "minstant"):
      values[reference] = estimate(1.0)
    failures, report = speed_evidence.validate_cell(
      "Windows x64", values, "x86_64-pc-windows-msvc"
    )
    self.assertEqual(failures, [])
    comparisons = report["same_thread_elapsed"]["now"]["comparisons"]
    self.assertEqual(comparisons["quanta"]["reference_ns"], 1.0)
    self.assertFalse(comparisons["quanta"]["eligible_for_reliable_contract"])
    self.assertEqual(
      comparisons["quanta"]["eligibility_reason"],
      "windows_raw_counter_does_not_meet_qpc_reliability_contract",
    )
    for reference in ("fastant", "minstant"):
      self.assertFalse(comparisons[reference]["eligible_for_reliable_contract"])
      self.assertEqual(
        comparisons[reference]["eligibility_reason"],
        "windows_system_time_fallback_is_not_monotonic_qpc",
      )
    self.assertTrue(comparisons["std"]["eligible_for_reliable_contract"])
    self.assertTrue(report["same_thread_elapsed"]["now"]["passed"])

  def test_windows_std_qpc_remains_a_speed_gate(self) -> None:
    values = clocks()
    values["std"] = estimate(1.0)
    failures, report = speed_evidence.validate_cell(
      "Windows x64", values, "x86_64-pc-windows-msvc"
    )
    self.assertTrue(any("Instant now is materially slower than std" in item for item in failures))
    self.assertFalse(report["same_thread_elapsed"]["now"]["passed"])
    self.assertTrue(
      any("OrderedInstant now is materially slower than std" in item for item in failures)
    )

  def test_windows_i686_quanta_qpc_fallback_remains_a_speed_gate(self) -> None:
    values = clocks()
    values["quanta"] = estimate(1.0)
    failures, report = speed_evidence.validate_cell(
      "Windows i686", values, "i686-pc-windows-msvc"
    )
    self.assertTrue(any("Instant now is materially slower than quanta" in item for item in failures))
    self.assertTrue(
      report["same_thread_elapsed"]["now"]["comparisons"]["quanta"][
        "eligible_for_reliable_contract"
      ]
    )

  def test_windows_thread_cpu_fixed_provider_covers_every_supported_architecture(self) -> None:
    selection = windows_thread_cpu_selection()
    for target in (
      "i686-pc-windows-msvc",
      "x86_64-pc-windows-msvc",
      "aarch64-pc-windows-msvc",
    ):
      with self.subTest(target=target):
        failures: list[str] = []
        result = speed_evidence.validate_thread_cpu_selector(
          target, selection, failures, target
        )
        self.assertEqual(failures, [])
        self.assertEqual(result["winner"], "windows_thread_times")
        self.assertEqual(
          result["selected_mechanism"],
          "get_thread_times_current_thread_pseudohandle",
        )
        self.assertFalse(result["fallback"]["eligible_for_thread_cpu_speed_claim"])
        self.assertTrue(result["fallback"]["exact_route_measured"])
        self.assertFalse(
          result["fallback"]["observed_as_public_provider_during_campaign"]
        )

  def test_windows_thread_cpu_selector_rejects_hidden_fallback_or_wrong_target(self) -> None:
    selection = windows_thread_cpu_selection()
    selection.pop("failure_fallback")
    failures: list[str] = []
    speed_evidence.validate_thread_cpu_selector(
      "missing fallback", selection, failures, "x86_64-pc-windows-msvc"
    )
    self.assertTrue(any("failure fallback is not explicit" in item for item in failures))

    selection = windows_thread_cpu_selection()
    failures = []
    speed_evidence.validate_thread_cpu_selector(
      "wrong target", selection, failures, "x86_64-unknown-linux-gnu"
    )
    self.assertTrue(any("unsupported target" in item for item in failures))

    selection = windows_thread_cpu_selection()
    selection["failure_fallback"]["observed_as_public_provider_during_campaign"] = True
    failures = []
    speed_evidence.validate_thread_cpu_selector(
      "observed wall fallback", selection, failures, "x86_64-pc-windows-msvc"
    )
    self.assertTrue(
      any("observed Windows wall fallback cannot be thread-CPU" in item for item in failures)
    )

  def test_macos_fixed_native_thread_cpu_selector_has_exact_direct_rows(self) -> None:
    selection = macos_fixed_native_thread_cpu_selection()
    for target in ("x86_64-apple-darwin", "aarch64-apple-darwin"):
      with self.subTest(target=target):
        failures: list[str] = []
        result = speed_evidence.validate_thread_cpu_selector(
          target, selection, failures, target
        )
        self.assertEqual(failures, [])
        self.assertEqual(result["winner"], "posix_thread_cpu_clock")
        self.assertEqual(result["selected_mechanism"], selection["selected_mechanism"])
        self.assertIsNone(result["fallback"])

    candidate = selection["eligible_direct_candidates"][0]
    values = clocks("POSIX thread CPU clock", "system call")
    values["tach_thread_cpu"]["selection"] = selection
    values[candidate] = {
      **estimate(10.0),
      "provider": selection["selected_mechanism"],
      "read_cost": "system call",
      "time_domain": "thread CPU",
      "benchmark": candidate,
    }
    values["direct_selected_thread_cpu"] = {
      **estimate(10.0),
      "provider": selection["selected_mechanism"],
      "read_cost": "system call",
      "time_domain": "thread CPU",
      "benchmark": selection["selected_native_benchmark"],
    }
    failures, report = speed_evidence.validate_cell(
      "macOS fixed native", values, "aarch64-apple-darwin"
    )
    self.assertEqual(failures, [])
    parity = report["selected_thread_cpu_provider_parity"]
    self.assertTrue(parity["metrics"]["now"]["passed"])
    self.assertTrue(parity["metrics"]["elapsed"]["passed"])

    fabricated_tournament = copy.deepcopy(selection)
    fabricated_tournament["perf"] = {}
    failures = []
    speed_evidence.validate_thread_cpu_selector(
      "macOS fabricated tournament", fabricated_tournament, failures,
      "aarch64-apple-darwin",
    )
    self.assertTrue(any("must not declare a perf tournament" in item for item in failures))

    mismatched = copy.deepcopy(selection)
    mismatched["selected_native_benchmark"] = "direct_selected_thread_cpu__other"
    failures = []
    speed_evidence.validate_thread_cpu_selector(
      "macOS wrong selected identity", mismatched, failures, "aarch64-apple-darwin"
    )
    self.assertTrue(any("identity changed" in item for item in failures))

    wrong_primitive = copy.deepcopy(selection)
    wrong_primitive["fixed_provider"]["native_primitive"] = "clock_gettime"
    failures = []
    speed_evidence.validate_thread_cpu_selector(
      "macOS wrong native primitive", wrong_primitive, failures, "aarch64-apple-darwin"
    )
    self.assertTrue(any("basis is incomplete" in item for item in failures))

    failures = []
    speed_evidence.validate_thread_cpu_selector(
      "macOS wrong target", selection, failures, "x86_64-unknown-linux-gnu"
    )
    self.assertTrue(any("unsupported target" in item for item in failures))

  def test_freebsd_thread_cpu_selector_is_a_native_entry_tournament(self) -> None:
    selection = freebsd_thread_cpu_selection()
    failures: list[str] = []
    result = speed_evidence.validate_thread_cpu_selector(
      "FreeBSD runtime tournament",
      selection,
      failures,
      "x86_64-unknown-freebsd",
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["winner"], "posix_thread_cpu_clock")
    self.assertEqual(result["selected_mechanism"], "raw_entry")
    self.assertEqual(result["perf_counter"], {})
    self.assertEqual(result["perf_read_entry"], {})

  def test_aarch64_capability_policy_is_separate_from_its_performance_audit(self) -> None:
    selection = aarch64_capability_thread_cpu_selection()
    failures: list[str] = []
    result = speed_evidence.validate_thread_cpu_selector(
      "AArch64 capability policy",
      selection,
      failures,
      "aarch64-unknown-linux-gnu",
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["winner"], "linux_perf_mmap")
    self.assertEqual(result["fallback"][0], "posix_thread_cpu_clock")
    self.assertEqual(result["perf_path"]["audit"]["winner"], "linux_perf_mmap")

  def test_aarch64_capability_policy_fails_when_the_preferred_path_loses_audit(self) -> None:
    selection = aarch64_capability_thread_cpu_selection()
    selection["perf"]["path_probe"]["candidate_batches_ns"][1] = [120_000] * 9
    failures: list[str] = []
    speed_evidence.validate_thread_cpu_selector(
      "AArch64 losing capability policy",
      selection,
      failures,
      "aarch64-unknown-linux-gnu",
    )
    self.assertTrue(any("loses its performance audit" in item for item in failures))

  def test_aarch64_capability_policy_cannot_claim_native_when_mmap_is_eligible(self) -> None:
    selection = aarch64_capability_thread_cpu_selection()
    selection["perf"]["path_probe"]["selected_candidate"] = "posix_thread_cpu"
    failures: list[str] = []
    speed_evidence.validate_thread_cpu_selector(
      "AArch64 contradicted capability policy",
      selection,
      failures,
      "aarch64-unknown-linux-gnu",
    )
    self.assertTrue(any("capability policy does not reproduce" in item for item in failures))

  def test_windows_thread_cpu_claim_requires_public_direct_parity_for_both_metrics(self) -> None:
    selection = windows_thread_cpu_selection()
    mechanism = selection["selected_mechanism"]
    candidate = selection["eligible_direct_candidates"][0]
    values = clocks("Windows GetThreadTimes", "system call")
    values["tach_thread_cpu"]["selection"] = selection
    values[candidate] = {
      **estimate(10.0),
      "provider": mechanism,
      "read_cost": "system call",
      "time_domain": "thread CPU",
      "benchmark": candidate,
    }
    values["direct_selected_thread_cpu"] = {
      **estimate(10.0),
      "provider": mechanism,
      "read_cost": "system call",
      "time_domain": "thread CPU",
      "benchmark": selection["selected_native_benchmark"],
    }
    failure_fallback = selection["failure_fallback"]
    values["direct_failure_fallback_thread_cpu"] = {
      **estimate(12.0),
      "provider": failure_fallback["mechanism"],
      "read_cost": failure_fallback["read_cost"],
      "time_domain": failure_fallback["time_domain"],
      "benchmark": failure_fallback["exact_benchmark"],
      "eligible_for_thread_cpu_speed_claim": False,
    }
    failures, report = speed_evidence.validate_cell(
      "Windows x64", values, "x86_64-pc-windows-msvc"
    )
    self.assertEqual(failures, [])
    parity = report["selected_thread_cpu_provider_parity"]
    self.assertTrue(parity["metrics"]["now"]["passed"])
    self.assertTrue(parity["metrics"]["elapsed"]["passed"])
    measured_fallback = parity["failure_fallback"]
    self.assertEqual(measured_fallback["time_domain"], "monotonic wall fallback")
    self.assertFalse(measured_fallback["eligible_for_thread_cpu_speed_claim"])
    self.assertEqual(measured_fallback["now_ns"], 12.0)
    self.assertEqual(measured_fallback["elapsed_ns"], 12.0)

    missing = copy.deepcopy(values)
    missing.pop("direct_selected_thread_cpu")
    failures, _ = speed_evidence.validate_cell(
      "Windows missing selected route", missing, "x86_64-pc-windows-msvc"
    )
    self.assertTrue(any("lacks selected-native evidence" in item for item in failures))

    mislabeled = copy.deepcopy(values)
    mislabeled["direct_failure_fallback_thread_cpu"]["time_domain"] = "thread CPU"
    failures, _ = speed_evidence.validate_cell(
      "Windows mislabeled fallback", mislabeled, "x86_64-pc-windows-msvc"
    )
    self.assertTrue(any("fallback exact row is mislabeled" in item for item in failures))

  def test_extractor_consumes_fixed_windows_thread_cpu_rows(self) -> None:
    selection = windows_thread_cpu_selection()
    benchmarks = [
      *selection["eligible_direct_candidates"],
      selection["selected_native_benchmark"],
      selection["failure_fallback"]["exact_benchmark"],
    ]
    with tempfile.TemporaryDirectory() as directory:
      criterion = Path(directory)
      (criterion / "thread-cpu-selection.json").write_text(json.dumps(selection))
      for group in extract_speed.THREAD_CPU_GROUPS.values():
        for benchmark in benchmarks:
          write_criterion_estimate(criterion, group, benchmark)
      out = {
        "tach_thread_cpu": {
          "provider": "Windows GetThreadTimes",
          "read_cost": "system call",
          "time_domain": "thread CPU",
        }
      }
      extract_speed.add_thread_cpu_selector_evidence(criterion, out)

    candidate = selection["eligible_direct_candidates"][0]
    self.assertEqual(out[candidate]["provider"], selection["selected_mechanism"])
    self.assertEqual(out[candidate]["now"], 10.0)
    self.assertEqual(out[candidate]["elapsed"], 10.0)
    self.assertEqual(
      out["direct_selected_thread_cpu"]["benchmark"],
      selection["selected_native_benchmark"],
    )
    fallback = out["direct_failure_fallback_thread_cpu"]
    self.assertEqual(fallback["time_domain"], "monotonic wall fallback")
    self.assertFalse(fallback["eligible_for_thread_cpu_speed_claim"])
    self.assertEqual(fallback["benchmark"], selection["failure_fallback"]["exact_benchmark"])
    self.assertEqual(fallback["now"], 10.0)
    self.assertEqual(fallback["elapsed"], 10.0)

  def test_windows_thread_cpu_writer_removes_stale_evidence_before_native_guard(self) -> None:
    source = Path(__file__).with_name("instant.rs").read_text()
    start = source.index(
      '#[cfg(all(feature = "bench-internal", target_os = "windows"))]\n'
      "fn write_thread_cpu_selection()"
    )
    end = source.index(
      '#[cfg(all(not(feature = "bench-internal"), target_os = "windows"))]',
      start,
    )
    writer = source[start:end]
    remove_guard = writer.index("fs::remove_file(&selection_path)")
    provider_guard = writer.index("ThreadCpuInstant::provider()")
    write = writer.index("fs::write(")
    self.assertLess(remove_guard, provider_guard)
    self.assertLess(provider_guard, write)
    self.assertIn('"on_mismatch": "panic before thread-cpu-selection.json is written"', writer)

  def test_linux_perf_is_a_strict_thread_cpu_provider(self) -> None:
    failures, _ = speed_evidence.validate_cell(
      "synthetic", clocks("Linux perf task-clock mmap", "inline")
    )
    self.assertEqual(failures, [])

  def test_thread_cpu_provider_with_wrong_read_cost_is_rejected(self) -> None:
    failures, _ = speed_evidence.validate_cell(
      "synthetic", clocks("Linux perf task-clock mmap", "system call")
    )
    self.assertTrue(
      any("provider/read-cost pair is not a strict native provider" in item for item in failures)
    )

  def test_aarch32_perf_mmap_is_inline(self) -> None:
    target = "armv7-unknown-linux-gnueabihf"
    failures, _ = speed_evidence.validate_cell(
      "synthetic", clocks("Linux perf task-clock mmap", "inline"), target
    )
    self.assertEqual(failures, [])

    failures, _ = speed_evidence.validate_cell(
      "synthetic", clocks("Linux perf task-clock mmap", "system call"), target
    )
    self.assertTrue(
      any("provider/read-cost pair is not a strict native provider" in item for item in failures)
    )

  def test_thread_cpu_selector_reproduces_both_tournament_layers(self) -> None:
    selection = adaptive_thread_cpu_selection()
    failures: list[str] = []
    result = speed_evidence.validate_thread_cpu_selector(
      "synthetic", selection, failures
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["winner"], "linux_perf_mmap")
    self.assertEqual(
      result["selected_mechanism"], "linux_perf_mmap__x86_lfence_rdtsc_lfence"
    )

    tampered = copy.deepcopy(selection)
    tampered["selected_read_cost"] = "system call"
    failures = []
    speed_evidence.validate_thread_cpu_selector("tampered", tampered, failures)
    self.assertTrue(
      any("public/exact winner does not reproduce" in item for item in failures)
    )

    tampered = copy.deepcopy(selection)
    tampered["perf"]["mmap"]["counter_probe"]["selected_candidate"] = (
      "x86_cpuid_rdtsc_cpuid"
    )
    failures = []
    speed_evidence.validate_thread_cpu_selector("tampered", tampered, failures)
    self.assertTrue(any("perf counter winner does not reproduce" in item for item in failures))

  def test_thread_cpu_claim_requires_every_exact_row_and_public_parity(self) -> None:
    selection = adaptive_thread_cpu_selection()
    values = clocks("Linux perf task-clock mmap", "inline")
    values["tach_thread_cpu"]["selection"] = selection
    for benchmark in selection["eligible_direct_candidates"]:
      mechanism = benchmark.removeprefix("direct_thread_cpu__")
      values[benchmark] = {
        **estimate(10.0),
        "provider": mechanism,
        "read_cost": "inline" if mechanism.startswith("linux_perf_mmap__") else "system call",
        "time_domain": "thread CPU",
        "benchmark": benchmark,
      }
    values["direct_selected_thread_cpu"] = {
      **estimate(10.0),
      "provider": selection["selected_mechanism"],
      "read_cost": selection["selected_read_cost"],
      "time_domain": "thread CPU",
      "benchmark": selection["selected_native_benchmark"],
    }
    values["direct_fallback_thread_cpu"] = {
      **estimate(10.0),
      "provider": selection["fallback_mechanism"],
      "read_cost": selection["fallback_read_cost"],
      "time_domain": "thread CPU",
      "benchmark": selection["fallback_native_benchmark"],
    }
    failures, report = speed_evidence.validate_cell(
      "complete route", values, "x86_64-unknown-linux-gnu"
    )
    self.assertEqual(failures, [])
    self.assertTrue(report["selected_thread_cpu_provider_parity"]["passed"])
    self.assertTrue(
      report["selected_thread_cpu_provider_parity"]["measured_runner_up"]["passed"]
    )

    missing = copy.deepcopy(values)
    missing.pop(selection["eligible_direct_candidates"][-1])
    failures, _ = speed_evidence.validate_cell(
      "missing route", missing, "x86_64-unknown-linux-gnu"
    )
    self.assertTrue(any("lacks exact row" in failure for failure in failures))

    faster = copy.deepcopy(values)
    fastest_candidate = selection["eligible_direct_candidates"][0]
    faster[fastest_candidate].update(estimate(1.0))
    failures, report = speed_evidence.validate_cell(
      "mis-selected route", faster, "x86_64-unknown-linux-gnu"
    )
    self.assertTrue(
      any(
        f"eligible exact route {fastest_candidate}" in failure
        for failure in failures
      )
    )
    self.assertFalse(report["selected_thread_cpu_provider_parity"]["passed"])

  def test_aarch32_selector_reproduces_the_inline_perf_cost(self) -> None:
    selection = adaptive_thread_cpu_selection()
    failures: list[str] = []
    result = speed_evidence.validate_thread_cpu_selector(
      "synthetic", selection, failures, "armv7-unknown-linux-gnueabihf"
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["selected_read_cost"], "inline")

  def test_capable_but_unprofitable_perf_selects_native(self) -> None:
    selection = adaptive_thread_cpu_selection()
    selection["perf"]["path_probe"]["candidate_batches_ns"][1] = [99_000] * 9
    selection["perf"]["path_probe"]["candidate_batches_ns"][2] = [99_000] * 9
    selection["perf"]["path_probe"]["selected_candidate"] = "posix_thread_cpu"
    selection["perf"]["path_probe"]["fallback_candidate"] = "linux_perf_mmap"
    selection["perf"]["path_probe"]["capability_was_not_profitable"] = True
    selection["selected_provider"] = "posix_thread_cpu_clock"
    selection["selected_read_cost"] = "system call"
    selection["selected_mechanism"] = "raw_entry"
    selection["selected_native_benchmark"] = "direct_selected_thread_cpu__raw_entry"
    selection["fallback_provider"] = "linux_perf_mmap"
    selection["fallback_read_cost"] = "inline"
    selection["fallback_mechanism"] = "linux_perf_mmap__x86_lfence_rdtsc_lfence"
    selection["fallback_native_benchmark"] = (
      "direct_fallback_thread_cpu__linux_perf_mmap__x86_lfence_rdtsc_lfence"
    )
    failures: list[str] = []
    result = speed_evidence.validate_thread_cpu_selector(
      "synthetic", selection, failures
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["winner"], "posix_thread_cpu_clock")
    self.assertEqual(result["perf_path"]["fallback"]["winner"], "linux_perf_mmap")

  def test_capability_bit_does_not_override_measured_perf_read_win(self) -> None:
    selection = adaptive_thread_cpu_selection()
    path = selection["perf"]["path_probe"]
    path["candidate_batches_ns"] = [
      [100_000] * 9,
      [270_000] * 9,
      [90_000] * 9,
    ]
    path["selected_candidate"] = "linux_perf_read"
    path["fallback_candidate"] = "posix_thread_cpu"
    path["capability_was_not_profitable"] = True
    selection.update({
      "selected_provider": "linux_perf_read",
      "selected_read_cost": "system call",
      "selected_mechanism": "linux_perf_read__raw_read_syscall",
      "selected_native_benchmark": (
        "direct_selected_thread_cpu__linux_perf_read__raw_read_syscall"
      ),
      "fallback_provider": "posix_thread_cpu_clock",
      "fallback_read_cost": "system call",
      "fallback_mechanism": "raw_entry",
      "fallback_native_benchmark": "direct_fallback_thread_cpu__raw_entry",
    })
    failures: list[str] = []
    result = speed_evidence.validate_thread_cpu_selector(
      "virtualized capability loss", selection, failures, "x86_64-unknown-linux-gnu"
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["winner"], "linux_perf_read")
    self.assertEqual(result["perf_path"]["fallback"]["winner"], "posix_thread_cpu")

  def test_measured_thread_cpu_fallback_must_reproduce(self) -> None:
    selection = adaptive_thread_cpu_selection()
    selection["perf"]["path_probe"]["fallback_candidate"] = "posix_thread_cpu"
    failures: list[str] = []
    speed_evidence.validate_thread_cpu_selector("tampered fallback", selection, failures)
    self.assertTrue(any("measured fallback does not reproduce" in item for item in failures))

  def test_i686_perf_read_entry_tournament_reproduces(self) -> None:
    selection = adaptive_thread_cpu_selection()
    names = ["raw_read_int80", "libc_syscall_read", "libc_read"]
    probe = selection["perf"]["read"]["entry_probe"]
    probe.update({
      "selection_kind": "tournament",
      "candidate_names": names,
      "candidate_eligible": [True, True, True],
      "candidate_measured": [True, True, True],
      "candidate_batches_ns": [[120_000] * 9, [90_000] * 9, [100_000] * 9],
      "selected_candidate": "libc_syscall_read",
    })
    read_candidates = [
      f"direct_thread_cpu__linux_perf_read__{name}" for name in names
    ]
    read_mechanism = "linux_perf_read__libc_syscall_read"
    selection["perf"]["read"].update({
      "selected_mechanism": read_mechanism,
      "selected_candidate_benchmark": f"direct_thread_cpu__{read_mechanism}",
      "eligible_benchmarks": read_candidates,
    })
    selection["eligible_direct_candidates"] = [
      *selection["eligible_direct_candidates"][:4],
      *read_candidates,
    ]
    selection["fallback_mechanism"] = read_mechanism
    selection["fallback_native_benchmark"] = f"direct_fallback_thread_cpu__{read_mechanism}"
    failures: list[str] = []
    result = speed_evidence.validate_thread_cpu_selector(
      "i686 read ABI", selection, failures, "i686-unknown-linux-gnu"
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["perf_read_entry"]["winner"], "libc_syscall_read")

  def test_power_perf_read_entry_tournament_reproduces(self) -> None:
    selection = adaptive_thread_cpu_selection()
    names = ["raw_read_sc", "raw_read_scv", "libc_read"]
    probe = selection["perf"]["read"]["entry_probe"]
    probe.update({
      "selection_kind": "tournament",
      "candidate_names": names,
      "candidate_eligible": [True, True, True],
      "candidate_measured": [True, True, True],
      "candidate_batches_ns": [[120_000] * 9, [90_000] * 9, [100_000] * 9],
      "selected_candidate": "raw_read_scv",
    })
    read_candidates = [
      f"direct_thread_cpu__linux_perf_read__{name}" for name in names
    ]
    read_mechanism = "linux_perf_read__raw_read_scv"
    selection["perf"]["read"].update({
      "selected_mechanism": read_mechanism,
      "selected_candidate_benchmark": f"direct_thread_cpu__{read_mechanism}",
      "eligible_benchmarks": read_candidates,
    })
    selection["perf"]["path_probe"].update({
      "candidate_eligible": [True, False, True],
      "candidate_batches_ns": [[100_000] * 9, [0] * 9, [90_000] * 9],
      "selected_candidate": "linux_perf_read",
      "fallback_candidate": "posix_thread_cpu",
      "capability_was_not_profitable": False,
    })
    selection["perf"]["mmap"].update({
      "supported_on_target": False,
      "available": False,
      "selected_mechanism": None,
      "selected_candidate_benchmark": None,
      "eligible_benchmarks": [],
      "counter_probe": None,
    })
    selection["eligible_direct_candidates"] = [
      *selection["eligible_direct_candidates"][:2],
      *read_candidates,
    ]
    selection.update({
      "selected_provider": "linux_perf_read",
      "selected_read_cost": "system call",
      "selected_mechanism": read_mechanism,
      "selected_native_benchmark": f"direct_selected_thread_cpu__{read_mechanism}",
      "fallback_provider": "posix_thread_cpu_clock",
      "fallback_read_cost": "system call",
      "fallback_mechanism": "raw_entry",
      "fallback_native_benchmark": "direct_fallback_thread_cpu__raw_entry",
    })
    failures: list[str] = []
    result = speed_evidence.validate_thread_cpu_selector(
      "power read ABI", selection, failures, "powerpc64-unknown-linux-gnu"
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["perf_read_entry"]["winner"], "raw_read_scv")

  def test_fixed_perf_counter_has_no_invented_inner_tournament(self) -> None:
    selection = adaptive_thread_cpu_selection()
    mechanism = "linux_perf_mmap__riscv_fence_rdtime_fence"
    candidate = f"direct_thread_cpu__{mechanism}"
    selection["selected_mechanism"] = mechanism
    selection["selected_native_benchmark"] = f"direct_selected_thread_cpu__{mechanism}"
    selection["eligible_direct_candidates"] = [
      *selection["eligible_direct_candidates"][:2],
      candidate,
      *selection["perf"]["read"]["eligible_benchmarks"],
    ]
    selection["perf"]["mmap"]["selected_mechanism"] = mechanism
    selection["perf"]["mmap"]["selected_candidate_benchmark"] = candidate
    selection["perf"]["mmap"]["eligible_benchmarks"] = [candidate]
    selection["perf"]["mmap"]["counter_probe"] = {
      "candidate_names": ["riscv_fence_rdtime_fence"],
      "candidate_eligible": [True],
      "candidate_batches_ns": None,
      "selected_candidate": "riscv_fence_rdtime_fence",
      "selection_kind": "fixed_candidate",
      "reads_per_batch": None,
      "required_decisive_wins": None,
    }
    failures: list[str] = []
    result = speed_evidence.validate_thread_cpu_selector(
      "synthetic", selection, failures
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["perf_counter"]["selection_kind"], "fixed_candidate")
    self.assertEqual(result["perf_counter"]["decisions"], [])

  def test_generic_native_entry_tournament_reproduces_exact_candidates(self) -> None:
    selection = adaptive_thread_cpu_selection()
    native_candidates = ["libc_entry", "time32_entry", "time64_entry"]
    selection["native_entry_probe"] = {
      "selection_kind": "tournament",
      "candidate_names": native_candidates,
      "candidate_eligible": [True, True, True],
      "candidate_measured": [True, True, True],
      "candidate_batches_ns": [[100_000] * 9, [90_000] * 9, [85_000] * 9],
      "selected_candidate": "time64_entry",
      "reads_per_batch": 4_096,
      "required_decisive_wins": 8,
      "equivalence_band": {"floor_ns_per_read": 1, "relative_denominator": 20},
    }
    selection["eligible_direct_candidates"] = [
      *(f"direct_thread_cpu__{name}" for name in native_candidates),
      *selection["perf"]["mmap"]["eligible_benchmarks"],
      *selection["perf"]["read"]["eligible_benchmarks"],
    ]
    failures: list[str] = []
    result = speed_evidence.validate_thread_cpu_selector(
      "synthetic", selection, failures
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["native_entry"]["winner"], "time64_entry")
    self.assertEqual(len(result["native_entry"]["decisions"]), 2)

  def test_single_available_native_entry_has_no_invented_samples(self) -> None:
    selection = adaptive_thread_cpu_selection()
    selection["native_entry_probe"] = {
      "selection_kind": "fixed_candidate",
      "candidate_names": ["time32_entry"],
      "candidate_eligible": [True],
      "candidate_measured": [True],
      "candidate_batches_ns": None,
      "selected_candidate": "time32_entry",
      "reads_per_batch": 4_096,
      "required_decisive_wins": 8,
      "equivalence_band": {"floor_ns_per_read": 1, "relative_denominator": 20},
    }
    selection["eligible_direct_candidates"] = [
      "direct_thread_cpu__time32_entry",
      *selection["perf"]["mmap"]["eligible_benchmarks"],
      *selection["perf"]["read"]["eligible_benchmarks"],
    ]
    failures: list[str] = []
    result = speed_evidence.validate_thread_cpu_selector(
      "synthetic", selection, failures
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["native_entry"]["winner"], "time32_entry")

  def test_unavailable_perf_has_no_stale_probe_evidence(self) -> None:
    selection = adaptive_thread_cpu_selection()
    selection["selected_provider"] = "posix_thread_cpu_clock"
    selection["selected_read_cost"] = "system call"
    selection["selected_mechanism"] = "raw_entry"
    selection["selected_native_benchmark"] = "direct_selected_thread_cpu__raw_entry"
    selection["fallback_provider"] = None
    selection["fallback_read_cost"] = None
    selection["fallback_mechanism"] = None
    selection["fallback_native_benchmark"] = None
    selection["eligible_direct_candidates"] = selection["eligible_direct_candidates"][:2]
    selection["perf"].update({
      "event_available": False,
      "path_probe": None,
    })
    selection["perf"]["mmap"].update({
      "available": False,
      "selected_mechanism": None,
      "selected_candidate_benchmark": None,
      "eligible_benchmarks": [],
      "counter_probe": None,
    })
    selection["perf"]["read"].update({
      "available": False,
      "selected_mechanism": None,
      "selected_candidate_benchmark": None,
      "eligible_benchmarks": [],
      "entry_probe": None,
    })
    failures: list[str] = []
    result = speed_evidence.validate_thread_cpu_selector(
      "synthetic", selection, failures
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["winner"], "posix_thread_cpu_clock")

  def test_extractor_preserves_public_and_exact_thread_cpu_identities(self) -> None:
    selection = adaptive_thread_cpu_selection()
    with tempfile.TemporaryDirectory() as directory:
      criterion = Path(directory)
      (criterion / "thread-cpu-selection.json").write_text(json.dumps(selection))
      benchmarks = [
        *selection["eligible_direct_candidates"],
        selection["selected_native_benchmark"],
        selection["fallback_native_benchmark"],
      ]
      for group in extract_speed.THREAD_CPU_GROUPS.values():
        for benchmark in benchmarks:
          write_criterion_estimate(criterion, group, benchmark)
      out = {
        "tach_thread_cpu": {
          "provider": "Linux perf task-clock mmap",
          "read_cost": "inline",
        }
      }
      extract_speed.add_thread_cpu_selector_evidence(criterion, out)

    self.assertEqual(
      out["direct_selected_thread_cpu"]["provider"],
      "linux_perf_mmap__x86_lfence_rdtsc_lfence",
    )
    self.assertEqual(out["direct_selected_thread_cpu"]["read_cost"], "inline")
    self.assertEqual(
      out["direct_thread_cpu__linux_perf_mmap__x86_cpuid_rdtsc_cpuid"]["read_cost"],
      "inline",
    )
    self.assertEqual(out["direct_thread_cpu__raw_entry"]["read_cost"], "system call")
    self.assertEqual(
      out["direct_fallback_thread_cpu"]["provider"],
      "linux_perf_read__raw_read_syscall",
    )

  def test_extractor_preserves_system_call_perf_cost_for_every_exact_row(self) -> None:
    selection = adaptive_thread_cpu_selection()
    selection["selected_read_cost"] = "system call"
    selection["perf"]["mmap"]["read_cost"] = "system call"
    with tempfile.TemporaryDirectory() as directory:
      criterion = Path(directory)
      (criterion / "thread-cpu-selection.json").write_text(json.dumps(selection))
      benchmarks = [
        *selection["eligible_direct_candidates"],
        selection["selected_native_benchmark"],
        selection["fallback_native_benchmark"],
      ]
      for group in extract_speed.THREAD_CPU_GROUPS.values():
        for benchmark in benchmarks:
          write_criterion_estimate(criterion, group, benchmark)
      out = {
        "tach_thread_cpu": {
          "provider": "Linux perf task-clock mmap",
          "read_cost": "system call",
        }
      }
      extract_speed.add_thread_cpu_selector_evidence(criterion, out)

    self.assertEqual(out["direct_selected_thread_cpu"]["read_cost"], "system call")
    self.assertEqual(
      out["direct_thread_cpu__linux_perf_mmap__x86_cpuid_rdtsc_cpuid"]["read_cost"],
      "system call",
    )

  def test_extractor_consumes_generic_i686_native_candidates(self) -> None:
    selection = adaptive_thread_cpu_selection()
    selection.update({
      "selected_provider": "posix_thread_cpu_clock",
      "selected_read_cost": "system call",
      "selected_mechanism": "time32_entry",
      "selected_native_benchmark": "direct_selected_thread_cpu__time32_entry",
      "fallback_provider": None,
      "fallback_read_cost": None,
      "fallback_mechanism": None,
      "fallback_native_benchmark": None,
      "eligible_direct_candidates": ["direct_thread_cpu__time32_entry"],
      "native_entry_probe": {
        "selection_kind": "fixed_candidate",
        "candidate_names": ["time32_entry"],
        "candidate_eligible": [True],
        "candidate_measured": [True],
        "candidate_batches_ns": None,
        "selected_candidate": "time32_entry",
        "reads_per_batch": 4_096,
        "required_decisive_wins": 8,
        "equivalence_band": {"floor_ns_per_read": 1, "relative_denominator": 20},
      },
      "perf": {
        "event_available": False,
        "path_probe": None,
        "mmap": {
          "supported_on_target": True,
          "available": False,
          "read_cost": "inline",
          "selected_mechanism": None,
          "selected_candidate_benchmark": None,
          "eligible_benchmarks": [],
          "counter_probe": None,
        },
        "read": {
          "supported_on_target": True,
          "available": False,
          "read_cost": "system call",
          "selected_mechanism": None,
          "selected_candidate_benchmark": None,
          "eligible_benchmarks": [],
          "entry_probe": None,
        },
        "measurement_clock": (
          "raw SYS_clock_gettime(CLOCK_MONOTONIC_RAW), never libc/vDSO or the "
          "candidate under test"
        ),
        "decision_rule": "synthetic material-win tournament",
      },
    })
    with tempfile.TemporaryDirectory() as directory:
      criterion = Path(directory)
      (criterion / "thread-cpu-selection.json").write_text(json.dumps(selection))
      benchmarks = [
        *selection["eligible_direct_candidates"],
        selection["selected_native_benchmark"],
      ]
      for group in extract_speed.THREAD_CPU_GROUPS.values():
        for benchmark in benchmarks:
          write_criterion_estimate(criterion, group, benchmark)
      out = {
        "tach_thread_cpu": {
          "provider": "POSIX thread CPU clock",
          "read_cost": "system call",
        }
      }
      extract_speed.add_thread_cpu_selector_evidence(criterion, out)
    self.assertEqual(out["direct_selected_thread_cpu"]["provider"], "time32_entry")
    self.assertEqual(out["direct_selected_thread_cpu"]["read_cost"], "system call")

  def test_windows_serialize_candidate_reproduces(self) -> None:
    selection = {
      "selected_provider": {
        "instant": "windows_qpc",
        "ordered": "windows_qpc_x86_serialize",
      },
      "eligible_direct_candidates": {
        "instant": ["direct_wall__windows_qpc"],
        "ordered": [
          "direct_ordered_wall__windows_qpc_x86_cpuid",
          "direct_ordered_wall__windows_qpc_x86_serialize",
        ],
      },
      "ineligible_direct_candidates": {
        "windows_raw_tsc": {
          "contracts": ["instant", "ordered"],
          "eligibility": "ineligible",
          "reason": "local TSC evidence cannot establish the Windows QPC contract",
          "authority": speed_evidence.WINDOWS_QPC_AUTHORITY,
        },
      },
      "ordered_probe": {
        "reads_per_batch": 4_096,
        "required_decisive_wins": 8,
        "cpuid_batches_ns": [100_000] * 9,
        "lfence_eligible": False,
        "rdtscp_eligible": False,
        "mfence_eligible": False,
        "serialize_eligible": True,
        "serialize_batches_ns": [90_000] * 9,
        "selected_provider": "windows_qpc_x86_serialize",
      },
    }
    failures: list[str] = []
    result = speed_evidence.validate_windows_wall_selector(
      "synthetic", selection, failures
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["winner"], "windows_qpc_x86_serialize")
    self.assertIn("windows_raw_tsc", result["ineligible_direct_candidates"])

    tampered = copy.deepcopy(selection)
    del tampered["ineligible_direct_candidates"]["windows_raw_tsc"]["reason"]
    failures = []
    speed_evidence.validate_windows_wall_selector("tampered", tampered, failures)
    self.assertTrue(any("exclusion lacks a reason" in item for item in failures))

  def test_linux_x86_wall_selector_replays_metric_paired_parity(self) -> None:
    selection = linux_x86_wall_selection()
    failures: list[str] = []
    result = speed_evidence.validate_linux_x86_wall_selector(
      "synthetic", selection, failures
    )
    self.assertEqual(failures, [])
    for domain in ("instant", "ordered"):
      self.assertEqual(result[domain]["winner"], selection["selected_provider"][domain])
      self.assertTrue(result[domain]["public_exact"]["now"]["passed"])
      self.assertTrue(result[domain]["public_exact"]["elapsed"]["passed"])

    missing = copy.deepcopy(selection)
    del missing["public_exact_probe"]["ordered"]["elapsed"]
    failures = []
    speed_evidence.validate_linux_x86_wall_selector("missing", missing, failures)
    self.assertTrue(any("ordered elapsed lacks paired" in item for item in failures))

    asymmetric = copy.deepcopy(selection)
    del asymmetric["public_exact_probe"]["ordered"]["elapsed"]["call_boundary"]
    failures = []
    speed_evidence.validate_linux_x86_wall_selector(
      "asymmetric", asymmetric, failures
    )
    self.assertTrue(
      any("malformed Linux x86 ordered elapsed paired" in item for item in failures)
    )

    slower = copy.deepcopy(selection)
    slower["public_exact_probe"]["ordered"]["elapsed"]["public_batches_ns"] = [
      500_000
    ] * 9
    failures = []
    result = speed_evidence.validate_linux_x86_wall_selector(
      "slower", slower, failures
    )
    self.assertFalse(result["ordered"]["public_exact"]["elapsed"]["passed"])
    self.assertTrue(any("public read is repeatably slower" in item for item in failures))

  def test_linux_x86_route_gates_prefer_retained_pairs_to_independent_ci_edges(self) -> None:
    document = copy.deepcopy(
      supplemental_speed_documents()[
        "speed-supplemental-linux-x86_64-no-default.json"
      ]
    )
    values = document["clocks"]
    selection = linux_x86_wall_selection()
    values["tach"]["selection"] = copy.deepcopy(selection)
    values["tach_ordered"]["wall_selection"] = copy.deepcopy(selection)
    for domain, prefix, selected_key in (
      ("instant", "direct_wall__", "direct_selected_wall"),
      ("ordered", "direct_ordered_wall__", "direct_selected_ordered_wall"),
    ):
      for benchmark in selection["eligible_direct_candidates"][domain]:
        values[benchmark] = {
          **estimate(10.0),
          "provider": benchmark.removeprefix(prefix),
          "read_cost": "inline",
          "time_domain": f"{domain} wall",
          "benchmark": benchmark,
        }
      values[selected_key] = {
        **estimate(10.0),
        "provider": selection["selected_provider"][domain],
        "read_cost": "inline",
        "time_domain": f"{domain} wall",
        "benchmark": selection["selected_native_benchmark"][domain],
      }
      document["route_coverage"][domain]["eligible_exact_rows"] = copy.deepcopy(
        selection["eligible_direct_candidates"][domain]
      )

    values["tach_ordered"].update({
      "elapsed": 20.0,
      "elapsed_ci95": [20.0, 20.0],
    })
    values["std"] = estimate(30.0)
    failures: list[str] = []
    result = speed_evidence.validate_supplemental_route_coverage(
      "synthetic", document, failures
    )
    self.assertEqual(failures, [])
    selected = selection["eligible_direct_candidates"]["ordered"][0]
    elapsed = result["ordered"]["eligible_exact_routes"][selected]["metrics"][
      "elapsed"
    ]
    self.assertTrue(elapsed["passed"])
    self.assertEqual(
      elapsed["comparison_basis"], "alternating paired public/exact probe"
    )
    primary_values = copy.deepcopy(values)
    primary_values["tach_thread_cpu"].pop("selection", None)
    primary_failures, primary = speed_evidence.validate_cell(
      "synthetic primary", primary_values, "x86_64-unknown-linux-gnu"
    )
    self.assertEqual(primary_failures, [])
    self.assertEqual(
      primary["selected_wall_provider_parity"]["ordered"]["metrics"]["elapsed"][
        "comparison_basis"
      ],
      "alternating paired public/exact probe",
    )

  def test_residual_loongarch_selector_reproduces_and_rejects_tampering(self) -> None:
    selection = loongarch_wall_selection()
    failures: list[str] = []
    result = speed_evidence.validate_residual_wall_selector(
      "synthetic", selection, residual_rows(selection), failures
    )
    self.assertEqual(failures, [])
    self.assertEqual(result["instant"]["winner"], "linux_clock_monotonic_syscall")
    self.assertEqual(result["ordered"]["winner"], "linux_clock_monotonic_syscall")

    tampered = copy.deepcopy(selection)
    tampered["probe"]["instant"]["fallback_decisive_wins"] = 0
    failures = []
    speed_evidence.validate_residual_wall_selector(
      "tampered", tampered, residual_rows(tampered), failures
    )
    self.assertTrue(any("does not reproduce" in item for item in failures))

  def test_public_wall_clock_must_match_every_eligible_exact_route(self) -> None:
    selection = loongarch_wall_selection()
    values = clocks()
    values["tach"]["selection"] = selection
    values.update(residual_rows(selection))
    for domain, key in (
      ("instant", "direct_selected_wall"),
      ("ordered", "direct_selected_ordered_wall"),
    ):
      values[key] = {
        **estimate(10.0),
        "provider": selection["selected_provider"][domain],
        "read_cost": "system call",
        "time_domain": f"{domain} wall",
        "benchmark": selection["selected_native_benchmark"][domain],
      }

    fastest_candidate = selection["eligible_direct_candidates"]["instant"][0]
    values[fastest_candidate].update(estimate(1.0))
    failures, report = speed_evidence.validate_cell(
      "mis-selected wall route", values, "loongarch64-unknown-linux-gnu"
    )
    self.assertTrue(
      any(
        f"eligible exact route {fastest_candidate}" in failure
        for failure in failures
      )
    )
    self.assertFalse(report["selected_wall_provider_parity"]["instant"]["passed"])

  def test_wall_candidate_parity_rejects_every_identity_field_tamper(self) -> None:
    selection = loongarch_wall_selection()
    values = clocks()
    values["tach"]["selection"] = selection
    values.update(residual_rows(selection))
    for domain, key in (
      ("instant", "direct_selected_wall"),
      ("ordered", "direct_selected_ordered_wall"),
    ):
      values[key] = {
        **estimate(10.0),
        "provider": selection["selected_provider"][domain],
        "read_cost": "system call",
        "time_domain": f"{domain} wall",
        "benchmark": selection["selected_native_benchmark"][domain],
      }

    candidate = selection["eligible_direct_candidates"]["instant"][0]
    for field, invalid in (
      ("benchmark", "direct_wall__different_route"),
      ("provider", "different_route"),
      ("time_domain", "ordered wall"),
      ("read_cost", "system call"),
    ):
      with self.subTest(field=field):
        tampered = copy.deepcopy(values)
        tampered[candidate][field] = invalid
        failures, report = speed_evidence.validate_cell(
          f"tampered {field}", tampered, "loongarch64-unknown-linux-gnu"
        )
        self.assertTrue(
          any("identity does not match its key" in failure for failure in failures)
        )
        candidate_report = report["selected_wall_provider_parity"]["instant"][
          "eligible_candidates"
        ][candidate]
        self.assertFalse(candidate_report["identity_passed"])
        self.assertFalse(candidate_report["passed"])

  def test_freebsd_ordered_selector_preserves_complete_barrier_identity(self) -> None:
    selection = freebsd_wall_selection()
    failures: list[str] = []
    result = speed_evidence.validate_residual_wall_selector(
      "synthetic", selection, residual_rows(selection), failures
    )
    self.assertEqual(failures, [])
    self.assertEqual(
      result["ordered"]["winner"],
      "freebsd_clock_monotonic_x86_lfence",
    )

    tampered = copy.deepcopy(selection)
    tampered["selected_provider"]["ordered"] = "freebsd_clock_monotonic"
    failures = []
    speed_evidence.validate_residual_wall_selector(
      "tampered", tampered, residual_rows(tampered), failures
    )
    self.assertTrue(any("winner is not an eligible candidate" in item for item in failures))

  def test_residual_extractor_labels_direct_vdso_and_requires_elapsed_rows(self) -> None:
    selection = {
      "architecture": "loongarch64-linux",
      "selected_provider": {
        "instant": "linux_clock_monotonic_vdso_direct",
        "ordered": "linux_clock_monotonic_raw_vdso_direct",
      },
      "eligible_direct_candidates": {
        "instant": ["direct_wall__linux_clock_monotonic_vdso_direct"],
        "ordered": [
          "direct_ordered_wall__linux_clock_monotonic_raw_vdso_direct"
        ],
      },
    }
    with tempfile.TemporaryDirectory() as directory:
      criterion = Path(directory)
      (criterion / "residual-wall-selection.json").write_text(json.dumps(selection))
      for group in extract_speed.WALL_GROUPS.values():
        for benchmark in (
          "direct_wall__linux_clock_monotonic_vdso_direct",
          "direct_ordered_wall__linux_clock_monotonic_raw_vdso_direct",
        ):
          write_criterion_estimate(criterion, group, benchmark)
      out = {"tach": {}, "tach_ordered": {}}
      extract_speed.add_wall_selector_evidence(criterion, out)
    self.assertEqual(
      out["direct_wall__linux_clock_monotonic_vdso_direct"]["read_cost"],
      "direct vDSO call",
    )
    self.assertEqual(
      out["direct_wall__linux_clock_monotonic_vdso_direct"]["benchmark"],
      "direct_wall__linux_clock_monotonic_vdso_direct",
    )
    self.assertIn(
      "elapsed",
      out["direct_ordered_wall__linux_clock_monotonic_raw_vdso_direct"],
    )
    self.assertEqual(
      out["direct_ordered_wall__linux_clock_monotonic_raw_vdso_direct"]["benchmark"],
      "direct_ordered_wall__linux_clock_monotonic_raw_vdso_direct",
    )

  def test_checkout_binding_checks_commit_tree_and_worktree(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      git(root, "init", "-q")
      git(root, "config", "user.name", "tach test")
      git(root, "config", "user.email", "tach-test@example.invalid")
      (root / "src").mkdir()
      (root / "src/lib.rs").write_text("pub fn measured() {}\n")
      git(root, "add", "src/lib.rs")
      git(root, "commit", "-qm", "measured source")
      revision = git(root, "rev-parse", "HEAD")

      (root / "evidence.json").write_text("{}\n")
      git(root, "add", "evidence.json")
      git(root, "commit", "-qm", "evidence only")
      failures, binding = speed_evidence.validate_checkout_binding(root, revision)
      self.assertEqual(failures, [])
      self.assertTrue(binding["committed_tree_inputs_unchanged"])
      self.assertTrue(binding["working_tree_inputs_clean"])

      (root / "src/lib.rs").write_text("pub fn changed() {}\n")
      failures, binding = speed_evidence.validate_checkout_binding(root, revision)
      self.assertTrue(any("local benchmark inputs differ from HEAD" in item for item in failures))
      self.assertFalse(binding["working_tree_inputs_clean"])

      git(root, "add", "src/lib.rs")
      git(root, "commit", "-qm", "changed source")
      failures, binding = speed_evidence.validate_checkout_binding(root, revision)
      self.assertTrue(any("committed benchmark inputs changed" in item for item in failures))
      self.assertFalse(binding["committed_tree_inputs_unchanged"])

  def test_checkout_binding_rejects_a_git_replace_substituted_revision(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      git(root, "init", "-q")
      git(root, "config", "user.name", "tach test")
      git(root, "config", "user.email", "tach-test@example.invalid")
      (root / "src").mkdir()
      (root / "src/lib.rs").write_text("pub fn measured() {}\n")
      git(root, "add", "src/lib.rs")
      git(root, "commit", "-qm", "measured source")
      revision = git(root, "rev-parse", "HEAD")

      (root / "src/lib.rs").write_text("pub fn substituted() {}\n")
      git(root, "add", "src/lib.rs")
      git(root, "commit", "-qm", "substituted source")
      forged = git(root, "rev-parse", "HEAD")
      git(root, "replace", revision, forged)

      # Ordinary Git revision lookup now substitutes F for R. The campaign
      # validator must instead compare R's actual committed inputs.
      self.assertEqual(
        git(root, "diff", "--name-only", revision, "HEAD", "--", "src"),
        "",
      )
      failures, binding = speed_evidence.validate_checkout_binding(root, revision)
      self.assertTrue(any(
        "committed benchmark inputs changed" in failure for failure in failures
      ))
      self.assertFalse(binding["committed_tree_inputs_unchanged"])

  def test_campaign_revision_is_not_self_pinned(self) -> None:
    self.assertFalse(hasattr(speed_evidence, "CAMPAIGN_SOURCE_REVISION"))
    self.assertEqual(
      speed_evidence.BENCHMARK_FEATURES,
      ("bench-internal", "thread-cpu-inline"),
    )


if __name__ == "__main__":
  if sys.argv[1:] == ["--route-coverage-admission"]:
    failures = route_coverage_admission_failures()
    if failures:
      print("route coverage admission rejected:", file=sys.stderr)
      for failure in failures:
        print(f"- {failure}", file=sys.stderr)
      raise SystemExit(1)
    print("route coverage admission passed")
    raise SystemExit(0)
  unittest.main()
