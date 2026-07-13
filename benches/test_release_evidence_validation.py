#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


BENCHES_DIR = Path(__file__).resolve().parent
if str(BENCHES_DIR) not in sys.path:
  sys.path.insert(0, str(BENCHES_DIR))
FULL_SPEED_ARTIFACT = "speed-supplemental-macos-x86_64.json"
RUNTIME_SMOKE_ARTIFACT = "speed-supplemental-wasip1-threads-smoke.json"
TAGGED_FALLBACK_ARTIFACT = "speed-supplemental-wasi-p1-wasmtime.json"


def load_script(filename: str, module_name: str):
  spec = importlib.util.spec_from_file_location(module_name, BENCHES_DIR / filename)
  if spec is None or spec.loader is None:
    raise RuntimeError(f"could not load {filename}")
  module = importlib.util.module_from_spec(spec)
  sys.modules[module_name] = module
  spec.loader.exec_module(module)
  return module


RELEASE_VALIDATOR = load_script("validate-release-evidence.py", "tach_release_evidence_validator")
SUPPLEMENTAL_VALIDATOR = load_script(
  "validate-supplemental-thread-cpu.py", "tach_supplemental_evidence_validator"
)


def write_cell(directory: Path, artifact: str, document: dict) -> Path:
  path = directory / artifact
  path.write_text(json.dumps(document), encoding="utf-8")
  return path


def full_speed_document(bundle_path: str = "collector.bundle") -> dict:
  return {"collector_bundle": {"path": bundle_path}}


class RetainedBundlePathTests(unittest.TestCase):
  def test_full_cells_map_to_a_regular_sibling_bundle(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      cell = write_cell(root, FULL_SPEED_ARTIFACT, full_speed_document())
      bundle = root / "collector.bundle"
      bundle.mkdir()

      resolved, error = RELEASE_VALIDATOR.retained_collector_bundle_path(
        root, full_speed_document()
      )
      self.assertIsNone(error)
      self.assertEqual(resolved, bundle.resolve())

      resolved, error = SUPPLEMENTAL_VALIDATOR.retained_collector_bundle_path(
        cell, full_speed_document()
      )
      self.assertIsNone(error)
      self.assertEqual(resolved, bundle.resolve())

  def test_full_cells_reject_bundle_paths_outside_the_cell_directory(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      cell = write_cell(root, FULL_SPEED_ARTIFACT, full_speed_document("../outside"))

      resolved, error = RELEASE_VALIDATOR.retained_collector_bundle_path(
        root, full_speed_document("../outside")
      )
      self.assertIsNone(resolved)
      self.assertEqual(error, "collector bundle descriptor has no safe relative path")

      resolved, error = SUPPLEMENTAL_VALIDATOR.retained_collector_bundle_path(
        cell, full_speed_document("../outside")
      )
      self.assertIsNone(resolved)
      self.assertEqual(error, "collector bundle descriptor has no safe relative path")


class SupplementalValidatorTests(unittest.TestCase):
  def test_campaign_passes_safe_cell_paths_to_bound_validator(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      cell = write_cell(root, FULL_SPEED_ARTIFACT, full_speed_document())
      (root / "collector.bundle").mkdir()
      report = {"passed": True, "failures": []}
      with mock.patch.object(
        RELEASE_VALIDATOR.speed_evidence,
        "validate_supplemental_speed_campaign",
        return_value=report,
      ) as validate:
        result = RELEASE_VALIDATOR.validate_supplemental_campaign(root)

      self.assertTrue(result["passed"])
      documents, cell_paths = validate.call_args.args[:2]
      self.assertEqual(documents[FULL_SPEED_ARTIFACT], full_speed_document())
      self.assertEqual(cell_paths[FULL_SPEED_ARTIFACT], cell.resolve())
      self.assertEqual(validate.call_args.kwargs, {"require_bound_observations": True})

  def test_campaign_does_not_map_an_escaping_bundle_path(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      write_cell(root, FULL_SPEED_ARTIFACT, full_speed_document("../outside"))
      report = {"passed": True, "failures": []}
      with mock.patch.object(
        RELEASE_VALIDATOR.speed_evidence,
        "validate_supplemental_speed_campaign",
        return_value=report,
      ) as validate:
        result = RELEASE_VALIDATOR.validate_supplemental_campaign(root)

      self.assertFalse(result["passed"])
      _, cell_paths = validate.call_args.args[:2]
      self.assertNotIn(FULL_SPEED_ARTIFACT, cell_paths)
      self.assertTrue(any("safe relative path" in failure for failure in result["failures"]))

  def test_runtime_smoke_never_uses_a_collector_bundle(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      cell = write_cell(
        Path(directory),
        RUNTIME_SMOKE_ARTIFACT,
        {"collector_bundle": {"path": "generic.bundle"}},
      )
      report = {"artifact": RUNTIME_SMOKE_ARTIFACT, "passed": True, "failures": []}
      with mock.patch.object(
        SUPPLEMENTAL_VALIDATOR.speed_evidence,
        "validate_supplemental_speed_cell",
        return_value=report,
      ) as direct, mock.patch.object(
        SUPPLEMENTAL_VALIDATOR.speed_evidence,
        "validate_supplemental_speed_cell_from_bundle",
      ) as bound:
        result = SUPPLEMENTAL_VALIDATOR.validate_cell_artifact(
          RUNTIME_SMOKE_ARTIFACT, cell
        )

      self.assertTrue(result["passed"])
      direct.assert_called_once_with(RUNTIME_SMOKE_ARTIFACT, {"collector_bundle": {"path": "generic.bundle"}})
      bound.assert_not_called()

  def test_full_cell_uses_the_retained_bundle_observation(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      document = full_speed_document()
      cell = write_cell(root, FULL_SPEED_ARTIFACT, document)
      bundle = root / "collector.bundle"
      bundle.mkdir()
      report = {"artifact": FULL_SPEED_ARTIFACT, "passed": True, "failures": []}
      with mock.patch.object(
        SUPPLEMENTAL_VALIDATOR.speed_evidence,
        "validate_supplemental_speed_cell_from_bundle",
        return_value=report,
      ) as bound:
        result = SUPPLEMENTAL_VALIDATOR.validate_cell_artifact(FULL_SPEED_ARTIFACT, cell)

      self.assertTrue(result["passed"])
      bound.assert_called_once_with(FULL_SPEED_ARTIFACT, document, bundle.resolve())

  def test_tagged_fallback_remains_fail_closed(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      cell = write_cell(Path(directory), TAGGED_FALLBACK_ARTIFACT, {})
      report = {"artifact": TAGGED_FALLBACK_ARTIFACT, "passed": True, "failures": []}
      with mock.patch.object(
        SUPPLEMENTAL_VALIDATOR.speed_evidence,
        "validate_supplemental_speed_cell",
        return_value=report,
      ):
        result = SUPPLEMENTAL_VALIDATOR.validate_cell_artifact(
          TAGGED_FALLBACK_ARTIFACT, cell
        )

      self.assertFalse(result["passed"])
      self.assertTrue(any("producer-specific" in failure for failure in result["failures"]))


if __name__ == "__main__":
  unittest.main()
