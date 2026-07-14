#!/usr/bin/env python3

from __future__ import annotations

import ast
import base64
import hashlib
import importlib.util
import inspect
import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from pathlib import Path

import extract_speed


BENCHES_DIR = Path(__file__).resolve().parent
COLLECTOR_SCRIPT = BENCHES_DIR / "collect-speed-bundle.py"
HOST_COLLECTOR_SCRIPT = BENCHES_DIR / "collect-host-speed-bundle.py"
HOST_EXTRACTOR_SCRIPT = BENCHES_DIR / "host_speed.py"
EXTRACTOR_SCRIPT = BENCHES_DIR / "extract_speed.py"
SEALER_SCRIPT = BENCHES_DIR / "seal-speed-source.py"


def load_script_module(name: str, path: Path):
    specification = importlib.util.spec_from_file_location(name, path)
    assert specification is not None and specification.loader is not None
    module = importlib.util.module_from_spec(specification)
    sys.modules[name] = module
    specification.loader.exec_module(module)
    return module


seal_speed_source = load_script_module("tach_test_seal_speed_source", SEALER_SCRIPT)
collect_speed_bundle = load_script_module(
    "tach_test_collect_speed_bundle",
    COLLECTOR_SCRIPT,
)
host_speed = load_script_module("tach_test_host_speed", HOST_EXTRACTOR_SCRIPT)
collect_host_speed_bundle = load_script_module(
    "tach_test_collect_host_speed_bundle",
    HOST_COLLECTOR_SCRIPT,
)


class RemoteEvidencePythonCompatibilityTests(unittest.TestCase):
    def test_aws_seal_and_collect_import_graph_supports_python_3_9(self) -> None:
        modules = (
            (EXTRACTOR_SCRIPT, extract_speed),
            (SEALER_SCRIPT, seal_speed_source),
            (COLLECTOR_SCRIPT, collect_speed_bundle),
            (HOST_EXTRACTOR_SCRIPT, host_speed),
            (HOST_COLLECTOR_SCRIPT, collect_host_speed_bundle),
        )
        for path, module in modules:
            with self.subTest(module=path.name):
                source = path.read_text(encoding="utf-8")
                tree = ast.parse(source, filename=str(path), feature_version=(3, 9))
                future_imports = {
                    alias.name
                    for node in tree.body
                    if isinstance(node, ast.ImportFrom) and node.module == "__future__"
                    for alias in node.names
                }
                self.assertIn("annotations", future_imports)

                annotations = [
                    annotation
                    for value in vars(module).values()
                    if inspect.isfunction(value) and value.__module__ == module.__name__
                    for annotation in value.__annotations__.values()
                ]
                self.assertTrue(annotations)
                self.assertTrue(
                    all(isinstance(annotation, str) for annotation in annotations)
                )


class SourceSealSnapshotTests(unittest.TestCase):
    def stat_value(self, *, inode: int, size: int = 10, mtime_ns: int = 20):
        value = mock.Mock()
        value.st_mode = stat.S_IFREG | 0o600
        value.st_dev = 1
        value.st_ino = inode
        value.st_nlink = 1
        value.st_size = size
        value.st_mtime_ns = mtime_ns
        value.st_ctime_ns = 30
        return value

    def test_windows_accepts_path_descriptor_identity_representation_difference(self) -> None:
        initial = self.stat_value(inode=0)
        opened = self.stat_value(inode=42)
        for module in (seal_speed_source, collect_speed_bundle):
            with self.subTest(module=module.__name__):
                with mock.patch.object(module.os, "name", "nt"):
                    self.assertTrue(module._path_matches_opened_file(initial, opened))

    def test_windows_still_rejects_content_metadata_change(self) -> None:
        initial = self.stat_value(inode=0)
        opened = self.stat_value(inode=42, size=11)
        for module in (seal_speed_source, collect_speed_bundle):
            with self.subTest(module=module.__name__):
                with mock.patch.object(module.os, "name", "nt"):
                    self.assertFalse(module._path_matches_opened_file(initial, opened))

    def test_posix_requires_full_path_descriptor_identity(self) -> None:
        initial = self.stat_value(inode=1)
        opened = self.stat_value(inode=2)
        for module in (seal_speed_source, collect_speed_bundle):
            with self.subTest(module=module.__name__):
                with mock.patch.object(module.os, "name", "posix"):
                    self.assertFalse(module._path_matches_opened_file(initial, opened))


def write_benchmark(
    criterion: Path,
    group_dir: str,
    group_id: str,
    function_id: str,
    directory_name: str,
    point: float = 7.25,
) -> Path:
    output = criterion / group_dir / directory_name / "new"
    output.mkdir(parents=True)
    (output / "benchmark.json").write_text(json.dumps({
        "group_id": group_id,
        "function_id": function_id,
        "full_id": f"{group_id}/{function_id}",
        "directory_name": f"{group_dir}/{directory_name}",
    }))
    estimate = output / "estimates.json"
    estimate.write_text(json.dumps({
        "median": {
            "point_estimate": point,
            "confidence_interval": {
                "lower_bound": point - 0.25,
                "upper_bound": point + 0.25,
            },
        }
    }))
    return estimate


