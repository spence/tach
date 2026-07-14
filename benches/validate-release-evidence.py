#!/usr/bin/env python3
"""Release gate for bound timer evidence and decision-boundary admission.

The speed validators prove the contents of individual campaign documents.  This
module adds the release-only proof that each required document is the retained
observation it claims to be, that the 15 runtime decisions are complete, and
that every decision is an exact member of the frozen static route contract. A
document cannot turn one measured build into another by supplying a
caller-shaped route record: its filename, bytes, target, build mode, source
revision, evidence class, and host profile are all checked before admission.
"""

from __future__ import annotations

import argparse
from collections.abc import Iterable
import ctypes
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess

import release_matrix
import route_observation
import speed_evidence


ROOT = Path(__file__).resolve().parent
ROUTE_OBSERVATIONS_FILENAME = "route-observations-v1.json"
ROUTE_OBSERVATIONS_SCHEMA = "tach-release-route-observations-v1"
MULTI_REVISION_ROUTE_OBSERVATIONS_SCHEMA = "tach-release-route-observations-v2"
ROUTE_COVERAGE_GIT_PATH = "benches/route-coverage.toml"
RELEASE_BOUNDARIES_GIT_PATH = "benches/release-boundaries.toml"
ROUTE_MANIFEST_BINDING_SCHEMA = "tach-release-route-manifest-binding-v1"
SHIPPING_CODE_BINDING_SCHEMA = "tach-shipping-code-binding-v1"
SOURCE_REVISION_RE = re.compile(r"[0-9a-f]{40}(?:[0-9a-f]{24})?")
SHA256_RE = re.compile(r"[0-9a-f]{64}")
REQUIRED_SHIPPING_PATHS = ("Cargo.lock", "Cargo.toml", "src")


def _reject_duplicate_json_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
  result: dict[str, object] = {}
  for key, value in pairs:
    if key in result:
      raise ValueError(f"duplicate JSON key {key!r}")
    result[key] = value
  return result


def _read_regular_file_bytes_once(path: Path, label: str) -> bytes:
  """Read one regular file from one no-follow descriptor.

  The descriptor, rather than a preceding ``lstat()``, is the authority for
  the snapshot.  A rename after ``open`` leaves the descriptor attached to the
  bytes we validate and hash; a final-component symlink is rejected before it
  can redirect the read.
  """
  no_follow = getattr(os, "O_NOFOLLOW", None)
  if os.name == "nt":
    open_descriptor = lambda: _open_windows_no_reparse(path)
  elif no_follow is None:
    raise ValueError(f"secure snapshots require O_NOFOLLOW for {label} {path}")
  else:
    open_descriptor = lambda: os.open(path, os.O_RDONLY | no_follow)
  descriptor = -1
  try:
    descriptor = open_descriptor()
    mode = os.fstat(descriptor).st_mode
    if not stat.S_ISREG(mode):
      raise ValueError(f"{label} is not a regular file: {path}")
    with os.fdopen(descriptor, "rb", closefd=True) as source:
      descriptor = -1
      return source.read()
  except OSError as error:
    raise ValueError(f"could not read {label} {path}: {error}") from error
  finally:
    if descriptor >= 0:
      os.close(descriptor)


def _open_windows_no_reparse(
  path: Path,
  kernel32=None,
  open_osfhandle=None,
  get_last_error=None,
) -> int:
  """Open one Windows path without traversing a final reparse point."""
  class FileInformation(ctypes.Structure):
    _fields_ = [
      ("dwFileAttributes", ctypes.c_uint32),
      ("ftCreationTimeLow", ctypes.c_uint32),
      ("ftCreationTimeHigh", ctypes.c_uint32),
      ("ftLastAccessTimeLow", ctypes.c_uint32),
      ("ftLastAccessTimeHigh", ctypes.c_uint32),
      ("ftLastWriteTimeLow", ctypes.c_uint32),
      ("ftLastWriteTimeHigh", ctypes.c_uint32),
      ("dwVolumeSerialNumber", ctypes.c_uint32),
      ("nFileSizeHigh", ctypes.c_uint32),
      ("nFileSizeLow", ctypes.c_uint32),
      ("nNumberOfLinks", ctypes.c_uint32),
      ("nFileIndexHigh", ctypes.c_uint32),
      ("nFileIndexLow", ctypes.c_uint32),
    ]

  if kernel32 is None:
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
  if open_osfhandle is None:
    import msvcrt
    open_osfhandle = msvcrt.open_osfhandle
  if get_last_error is None:
    get_last_error = ctypes.get_last_error

  handle_type = ctypes.c_void_p
  kernel32.CreateFileW.argtypes = [
    ctypes.c_wchar_p,
    ctypes.c_uint32,
    ctypes.c_uint32,
    ctypes.c_void_p,
    ctypes.c_uint32,
    ctypes.c_uint32,
    handle_type,
  ]
  kernel32.CreateFileW.restype = handle_type
  kernel32.GetFileInformationByHandle.argtypes = [
    handle_type,
    ctypes.POINTER(FileInformation),
  ]
  kernel32.GetFileInformationByHandle.restype = ctypes.c_int
  kernel32.CloseHandle.argtypes = [handle_type]
  kernel32.CloseHandle.restype = ctypes.c_int

  generic_read = 0x80000000
  file_share_read_write_delete = 0x00000007
  open_existing = 3
  file_attribute_normal = 0x00000080
  file_flag_open_reparse_point = 0x00200000
  file_attribute_reparse_point = 0x00000400
  invalid_handle = ctypes.c_void_p(-1).value
  handle = kernel32.CreateFileW(
    str(path),
    generic_read,
    file_share_read_write_delete,
    None,
    open_existing,
    file_attribute_normal | file_flag_open_reparse_point,
    None,
  )
  if handle == invalid_handle:
    error_code = get_last_error()
    raise OSError(error_code, f"CreateFileW failed with Windows error {error_code}", path)

  transferred = False
  try:
    information = FileInformation()
    if not kernel32.GetFileInformationByHandle(handle, ctypes.byref(information)):
      error_code = get_last_error()
      raise OSError(
        error_code,
        f"GetFileInformationByHandle failed with Windows error {error_code}",
        path,
      )
    if information.dwFileAttributes & file_attribute_reparse_point:
      raise ValueError(f"secure snapshots reject a reparse point: {path}")
    flags = (
      os.O_RDONLY
      | getattr(os, "O_BINARY", 0)
      | getattr(os, "O_NOINHERIT", 0)
    )
    descriptor = open_osfhandle(handle, flags)
    if descriptor < 0:
      raise OSError("open_osfhandle returned an invalid descriptor")
    transferred = True
    return descriptor
  finally:
    if not transferred:
      kernel32.CloseHandle(handle)


