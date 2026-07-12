#!/usr/bin/env python3

from __future__ import annotations

import json
import os
import tempfile
import unittest
from pathlib import Path

import extract_speed


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

    def test_equal_mtime_duplicate_identity_fails_as_ambiguous(self) -> None:
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
            for estimate in estimates:
                os.utime(estimate, ns=(timestamp_ns, timestamp_ns))

            with self.assertRaisesRegex(RuntimeError, "ambiguous newest benchmark"):
                extract_speed.median_estimate(criterion, group_dir, function_id)

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


if __name__ == "__main__":
    unittest.main()
