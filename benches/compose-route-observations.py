#!/usr/bin/env python3
"""Finalize retained evidence documents into exact route observations."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import stat
import sys

import release_matrix
import route_observation
import speed_evidence


ROOT = Path(__file__).resolve().parent
OUTPUT_NAME = "route-observations-v1.json"


def _load_release_validator():
  module_name = "tach_route_observation_release_validator"
  existing = sys.modules.get(module_name)
  if existing is not None:
    return existing
  spec = importlib.util.spec_from_file_location(
    module_name, ROOT / "validate-release-evidence.py"
  )
  if spec is None or spec.loader is None:
    raise RuntimeError("could not load release evidence validator")
  module = importlib.util.module_from_spec(spec)
  sys.modules[module_name] = module
  spec.loader.exec_module(module)
  return module


def _failures_text(failures: list[str]) -> str:
  return "route-observation composition failed:\n  " + "\n  ".join(failures)


def _validated_contexts(data_dir: Path, checkout_root: Path):
  validator = _load_release_validator()
  primary, primary_load_failures = validator.load_primary_campaign(data_dir)
  supplemental, _, supplemental_load_failures = (
    validator.load_supplemental_campaign(data_dir)
  )
  failures = [*primary_load_failures, *supplemental_load_failures]
  discovered = {path.name for path in data_dir.glob("speed-*.json")}
  loaded = {*primary, *supplemental}
  unexpected = sorted(discovered - loaded)
  if unexpected:
    failures.append(f"campaign contains unknown evidence documents: {unexpected!r}")
  if not primary and not supplemental:
    failures.append("campaign contains no evidence documents")

  contexts = {}
  snapshots = {**primary, **supplemental}
  for artifact_id, snapshot in sorted(primary.items()):
    bundle_path, bundle_error = validator.retained_collector_bundle_path(
      snapshot.directory, snapshot.document
    )
    if bundle_path is None:
      failures.append(
        f"primary {artifact_id}: {bundle_error or 'missing retained collector bundle'}"
      )
      continue
    result = speed_evidence.validate_primary_speed_cell_from_bundle(
      artifact_id, snapshot.document, bundle_path
    )
    failures.extend(result.get("failures", []))
    context, context_failures = validator._primary_context(
      artifact_id, snapshot.document, result
    )
    failures.extend(f"primary {artifact_id}: {failure}" for failure in context_failures)
    if result.get("passed") is True and not context_failures:
      contexts[artifact_id] = context

  for artifact_id, snapshot in sorted(supplemental.items()):
    expected = speed_evidence.SUPPLEMENTAL_SPEED_CELLS.get(artifact_id)
    if expected is None:
      failures.append(f"supplemental {artifact_id}: unknown supplemental artifact")
      continue
    mode = expected[2]
    if mode in {"full_speed_cell", "tagged_wall_fallback"}:
      bundle_path, bundle_error = validator.retained_collector_bundle_path(
        snapshot.directory, snapshot.document
      )
      if bundle_path is None:
        failures.append(
          f"supplemental {artifact_id}: "
          f"{bundle_error or 'missing retained collector bundle'}"
        )
        continue
      result = speed_evidence.validate_supplemental_speed_cell_from_bundle(
        artifact_id, snapshot.document, bundle_path
      )
    elif mode == "runtime_smoke":
      result = speed_evidence.validate_supplemental_speed_cell(
        artifact_id, snapshot.document
      )
    else:
      result = speed_evidence.validate_supplemental_speed_cell(
        artifact_id, snapshot.document
      )
      result["failures"] = [
        *result.get("failures", []),
        f"supplemental {artifact_id}: {mode} lacks a retained producer observation",
      ]
      result["passed"] = False
    failures.extend(result.get("failures", []))
    context, context_failures = validator._supplemental_context(
      artifact_id, snapshot.document
    )
    failures.extend(
      f"supplemental {artifact_id}: {failure}" for failure in context_failures
    )
    if result.get("passed") is True and not context_failures:
      contexts[artifact_id] = context

  if set(contexts) != set(snapshots):
    missing = sorted(set(snapshots) - set(contexts))
    if missing:
      failures.append(f"campaign has invalid or unbound artifacts: {missing!r}")
  revisions = {
    context.get("source_revision")
    for context in contexts.values()
    if isinstance(context.get("source_revision"), str)
  }
  if failures:
    raise ValueError(_failures_text(failures))
  matrices = {}
  for revision in sorted(revisions):
    checkout_failures, _ = validator.validate_shipping_code_binding(
      checkout_root,
      revision,
      validator.REQUIRED_SHIPPING_PATHS,
    )
    if checkout_failures:
      failures.extend(f"{revision}: {failure}" for failure in checkout_failures)
      continue
    matrix, _ = validator.load_campaign_route_matrix(checkout_root, revision)
    matrices[revision] = matrix
  if failures:
    raise ValueError(_failures_text(failures))
  source_revision = next(iter(revisions)) if len(revisions) == 1 else None
  matrix = next(iter(matrices.values())) if len(matrices) == 1 else matrices
  return source_revision, matrix, snapshots, contexts


def compose_campaign(data_dir: Path, checkout_root: Path) -> dict[str, object]:
  """Validate an assembled subset and compose its deterministic manifest."""
  source_revision, matrix, snapshots, contexts = _validated_contexts(
    data_dir, checkout_root
  )
  return compose_validated_subset(source_revision, matrix, snapshots, contexts)


def compose_validated_subset(
  source_revision: str,
  matrix: release_matrix.RouteMatrix | dict[str, release_matrix.RouteMatrix],
  snapshots: dict[str, object],
  contexts: dict[str, dict[str, object]],
) -> dict[str, object]:
  """Bind already validated snapshot bytes to their unique committed requirements."""
  if set(snapshots) != set(contexts):
    raise ValueError("validated snapshots and contexts differ")
  artifacts = []
  for artifact_id, snapshot in sorted(snapshots.items()):
    context = contexts[artifact_id]
    artifact_revision = context.get("source_revision")
    if not isinstance(artifact_revision, str):
      raise ValueError(
        _failures_text([f"artifact {artifact_id!r} has no source revision"])
      )
    if source_revision is not None and artifact_revision != source_revision:
      raise ValueError(
        _failures_text([f"artifact {artifact_id!r} uses a different source revision"])
      )
    artifact_matrix = (
      matrix.get(artifact_revision)
      if isinstance(matrix, dict)
      else matrix
    )
    if artifact_matrix is None:
      raise ValueError(
        _failures_text([
          f"artifact {artifact_id!r} has no static matrix for revision {artifact_revision}"
        ])
      )
    matches = [
      requirement
      for requirement in artifact_matrix.requirements
      if requirement.identity.target == context["target"]
      and requirement.identity.build_mode == context["build_mode"]
      and requirement.identity.runtime_profile == context["runtime_profile"]
    ]
    if len(matches) != 1:
      raise ValueError(
        _failures_text([
          f"artifact {artifact_id!r} matches {len(matches)} committed route requirements"
        ])
      )
    requirement = matches[0]
    if requirement.required_kind is not context["evidence_kind"]:
      raise ValueError(
        _failures_text([
          f"artifact {artifact_id!r} evidence kind does not match its committed requirement"
        ])
      )
    artifacts.append(route_observation.ArtifactBindingInput(
      artifact_id,
      snapshot.sha256,
      requirement,
      artifact_revision,
    ))
  return route_observation.compose_manifest(source_revision, artifacts)


def write_exclusive_no_replace(path: Path, document: dict[str, object]) -> None:
  """Publish complete JSON by committing its first byte last.

  Until the final one-byte write succeeds, the exclusive destination begins
  with an invalid JSON sentinel. Failures are retained so strict readers fail
  closed and retries require a fresh campaign directory. The read-only mode
  blocks permission-honoring secondary writers while the creator descriptor
  remains writable for the final commit.
  """
  rendered = (json.dumps(document, indent=2) + "\n").encode("utf-8")
  if not rendered.startswith(b"{"):
    raise ValueError("route-observation manifest must be a JSON object")
  invalid_body = b"!" + rendered[1:]
  flags = os.O_RDWR | os.O_CREAT | os.O_EXCL | getattr(os, "O_BINARY", 0)
  try:
    descriptor = os.open(path, flags, 0o400)
  except FileExistsError as error:
    raise ValueError(f"refusing to replace existing destination: {path}") from error
  try:
    _write_all(descriptor, invalid_body)
    os.fsync(descriptor)
    owned_identity = os.fstat(descriptor)
    if (
      not stat.S_ISREG(owned_identity.st_mode)
      or owned_identity.st_size != len(invalid_body)
    ):
      raise ValueError("route-observation destination has an invalid staged inode")
    os.lseek(descriptor, 0, os.SEEK_SET)
    if _read_exact(descriptor, len(invalid_body)) != invalid_body:
      raise ValueError("route-observation destination changed before commit")
    published_path = os.stat(path, follow_symlinks=False)
    if (
      not _same_inode(published_path, owned_identity)
      or not stat.S_ISREG(published_path.st_mode)
    ):
      raise ValueError("route-observation path does not name the staged inode")
    os.lseek(descriptor, 0, os.SEEK_SET)
    if os.write(descriptor, b"{") != 1:
      raise OSError("could not commit route-observation manifest")
  except BaseException:
    try:
      os.close(descriptor)
    except OSError:
      pass
    raise

  try:
    os.close(descriptor)
  except OSError:
    pass


def _write_all(descriptor: int, payload: bytes) -> None:
  view = memoryview(payload)
  while view:
    written = os.write(descriptor, view)
    if written <= 0:
      raise OSError("could not write route-observation manifest")
    view = view[written:]


def _read_exact(descriptor: int, size: int) -> bytes:
  chunks = []
  remaining = size
  while remaining:
    chunk = os.read(descriptor, remaining)
    if not chunk:
      break
    chunks.append(chunk)
    remaining -= len(chunk)
  return b"".join(chunks)


def _same_inode(left: os.stat_result, right: os.stat_result) -> bool:
  return left.st_dev == right.st_dev and left.st_ino == right.st_ino


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("data_dir", type=Path)
  parser.add_argument("--checkout-root", type=Path, default=ROOT.parent)
  args = parser.parse_args()
  try:
    if args.data_dir.is_symlink():
      raise ValueError(f"evidence directory must not be a symbolic link: {args.data_dir}")
    directory = args.data_dir.resolve(strict=True)
    if not directory.is_dir():
      raise ValueError(f"evidence path is not a directory: {args.data_dir}")
    destination = directory / OUTPUT_NAME
    if destination.exists() or destination.is_symlink():
      raise ValueError(f"refusing to replace existing destination: {destination}")
    document = compose_campaign(directory, args.checkout_root)
    write_exclusive_no_replace(destination, document)
  except (OSError, release_matrix.RouteMatrixError, ValueError) as error:
    parser.error(str(error))
  print(destination)


if __name__ == "__main__":
  main()