def parse_strict_json_object(raw: bytes, label: str) -> dict:
  """Decode an exact JSON byte stream into one duplicate-free object."""
  try:
    value = json.loads(raw.decode("utf-8"), object_pairs_hook=_reject_duplicate_json_keys)
  except (UnicodeDecodeError, json.JSONDecodeError, ValueError) as error:
    raise ValueError(f"could not decode {label}: {error}") from error
  if not isinstance(value, dict):
    raise ValueError(f"{label} is not a JSON object")
  return value


def load_strict_json_document(path: Path, label: str) -> tuple[dict, bytes]:
  """Snapshot and decode one regular JSON object without duplicate keys."""
  try:
    raw = _read_regular_file_bytes_once(path, label)
    value = parse_strict_json_object(raw, f"{label} {path}")
  except ValueError as error:
    raise ValueError(f"could not read {label} {path}: {error}") from error
  return value, raw


@dataclass(frozen=True)
class EvidenceSnapshot:
  """One evidence document, parsed and hash-bound from one opened byte stream."""

  artifact_id: str
  path: Path
  directory: Path
  document: dict
  raw: bytes
  sha256: str


@dataclass(frozen=True)
class ReleaseEvidenceSnapshot:
  """One full release-gate evaluation and the exact primary bytes it admitted."""

  report: dict
  primary_cells: tuple[EvidenceSnapshot, ...]
  supplemental_cells: tuple[EvidenceSnapshot, ...]

  def primary_chart_documents(self) -> dict[str, dict]:
    """Return fresh chart inputs decoded only from admitted primary snapshot bytes."""
    if self.report.get("passed") is not True:
      raise ValueError("release evidence snapshot did not pass")
    documents: dict[str, dict] = {}
    for cell in self.primary_cells:
      if cell.artifact_id in documents:
        raise ValueError(f"duplicate primary snapshot artifact {cell.artifact_id!r}")
      documents[cell.artifact_id] = parse_strict_json_object(
        cell.raw,
        f"captured primary evidence {cell.artifact_id}",
      )
    return documents


def snapshot_evidence_document(
  path: Path,
  directory: Path,
  artifact_id: str,
  label: str,
) -> EvidenceSnapshot:
  """Capture the only document bytes this admission run may inspect."""
  document, raw = load_strict_json_document(path, label)
  return EvidenceSnapshot(
    artifact_id=artifact_id,
    path=path,
    directory=directory,
    document=document,
    raw=raw,
    sha256=hashlib.sha256(raw).hexdigest(),
  )


def evidence_directory(data_dir: Path) -> Path:
  """Resolve the evidence root while rejecting a non-directory input."""
  try:
    directory = data_dir.resolve(strict=True)
    mode = directory.lstat().st_mode
  except OSError as error:
    raise ValueError(f"could not read evidence directory {data_dir}: {error}") from error
  if not stat.S_ISDIR(mode):
    raise ValueError(f"evidence directory is not a directory: {data_dir}")
  return directory


def retained_collector_bundle_path(
  cell_directory: Path,
  document: object,
) -> tuple[Path | None, str | None]:
  """Resolve a snapshot-owned full-speed cell's retained bundle safely."""
  try:
    cell_directory = cell_directory.resolve(strict=True)
    directory_mode = cell_directory.lstat().st_mode
  except OSError as error:
    return None, f"could not resolve cell document directory: {error}"
  if not stat.S_ISDIR(directory_mode):
    return None, "cell document directory is not a directory"
  descriptor = document.get("collector_bundle") if isinstance(document, dict) else None
  relative = descriptor.get("path") if isinstance(descriptor, dict) else None
  if not isinstance(relative, str) or not relative or "\\" in relative:
    return None, "collector bundle descriptor has no safe relative path"
  bundle_parts = PurePosixPath(relative)
  if (
    bundle_parts.is_absolute()
    or not bundle_parts.parts
    or any(part in {"", ".", ".."} for part in bundle_parts.parts)
    or bundle_parts.as_posix() != relative
  ):
    return None, "collector bundle descriptor has no safe relative path"

  bundle_path = cell_directory.joinpath(*bundle_parts.parts)
  try:
    bundle_path.resolve(strict=False).relative_to(cell_directory)
  except ValueError:
    return None, "collector bundle escapes the cell document directory"

  current = cell_directory
  bundle_mode = None
  for part in bundle_parts.parts:
    current /= part
    try:
      mode = current.lstat().st_mode
    except FileNotFoundError:
      return None, "retained collector bundle is missing"
    except OSError as error:
      return None, f"could not stat retained collector bundle: {error}"
    if stat.S_ISLNK(mode):
      return None, "retained collector bundle must not contain symbolic links"
    bundle_mode = mode
  if bundle_mode is None or not stat.S_ISDIR(bundle_mode):
    return None, "retained collector bundle is not a directory"
  return bundle_path, None


def load_primary_campaign(
  data_dir: Path,
  artifact_ids: Iterable[str] | None = None,
) -> tuple[dict[str, EvidenceSnapshot], list[str]]:
  """Snapshot primary cells keyed by retained artifact filename."""
  directory = evidence_directory(data_dir)
  documents: dict[str, EvidenceSnapshot] = {}
  failures: list[str] = []
  cell_paths = (
    sorted(directory.glob("speed-[0-9]-*.json"))
    if artifact_ids is None
    else [directory / artifact_id for artifact_id in sorted(artifact_ids)]
  )
  for cell_path in cell_paths:
    artifact_id = cell_path.name
    if not cell_path.exists():
      failures.append(f"required primary artifact is missing: {artifact_id}")
      continue
    try:
      snapshot = snapshot_evidence_document(
        cell_path, directory, artifact_id, f"primary {artifact_id}"
      )
    except ValueError as error:
      failures.append(str(error))
      continue
    documents[artifact_id] = snapshot
  return documents, failures


def load_supplemental_campaign(
  data_dir: Path,
  artifact_ids: Iterable[str] | None = None,
) -> tuple[dict[str, EvidenceSnapshot], dict[str, Path], list[str]]:
  """Snapshot supplemental cells and check their bundle-relative paths."""
  directory = evidence_directory(data_dir)
  documents: dict[str, EvidenceSnapshot] = {}
  cell_paths: dict[str, Path] = {}
  failures: list[str] = []
  cell_paths_to_load = (
    sorted(directory.glob("speed-supplemental-*.json"))
    if artifact_ids is None
    else [directory / artifact_id for artifact_id in sorted(artifact_ids)]
  )
  for cell_path in cell_paths_to_load:
    name = cell_path.name
    if not cell_path.exists():
      failures.append(f"required supplemental artifact is missing: {name}")
      continue
    try:
      snapshot = snapshot_evidence_document(
        cell_path, directory, name, f"supplemental {name}"
      )
    except ValueError as error:
      failures.append(str(error))
      continue
    documents[name] = snapshot
    expected = speed_evidence.SUPPLEMENTAL_SPEED_CELLS.get(name)
    if expected is not None and expected[2] == "full_speed_cell":
      _, error = retained_collector_bundle_path(directory, snapshot.document)
    else:
      error = None
    if error is not None:
      failures.append(f"supplemental {name}: {error}")
      continue
    cell_paths[name] = snapshot.path
  return documents, cell_paths, failures


