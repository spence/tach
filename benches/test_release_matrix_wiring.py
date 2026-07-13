#!/usr/bin/env python3
"""Release-validator wiring tests for exhaustive, bound route observations."""

from __future__ import annotations

from contextlib import nullcontext
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


BENCHES_DIR = Path(__file__).resolve().parent
if str(BENCHES_DIR) not in sys.path:
  sys.path.insert(0, str(BENCHES_DIR))

import release_matrix
import speed_evidence


def load_script(filename: str, module_name: str):
  spec = importlib.util.spec_from_file_location(module_name, BENCHES_DIR / filename)
  if spec is None or spec.loader is None:
    raise RuntimeError(f"could not load {filename}")
  module = importlib.util.module_from_spec(spec)
  sys.modules[module_name] = module
  spec.loader.exec_module(module)
  return module


RELEASE_VALIDATOR = load_script(
  "validate-release-evidence.py", "tach_release_matrix_wiring_validator"
)
SOURCE_REVISION = "a" * 40
CLOSURE_DIGEST = "b" * 64
PRIMARY_ARTIFACT = "speed-2-inteln.json"
PRIMARY_TARGET = "x86_64-unknown-linux-gnu"
PRIMARY_PROFILE = "native_criterion"


def primary_document(
  *,
  build_mode: str = "default",
  legacy: bool = False,
  source_revision: str = SOURCE_REVISION,
) -> dict:
  features = list(speed_evidence.BENCHMARK_FEATURES_BY_BUILD_MODE[build_mode])
  document = {
    "order": 2,
    "triple": PRIMARY_TARGET,
    "provenance": {
      "source_revision": source_revision,
      "features": features,
      "harness": "criterion",
    },
  }
  if not legacy:
    document.update({
      "artifact_id": PRIMARY_ARTIFACT,
      "build_mode": build_mode,
      "evidence_kind": "full_speed",
    })
  return document


def route_observation(
  artifact_id: str,
  *,
  case_id: str = "native",
  target: str = PRIMARY_TARGET,
  build_mode: str = "default",
  runtime_profile: str = PRIMARY_PROFILE,
  evidence_kind: str = "full_speed",
  source_revision: str = SOURCE_REVISION,
) -> dict:
  return {
    "schema": "tach-route-observation-v1",
    "artifact_id": artifact_id,
    "identity": {
      "case_id": case_id,
      "target": target,
      "build_mode": build_mode,
      "runtime_profile": runtime_profile,
    },
    "evidence_kind": evidence_kind,
    "frozen": {
      "source_revision": source_revision,
      "target": target,
      "runtime_profile": runtime_profile,
      "closure_digest": CLOSURE_DIGEST,
    },
  }


def requirement(build_mode: str) -> release_matrix.CoverageRequirement:
  identity = release_matrix.RouteIdentity(
    "native", PRIMARY_TARGET, build_mode, PRIMARY_PROFILE
  )
  return release_matrix.CoverageRequirement(
    identity,
    release_matrix.EvidenceKind.FULL_SPEED,
    "test-producer",
    "three_timer_direct",
  )


def write_json(path: Path, document: dict) -> None:
  path.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


def write_manifest(
  directory: Path,
  observations: list[dict],
  *,
  equivalences: list[dict] | None = None,
  source_revision: str = SOURCE_REVISION,
) -> None:
  bindings = []
  for observation in observations:
    artifact_id = observation["artifact_id"]
    raw = (directory / artifact_id).read_bytes()
    bindings.append({
      "artifact_id": artifact_id,
      "document_sha256": hashlib.sha256(raw).hexdigest(),
      "route_observation": observation,
    })
  write_json(directory / RELEASE_VALIDATOR.ROUTE_OBSERVATIONS_FILENAME, {
    "schema": RELEASE_VALIDATOR.ROUTE_OBSERVATIONS_SCHEMA,
    "source_revision": source_revision,
    "bindings": bindings,
    "equivalences": [] if equivalences is None else equivalences,
  })


def passed_primary_report(source_revision: str = SOURCE_REVISION) -> dict:
  return {
    "passed": True,
    "failures": [],
    "source_revision": source_revision,
    "cells": [{
      "artifact_id": PRIMARY_ARTIFACT,
      "source_revision": source_revision,
      "triple": PRIMARY_TARGET,
      "build_mode": "default",
      "evidence_kind": "full_speed",
      "bound_observation": True,
      "passed": True,
    }],
  }


def passed_supplemental_report(source_revision: str = SOURCE_REVISION) -> dict:
  return {"passed": True, "failures": [], "source_revision": source_revision}


def bound_manifest_report(source_revision: str) -> dict:
  return {
    "schema": RELEASE_VALIDATOR.ROUTE_MANIFEST_BINDING_SCHEMA,
    "path": RELEASE_VALIDATOR.ROUTE_COVERAGE_GIT_PATH,
    "source_revision": source_revision,
    "sha256": "d" * 64,
    "passed": True,
    "failures": [],
  }