def runtime_attestation(invocation_id: str = "test-invocation") -> dict:
    return {
        "schema": "tach-benchmark-runtime-v2",
        "invocation_id": invocation_id,
        "harness": "criterion",
        "target": {"arch": "x86_64", "os": "linux", "env": "gnu"},
        "features": ["bench-internal"],
        "build_mode": "no-default",
        "build_profile": "optimized",
        "source_revision": None,
        "runner": None,
        "output_isolated": False,
    }


def write_complete_criterion(criterion: Path, attestation: dict) -> None:
    criterion.mkdir(parents=True)
    (criterion / "runtime-attestation.json").write_text(
        json.dumps(attestation), encoding="utf-8"
    )
    for metric, group_dir in extract_speed.WALL_GROUPS.items():
        group_id = extract_speed.CRITERION_GROUP_IDS[group_dir]
        for index, function_id in enumerate(extract_speed.WALL_FUNS):
            write_benchmark(
                criterion,
                group_dir,
                group_id,
                function_id,
                function_id,
                point=7.25 + index,
            )

    thread_cpu_ids = (
        "tach_thread_cpu__posix_thread_cpu_clock__system_call",
        "native_thread_cpu__libc_clock_gettime_clock_thread_cputime_id",
    )
    for metric, group_dir in extract_speed.THREAD_CPU_GROUPS.items():
        group_id = extract_speed.CRITERION_GROUP_IDS[group_dir]
        for index, function_id in enumerate(thread_cpu_ids):
            write_benchmark(
                criterion,
                group_dir,
                group_id,
                function_id,
                function_id,
                point=11.25 + index,
            )

    (criterion / "ordered-selection.json").write_text(
        json.dumps({"eligible_direct_candidates": []}), encoding="utf-8"
    )
    (criterion / "linux-x86-wall-selection.json").write_text(
        json.dumps({"eligible_direct_candidates": {"instant": [], "ordered": []}}),
        encoding="utf-8",
    )
    (criterion / "thread-cpu-behavior.json").write_text(
        json.dumps({
            "schema": "tach-thread-cpu-behavior-v2",
            "runtime_attestation": attestation,
            "direct_benchmark": (
                "native_thread_cpu__libc_clock_gettime_clock_thread_cputime_id"
            ),
            "sample_count": 3,
            "busy": {},
            "sleep": {},
            "sibling_isolation": {},
        }),
        encoding="utf-8",
    )


def seal_criterion(criterion: Path) -> Path:
    return seal_speed_source.seal_criterion_source(criterion)


def collect_fixture(root: Path, attestation: dict | None = None) -> tuple[Path, dict]:
    criterion = root / "source-criterion"
    if attestation is None:
        attestation = runtime_attestation()
    write_complete_criterion(criterion, attestation)
    seal_criterion(criterion)
    bundle = root / "bundle"
    completed = run_collector(criterion, bundle)
    if completed.returncode:
        raise AssertionError(
            "collector failed:\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        )
    return bundle, attestation


def run_collector(criterion: Path, bundle: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(COLLECTOR_SCRIPT), str(criterion), str(bundle)],
        check=False,
        capture_output=True,
        text=True,
    )


def rewrite_manifest(bundle: Path, manifest: dict) -> None:
    (bundle / extract_speed.COLLECTOR_MANIFEST_FILENAME).write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def lambda_runtime_attestation() -> dict:
    return {
        "schema": "tach-benchmark-runtime-v2",
        "invocation_id": "lambda-test-invocation",
        "harness": "lambda",
        "target": {"arch": "x86_64", "os": "linux", "env": "gnu"},
        "features": ["bench-internal", "thread-cpu-inline"],
        "build_mode": "default",
        "build_profile": "optimized",
        "source_revision": "1" * 40,
        "runner": "aws-lambda-x86_64",
        "output_isolated": True,
    }