def validate_supplemental_snapshots(
  snapshots: dict[str, EvidenceSnapshot],
  cell_paths: dict[str, Path],
  binding_failures: list[str] = (),
  expected_artifact_ids: set[str] | None = None,
) -> dict:
  """Validate supplemental claims from their initial document snapshots."""
  documents = {name: snapshot.document for name, snapshot in snapshots.items()}
  validation_options = {"require_bound_observations": True}
  if expected_artifact_ids is not None:
    validation_options["expected_artifact_ids"] = expected_artifact_ids
  report = speed_evidence.validate_supplemental_speed_campaign(
    documents,
    cell_paths,
    **validation_options,
  )
  report = dict(report)
  report["failures"] = [*report.get("failures", []), *binding_failures]
  report["passed"] = not report["failures"]
  return report


def validate_supplemental_campaign(data_dir: Path) -> dict:
  """Validate supplemental claims through one snapshot pass when called directly."""
  snapshots, cell_paths, binding_failures = load_supplemental_campaign(data_dir)
  return validate_supplemental_snapshots(snapshots, cell_paths, binding_failures)


def validate_primary_snapshots(
  snapshots: dict[str, EvidenceSnapshot],
  checkout_root: Path,
  load_failures: list[str] = (),
  expected_artifact_ids: set[str] | None = None,
  shipping_paths: tuple[str, ...] = ("Cargo.lock", "Cargo.toml", "src"),
) -> dict:
  """Validate and bind primary cells without reopening their document paths."""
  documents = {name: snapshot.document for name, snapshot in snapshots.items()}
  failures = [*load_failures]
  results = []
  expected_names = (
    set(speed_evidence.PRIMARY_SPEED_CELLS)
    if expected_artifact_ids is None
    else set(expected_artifact_ids)
  )
  unknown_names = expected_names - set(speed_evidence.PRIMARY_SPEED_CELLS)
  if unknown_names:
    failures.append(f"primary campaign requires unknown artifacts: {sorted(unknown_names)!r}")
  actual_names = set(documents)
  if actual_names != expected_names:
    failures.append(
      "primary three-clock artifacts differ: "
      f"missing={sorted(expected_names - actual_names)!r}, "
      f"unexpected={sorted(actual_names - expected_names)!r}"
    )

  identities = []
  revisions = set()
  for artifact_id in speed_evidence.PRIMARY_SPEED_CELLS:
    if artifact_id not in expected_names:
      continue
    snapshot = snapshots.get(artifact_id)
    if snapshot is None:
      continue
    document = snapshot.document
    provenance = document.get("provenance") if isinstance(document, dict) else None
    provenance = provenance if isinstance(provenance, dict) else {}
    identities.append((
      document.get("order") if isinstance(document, dict) else None,
      document.get("title") if isinstance(document, dict) else None,
      document.get("instance") if isinstance(document, dict) else None,
      document.get("triple") if isinstance(document, dict) else None,
      provenance.get("harness"),
    ))
    bundle_path, bundle_error = retained_collector_bundle_path(
      snapshot.directory,
      document,
    )
    if bundle_path is not None:
      result = speed_evidence.validate_primary_speed_cell_from_bundle(
        artifact_id,
        document,
        bundle_path,
      )
    else:
      result = speed_evidence.validate_primary_speed_cell(artifact_id, document)
      result["failures"] = [
        *result.get("failures", []),
        bundle_error or f"primary {artifact_id}: missing retained collector bundle path",
      ]
      result["passed"] = False
      result["bound_observation"] = False
    failures.extend(result.get("failures", []))
    revision = result.get("source_revision")
    if _source_revision(revision) is not None:
      revisions.add(revision)
    results.append(result)

  expected_identities = tuple(
    values[:5]
    for artifact_id, values in speed_evidence.PRIMARY_SPEED_CELLS.items()
    if artifact_id in expected_names
  )
  if tuple(identities) != expected_identities:
    failures.append(
      "primary campaign environments differ from the decision-boundary contract: "
      f"{identities!r}"
    )
  source_revisions = sorted(revisions)
  checkout_bindings = []
  checkout_failures = []
  for revision in source_revisions:
    revision_failures, binding = validate_shipping_code_binding(
      checkout_root,
      revision,
      shipping_paths,
    )
    checkout_failures.extend(
      f"{revision}: {failure}" for failure in revision_failures
    )
    checkout_bindings.append(binding)
  if not source_revisions:
    checkout_failures.append("campaign has no source revision to bind")
  failures.extend(checkout_failures)
  return {
    "schema": speed_evidence.PRIMARY_SPEED_REPORT_SCHEMA,
    "claim": (
      "each tach timer selects the fastest audited eligible reliable steady-state provider "
      "for its contract in every measured primary environment"
    ),
    "equivalence_rule": (
      "tach is faster than or materially tied with every eligible reference: its point "
      "estimate and conservative 95% CI comparison fit within max(1 ns, 5%)"
    ),
    "source_revision": source_revisions[0] if len(source_revisions) == 1 else None,
    "source_revisions": source_revisions,
    "shipping_code_bindings": {
      "passed": not checkout_failures,
      "revisions": checkout_bindings,
    },
    "passed": not failures,
    "failures": failures,
    "cells": results,
  }


def _source_revision(value: object) -> str | None:
  return value if isinstance(value, str) and SOURCE_REVISION_RE.fullmatch(value) else None


def _sha256(value: object) -> str | None:
  return value if isinstance(value, str) and SHA256_RE.fullmatch(value) else None


def _git_bytes(checkout_root: Path, *arguments: str) -> subprocess.CompletedProcess[bytes]:
  return subprocess.run(
    ["git", "--no-replace-objects", *arguments],
    cwd=checkout_root,
    capture_output=True,
    check=False,
  )


