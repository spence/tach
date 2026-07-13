#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import sys
import tempfile
import unittest
from unittest import mock


BENCHES_DIR = Path(__file__).resolve().parent
if str(BENCHES_DIR) not in sys.path:
  sys.path.insert(0, str(BENCHES_DIR))

import release_matrix
import route_observation


def load_composer():
  name = "tach_route_observation_composer_tests"
  spec = importlib.util.spec_from_file_location(
    name, BENCHES_DIR / "compose-route-observations.py"
  )
  assert spec is not None and spec.loader is not None
  module = importlib.util.module_from_spec(spec)
  sys.modules[name] = module
  spec.loader.exec_module(module)
  return module


COMPOSER = load_composer()
REVISION = "a" * 40


def requirement(
  artifact: str,
  target: str = "x86_64-unknown-linux-gnu",
) -> tuple[str, release_matrix.CoverageRequirement]:
  identity = release_matrix.RouteIdentity(
    f"case-{artifact}", target, "default", "native_criterion"
  )
  return artifact, release_matrix.CoverageRequirement(
    identity,
    release_matrix.EvidenceKind.FULL_SPEED,
    f"producer-{artifact}",
    "three_timer_direct",
  )


class RouteObservationTests(unittest.TestCase):
  def test_closure_digest_uses_exact_canonical_bytes(self) -> None:
    artifact, item = requirement("speed-a.json")
    document_sha256 = "b" * 64
    payload = {
      "schema": "tach-route-observation-closure-v1",
      "artifact_id": artifact,
      "document_sha256": document_sha256,
      "source_revision": REVISION,
      "identity": item.identity.to_mapping(),
      "evidence_kind": "full_speed",
      "producer": item.producer,
      "route_contract": item.route_contract,
    }
    expected = hashlib.sha256(json.dumps(
      payload, ensure_ascii=True, separators=(",", ":"), sort_keys=True
    ).encode()).hexdigest()

    self.assertEqual(
      route_observation.closure_digest(artifact, document_sha256, REVISION, item),
      expected,
    )

  def test_manifest_order_is_deterministic_and_equivalence_is_empty(self) -> None:
    artifact_a, requirement_a = requirement("speed-a.json")
    artifact_b, requirement_b = requirement("speed-b.json", "aarch64-unknown-linux-gnu")
    inputs = [
      route_observation.ArtifactBindingInput(artifact_b, "d" * 64, requirement_b),
      route_observation.ArtifactBindingInput(artifact_a, "c" * 64, requirement_a),
    ]

    first = route_observation.compose_manifest(REVISION, inputs)
    second = route_observation.compose_manifest(REVISION, list(reversed(inputs)))

    self.assertEqual(first, second)
    self.assertEqual(
      [binding["artifact_id"] for binding in first["bindings"]],
      [artifact_a, artifact_b],
    )
    self.assertEqual(first["equivalences"], [])

  def test_one_artifact_core_manifest_does_not_satisfy_a_two_identity_matrix(self) -> None:
    artifact_a, requirement_a = requirement("speed-a.json")
    _, requirement_b = requirement("speed-b.json", "aarch64-unknown-linux-gnu")
    document_sha256 = "c" * 64
    manifest = route_observation.compose_manifest(
      REVISION,
      [route_observation.ArtifactBindingInput(
        artifact_a,
        document_sha256,
        requirement_a,
      )],
    )

    binding = manifest["bindings"][0]
    observation = release_matrix.ObservedCoverage.from_mapping(
      binding["route_observation"]
    )
    self.assertEqual(binding["artifact_id"], artifact_a)
    self.assertEqual(binding["document_sha256"], document_sha256)
    self.assertEqual(observation.identity, requirement_a.identity)
    self.assertEqual(
      observation.frozen.closure_digest,
      route_observation.closure_digest(
        artifact_a,
        document_sha256,
        REVISION,
        requirement_a,
      ),
    )

    report = release_matrix.validate_route_matrix(
      release_matrix.RouteMatrix((requirement_a, requirement_b)),
      [observation],
    )
    self.assertFalse(report.passed)
    self.assertTrue(any(
      failure.code is release_matrix.FailureCode.MISSING_COVERAGE
      and failure.identity == requirement_b.identity
      for failure in report.failures
    ))

  def test_validated_subset_rejects_mixed_revision_and_unknown_identity(self) -> None:
    artifact, item = requirement("speed-a.json")
    snapshots = {artifact: SimpleNamespace(sha256="c" * 64)}
    context = {
      "target": item.identity.target,
      "build_mode": item.identity.build_mode,
      "runtime_profile": item.identity.runtime_profile,
      "evidence_kind": item.required_kind,
      "source_revision": "d" * 40,
    }
    matrix = release_matrix.RouteMatrix((item,))
    with self.assertRaisesRegex(ValueError, "different source revision"):
      COMPOSER.compose_validated_subset(
        REVISION, matrix, snapshots, {artifact: context}
      )

    context["source_revision"] = REVISION
    context["target"] = "i686-unknown-linux-gnu"
    with self.assertRaisesRegex(ValueError, "matches 0 committed route requirements"):
      COMPOSER.compose_validated_subset(
        REVISION, matrix, snapshots, {artifact: context}
      )

  def test_exclusive_writer_rejects_existing_file_and_symlink(self) -> None:
    document = {"schema": route_observation.MANIFEST_SCHEMA}
    with tempfile.TemporaryDirectory() as temporary:
      root = Path(temporary)
      destination = root / "route-observations-v1.json"
      destination.write_text("existing")
      with self.assertRaisesRegex(ValueError, "refusing to replace"):
        COMPOSER.write_exclusive_no_replace(destination, document)
      self.assertEqual(destination.read_text(), "existing")
      destination.unlink()
      target = root / "target"
      target.write_text("target")
      destination.symlink_to(target)
      with self.assertRaisesRegex(ValueError, "refusing to replace"):
        COMPOSER.write_exclusive_no_replace(destination, document)
      self.assertTrue(destination.is_symlink())
      self.assertEqual(target.read_text(), "target")

  def test_exclusive_writer_preserves_exact_rendered_bytes(self) -> None:
    document = {"schema": route_observation.MANIFEST_SCHEMA, "bindings": []}
    with tempfile.TemporaryDirectory() as temporary:
      root = Path(temporary)
      destination = root / "route-observations-v1.json"
      COMPOSER.write_exclusive_no_replace(destination, document)
      expected = (json.dumps(document, indent=2) + "\n").encode()
      self.assertEqual(destination.read_bytes(), expected)

  def test_final_stat_substitution_rejects_and_preserves_foreign_file(self) -> None:
    document = {"schema": route_observation.MANIFEST_SCHEMA, "bindings": []}
    with tempfile.TemporaryDirectory() as temporary:
      root = Path(temporary)
      destination = root / "route-observations-v1.json"
      real_stat = COMPOSER.os.stat
      substituted = False

      def substitute_before_stat(path, *, follow_symlinks=True):
        nonlocal substituted
        if Path(path) == destination and not substituted:
          substituted = True
          destination.unlink()
          destination.write_bytes(b"foreign destination bytes")
        return real_stat(path, follow_symlinks=follow_symlinks)

      with mock.patch.object(COMPOSER.os, "stat", side_effect=substitute_before_stat):
        with self.assertRaisesRegex(
          ValueError,
          "route-observation path does not name the staged inode",
        ):
          COMPOSER.write_exclusive_no_replace(destination, document)

      self.assertEqual(destination.read_bytes(), b"foreign destination bytes")

  def test_all_but_newline_failure_leaves_sentinel_invalid_destination(self) -> None:
    document = {"schema": route_observation.MANIFEST_SCHEMA, "bindings": []}
    with tempfile.TemporaryDirectory() as temporary:
      root = Path(temporary)
      destination = root / "route-observations-v1.json"
      rendered = (json.dumps(document, indent=2) + "\n").encode()
      real_write = COMPOSER.os.write
      calls = 0

      def fail_after_valid_json(descriptor, data):
        nonlocal calls
        calls += 1
        if calls == 1:
          return real_write(descriptor, data[:-1])
        raise OSError("simulated write failure")

      with mock.patch.object(COMPOSER.os, "write", side_effect=fail_after_valid_json):
        with self.assertRaisesRegex(OSError, "simulated write failure"):
          COMPOSER.write_exclusive_no_replace(destination, document)

      self.assertEqual(destination.read_bytes(), b"!" + rendered[1:-1])
      with self.assertRaises(json.JSONDecodeError):
        json.loads(destination.read_bytes())

  def test_commit_write_failure_leaves_sentinel_invalid_destination(self) -> None:
    document = {"schema": route_observation.MANIFEST_SCHEMA, "bindings": []}
    with tempfile.TemporaryDirectory() as temporary:
      destination = Path(temporary) / "route-observations-v1.json"
      rendered = (json.dumps(document, indent=2) + "\n").encode()
      real_write = COMPOSER.os.write

      def fail_commit(descriptor, data):
        if bytes(data) == b"{":
          raise OSError("simulated commit failure")
        return real_write(descriptor, data)

      with mock.patch.object(COMPOSER.os, "write", side_effect=fail_commit):
        with self.assertRaisesRegex(OSError, "simulated commit failure"):
          COMPOSER.write_exclusive_no_replace(destination, document)

      self.assertEqual(destination.read_bytes(), b"!" + rendered[1:])
      with self.assertRaises(json.JSONDecodeError):
        json.loads(destination.read_bytes())

  def test_precommit_same_inode_mutation_is_detected(self) -> None:
    document = {"schema": route_observation.MANIFEST_SCHEMA, "bindings": []}
    with tempfile.TemporaryDirectory() as temporary:
      destination = Path(temporary) / "route-observations-v1.json"
      real_read = COMPOSER.os.read
      real_lseek = COMPOSER.os.lseek
      real_write = COMPOSER.os.write
      mutated = False

      def mutate_before_read(descriptor, size):
        nonlocal mutated
        if not mutated:
          mutated = True
          real_lseek(descriptor, 1, COMPOSER.os.SEEK_SET)
          real_write(descriptor, b"X")
          real_lseek(descriptor, 0, COMPOSER.os.SEEK_SET)
        return real_read(descriptor, size)

      with mock.patch.object(COMPOSER.os, "read", side_effect=mutate_before_read):
        with self.assertRaisesRegex(ValueError, "changed before commit"):
          COMPOSER.write_exclusive_no_replace(destination, document)

      self.assertTrue(destination.read_bytes().startswith(b"!X"))
      with self.assertRaises(json.JSONDecodeError):
        json.loads(destination.read_bytes())

  def test_close_failure_after_commit_does_not_report_failure(self) -> None:
    document = {"schema": route_observation.MANIFEST_SCHEMA, "bindings": []}
    with tempfile.TemporaryDirectory() as temporary:
      destination = Path(temporary) / "route-observations-v1.json"
      real_close = COMPOSER.os.close

      def close_then_fail(descriptor):
        real_close(descriptor)
        raise OSError("simulated close failure")

      with mock.patch.object(COMPOSER.os, "close", side_effect=close_then_fail):
        COMPOSER.write_exclusive_no_replace(destination, document)

      self.assertEqual(
        destination.read_bytes(),
        (json.dumps(document, indent=2) + "\n").encode(),
      )

  def test_read_only_mode_blocks_separate_writer_at_commit(self) -> None:
    document = {"schema": route_observation.MANIFEST_SCHEMA, "bindings": []}
    with tempfile.TemporaryDirectory() as temporary:
      destination = Path(temporary) / "route-observations-v1.json"
      real_write = COMPOSER.os.write

      def probe_separate_writer(descriptor, data):
        if bytes(data) == b"{":
          flags = COMPOSER.os.O_WRONLY | getattr(COMPOSER.os, "O_BINARY", 0)
          try:
            secondary = COMPOSER.os.open(destination, flags)
          except PermissionError:
            pass
          else:
            COMPOSER.os.close(secondary)
            self.fail("read-only destination admitted a separate writer")
        return real_write(descriptor, data)

      with mock.patch.object(COMPOSER.os, "write", side_effect=probe_separate_writer):
        COMPOSER.write_exclusive_no_replace(destination, document)

      self.assertEqual(
        destination.read_bytes(),
        (json.dumps(document, indent=2) + "\n").encode(),
      )

  def test_cli_publishes_fixed_destination_once(self) -> None:
    document = {
      "schema": route_observation.MANIFEST_SCHEMA,
      "source_revision": REVISION,
      "bindings": [],
      "equivalences": [],
    }
    with tempfile.TemporaryDirectory() as temporary:
      root = Path(temporary)
      argv = [
        "compose-route-observations.py",
        str(root),
        "--checkout-root",
        str(root),
      ]
      with (
        mock.patch.object(COMPOSER, "compose_campaign", return_value=document),
        mock.patch.object(sys, "argv", argv),
        mock.patch("builtins.print"),
      ):
        COMPOSER.main()
        destination = root / COMPOSER.OUTPUT_NAME
        original = destination.read_bytes()
        with self.assertRaises(SystemExit):
          COMPOSER.main()
      self.assertEqual(
        original,
        (json.dumps(document, indent=2) + "\n").encode(),
      )
      self.assertEqual(destination.read_bytes(), original)

  def test_validated_contexts_requires_bound_primary_validation(self) -> None:
    artifact, item = requirement("speed-a.json")
    context = {
      "artifact_id": artifact,
      "target": item.identity.target,
      "build_mode": item.identity.build_mode,
      "runtime_profile": item.identity.runtime_profile,
      "source_revision": REVISION,
      "evidence_kind": item.required_kind,
      "scope": "primary",
    }
    with tempfile.TemporaryDirectory() as temporary:
      root = Path(temporary)
      (root / artifact).write_text("{}")
      bundle = root / "collector.bundle"
      bundle.mkdir()
      snapshot = SimpleNamespace(
        directory=root,
        document={"artifact_id": artifact},
        sha256="c" * 64,
      )
      validator = SimpleNamespace(
        load_primary_campaign=mock.Mock(return_value=({artifact: snapshot}, [])),
        load_supplemental_campaign=mock.Mock(return_value=({}, {}, [])),
        retained_collector_bundle_path=mock.Mock(return_value=(bundle, None)),
        _primary_context=mock.Mock(return_value=(context, [])),
        load_campaign_route_matrix=mock.Mock(
          return_value=(release_matrix.RouteMatrix((item,)), {})
        ),
      )
      passed = {
        "artifact_id": artifact,
        "source_revision": REVISION,
        "passed": True,
        "bound_observation": True,
        "failures": [],
      }
      with (
        mock.patch.object(COMPOSER, "_load_release_validator", return_value=validator),
        mock.patch.object(
          COMPOSER.speed_evidence,
          "validate_primary_speed_cell_from_bundle",
          return_value=passed,
        ) as validate_primary,
        mock.patch.object(
          COMPOSER.speed_evidence,
          "validate_checkout_binding",
          return_value=([], {}),
        ),
      ):
        revision, matrix, snapshots, contexts = COMPOSER._validated_contexts(root, root)
      self.assertEqual(revision, REVISION)
      self.assertEqual(matrix.requirements, (item,))
      self.assertEqual(set(snapshots), {artifact})
      self.assertEqual(contexts, {artifact: context})
      validate_primary.assert_called_once_with(
        artifact,
        snapshot.document,
        bundle,
      )

      failed = {**passed, "passed": False, "bound_observation": False,
                "failures": ["retained bundle mismatch"]}
      with (
        mock.patch.object(COMPOSER, "_load_release_validator", return_value=validator),
        mock.patch.object(
          COMPOSER.speed_evidence,
          "validate_primary_speed_cell_from_bundle",
          return_value=failed,
        ),
      ):
        with self.assertRaisesRegex(ValueError, "retained bundle mismatch"):
          COMPOSER._validated_contexts(root, root)

  def test_manifest_requires_at_least_one_artifact(self) -> None:
    with self.assertRaisesRegex(ValueError, "at least one artifact"):
      route_observation.compose_manifest(REVISION, [])


if __name__ == "__main__":
  unittest.main()