def write_lambda_host_observation(root: Path) -> dict:
    attestation = lambda_runtime_attestation()
    wall_selection = {
        "eligible_direct_candidates": {
            "instant": ["direct_wall__test"],
            "ordered": ["direct_ordered_wall__test"],
        }
    }
    thread_selection = {
        "eligible_direct_candidates": ["direct_thread_cpu__test"],
        "fallback_native_benchmark": None,
    }
    clock_keys = (
        "tach",
        "tach_ordered",
        "quanta",
        "fastant",
        "minstant",
        "std",
        "tach_thread_cpu",
        "native_thread_cpu",
        "direct_selected_wall",
        "direct_selected_ordered_wall",
        "direct_selected_thread_cpu",
        "direct_wall__test",
        "direct_ordered_wall__test",
        "direct_thread_cpu__test",
    )
    root.mkdir()
    (root / "runtime-attestation.json").write_text(json.dumps(attestation))
    for run in range(1, 6):
        def behavior_phase(wall: int, public: int, direct: int) -> dict:
            sample = {
                "wall_delta_ns": wall,
                "public_delta_ns": public,
                "direct_delta_ns": direct,
            }
            return {**sample, "samples": [sample, sample, sample]}

        payload = {
            "runtime_attestation": attestation,
            "wall_selection": wall_selection,
            "thread_cpu_selection": thread_selection,
            "thread_cpu_behavior": {
                "schema": "tach-thread-cpu-behavior-v2",
                "runtime_attestation": attestation,
                "direct_benchmark": (
                    "native_thread_cpu__raw_syscall_clock_thread_cputime_id"
                ),
                "sample_count": 3,
                "busy": behavior_phase(20_000_000, 20_000_000, 20_000_000),
                "sleep": behavior_phase(20_000_000, 10_000, 10_000),
                "sibling_isolation": behavior_phase(20_000_000, 10_000, 10_000),
            },
        }
        for key in clock_keys:
            row = {
                "now_samples": [float(run)] * 31,
                "elapsed_samples": [float(run + 1)] * 31,
            }
            if key.startswith("direct_") or key in {
                "tach",
                "tach_ordered",
                "tach_thread_cpu",
                "native_thread_cpu",
            }:
                row.update({
                    "provider": f"provider-{key}",
                    "read_cost": "system call",
                    "time_domain": (
                        "thread CPU"
                        if "thread_cpu" in key or key in {"tach_thread_cpu", "native_thread_cpu"}
                        else ("ordered wall" if key == "tach_ordered" else "instant wall")
                    ),
                })
            if key.startswith("direct_"):
                row["benchmark"] = key
            elif key == "native_thread_cpu":
                row["benchmark"] = (
                    "native_thread_cpu__raw_syscall_clock_thread_cputime_id"
                )
            payload[key] = row
        (root / f"run-{run}.json").write_text(json.dumps(payload))
        (root / f"invoke-{run}.json").write_text(json.dumps({"StatusCode": 200}))
    return attestation


def write_node_wasm_host_observation(root: Path) -> dict:
    attestation = write_lambda_host_observation(root)
    attestation.update({
        "harness": "node-wasm-bindgen",
        "target": {"arch": "wasm32", "os": "unknown", "env": ""},
        "runner": "node-wasm-bindgen",
    })
    (root / "runtime-attestation.json").write_text(json.dumps(attestation))
    for run in range(1, 6):
        (root / f"invoke-{run}.json").unlink()
        payload_path = root / f"run-{run}.json"
        payload = json.loads(payload_path.read_text())
        payload["runtime_attestation"] = attestation
        payload["thread_cpu_behavior"]["runtime_attestation"] = attestation
        payload["wall_selection"] = {
            "architecture": "wasm32-host",
            "selected_provider": {"instant": "test", "ordered": "test"},
            "selected_native_benchmark": {
                "instant": "direct_selected_wall__test",
                "ordered": "direct_selected_ordered_wall__test",
            },
            "eligible_direct_candidates": {
                "instant": ["direct_wall__test"],
                "ordered": ["direct_ordered_wall__test"],
            },
            "probe": {"observation": run},
        }
        for key, provider, domain in (
            ("tach", "performance.now", "instant wall"),
            ("tach_ordered", "performance.now", "ordered wall"),
        ):
            payload[key].update({
                "provider": provider,
                "read_cost": "host call",
                "time_domain": domain,
            })
        payload.pop("std")
        payload_path.write_text(json.dumps(payload))
    return attestation