def _git_revision(checkout_root: Path, revision: str) -> str:
  result = _git_bytes(checkout_root, "rev-parse", f"{revision}^{{commit}}")
  if result.returncode:
    raise ValueError(f"Git revision is unavailable: {revision}")
  resolved = result.stdout.decode("ascii", errors="strict").strip()
  if _source_revision(resolved) is None:
    raise ValueError(f"Git revision did not resolve to a full commit: {revision}")
  return resolved


def _git_tree_snapshot(
  checkout_root: Path,
  revision: str,
  paths: tuple[str, ...],
) -> tuple[str, tuple[str, ...]]:
  result = _git_bytes(
    checkout_root,
    "ls-tree",
    "-r",
    "-z",
    "--full-tree",
    revision,
    "--",
    *paths,
  )
  if result.returncode:
    detail = result.stderr.decode("utf-8", errors="replace").strip()
    raise ValueError(
      f"could not list shipping code at {revision}" + (f": {detail}" if detail else "")
    )

  blobs: dict[str, bytes] = {}
  for raw_entry in result.stdout.split(b"\0"):
    if not raw_entry:
      continue
    try:
      raw_header, raw_path = raw_entry.split(b"\t", 1)
      mode, object_type, object_id = raw_header.decode("ascii").split(" ")
      path = raw_path.decode("utf-8")
    except (UnicodeDecodeError, ValueError) as error:
      raise ValueError(f"malformed Git tree entry at {revision}") from error
    if object_type != "blob" or mode not in {"100644", "100755"}:
      raise ValueError(f"shipping code contains unsupported Git entry {path!r}")
    blob = _git_bytes(checkout_root, "cat-file", "blob", object_id)
    if blob.returncode:
      raise ValueError(f"could not read shipping blob {object_id} for {path!r}")
    blobs[path] = blob.stdout

  for declared in paths:
    if not any(path == declared or path.startswith(f"{declared}/") for path in blobs):
      raise ValueError(f"shipping path {declared!r} is absent at {revision}")

  digest = hashlib.sha256()
  digest.update(b"tach-shipping-code-closure-v1\0")
  for path, raw in sorted(blobs.items()):
    encoded_path = path.encode("utf-8")
    digest.update(len(encoded_path).to_bytes(8, "big"))
    digest.update(encoded_path)
    digest.update(len(raw).to_bytes(8, "big"))
    digest.update(raw)
  return digest.hexdigest(), tuple(sorted(blobs))


def validate_shipping_code_binding(
  checkout_root: Path,
  campaign_revision: str,
  shipping_paths: tuple[str, ...],
) -> tuple[list[str], dict[str, object]]:
  """Prove that the candidate ships the exact code measured by the campaign."""
  failures: list[str] = []
  root = checkout_root.resolve()
  if shipping_paths != REQUIRED_SHIPPING_PATHS:
    failures.append(
      "release-boundary shipping paths differ from the required Cargo/src closure"
    )
  try:
    campaign = _git_revision(root, campaign_revision)
    candidate = _git_revision(root, "HEAD")
    campaign_digest, campaign_files = _git_tree_snapshot(root, campaign, shipping_paths)
    candidate_digest, candidate_files = _git_tree_snapshot(root, candidate, shipping_paths)
  except (OSError, UnicodeError, ValueError) as error:
    return [*failures, str(error)], {
      "schema": SHIPPING_CODE_BINDING_SCHEMA,
      "campaign_revision": campaign_revision,
      "candidate_revision": None,
      "shipping_paths": list(shipping_paths),
      "passed": False,
    }

  ancestor = _git_bytes(root, "merge-base", "--is-ancestor", campaign, candidate)
  if ancestor.returncode:
    failures.append("campaign source revision is not an ancestor of the release candidate")
  if campaign_files != candidate_files:
    failures.append("shipping file set changed since the measured campaign")
  if campaign_digest != candidate_digest:
    failures.append("shipping code changed since the measured campaign")

  status = _git_bytes(
    root,
    "status",
    "--porcelain=v1",
    "--untracked-files=all",
    "--",
    *shipping_paths,
  )
  if status.returncode:
    working_changes: list[str] = []
    failures.append("could not inspect local shipping-code inputs")
  else:
    working_changes = [
      line for line in status.stdout.decode("utf-8", errors="replace").splitlines() if line
    ]
    if working_changes:
      failures.append("local shipping code differs from HEAD: " + ", ".join(working_changes))

  return failures, {
    "schema": SHIPPING_CODE_BINDING_SCHEMA,
    "campaign_revision": campaign,
    "candidate_revision": candidate,
    "shipping_paths": list(shipping_paths),
    "campaign_sha256": campaign_digest,
    "candidate_sha256": candidate_digest,
    "file_count": len(candidate_files),
    "files_match": campaign_files == candidate_files,
    "working_tree_changes": working_changes,
    "passed": not failures,
  }


def load_release_boundary_matrix(
  checkout_root: Path,
) -> tuple[release_matrix.DecisionBoundaryMatrix, dict[str, object]]:
  """Load the decision-boundary manifest from committed candidate bytes."""
  root = checkout_root.resolve()
  candidate = _git_revision(root, "HEAD")
  object_name = f"{candidate}:{RELEASE_BOUNDARIES_GIT_PATH}"
  result = _git_bytes(root, "show", object_name)
  if result.returncode:
    detail = result.stderr.decode("utf-8", errors="replace").strip()
    raise release_matrix.RouteMatrixError(
      f"release-boundary manifest is unavailable at {object_name}"
      + (f": {detail}" if detail else "")
    )
  boundaries = release_matrix.parse_release_boundaries_bytes(
    result.stdout,
    f"release candidate {object_name}",
  )
  status = _git_bytes(
    root,
    "status",
    "--porcelain=v1",
    "--untracked-files=all",
    "--",
    RELEASE_BOUNDARIES_GIT_PATH,
  )
  changes = (
    [line for line in status.stdout.decode("utf-8", errors="replace").splitlines() if line]
    if status.returncode == 0
    else ["could not inspect release-boundary manifest worktree state"]
  )
  failures = (
    ["local release-boundary manifest differs from committed candidate: " + ", ".join(changes)]
    if changes
    else []
  )
  return boundaries, {
    "schema": "tach-release-boundary-manifest-binding-v1",
    "path": RELEASE_BOUNDARIES_GIT_PATH,
    "candidate_revision": candidate,
    "sha256": hashlib.sha256(result.stdout).hexdigest(),
    "boundary_count": len(boundaries.boundaries),
    "artifact_ids": list(boundaries.artifact_ids),
    "passed": not failures,
    "failures": failures,
  }