def write_route_coverage(path: Path, modes: list[str]) -> None:
  mode_rows = ", ".join(f'"{mode}"' for mode in modes)
  path.parent.mkdir(parents=True, exist_ok=True)
  path.write_text(
    "\n".join((
      'schema = "tach-route-coverage-v1"',
      "",
      "[[case]]",
      'id = "native"',
      'targets = ["x86_64-unknown-linux-gnu"]',
      f"modes = [{mode_rows}]",
      'runtime_profile = "native_criterion"',
      'producer = "test-producer"',
      'route_contract = "three_timer_direct"',
      'runtime_proof = "not_collected"',
      "",
    )),
    encoding="utf-8",
  )


def git(root: Path, *args: str) -> str:
  result = subprocess.run(
    ["git", *args],
    cwd=root,
    capture_output=True,
    text=True,
    check=False,
  )
  if result.returncode:
    raise AssertionError(f"git {' '.join(args)} failed: {result.stderr}")
  return result.stdout.strip()


class ReleaseMatrixWiringTests(unittest.TestCase):
  def validate(
    self,
    directory: Path,
    route_matrix: release_matrix.RouteMatrix | None,
    *,
    primary_report: dict | None = None,
    primary_side_effect=None,
  ) -> dict:
    report = passed_primary_report() if primary_report is None else primary_report

    def validate_primary(*_args, **_kwargs):
      if primary_side_effect is not None:
        primary_side_effect()
      return report

    campaign_loader = (
      mock.patch.object(
        RELEASE_VALIDATOR,
        "load_campaign_route_matrix",
        return_value=(route_matrix, bound_manifest_report(report["source_revision"])),
      )
      if route_matrix is not None
      else nullcontext()
    )

    with mock.patch.object(
      RELEASE_VALIDATOR,
      "validate_primary_snapshots",
      side_effect=validate_primary,
    ), mock.patch.object(
      RELEASE_VALIDATOR,
      "validate_supplemental_snapshots",
      return_value=passed_supplemental_report(report["source_revision"]),
    ), campaign_loader:
      return RELEASE_VALIDATOR.validate_release_evidence(directory, directory)

  def test_legacy_primary_cell_without_retained_observation_is_rejected(self) -> None:
    with tempfile.TemporaryDirectory() as temporary:
      directory = Path(temporary)
      write_json(directory / PRIMARY_ARTIFACT, primary_document(legacy=True))

      report = self.validate(directory, release_matrix.RouteMatrix((requirement("default"),)))

    self.assertFalse(report["passed"])
    binding_failures = report["route_matrix"]["bindings"]["failures"]
    self.assertTrue(any("legacy primary evidence lacks" in failure for failure in binding_failures))

  def test_primary_report_without_bound_cells_cannot_admit_a_hash_labeled_legacy_cell(self) -> None:
    with tempfile.TemporaryDirectory() as temporary:
      directory = Path(temporary)
      write_json(directory / PRIMARY_ARTIFACT, primary_document())
      write_manifest(directory, [route_observation(PRIMARY_ARTIFACT)])
      legacy_report = {"passed": True, "failures": [], "source_revision": SOURCE_REVISION}

      report = self.validate(
        directory,
        release_matrix.RouteMatrix((requirement("default"),)),
        primary_report=legacy_report,
      )

    self.assertFalse(report["passed"])
    binding_failures = report["route_matrix"]["bindings"]["failures"]
    self.assertTrue(any(
      "did not report per-artifact retained observations" in failure
      for failure in binding_failures
    ))
    self.assertTrue(any(
      "has no validator-bound retained observation" in failure
      for failure in binding_failures
    ))

  def test_default_observation_cannot_cover_no_default_route(self) -> None:
    with tempfile.TemporaryDirectory() as temporary:
      directory = Path(temporary)
      write_json(directory / PRIMARY_ARTIFACT, primary_document())
      write_manifest(directory, [route_observation(PRIMARY_ARTIFACT)])

      report = self.validate(
        directory,
        release_matrix.RouteMatrix((requirement("default"), requirement("no-default"))),
      )

    self.assertFalse(report["passed"])
    failures = report["route_matrix"]["admission"]["failures"]
    self.assertTrue(any(
      failure["code"] == "missing_coverage"
      and failure["identity"]["build_mode"] == "no-default"
      for failure in failures
    ))

  def test_observation_mismatch_is_rejected_before_matrix_admission(self) -> None:
    with tempfile.TemporaryDirectory() as temporary:
      directory = Path(temporary)
      write_json(directory / PRIMARY_ARTIFACT, primary_document())
      write_manifest(directory, [route_observation(
        PRIMARY_ARTIFACT,
        target="aarch64-unknown-linux-gnu",
      )])

      report = self.validate(directory, release_matrix.RouteMatrix((requirement("default"),)))

    self.assertFalse(report["passed"])
    binding_failures = report["route_matrix"]["bindings"]["failures"]
    self.assertTrue(any("target does not match validated document" in failure for failure in binding_failures))
    self.assertEqual(report["route_matrix"]["bindings"]["artifacts"], [])

  def test_release_report_includes_structured_route_matrix_admission(self) -> None:
    with tempfile.TemporaryDirectory() as temporary:
      directory = Path(temporary)
      write_json(directory / PRIMARY_ARTIFACT, primary_document())
      write_manifest(directory, [route_observation(PRIMARY_ARTIFACT)])

      report = self.validate(directory, release_matrix.RouteMatrix((requirement("default"),)))

    route_matrix = report["route_matrix"]
    self.assertTrue(report["passed"], report["failures"])
    self.assertEqual(route_matrix["schema"], "tach-release-route-matrix-admission-v1")
    self.assertTrue(route_matrix["passed"])
    self.assertEqual(route_matrix["admission"]["schema"], "tach-release-route-matrix-report-v1")
    self.assertEqual(len(route_matrix["admission"]["decisions"]), 1)

  def test_document_swap_after_validation_cannot_rebind_route_observation(self) -> None:
    """A route manifest must bind the bytes validated before an attacker swap."""
    with tempfile.TemporaryDirectory() as temporary:
      directory = Path(temporary)
      artifact_path = directory / PRIMARY_ARTIFACT
      write_json(artifact_path, primary_document())
      swapped = primary_document()
      swapped["unvalidated_payload"] = "attacker-controlled-after-primary-validation"
      swapped_bytes = json.dumps(swapped, indent=2).encode("utf-8") + b"\n"
      write_json(directory / RELEASE_VALIDATOR.ROUTE_OBSERVATIONS_FILENAME, {
        "schema": RELEASE_VALIDATOR.ROUTE_OBSERVATIONS_SCHEMA,
        "source_revision": SOURCE_REVISION,
        "bindings": [{
          "artifact_id": PRIMARY_ARTIFACT,
          "document_sha256": hashlib.sha256(swapped_bytes).hexdigest(),
          "route_observation": route_observation(PRIMARY_ARTIFACT),
        }],
        "equivalences": [],
      })

      def swap_document() -> None:
        artifact_path.write_bytes(swapped_bytes)

      report = self.validate(
        directory,
        release_matrix.RouteMatrix((requirement("default"),)),
        primary_side_effect=swap_document,
      )

    self.assertFalse(report["passed"])
    self.assertTrue(any(
      "document_sha256 does not bind captured document bytes" in failure
      for failure in report["route_matrix"]["bindings"]["failures"]
    ))

  def test_live_route_manifest_swap_cannot_shrink_campaign_requirements(self) -> None:
    """Admission must use the campaign commit, not a later live TOML edit."""
    with tempfile.TemporaryDirectory() as temporary:
      root = Path(temporary)
      route_path = root / RELEASE_VALIDATOR.ROUTE_COVERAGE_GIT_PATH
      write_route_coverage(route_path, ["default", "no-default"])
      committed_route_bytes = route_path.read_bytes()
      git(root, "init", "-q")
      git(root, "add", RELEASE_VALIDATOR.ROUTE_COVERAGE_GIT_PATH)
      git(
        root,
        "-c", "user.email=tach-test@example.invalid",
        "-c", "user.name=tach test",
        "commit", "-qm", "route coverage",
      )
      source_revision = git(root, "rev-parse", "HEAD")

      write_json(
        root / PRIMARY_ARTIFACT,
        primary_document(source_revision=source_revision),
      )
      write_manifest(
        root,
        [route_observation(PRIMARY_ARTIFACT, source_revision=source_revision)],
        source_revision=source_revision,
      )

      def shrink_live_manifest() -> None:
        write_route_coverage(route_path, ["default"])

      live_loader = release_matrix.load_route_matrix
      with mock.patch.object(
        RELEASE_VALIDATOR.release_matrix,
        "load_route_matrix",
        side_effect=lambda: live_loader(route_path),
      ):
        report = self.validate(
          root,
          None,
          primary_report=passed_primary_report(source_revision),
          primary_side_effect=shrink_live_manifest,
        )

    self.assertFalse(report["passed"])
    manifest_binding = report["route_matrix"]["manifest_binding"]
    self.assertTrue(manifest_binding["passed"])
    self.assertEqual(manifest_binding["source_revision"], source_revision)
    self.assertEqual(
      manifest_binding["sha256"], hashlib.sha256(committed_route_bytes).hexdigest()
    )
    self.assertTrue(any(
      failure["code"] == "missing_coverage"
      and failure["identity"]["build_mode"] == "no-default"
      for failure in report["route_matrix"]["admission"]["failures"]
    ))

  def test_equivalences_are_prohibited_before_closure_digest_is_canonical(self) -> None:
    with tempfile.TemporaryDirectory() as temporary:
      directory = Path(temporary)
      write_json(directory / PRIMARY_ARTIFACT, primary_document())
      write_manifest(directory, [route_observation(PRIMARY_ARTIFACT)], equivalences=[{
        "artifact_id": PRIMARY_ARTIFACT,
      }])

      report = self.validate(directory, release_matrix.RouteMatrix((requirement("default"),)))

    self.assertFalse(report["passed"])
    self.assertTrue(any(
      "mode equivalences are prohibited" in failure
      for failure in report["route_matrix"]["bindings"]["failures"]
    ))


if __name__ == "__main__":
  unittest.main()
