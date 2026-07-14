#!/usr/bin/env python3
"""Public claim-surface regressions for full-release snapshot handoff."""

from __future__ import annotations

from contextlib import redirect_stdout
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


BENCHES_DIR = Path(__file__).resolve().parent
if str(BENCHES_DIR) not in sys.path:
  sys.path.insert(0, str(BENCHES_DIR))


def load_script(filename: str, module_name: str):
  spec = importlib.util.spec_from_file_location(module_name, BENCHES_DIR / filename)
  if spec is None or spec.loader is None:
    raise RuntimeError(f"could not load {filename}")
  module = importlib.util.module_from_spec(spec)
  sys.modules[module_name] = module
  spec.loader.exec_module(module)
  return module


RELEASE_VALIDATOR = load_script(
  "validate-release-evidence.py", "tach_release_claim_surface_validator"
)
SPEED_VALIDATOR = load_script(
  "validate-speed-evidence.py", "tach_release_claim_surface_speed_validator"
)
USE_CASES_CHART = load_script(
  "summary-use-cases.py", "tach_release_claim_surface_use_cases"
)
THREAD_CPU_CHART = load_script(
  "summary-thread-cpu.py", "tach_release_claim_surface_thread_cpu"
)


SOURCE_REVISION = "a" * 40


def clock(now: float, elapsed: float) -> dict[str, float]:
  return {"now": now, "elapsed": elapsed}


def chart_document(index: int, marker: float = 1.23) -> dict:
  clocks = {
    "tach": clock(marker if index == 0 else 4.0, marker + 1.0 if index == 0 else 5.0),
    "quanta": clock(5.0, 6.0),
    "fastant": clock(7.0, 8.0),
    "minstant": clock(9.0, 10.0),
    "std": clock(11.0, 12.0),
    "tach_ordered": clock(13.0, 14.0),
    "tach_thread_cpu": {
      **clock(marker if index == 0 else 15.0, marker + 1.0 if index == 0 else 16.0),
      "provider": "snapshot thread provider",
      "read_cost": "inline",
      "time_domain": "thread CPU",
    },
    "native_thread_cpu": {
      **clock(17.0, 18.0),
      "provider": "native thread provider",
      "read_cost": "system call",
      "time_domain": "thread CPU",
    },
  }
  return {
    "order": index,
    "title": f"cell {index}",
    "instance": f"instance-{index}",
    "triple": f"target-{index}",
    "clocks": clocks,
  }


def write_primary_documents(root: Path, marker: float = 1.23) -> tuple[dict[str, dict], Path]:
  documents: dict[str, dict] = {}
  artifact_ids = tuple(RELEASE_VALIDATOR.speed_evidence.PRIMARY_SPEED_CELLS)
  first_path = root / artifact_ids[0]
  for index, artifact_id in enumerate(artifact_ids):
    document = chart_document(index, marker)
    documents[artifact_id] = document
    (root / artifact_id).write_text(json.dumps(document), encoding="utf-8")
  return documents, first_path


def primary_report() -> dict:
  return {
    "passed": True,
    "failures": [],
    "source_revision": SOURCE_REVISION,
    "cells": [],
  }


def supplemental_report() -> dict:
  return {
    "passed": True,
    "failures": [],
    "source_revision": SOURCE_REVISION,
  }


def primary_only_supplemental_report() -> dict:
  return {
    "passed": False,
    "failures": ["primary-only evidence is not a release claim"],
    "source_revision": SOURCE_REVISION,
  }


def route_report() -> dict:
  return {"passed": True, "failures": []}


def boundary_matrix():
  boundaries = []
  for index, artifact_id in enumerate(RELEASE_VALIDATOR.speed_evidence.PRIMARY_SPEED_CELLS):
    identity = RELEASE_VALIDATOR.release_matrix.RouteIdentity(
      f"case-{index}",
      f"target-{index}",
      "default",
      f"runtime-{index}",
    )
    requirement = RELEASE_VALIDATOR.release_matrix.CoverageRequirement(
      identity,
      RELEASE_VALIDATOR.release_matrix.EvidenceKind.FULL_SPEED,
      f"producer-{index}",
      "three_timer_direct",
    )
    boundaries.append(RELEASE_VALIDATOR.release_matrix.DecisionBoundary(
      f"boundary-{index}", artifact_id, requirement
    ))
  matrix = RELEASE_VALIDATOR.release_matrix.DecisionBoundaryMatrix(
    tuple(boundaries), RELEASE_VALIDATOR.REQUIRED_SHIPPING_PATHS
  )
  binding = {
    "passed": True,
    "failures": [],
  }
  return matrix, binding