def _primary_build_mode(document: dict) -> tuple[str | None, list[str]]:
  provenance = document.get("provenance")
  features = provenance.get("features") if isinstance(provenance, dict) else None
  matching_modes = [
    build_mode
    for build_mode, expected_features in speed_evidence.BENCHMARK_FEATURES_BY_BUILD_MODE.items()
    if features == list(expected_features)
  ]
  if len(matching_modes) == 1:
    return matching_modes[0], []
  return None, [
    "primary document features do not prove exactly one advertised build mode"
  ]


def _criterion_runtime_profile(target: object) -> str | None:
  if not isinstance(target, str):
    return None
  if target.endswith("-apple-darwin"):
    return "macos_criterion"
  if target.endswith("-pc-windows-msvc"):
    return "windows_criterion"
  if target.endswith("-unknown-freebsd"):
    return "freebsd_criterion"
  if "linux" in target or "android" in target:
    return "native_criterion"
  return None


def _primary_context(
  artifact_id: str,
  document: dict,
  bound_cell: dict[str, object] | None,
) -> tuple[dict[str, object], list[str]]:
  """Derive every route fact primary evidence exposes independently of its label."""
  provenance = document.get("provenance")
  target = document.get("triple")
  revision = provenance.get("source_revision") if isinstance(provenance, dict) else None
  harness = provenance.get("harness") if isinstance(provenance, dict) else None
  build_mode, failures = _primary_build_mode(document)
  runtime_profile = None
  if harness == "criterion":
    runtime_profile = _criterion_runtime_profile(target)
  elif harness == "lambda":
    runtime_profile = {
      "x86_64-unknown-linux-gnu": "aws_lambda_x86_64",
      "aarch64-unknown-linux-gnu": "aws_lambda_aarch64",
    }.get(target)
  else:
    failures.append("primary document has no recognized retained runtime profile")
  if not isinstance(target, str) or not target:
    failures.append("primary document has no target triple")
  if _source_revision(revision) is None:
    failures.append("primary document has no full source revision")
  if runtime_profile is None:
    failures.append("primary document target/harness has no declared runtime profile")
  if document.get("artifact_id") != artifact_id:
    failures.append("primary document artifact_id does not match its retained filename")
  if document.get("build_mode") != build_mode:
    failures.append("primary document build_mode does not match its feature identity")
  if document.get("evidence_kind") != release_matrix.EvidenceKind.FULL_SPEED.value:
    failures.append("primary document evidence_kind is not full_speed")
  if bound_cell is None:
    failures.append("primary document has no validator-bound retained observation")
  else:
    if bound_cell.get("artifact_id") != artifact_id:
      failures.append("primary validator artifact_id does not match retained filename")
    if bound_cell.get("source_revision") != revision:
      failures.append("primary validator source revision does not match document")
    if bound_cell.get("triple") != target:
      failures.append("primary validator target does not match document")
    if bound_cell.get("build_mode") != build_mode:
      failures.append("primary validator build mode does not match document")
    if bound_cell.get("evidence_kind") != release_matrix.EvidenceKind.FULL_SPEED.value:
      failures.append("primary validator evidence kind does not match document")
    if bound_cell.get("bound_observation") is not True or bound_cell.get("passed") is not True:
      failures.append("primary validator did not bind a passing retained observation")
  return {
    "artifact_id": artifact_id,
    "target": target,
    "build_mode": build_mode,
    "runtime_profile": runtime_profile,
    "source_revision": revision,
    "evidence_kind": release_matrix.EvidenceKind.FULL_SPEED,
    "scope": "primary",
  }, failures


def primary_bound_cells(primary_report: dict) -> tuple[dict[str, dict[str, object]], list[str]]:
  """Extract the primary validator's retained-observation result per artifact.

  Legacy campaign reports predate this field.  They deliberately produce no
  eligible records, so a historical JSON cell cannot be promoted merely by
  adding a route-observation registry beside it.
  """
  raw_cells = primary_report.get("cells")
  if not isinstance(raw_cells, list):
    return {}, ["primary validator did not report per-artifact retained observations"]
  cells: dict[str, dict[str, object]] = {}
  failures: list[str] = []
  required_keys = {
    "artifact_id",
    "source_revision",
    "triple",
    "build_mode",
    "evidence_kind",
    "bound_observation",
    "passed",
  }
  for index, raw_cell in enumerate(raw_cells):
    label = f"primary validator cell {index}"
    if not isinstance(raw_cell, dict) or not required_keys <= set(raw_cell):
      failures.append(f"{label}: retained-observation result schema changed")
      continue
    artifact_id = _safe_artifact_name(raw_cell.get("artifact_id"))
    if artifact_id is None:
      failures.append(f"{label}: artifact_id is not a safe filename")
      continue
    if artifact_id in cells:
      failures.append(f"{label}: duplicate artifact_id {artifact_id!r}")
      continue
    cells[artifact_id] = raw_cell
  return cells, failures


def _supplemental_runtime_profile(target: object, harness: object) -> str | None:
  if harness == "criterion":
    return _criterion_runtime_profile(target)
  return {
    "node-wasm-bindgen": "node_wasm_bindgen",
    "browser": "browser_wasm_bindgen",
    "emcc-node": "emcc_node",
    "node-uvwasi": "node_uvwasi",
    "wasmtime": "wasmtime",
    "wasmtime-component": "wasmtime_component",
    "wasi-threads-smoke": "wasi_threads_smoke",
    "wasm32v1-none-smoke": "wasm32v1_none_smoke",
  }.get(harness)


def _supplemental_context(artifact_id: str, document: dict) -> tuple[dict[str, object], list[str]]:
  """Derive supplemental route facts from the independently validated cell contract."""
  expected = speed_evidence.SUPPLEMENTAL_SPEED_CELLS.get(artifact_id)
  if expected is None:
    return {}, [f"supplemental artifact {artifact_id!r} is not declared"]
  expected_target, expected_harness, mode, expected_build_mode = expected
  expected_kind = {
    "full_speed_cell": release_matrix.EvidenceKind.FULL_SPEED,
    "runtime_smoke": release_matrix.EvidenceKind.RUNTIME_SMOKE,
    "tagged_wall_fallback": release_matrix.EvidenceKind.TAGGED_WALL_FALLBACK,
  }.get(mode)
  provenance = document.get("provenance")
  target = document.get("triple")
  build_mode = document.get("build_mode")
  revision = provenance.get("source_revision") if isinstance(provenance, dict) else None
  harness = provenance.get("harness") if isinstance(provenance, dict) else None
  failures: list[str] = []
  if target != expected_target:
    failures.append("supplemental document target disagrees with its artifact identity")
  if build_mode != expected_build_mode:
    failures.append("supplemental document build mode disagrees with its artifact identity")
  if harness != expected_harness:
    failures.append("supplemental document harness disagrees with its artifact identity")
  if _source_revision(revision) is None:
    failures.append("supplemental document has no full source revision")
  runtime_profile = _supplemental_runtime_profile(target, harness)
  if runtime_profile is None:
    failures.append("supplemental document target/harness has no declared runtime profile")
  if expected_kind is None:
    failures.append("supplemental document has an unknown evidence class")
  return {
    "artifact_id": artifact_id,
    "target": target,
    "build_mode": build_mode,
    "runtime_profile": runtime_profile,
    "source_revision": revision,
    "evidence_kind": expected_kind,
    "scope": "supplemental",
  }, failures