class HostCollectorBundleTests(unittest.TestCase):
    def test_node_wasm_observations_may_omit_the_ineligible_std_row(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            host = root / "host-input"
            attestation = write_node_wasm_host_observation(host)
            bundle = root / "bundle"

            collect_host_speed_bundle.collect_host_bundle(host, bundle)
            observation = extract_speed.extract_collector_bundle_observation(bundle)

            self.assertEqual(
                observation["collector_attestation"]["runtime_attestation"],
                attestation,
            )
            self.assertNotIn("std", observation["clocks"])
            self.assertEqual(len(observation["clocks"]["tach"]["now_samples"]), 155)
            self.assertEqual(
                len(observation["clocks"]["tach"]["selection"]["probe_observations"]),
                5,
            )

    def test_lambda_raw_invocations_reaggregate_from_retained_bundle(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            host = root / "host-input"
            attestation = write_lambda_host_observation(host)
            bundle = root / "bundle"

            collect_host_speed_bundle.collect_host_bundle(host, bundle)
            observation = extract_speed.extract_collector_bundle_observation(bundle)

            self.assertEqual(
                observation["collector_attestation"]["runtime_attestation"],
                attestation,
            )
            self.assertEqual(observation["clocks"]["tach"]["now"], 3.0)
            self.assertEqual(
                len(observation["clocks"]["tach"]["now_samples"]),
                155,
            )
            self.assertEqual(
                observation["clocks"]["direct_selected_thread_cpu"]["benchmark"],
                "direct_selected_thread_cpu",
            )

    def test_tampered_retained_lambda_payload_rejects_extraction(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            host = root / "host-input"
            write_lambda_host_observation(host)
            bundle = root / "bundle"
            collect_host_speed_bundle.collect_host_bundle(host, bundle)
            payload = bundle / "host" / "run-3.json"
            payload.write_text(payload.read_text() + "\n")

            with self.assertRaisesRegex(RuntimeError, "hash mismatch"):
                extract_speed.extract_collector_bundle_observation(bundle)

    def test_host_collector_rejects_links_extra_files_and_nested_destination(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            linked = root / "linked"
            write_lambda_host_observation(linked)
            (linked / "run-5.json").unlink()
            (linked / "run-5.json").symlink_to(linked / "run-4.json")
            with self.assertRaisesRegex(RuntimeError, "nonregular input"):
                collect_host_speed_bundle.collect_host_bundle(
                    linked,
                    root / "linked-bundle",
                )

            extra = root / "extra"
            write_lambda_host_observation(extra)
            (extra / "unbound.json").write_text("{}")
            with self.assertRaisesRegex(RuntimeError, "file set changed"):
                collect_host_speed_bundle.collect_host_bundle(
                    extra,
                    root / "extra-bundle",
                )

            nested = root / "nested"
            write_lambda_host_observation(nested)
            with self.assertRaisesRegex(RuntimeError, "must not be inside"):
                collect_host_speed_bundle.collect_host_bundle(
                    nested,
                    nested / "bundle",
                )

    def test_lambda_attestation_must_match_every_raw_invocation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            host = root / "host-input"
            write_lambda_host_observation(host)
            payload_path = host / "run-4.json"
            payload = json.loads(payload_path.read_text())
            payload["runtime_attestation"]["invocation_id"] = "foreign-run"
            payload_path.write_text(json.dumps(payload))

            with self.assertRaisesRegex(RuntimeError, "attestation changed"):
                collect_host_speed_bundle.collect_host_bundle(host, root / "bundle")


class CriterionLookupTests(unittest.TestCase):
    def test_truncated_directory_resolves_full_recorded_identity(self) -> None:
        group_dir = extract_speed.THREAD_CPU_GROUPS["now"]
        group_id = "ThreadCpuInstant::now()"
        function_id = (
            "direct_selected_thread_cpu__linux_perf_mmap__"
            "x86_serialize_rdtsc_serialize"
        )
        truncated = function_id[:64]

        with tempfile.TemporaryDirectory() as directory:
            criterion = Path(directory)
            write_benchmark(
                criterion, group_dir, group_id, function_id, truncated
            )

            self.assertEqual(
                extract_speed.find_benchmark(
                    criterion, group_dir, "direct_selected_thread_cpu"
                ),
                function_id,
            )
            self.assertTrue(
                extract_speed.has_benchmark(
                    criterion, group_dir, "direct_selected_thread_cpu"
                )
            )
            self.assertEqual(
                extract_speed.median_estimate(criterion, group_dir, function_id),
                {"point": 7.25, "ci95": [7.0, 7.5]},
            )

    def test_duplicate_complete_identity_rejects_regardless_of_mtime(self) -> None:
        group_dir = extract_speed.THREAD_CPU_GROUPS["now"]
        group_id = "ThreadCpuInstant::now()"
        function_id = "direct_selected_thread_cpu__full_provider_identity"

        with tempfile.TemporaryDirectory() as directory:
            criterion = Path(directory)
            estimates = []
            for suffix in ("truncated-a", "truncated-b"):
                estimates.append(
                    write_benchmark(
                        criterion, group_dir, group_id, function_id, suffix
                    )
                )

            timestamp_ns = 1_700_000_000_000_000_000
            for offset, estimate in enumerate(estimates):
                os.utime(estimate, ns=(timestamp_ns + offset, timestamp_ns + offset))

            with self.assertRaisesRegex(
                RuntimeError, "duplicate Criterion benchmark identity"
            ):
                extract_speed.median_estimate(criterion, group_dir, function_id)

    def test_thread_cpu_entries_retain_full_recorded_benchmark_identity(self) -> None:
        identities = {
            "tach_thread_cpu": (
                "tach_thread_cpu__posix_thread_cpu_clock__system_call"
            ),
            "native_thread_cpu": (
                "native_thread_cpu__clock_gettime_nsec_np_clock_thread_cputime_id"
            ),
        }

        with tempfile.TemporaryDirectory() as directory:
            criterion = Path(directory)
            for metric, group_dir in extract_speed.THREAD_CPU_GROUPS.items():
                group_id = (
                    "ThreadCpuInstant::now()"
                    if metric == "now"
                    else "ThreadCpuInstant::now() + elapsed()"
                )
                for index, function_id in enumerate(identities.values()):
                    write_benchmark(
                        criterion,
                        group_dir,
                        group_id,
                        function_id,
                        f"criterion-truncated-{index}",
                    )

            for prefix, function_id in identities.items():
                with self.subTest(prefix=prefix):
                    entry = extract_speed.thread_cpu_entry(criterion, prefix)
                    self.assertEqual(entry["benchmark"], function_id)
                    self.assertEqual(entry["now"], 7.25)
                    self.assertEqual(entry["elapsed"], 7.25)

    def test_fixed_native_selector_extracts_full_macos_identity(self) -> None:
        mechanism = "macos_clock_gettime_nsec_np_thread_cpu"
        candidate = f"direct_thread_cpu__{mechanism}"
        selected = f"direct_selected_thread_cpu__{mechanism}"
        selection = {
            "selection_kind": "fixed_native",
            "selected_provider": "posix_thread_cpu_clock",
            "selected_mechanism": mechanism,
            "selected_read_cost": "system call",
            "selected_native_benchmark": selected,
            "fallback_provider": None,
            "fallback_mechanism": None,
            "fallback_read_cost": None,
            "fallback_native_benchmark": None,
            "eligible_direct_candidates": [candidate],
            "fixed_provider": {
                "candidate": mechanism,
                "supported_architectures": ["x86_64", "aarch64"],
                "native_primitive": (
                    "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)"
                ),
                "selection_basis": "macOS native current-thread CPU clock",
                "time_domain": "thread CPU",
            },
            "read_cost_basis": "native system-call tier",
        }

        with tempfile.TemporaryDirectory() as directory:
            criterion = Path(directory)
            selection_path = criterion / "thread-cpu-selection.json"
            selection_path.write_text(json.dumps(selection))
            for metric, group_dir in extract_speed.THREAD_CPU_GROUPS.items():
                group_id = (
                    "ThreadCpuInstant::now()"
                    if metric == "now"
                    else "ThreadCpuInstant::now() + elapsed()"
                )
                for index, benchmark in enumerate((candidate, selected)):
                    write_benchmark(
                        criterion,
                        group_dir,
                        group_id,
                        benchmark,
                        f"criterion-truncated-{index}",
                        point=7.25 + index,
                    )

            out = {
                "tach_thread_cpu": {
                    "provider": "POSIX thread CPU clock",
                    "read_cost": "system call",
                    "time_domain": "thread CPU",
                }
            }
            extract_speed.add_thread_cpu_selector_evidence(criterion, out)

            self.assertEqual(out[candidate]["provider"], mechanism)
            self.assertEqual(out[candidate]["benchmark"], candidate)
            self.assertEqual(out[candidate]["now"], 7.25)
            self.assertEqual(out[candidate]["elapsed"], 7.25)
            self.assertEqual(out["direct_selected_thread_cpu"]["provider"], mechanism)
            self.assertEqual(out["direct_selected_thread_cpu"]["benchmark"], selected)
            self.assertEqual(out["direct_selected_thread_cpu"]["now"], 8.25)
            self.assertEqual(out["direct_selected_thread_cpu"]["elapsed"], 8.25)

            malformed = dict(selection)
            malformed["selected_mechanism"] = "truncated_or_wrong"
            selection_path.write_text(json.dumps(malformed))
            with self.assertRaisesRegex(RuntimeError, "malformed fixed-native"):
                extract_speed.add_thread_cpu_selector_evidence(criterion, out)

    def test_apple_ordered_wall_candidate_keeps_its_provider_identity(self) -> None:
        candidate = "direct_ordered_wall__apple_mach_absolute_time"

        with tempfile.TemporaryDirectory() as directory:
            criterion = Path(directory) / "criterion"
            write_complete_criterion(criterion, runtime_attestation())
            (criterion / "linux-x86-wall-selection.json").unlink()
            (criterion / "ordered-selection.json").write_text(
                json.dumps({"eligible_direct_candidates": [candidate]}),
                encoding="utf-8",
            )
            (criterion / "apple-wall-selection.json").write_text(
                json.dumps({
                    "eligible_direct_candidates": {
                        "instant": [],
                        "ordered": [candidate],
                    }
                }),
                encoding="utf-8",
            )
            for metric, group_dir in extract_speed.WALL_GROUPS.items():
                write_benchmark(
                    criterion,
                    group_dir,
                    extract_speed.CRITERION_GROUP_IDS[group_dir],
                    candidate,
                    f"apple-ordered-{metric}",
                )

            clocks = extract_speed.extract_criterion_directory(criterion)

            self.assertEqual(clocks[candidate]["benchmark"], candidate)
            self.assertEqual(
                clocks[candidate]["provider"], "apple_mach_absolute_time"
            )


class StrictCriterionJsonInputTests(unittest.TestCase):
    def test_duplicate_keys_reject_every_criterion_json_input_class(self) -> None:
        group_dir = extract_speed.THREAD_CPU_GROUPS["now"]
        group_id = "ThreadCpuInstant::now()"
        function_id = "tach_thread_cpu__posix_thread_cpu_clock__system_call"

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            with self.subTest(input="benchmark metadata"):
                criterion = root / "metadata"
                metadata_path = write_benchmark(
                    criterion, group_dir, group_id, function_id, "truncated"
                ).with_name("benchmark.json")
                metadata_path.write_text(
                    '{"group_id":"ThreadCpuInstant::now()",'
                    '"function_id":"first","function_id":"second",'
                    '"full_id":"ThreadCpuInstant::now()/second"}',
                    encoding="utf-8",
                )
                with self.assertRaisesRegex(RuntimeError, "duplicate JSON key 'function_id'"):
                    extract_speed.criterion_benchmarks(criterion, group_dir)

            with self.subTest(input="estimates"):
                criterion = root / "estimates"
                estimates_path = write_benchmark(
                    criterion, group_dir, group_id, function_id, "truncated"
                )
                estimates_path.write_text(
                    '{"median":{"point_estimate":7.25,"point_estimate":8.25,'
                    '"confidence_interval":{"lower_bound":7,"upper_bound":8}}}',
                    encoding="utf-8",
                )
                with self.assertRaisesRegex(RuntimeError, "duplicate JSON key 'point_estimate'"):
                    extract_speed.median_estimate(criterion, group_dir, function_id)

            with self.subTest(input="thread CPU selector"):
                criterion = root / "thread-selector"
                criterion.mkdir()
                (criterion / "thread-cpu-selection.json").write_text(
                    '{"selection_kind":"fixed_native","selection_kind":"fallback_only"}',
                    encoding="utf-8",
                )
                with self.assertRaisesRegex(RuntimeError, "duplicate JSON key 'selection_kind'"):
                    extract_speed.add_thread_cpu_selector_evidence(
                        criterion, {"tach_thread_cpu": {}}
                    )

            with self.subTest(input="wall selector"):
                criterion = root / "wall-selector"
                criterion.mkdir()
                (criterion / "linux-x86-wall-selection.json").write_text(
                    '{"eligible_direct_candidates":{},'
                    '"eligible_direct_candidates":{}}',
                    encoding="utf-8",
                )
                with self.assertRaisesRegex(
                    RuntimeError, "duplicate JSON key 'eligible_direct_candidates'"
                ):
                    extract_speed.add_wall_selector_evidence(
                        criterion, {"tach": {}, "tach_ordered": {}}
                    )

            with self.subTest(input="ordered selector helper"):
                criterion = root / "ordered-selector"
                write_complete_criterion(criterion, runtime_attestation())
                (criterion / "ordered-selection.json").write_text(
                    '{"eligible_direct_candidates":[],"eligible_direct_candidates":[]}',
                    encoding="utf-8",
                )
                with self.assertRaisesRegex(
                    RuntimeError, "duplicate JSON key 'eligible_direct_candidates'"
                ):
                    extract_speed.extract_criterion_directory(criterion)

            with self.subTest(input="thread CPU behavior sidecar"):
                criterion = root / "behavior-sidecar"
                criterion.mkdir()
                (criterion / "thread-cpu-behavior.json").write_text(
                    '{"schema":"tach-thread-cpu-behavior-v2",'
                    '"schema":"tach-thread-cpu-behavior-v2"}',
                    encoding="utf-8",
                )
                with self.assertRaisesRegex(RuntimeError, "duplicate JSON key 'schema'"):
                    extract_speed.validate_thread_cpu_behavior_attestation(
                        criterion, runtime_attestation()
                    )


class CollectorBundleTests(unittest.TestCase):
    def test_runtime_attestation_requires_a_matching_build_mode(self) -> None:
        attestation = runtime_attestation()
        attestation["build_mode"] = "default"
        with self.assertRaisesRegex(RuntimeError, "build mode"):
            extract_speed.validate_runtime_attestation(attestation)

    def test_valid_collected_bundle_extracts_verified_clocks(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            bundle, attestation = collect_fixture(Path(directory))

            clocks = extract_speed.extract_collector_bundle(bundle)
            collector = clocks["collector_attestation"]
            self.assertEqual(collector["schema"], "tach-speed-collector-v1")
            self.assertEqual(collector["invocation_id"], attestation["invocation_id"])
            self.assertEqual(collector["runtime_attestation"], attestation)
            self.assertRegex(collector["manifest_sha256"], r"^[0-9a-f]{64}$")
            self.assertEqual(
                clocks["native_thread_cpu"]["provider"],
                "libc::clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
            )

            completed = subprocess.run(
                [
                    sys.executable,
                    str(EXTRACTOR_SCRIPT),
                    "--collector-bundle",
                    str(bundle),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(
                json.loads(completed.stdout)["collector_attestation"], collector
            )

    def test_observation_extracts_only_from_its_private_snapshot(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            bundle, _ = collect_fixture(Path(directory))
            original_estimate = (
                bundle
                / "criterion"
                / extract_speed.WALL_GROUPS["now"]
                / "tach/new/estimates.json"
            )
            extract_from_snapshot = extract_speed.extract_criterion_directory

            def mutate_original_then_extract(snapshot: Path, *args, **kwargs) -> dict:
                original_estimate.write_text("not JSON", encoding="utf-8")
                return extract_from_snapshot(snapshot, *args, **kwargs)

            with mock.patch.object(
                extract_speed,
                "extract_criterion_directory",
                side_effect=mutate_original_then_extract,
            ):
                observation = extract_speed.extract_collector_bundle_observation(bundle)

            self.assertEqual(observation["clocks"]["tach"]["now"], 7.25)
            self.assertNotIn("collector_attestation", observation["clocks"])
            self.assertEqual(
                observation["thread_cpu_behavior"]["schema"],
                "tach-thread-cpu-behavior-v2",
            )
            self.assertIn("collector_attestation", observation)

    def test_attested_macos_rejects_foreign_linux_wall_selector(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            attestation = runtime_attestation()
            attestation["target"] = {
                "arch": "aarch64",
                "os": "macos",
                "env": "",
            }
            bundle, _ = collect_fixture(Path(directory), attestation)
            with self.assertRaisesRegex(
                RuntimeError,
                "cannot belong to attested target",
            ):
                extract_speed.extract_collector_bundle_observation(bundle)

    def test_attestation_only_behavior_sidecar_rejects_extraction(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            bundle, attestation = collect_fixture(Path(directory))
            sidecar_path = bundle / "criterion" / "thread-cpu-behavior.json"
            sidecar_path.write_text(
                json.dumps({
                    "schema": "tach-thread-cpu-behavior-v2",
                    "runtime_attestation": attestation,
                }),
                encoding="utf-8",
            )
            manifest_path = bundle / extract_speed.COLLECTOR_MANIFEST_FILENAME
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["files"]["thread-cpu-behavior.json"] = extract_speed.sha256_file(
                sidecar_path
            )
            rewrite_manifest(bundle, manifest)
            with self.assertRaisesRegex(
                RuntimeError,
                "thread-CPU behavior sidecar has an unexpected v2 shape",
            ):
                extract_speed.extract_collector_bundle_observation(bundle)

    def test_tampered_copied_input_rejects_extraction(self) -> None:
        targets = {
            "benchmark": (
                extract_speed.WALL_GROUPS["now"]
                + "/tach/new/estimates.json"
            ),
            "selector": "ordered-selection.json",
            "sidecar": "thread-cpu-behavior.json",
        }
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, relative in targets.items():
                with self.subTest(input=name):
                    bundle, _ = collect_fixture(root / name)
                    target = bundle / "criterion" / relative
                    target.write_text("tampered", encoding="utf-8")
                    with self.assertRaisesRegex(RuntimeError, "collector hash mismatch"):
                        extract_speed.extract_collector_bundle(bundle)

    def test_source_seal_records_exact_runtime_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            criterion = Path(directory) / "source-criterion"
            attestation = runtime_attestation()
            write_complete_criterion(criterion, attestation)
            seal = seal_criterion(criterion)

            document = json.loads(seal.read_text(encoding="utf-8"))
            runtime_bytes = (criterion / "runtime-attestation.json").read_bytes()
            runtime = document["runtime_attestation"]
            self.assertEqual(document["schema"], "tach-speed-source-seal-v1")
            self.assertEqual(
                base64.b64decode(runtime["base64"], validate=True),
                runtime_bytes,
            )
            self.assertEqual(
                runtime["sha256"],
                hashlib.sha256(runtime_bytes).hexdigest(),
            )
            self.assertEqual(list(document["files"]), sorted(document["files"]))
            self.assertEqual(
                document["files"]["runtime-attestation.json"],
                runtime["sha256"],
            )
            self.assertNotIn("tach-speed-source-seal.json", document["files"])

    def test_sealer_cli_writes_only_after_successful_command(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            criterion = root / "source-criterion"
            write_complete_criterion(criterion, runtime_attestation())
            seal = criterion / "tach-speed-source-seal.json"

            failed = subprocess.run(
                [
                    sys.executable,
                    str(SEALER_SCRIPT),
                    str(criterion),
                    "--",
                    sys.executable,
                    "-c",
                    "raise SystemExit(7)",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(failed.returncode, 7)
            self.assertFalse(seal.exists())

            succeeded = subprocess.run(
                [
                    sys.executable,
                    str(SEALER_SCRIPT),
                    str(criterion),
                    "--",
                    sys.executable,
                    "-c",
                    "pass",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(succeeded.returncode, 0, succeeded.stderr)
            self.assertTrue(seal.is_file())

    def test_collector_requires_a_completed_source_seal(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            criterion = root / "source-criterion"
            write_complete_criterion(criterion, runtime_attestation())

            completed = run_collector(criterion, root / "bundle")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("missing tach-speed-source-seal.json", completed.stderr)

    def test_post_seal_mutation_rejects_collection(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            with self.subTest(mutation="benchmark row"):
                criterion = root / "row"
                write_complete_criterion(criterion, runtime_attestation())
                seal_criterion(criterion)
                target = (
                    criterion
                    / extract_speed.WALL_GROUPS["now"]
                    / "tach/new/estimates.json"
                )
                target.write_text("tampered", encoding="utf-8")
                completed = run_collector(criterion, root / "row-bundle")
                self.assertNotEqual(completed.returncode, 0)
                self.assertIn("source seal hash mismatch", completed.stderr)

            with self.subTest(mutation="extra file"):
                criterion = root / "extra"
                write_complete_criterion(criterion, runtime_attestation())
                seal_criterion(criterion)
                (criterion / "unexpected.json").write_text("{}", encoding="utf-8")
                completed = run_collector(criterion, root / "extra-bundle")
                self.assertNotEqual(completed.returncode, 0)
                self.assertIn("does not match source seal", completed.stderr)

            with self.subTest(mutation="missing file"):
                criterion = root / "missing"
                write_complete_criterion(criterion, runtime_attestation())
                seal_criterion(criterion)
                target = (
                    criterion
                    / extract_speed.WALL_GROUPS["now"]
                    / "tach/new/estimates.json"
                )
                target.unlink()
                completed = run_collector(criterion, root / "missing-bundle")
                self.assertNotEqual(completed.returncode, 0)
                self.assertIn("does not match source seal", completed.stderr)

    def test_special_or_link_source_input_rejects_collection(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            criterion = root / "source-criterion"
            write_complete_criterion(criterion, runtime_attestation())
            seal_criterion(criterion)
            link = criterion / "runtime-link.json"
            try:
                link.symlink_to("runtime-attestation.json")
            except OSError as error:
                self.skipTest(f"test filesystem cannot create a symbolic link: {error}")

            completed = run_collector(criterion, root / "bundle")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("nonregular input", completed.stderr)

    def test_run_a_rows_with_run_b_attestation_rejects_collection(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            criterion = root / "source-criterion"
            write_complete_criterion(criterion, runtime_attestation("run-a"))
            seal_criterion(criterion)
            (criterion / "runtime-attestation.json").write_text(
                json.dumps(runtime_attestation("run-b")),
                encoding="utf-8",
            )

            completed = run_collector(criterion, root / "bundle")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn(
                "source runtime attestation disagrees with source seal",
                completed.stderr,
            )

    def test_runtime_attestation_replacement_between_verify_and_copy_rejects(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            criterion = root / "source-criterion"
            write_complete_criterion(criterion, runtime_attestation("run-a"))
            seal_criterion(criterion)
            runtime_path = criterion / "runtime-attestation.json"
            replacement = root / "replacement-runtime.json"
            replacement.write_text(
                json.dumps(runtime_attestation("run-b")),
                encoding="utf-8",
            )
            original_copy = collect_speed_bundle._copy_sealed_file
            replaced = False

            def replace_before_copy(
                source_path: Path,
                destination: Path,
                relative: str,
                expected_digest: str,
            ) -> str:
                nonlocal replaced
                if relative == "runtime-attestation.json" and not replaced:
                    os.replace(replacement, runtime_path)
                    replaced = True
                return original_copy(source_path, destination, relative, expected_digest)

            with mock.patch.object(
                collect_speed_bundle,
                "_copy_sealed_file",
                side_effect=replace_before_copy,
            ):
                with self.assertRaisesRegex(RuntimeError, "source seal hash mismatch"):
                    collect_speed_bundle.collect_criterion_bundle(
                        criterion,
                        root / "bundle",
                    )
            self.assertTrue(replaced)
            self.assertFalse((root / "bundle").exists())

    def test_malformed_source_seal_rejects_collection(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            criterion = root / "source-criterion"
            write_complete_criterion(criterion, runtime_attestation())
            seal = seal_criterion(criterion)
            seal.write_text('{"schema":"wrong"}', encoding="utf-8")

            completed = run_collector(criterion, root / "bundle")
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("malformed source seal", completed.stderr)

    def test_mismatched_or_malformed_runtime_attestation_rejects(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)

            with self.subTest(attestation="mismatched"):
                bundle, attestation = collect_fixture(root / "mismatched")
                runtime_path = bundle / "criterion" / "runtime-attestation.json"
                mismatched = dict(attestation)
                mismatched["invocation_id"] = "other-invocation"
                runtime_path.write_text(json.dumps(mismatched), encoding="utf-8")
                manifest_path = bundle / extract_speed.COLLECTOR_MANIFEST_FILENAME
                manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
                manifest["files"]["runtime-attestation.json"] = extract_speed.sha256_file(
                    runtime_path
                )
                rewrite_manifest(bundle, manifest)
                with self.assertRaisesRegex(
                    RuntimeError,
                    "snapshot runtime attestation disagrees",
                ):
                    extract_speed.extract_collector_bundle(bundle)

            with self.subTest(attestation="malformed"):
                bundle, attestation = collect_fixture(root / "malformed")
                runtime_path = bundle / "criterion" / "runtime-attestation.json"
                malformed = dict(attestation)
                malformed["features"] = ["z-feature", "a-feature"]
                runtime_path.write_text(json.dumps(malformed), encoding="utf-8")
                manifest_path = bundle / extract_speed.COLLECTOR_MANIFEST_FILENAME
                manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
                manifest["runtime_attestation"] = malformed
                manifest["files"]["runtime-attestation.json"] = extract_speed.sha256_file(
                    runtime_path
                )
                rewrite_manifest(bundle, manifest)
                with self.assertRaisesRegex(
                    RuntimeError,
                    "malformed collector manifest runtime attestation: enabled features",
                ):
                    extract_speed.extract_collector_bundle(bundle)

            with self.subTest(attestation="sidecar mismatch"):
                bundle, attestation = collect_fixture(root / "sidecar-mismatch")
                sidecar_path = bundle / "criterion" / "thread-cpu-behavior.json"
                sidecar = json.loads(sidecar_path.read_text(encoding="utf-8"))
                mismatched = dict(attestation)
                mismatched["invocation_id"] = "other-sidecar-invocation"
                sidecar["runtime_attestation"] = mismatched
                sidecar_path.write_text(json.dumps(sidecar), encoding="utf-8")
                manifest_path = bundle / extract_speed.COLLECTOR_MANIFEST_FILENAME
                manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
                manifest["files"]["thread-cpu-behavior.json"] = extract_speed.sha256_file(
                    sidecar_path
                )
                rewrite_manifest(bundle, manifest)
                with self.assertRaisesRegex(
                    RuntimeError,
                    "thread-CPU behavior runtime attestation disagrees",
                ):
                    extract_speed.extract_collector_bundle(bundle)


if __name__ == "__main__":
    unittest.main()