class ReleaseClaimSurfaceTests(unittest.TestCase):
  def test_ci_validator_rejects_primary_only_release_report(self) -> None:
    with tempfile.TemporaryDirectory() as temporary:
      data_dir = Path(temporary)
      write_primary_documents(data_dir)
      with mock.patch.object(
        RELEASE_VALIDATOR,
        "load_release_boundary_matrix",
        return_value=boundary_matrix(),
      ), mock.patch.object(
        RELEASE_VALIDATOR,
        "validate_primary_snapshots",
        return_value=primary_report(),
      ), mock.patch.object(
        RELEASE_VALIDATOR,
        "validate_supplemental_snapshots",
        return_value=primary_only_supplemental_report(),
      ), mock.patch.object(
        RELEASE_VALIDATOR,
        "validate_route_matrix_admission",
        return_value=route_report(),
      ), mock.patch.object(
        SPEED_VALIDATOR,
        "load_release_validator",
        return_value=RELEASE_VALIDATOR,
      ), mock.patch.object(
        sys,
        "argv",
        ["validate-speed-evidence.py", "--data-dir", str(data_dir)],
      ):
        report = SPEED_VALIDATOR.validate(data_dir, data_dir)
        rendered = io.StringIO()
        with redirect_stdout(rendered), self.assertRaises(SystemExit) as exited:
          SPEED_VALIDATOR.main()

    self.assertEqual(exited.exception.code, 1)
    self.assertTrue(report["primary"]["passed"])
    self.assertFalse(report["supplemental_speed"]["passed"])
    self.assertIn("primary-only evidence", rendered.getvalue())

  def test_chart_clis_reject_primary_only_release_snapshot(self) -> None:
    for chart, stem in (
      (USE_CASES_CHART, "summary-use-cases"),
      (THREAD_CPU_CHART, "summary-thread-cpu"),
    ):
      with self.subTest(chart=stem), tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_primary_documents(root)
        output_dir = root / "output"
        with mock.patch.object(
          RELEASE_VALIDATOR,
          "load_release_boundary_matrix",
          return_value=boundary_matrix(),
        ), mock.patch.object(
          RELEASE_VALIDATOR,
          "validate_primary_snapshots",
          return_value=primary_report(),
        ), mock.patch.object(
          RELEASE_VALIDATOR,
          "validate_supplemental_snapshots",
          return_value=primary_only_supplemental_report(),
        ), mock.patch.object(
          RELEASE_VALIDATOR,
          "validate_route_matrix_admission",
          return_value=route_report(),
        ), mock.patch.object(
          chart,
          "load_release_validator",
          return_value=RELEASE_VALIDATOR,
        ), mock.patch.object(
          sys,
          "argv",
          [
            f"{stem}.py",
            "--data-dir",
            str(root),
            "--output-dir",
            str(output_dir),
            "--svg-only",
          ],
        ):
          with self.assertRaises(SystemExit) as exited:
            chart.main()

        self.assertIn("primary-only evidence", str(exited.exception))
        self.assertFalse(output_dir.exists())

  def test_chart_clis_render_captured_bytes_after_live_document_swap(self) -> None:
    for chart, stem in (
      (USE_CASES_CHART, "summary-use-cases"),
      (THREAD_CPU_CHART, "summary-thread-cpu"),
    ):
      with self.subTest(chart=stem), tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        _, first_path = write_primary_documents(root)
        swapped = chart_document(0, 97.0)

        def validate_primary(*_args, **_kwargs):
          first_path.write_text(json.dumps(swapped), encoding="utf-8")
          return primary_report()

        output_dir = root / "output"
        with mock.patch.object(
          RELEASE_VALIDATOR,
          "load_release_boundary_matrix",
          return_value=boundary_matrix(),
        ), mock.patch.object(
          RELEASE_VALIDATOR,
          "validate_primary_snapshots",
          side_effect=validate_primary,
        ), mock.patch.object(
          RELEASE_VALIDATOR,
          "validate_supplemental_snapshots",
          return_value=supplemental_report(),
        ), mock.patch.object(
          RELEASE_VALIDATOR,
          "validate_route_matrix_admission",
          return_value=route_report(),
        ), mock.patch.object(
          chart,
          "load_release_validator",
          return_value=RELEASE_VALIDATOR,
        ), mock.patch.object(
          sys,
          "argv",
          [
            f"{stem}.py",
            "--data-dir",
            str(root),
            "--output-dir",
            str(output_dir),
            "--svg-only",
          ],
        ):
          chart.main()

        rendered = (output_dir / f"{stem}.svg").read_text(encoding="utf-8")
        self.assertIn("1.23", rendered)
        self.assertNotIn("97.0", rendered)
        self.assertIn("97.0", first_path.read_text(encoding="utf-8"))


if __name__ == "__main__":
  unittest.main()