def document_contexts(
  primary_documents: dict[str, EvidenceSnapshot],
  bound_primary_cells: dict[str, dict[str, object]],
  supplemental_documents: dict[str, EvidenceSnapshot],
) -> tuple[dict[str, dict[str, object]], list[str]]:
  """Derive route facts only from the initial document snapshots."""
  contexts: dict[str, dict[str, object]] = {}
  failures: list[str] = []
  for artifact_id, snapshot in sorted(primary_documents.items()):
    context, errors = _primary_context(
      artifact_id, snapshot.document, bound_primary_cells.get(artifact_id)
    )
    contexts[artifact_id] = context
    failures.extend(f"primary {artifact_id}: {error}" for error in errors)
  for artifact_id, snapshot in sorted(supplemental_documents.items()):
    context, errors = _supplemental_context(artifact_id, snapshot.document)
    contexts[artifact_id] = context
    failures.extend(f"supplemental {artifact_id}: {error}" for error in errors)
  return contexts, failures


def _safe_artifact_name(value: object) -> str | None:
  if not isinstance(value, str) or not value or "\\" in value:
    return None
  path = PurePosixPath(value)
  if path.is_absolute() or len(path.parts) != 1 or path.name != value:
    return None
  return value


def _binding_failure(report: dict, detail: str) -> None:
  report["failures"].append(detail)


def load_route_observations(
  data_dir: Path,
  documents: dict[str, EvidenceSnapshot],
  contexts: dict[str, dict[str, object]],
  matrix: release_matrix.RouteMatrix,
  artifact_by_identity: dict[release_matrix.RouteIdentity, str] | None = None,
) -> tuple[list[release_matrix.ObservedCoverage], list[release_matrix.ModeEquivalence], dict]:
  """Load the retained, hash-bound observations used for route admission.

  Each record is embedded in a regular, retained manifest and hash-bound to one
  evidence document.  Route records supplied only via a caller argument are
  intentionally not accepted.
  """
  directory = evidence_directory(data_dir)
  manifest_path = directory / ROUTE_OBSERVATIONS_FILENAME
  report = {
    "schema": ROUTE_OBSERVATIONS_SCHEMA,
    "path": ROUTE_OBSERVATIONS_FILENAME,
    "passed": False,
    "failures": [],
    "artifacts": [],
    "ignored_artifacts": [],
  }
  try:
    manifest, _ = load_strict_json_document(manifest_path, "retained route-observation manifest")
  except ValueError as error:
    _binding_failure(report, str(error))
    for artifact_id, context in sorted(contexts.items()):
      if context.get("scope") == "primary":
        _binding_failure(
          report,
          f"primary {artifact_id}: legacy primary evidence lacks a retained "
          "route-observation binding",
        )
    return [], [], report

  schema = manifest.get("schema")
  expected_keys = (
    {"schema", "source_revision", "bindings", "equivalences"}
    if schema == ROUTE_OBSERVATIONS_SCHEMA
    else {"schema", "bindings", "equivalences"}
  )
  if (
    schema not in {
      ROUTE_OBSERVATIONS_SCHEMA,
      MULTI_REVISION_ROUTE_OBSERVATIONS_SCHEMA,
    }
    or set(manifest) != expected_keys
  ):
    _binding_failure(
      report,
      "retained route-observation manifest has an unknown or malformed schema",
    )
    return [], [], report
  manifest_revision = (
    _source_revision(manifest.get("source_revision"))
    if schema == ROUTE_OBSERVATIONS_SCHEMA
    else None
  )
  if schema == ROUTE_OBSERVATIONS_SCHEMA and manifest_revision is None:
    _binding_failure(report, "retained route-observation manifest has no full source revision")
    return [], [], report
  raw_bindings = manifest.get("bindings")
  raw_equivalences = manifest.get("equivalences")
  if not isinstance(raw_bindings, list) or not isinstance(raw_equivalences, list):
    _binding_failure(
      report,
      "retained route-observation manifest has malformed bindings or equivalences",
    )
    return [], [], report

  observations: list[release_matrix.ObservedCoverage] = []
  accepted_artifacts: set[str] = set()
  seen_artifacts: set[str] = set()
  expected_artifacts = set(documents)
  for index, raw_binding in enumerate(raw_bindings):
    label = f"route-observation binding {index}"
    if not isinstance(raw_binding, dict) or set(raw_binding) != {
      "artifact_id", "document_sha256", "route_observation"
    }:
      _binding_failure(report, f"{label}: schema changed")
      continue
    artifact_id = _safe_artifact_name(raw_binding.get("artifact_id"))
    if artifact_id is None:
      _binding_failure(report, f"{label}: artifact_id is not a safe filename")
      continue
    if artifact_id not in expected_artifacts:
      report["ignored_artifacts"].append(artifact_id)
      continue
    if artifact_id in seen_artifacts:
      _binding_failure(report, f"{label}: duplicate artifact_id {artifact_id!r}")
      continue
    seen_artifacts.add(artifact_id)
    snapshot = documents.get(artifact_id)
    context = contexts.get(artifact_id)
    if snapshot is None or context is None:
      _binding_failure(
        report,
        f"{label}: artifact {artifact_id!r} is not a loaded evidence document",
      )
      continue
    digest = _sha256(raw_binding.get("document_sha256"))
    if digest is None:
      _binding_failure(report, f"{label}: document_sha256 is malformed")
      continue
    if snapshot.sha256 != digest:
      _binding_failure(report, f"{label}: document_sha256 does not bind captured document bytes")
      continue
    if context.get("scope") not in {"primary", "supplemental"}:
      _binding_failure(report, f"{label}: artifact has no recognized evidence scope")
      continue
    raw_observation = raw_binding.get("route_observation")
    try:
      observation = release_matrix.ObservedCoverage.from_mapping(raw_observation)
    except release_matrix.RouteMatrixError as error:
      _binding_failure(report, f"{label}: invalid route_observation: {error}")
      continue

    binding_errors: list[str] = []
    if observation.artifact_id != artifact_id:
      binding_errors.append("route_observation artifact_id does not match its filename")
    if observation.identity.target != context.get("target"):
      binding_errors.append("route_observation target does not match validated document")
    if observation.identity.build_mode != context.get("build_mode"):
      binding_errors.append("route_observation build mode does not match validated document")
    if observation.identity.runtime_profile != context.get("runtime_profile"):
      binding_errors.append("route_observation runtime profile does not match validated document")
    if observation.evidence_kind is not context.get("evidence_kind"):
      binding_errors.append("route_observation evidence kind does not match validated document")
    if observation.frozen.source_revision != context.get("source_revision"):
      binding_errors.append("route_observation source revision does not match validated document")
    if (
      manifest_revision is not None
      and observation.frozen.source_revision != manifest_revision
    ):
      binding_errors.append("route_observation source revision does not match retained manifest")
    if observation.frozen.target != context.get("target"):
      binding_errors.append("route_observation frozen target does not match validated document")
    if observation.frozen.runtime_profile != context.get("runtime_profile"):
      binding_errors.append(
        "route_observation frozen runtime profile does not match validated document"
      )
    requirement = matrix.by_identity.get(observation.identity)
    if requirement is None:
      binding_errors.append("route_observation identity is absent from the committed route matrix")
    else:
      expected_artifact = (
        artifact_by_identity.get(observation.identity)
        if artifact_by_identity is not None
        else None
      )
      if expected_artifact is not None and artifact_id != expected_artifact:
        binding_errors.append(
          "route_observation artifact does not match its release decision boundary"
        )
      expected_closure = route_observation.closure_digest(
        artifact_id,
        snapshot.sha256,
        observation.frozen.source_revision,
        requirement,
      )
      if observation.frozen.closure_digest != expected_closure:
        binding_errors.append(
          "route_observation closure digest does not bind its committed requirement"
        )
    if binding_errors:
      for error in binding_errors:
        _binding_failure(report, f"{label}: {error}")
      continue
    observations.append(observation)
    accepted_artifacts.add(artifact_id)
    report["artifacts"].append(artifact_id)

  missing_artifacts = expected_artifacts - seen_artifacts
  if missing_artifacts:
    _binding_failure(
      report,
      "retained route-observation manifest artifacts differ: "
      f"missing={sorted(missing_artifacts)!r}",
    )
  for artifact_id in sorted(expected_artifacts - accepted_artifacts):
    if contexts[artifact_id].get("scope") == "primary":
      _binding_failure(
        report,
        f"primary {artifact_id}: legacy primary evidence lacks a valid retained "
        "route-observation binding",
      )

  equivalences: list[release_matrix.ModeEquivalence] = []
  if raw_equivalences:
    _binding_failure(
      report,
      "mode equivalences are prohibited until a canonical producer-bound closure digest exists",
    )

  report["artifacts"].sort()
  report["ignored_artifacts"].sort()
  report["passed"] = not report["failures"]
  return observations, equivalences, report


