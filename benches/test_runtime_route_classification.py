from __future__ import annotations

from dataclasses import replace
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


BENCHES_DIR = Path(__file__).resolve().parent
if str(BENCHES_DIR) not in sys.path:
  sys.path.insert(0, str(BENCHES_DIR))

import release_matrix
import runtime_route_classification


REVISION = "a" * 40


class RuntimeRouteClassificationTests(unittest.TestCase):
  def test_checked_in_matrix_classifies_every_exact_runtime_identity(self) -> None:
    report = runtime_route_classification.classify(REVISION)

    self.assertEqual(report["counts"]["runtime_identities"], 55)
    self.assertEqual(report["counts"]["by_evidence_kind"], {
      "full_speed": 45,
      "runtime_smoke": 4,
      "tagged_wall_fallback": 6,
    })
    self.assertEqual(report["counts"]["by_classification"], {
      "producer_ready_artifact_declared": 55,
    })
    self.assertEqual(len(report["routes"]), 55)
    self.assertTrue(all(route["producer"]["state"] == "ready" for route in report["routes"]))
    self.assertTrue(all(route["artifact_id"] is not None for route in report["routes"]))

  def test_fallback_artifacts_match_fallback_requirements(self) -> None:
    report = runtime_route_classification.classify(REVISION)
    by_artifact = {
      route["artifact_id"]: route
      for route in report["routes"]
      if route["artifact_id"] is not None
    }
    fallback_artifacts = {
      "speed-supplemental-browser-negative.json",
      "speed-supplemental-browser-negative-no-default.json",
      "speed-supplemental-wasi-p1-wasmtime.json",
      "speed-supplemental-wasi-p1-wasmtime-no-default.json",
      "speed-supplemental-wasi-p2-wasmtime.json",
      "speed-supplemental-wasi-p2-wasmtime-no-default.json",
    }
    self.assertEqual(
      {
        artifact_id
        for artifact_id, route in by_artifact.items()
        if route["required_evidence_kind"]
        == release_matrix.EvidenceKind.TAGGED_WALL_FALLBACK.value
      },
      fallback_artifacts,
    )
    for artifact_id in fallback_artifacts:
      self.assertEqual(
        by_artifact[artifact_id]["required_evidence_kind"],
        release_matrix.EvidenceKind.TAGGED_WALL_FALLBACK.value,
      )

  def test_unready_producer_is_an_explicit_gap(self) -> None:
    with tempfile.TemporaryDirectory() as temporary:
      path = Path(temporary) / "route-coverage.toml"
      rendered = release_matrix.DEFAULT_ROUTE_COVERAGE_PATH.read_text().replace(
        'id = "criterion_linux_adaptive"\nkind = "criterion"\nstate = "ready"',
        'id = "criterion_linux_adaptive"\nkind = "criterion"\nstate = "planned"',
        1,
      )
      path.write_text(rendered)
      report = runtime_route_classification.classify(REVISION, path)
    affected = [
      route for route in report["routes"]
      if route["producer"]["id"] == "criterion_linux_adaptive"
    ]
    self.assertTrue(affected)
    self.assertTrue(all(route["classification"] == "open_producer_gap" for route in affected))

  def test_artifact_kind_mismatch_fails_closed(self) -> None:
    routes = list(runtime_route_classification.artifact_routes())
    index = next(
      index for index, route in enumerate(routes)
      if route.artifact_id == "speed-supplemental-browser-negative.json"
    )
    routes[index] = replace(
      routes[index], evidence_kind=release_matrix.EvidenceKind.FULL_SPEED
    )
    with mock.patch.object(
      runtime_route_classification, "artifact_routes", return_value=tuple(routes)
    ):
      with self.assertRaisesRegex(ValueError, "requires 'tagged_wall_fallback'"):
        runtime_route_classification.classify(REVISION)

  def test_classification_writer_never_replaces_evidence(self) -> None:
    document = runtime_route_classification.classify(REVISION)
    with tempfile.TemporaryDirectory() as temporary:
      path = Path(temporary) / "runtime-routes.json"
      runtime_route_classification.write_exclusive(path, document)
      original = path.read_bytes()
      with self.assertRaises(FileExistsError):
        runtime_route_classification.write_exclusive(path, document)
      self.assertEqual(path.read_bytes(), original)


if __name__ == "__main__":
  unittest.main()