def load_campaign_route_matrix(
  checkout_root: Path,
  source_revision: object,
) -> tuple[release_matrix.RouteMatrix, dict[str, object]]:
  """Load the route requirements from the campaign commit, never a live path."""
  revision = _source_revision(source_revision)
  if revision is None:
    raise release_matrix.RouteMatrixError(
      "route admission has no valid campaign source revision for route coverage"
    )
  object_name = f"{revision}:{ROUTE_COVERAGE_GIT_PATH}"
  try:
    result = subprocess.run(
      ["git", "--no-replace-objects", "show", object_name],
      cwd=checkout_root,
      capture_output=True,
      check=False,
    )
  except OSError as error:
    raise release_matrix.RouteMatrixError(
      f"could not load campaign route coverage manifest {object_name}: {error}"
    ) from error
  if result.returncode:
    detail = result.stderr.decode("utf-8", errors="replace").strip()
    raise release_matrix.RouteMatrixError(
      f"campaign route coverage manifest is unavailable at {object_name}"
      + (f": {detail}" if detail else "")
    )
  source = f"campaign source {object_name}"
  matrix = release_matrix.parse_route_coverage_bytes(result.stdout, source)
  return matrix, {
    "schema": ROUTE_MANIFEST_BINDING_SCHEMA,
    "path": ROUTE_COVERAGE_GIT_PATH,
    "source_revision": revision,
    "sha256": hashlib.sha256(result.stdout).hexdigest(),
    "passed": True,
    "failures": [],
  }


def validate_route_matrix_admission(
  data_dir: Path,
  checkout_root: Path,
  primary_documents: dict[str, EvidenceSnapshot],
  primary_report: dict,
  supplemental_documents: dict[str, EvidenceSnapshot],
  boundaries: release_matrix.DecisionBoundaryMatrix,
  boundary_manifest_binding: dict[str, object],
) -> dict:
  """Bind validated evidence to each runtime decision and its static route."""
  bound_primary_cells, primary_binding_failures = primary_bound_cells(primary_report)
  contexts, context_failures = document_contexts(
    primary_documents,
    bound_primary_cells,
    supplemental_documents,
  )
  documents = {**primary_documents, **supplemental_documents}
  try:
    contract_failures = []
    revision_bindings: dict[str, dict[str, object]] = {}
    static_matrices: dict[str, release_matrix.RouteMatrix] = {}
    for boundary in boundaries.boundaries:
      context = contexts.get(boundary.artifact_id)
      revision = _source_revision(
        context.get("source_revision") if context is not None else None
      )
      if revision is None:
        contract_failures.append(
          f"decision boundary {boundary.boundary_id!r} has no artifact source revision"
        )
        continue
      if revision not in static_matrices:
        static_matrix, route_binding = load_campaign_route_matrix(
          checkout_root,
          revision,
        )
        shipping_failures, shipping_binding = validate_shipping_code_binding(
          checkout_root,
          revision,
          boundaries.shipping_paths,
        )
        route_binding = {
          **route_binding,
          "shipping_code_binding": shipping_binding,
          "passed": route_binding["passed"] and not shipping_failures,
          "failures": [*route_binding["failures"], *shipping_failures],
        }
        static_matrices[revision] = static_matrix
        revision_bindings[revision] = route_binding
      static_requirement = static_matrices[revision].by_identity.get(
        boundary.requirement.identity
      )
      if static_requirement != boundary.requirement:
        contract_failures.append(
          f"decision boundary {boundary.boundary_id!r} does not match the static route "
          f"at artifact revision {revision}"
        )
    manifest_failures = [
      f"{revision}: {failure}"
      for revision, revision_binding in sorted(revision_bindings.items())
      for failure in revision_binding["failures"]
    ]
    if len(revision_bindings) == 1:
      manifest_binding = next(iter(revision_bindings.values()))
    else:
      manifest_binding = {
        "schema": "tach-release-route-manifest-bindings-v2",
        "path": ROUTE_COVERAGE_GIT_PATH,
        "source_revisions": sorted(revision_bindings),
        "revisions": [revision_bindings[key] for key in sorted(revision_bindings)],
        "passed": not manifest_failures,
        "failures": manifest_failures,
      }
    matrix = boundaries.route_matrix
    observations, equivalences, binding = load_route_observations(
      data_dir,
      documents,
      contexts,
      matrix,
      boundaries.artifact_by_identity,
    )
    binding["failures"] = [
      *primary_binding_failures,
      *context_failures,
      *contract_failures,
      *binding["failures"],
    ]
    binding["passed"] = not binding["failures"]
    admission = release_matrix.validate_route_matrix(matrix, observations, equivalences)
    admission_payload = admission.to_mapping()
    admission_failures = admission.rendered_failures()
  except release_matrix.RouteMatrixError as error:
    binding = {
      "schema": ROUTE_OBSERVATIONS_SCHEMA,
      "path": ROUTE_OBSERVATIONS_FILENAME,
      "passed": False,
      "failures": [*primary_binding_failures, *context_failures, str(error)],
      "artifacts": [],
      "ignored_artifacts": [],
    }
    manifest_binding = {
      "schema": "tach-release-route-manifest-bindings-v2",
      "path": ROUTE_COVERAGE_GIT_PATH,
      "source_revisions": [],
      "revisions": [],
      "passed": False,
      "failures": [str(error)],
    }
    admission_payload = {
      "schema": "tach-release-route-matrix-report-v1",
      "passed": False,
      "decisions": [],
      "failures": [{"code": "malformed_record", "detail": str(error), "identity": None}],
    }
    admission_failures = [f"malformed_record: {error}"]
  return {
    "schema": "tach-release-route-matrix-admission-v1",
    "passed": (
      binding["passed"]
      and manifest_binding["passed"]
      and boundary_manifest_binding["passed"]
      and admission_payload["passed"]
    ),
    "bindings": binding,
    "manifest_binding": manifest_binding,
    "boundary_manifest_binding": boundary_manifest_binding,
    "admission": admission_payload,
    "failures": [
      *binding["failures"],
      *manifest_binding["failures"],
      *boundary_manifest_binding["failures"],
      *admission_failures,
    ],
  }


def _report_with_load_failures(report: dict, failures: list[str]) -> dict:
  result = dict(report)
  result["failures"] = [*result.get("failures", []), *failures]
  result["passed"] = not result["failures"]
  return result


def validate_release_snapshot(
  data_dir: Path,
  checkout_root: Path = ROOT.parent,
) -> ReleaseEvidenceSnapshot:
  """Run the full release gate and retain the exact admitted evidence bytes."""
  try:
    boundaries, boundary_manifest_binding = load_release_boundary_matrix(checkout_root)
  except (OSError, UnicodeError, ValueError, release_matrix.RouteMatrixError) as error:
    report = {
      "schema": "tach-release-speed-evidence-v4",
      "passed": False,
      "failures": [f"release boundary manifest: {error}"],
      "primary": None,
      "supplemental_speed": None,
      "route_matrix": None,
    }
    return ReleaseEvidenceSnapshot(report, (), ())

  required_artifacts = set(boundaries.artifact_ids)
  primary_artifacts = required_artifacts & set(speed_evidence.PRIMARY_SPEED_CELLS)
  supplemental_artifacts = required_artifacts & set(speed_evidence.SUPPLEMENTAL_SPEED_CELLS)
  catalog_failures = []
  unknown_artifacts = required_artifacts - primary_artifacts - supplemental_artifacts
  if unknown_artifacts:
    catalog_failures.append(
      f"release boundaries reference unknown artifacts: {sorted(unknown_artifacts)!r}"
    )

  primary_documents, primary_load_failures = load_primary_campaign(
    data_dir,
    primary_artifacts,
  )
  primary = validate_primary_snapshots(
    primary_documents,
    checkout_root,
    primary_load_failures,
    primary_artifacts,
    boundaries.shipping_paths,
  )
  supplemental_documents, supplemental_cell_paths, supplemental_load_failures = (
    load_supplemental_campaign(data_dir, supplemental_artifacts)
  )
  supplemental = validate_supplemental_snapshots(
    supplemental_documents,
    supplemental_cell_paths,
    supplemental_load_failures,
    supplemental_artifacts,
  )
  route_matrix = validate_route_matrix_admission(
    data_dir,
    checkout_root,
    primary_documents,
    primary,
    supplemental_documents,
    boundaries,
    boundary_manifest_binding,
  )
  failures = [
    *catalog_failures,
    *(f"primary: {failure}" for failure in primary["failures"]),
    *(f"supplemental: {failure}" for failure in supplemental["failures"]),
    *(f"route matrix: {failure}" for failure in route_matrix["failures"]),
  ]
  report = {
    "schema": "tach-release-speed-evidence-v4",
    "passed": not failures,
    "failures": failures,
    "primary": primary,
    "supplemental_speed": supplemental,
    "route_matrix": route_matrix,
  }
  return ReleaseEvidenceSnapshot(
    report=report,
    primary_cells=tuple(
      snapshot for _, snapshot in sorted(primary_documents.items())
    ),
    supplemental_cells=tuple(
      snapshot for _, snapshot in sorted(supplemental_documents.items())
    ),
  )


def require_validated_release_snapshot(
  data_dir: Path,
  checkout_root: Path = ROOT.parent,
) -> ReleaseEvidenceSnapshot:
  """Return snapshot-owned claim inputs only after every release gate passes."""
  snapshot = validate_release_snapshot(data_dir, checkout_root)
  if snapshot.report.get("passed") is not True:
    failures = snapshot.report.get("failures", [])
    detail = "\n  ".join(str(failure) for failure in failures)
    raise ValueError("release evidence failed:\n  " + detail)
  return snapshot


def validate_release_evidence(data_dir: Path, checkout_root: Path = ROOT.parent) -> dict:
  """Return the full release-gate report without exposing a claim snapshot."""
  return validate_release_snapshot(data_dir, checkout_root).report


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--data-dir", type=Path, default=ROOT)
  parser.add_argument("--output", type=Path)
  args = parser.parse_args()
  try:
    report = validate_release_evidence(args.data_dir)
  except ValueError as error:
    parser.error(str(error))
  rendered = json.dumps(report, indent=2) + "\n"
  if args.output:
    args.output.write_text(rendered)
  print(rendered, end="")
  if not report["passed"]:
    raise SystemExit(1)


if __name__ == "__main__":
  main()
