#!/usr/bin/env python3
"""Validate all three clocks on hosted targets outside the six-cell campaign."""

from __future__ import annotations

import argparse
import json
from pathlib import Path, PurePosixPath
import stat

import bench_data
import speed_evidence


ROOT = Path(__file__).resolve().parent


def retained_collector_bundle_path(cell_path: Path, document: object) -> tuple[Path | None, str | None]:
  """Resolve a full-speed cell's retained bundle without following an escape path."""
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

  try:
    cell_mode = cell_path.lstat().st_mode
  except OSError as error:
    return None, f"could not stat cell document: {error}"
  if not stat.S_ISREG(cell_mode):
    return None, "cell document is not a regular file"

  try:
    cell_directory = cell_path.parent.resolve(strict=True)
  except OSError as error:
    return None, f"could not resolve cell document directory: {error}"
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


def validate_cell_artifact(artifact: str, cell_path: Path) -> dict:
  """Validate one artifact, binding full Criterion cells to their retained bundle."""
  document = bench_data.load_json_document(cell_path)
  _, _, mode, _ = speed_evidence.SUPPLEMENTAL_SPEED_CELLS[artifact]
  if mode == "runtime_smoke":
    return speed_evidence.validate_supplemental_speed_cell(artifact, document)
  if mode == "tagged_wall_fallback":
    report = speed_evidence.validate_supplemental_speed_cell(artifact, document)
    report["failures"].append(
      f"supplemental {artifact}: tagged wall fallback requires a producer-specific "
      "attested observation bundle"
    )
    report["passed"] = False
    return report

  bundle_path, error = retained_collector_bundle_path(cell_path, document)
  if bundle_path is not None:
    return speed_evidence.validate_supplemental_speed_cell_from_bundle(
      artifact, document, bundle_path
    )

  report = speed_evidence.validate_supplemental_speed_cell(artifact, document)
  report["bundle_binding"] = {"path": None, "passed": False}
  report["failures"].append(f"supplemental {artifact}: {error}")
  report["passed"] = False
  return report


def load_campaign_with_paths(data_dir: Path) -> tuple[dict[str, dict], dict[str, Path], list[str]]:
  """Load regular supplemental cells and retain only safely rooted full-cell paths."""
  documents: dict[str, dict] = {}
  cell_paths: dict[str, Path] = {}
  failures: list[str] = []
  try:
    directory = data_dir.resolve(strict=True)
    directory_mode = directory.lstat().st_mode
  except OSError as error:
    raise ValueError(f"could not read supplemental evidence directory {data_dir}: {error}") from error
  if not stat.S_ISDIR(directory_mode):
    raise ValueError(f"supplemental evidence directory is not a directory: {data_dir}")

  for cell_path in sorted(directory.glob("speed-supplemental-*.json")):
    name = cell_path.name
    try:
      cell_mode = cell_path.lstat().st_mode
    except OSError as error:
      raise ValueError(f"could not stat supplemental cell {cell_path}: {error}") from error
    if not stat.S_ISREG(cell_mode):
      failures.append(f"supplemental {name}: cell document is not a regular file")
      continue
    document = bench_data.load_json_document(cell_path)
    documents[name] = document
    expected = speed_evidence.SUPPLEMENTAL_SPEED_CELLS.get(name)
    if expected is None or expected[2] != "full_speed_cell":
      cell_paths[name] = cell_path
      continue
    _, error = retained_collector_bundle_path(cell_path, document)
    if error is None:
      cell_paths[name] = cell_path
    else:
      failures.append(f"supplemental {name}: {error}")
  return documents, cell_paths, failures


def validate_campaign(data_dir: Path) -> dict:
  """Validate all cells with the release-grade retained-observation requirement."""
  documents, cell_paths, binding_failures = load_campaign_with_paths(data_dir)
  report = speed_evidence.validate_supplemental_speed_campaign(
    documents, cell_paths, require_bound_observations=True
  )
  report["failures"].extend(binding_failures)
  report["passed"] = not report["failures"]
  return report


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--data-dir", type=Path, default=ROOT)
  parser.add_argument("--output", type=Path)
  parser.add_argument(
    "--artifact",
    choices=sorted(speed_evidence.SUPPLEMENTAL_SPEED_CELLS),
    help="validate one composed cell instead of the complete campaign",
  )
  parser.add_argument(
    "--cell",
    type=Path,
    help="path to the one cell selected by --artifact",
  )
  args = parser.parse_args()
  if (args.artifact is None) != (args.cell is None):
    parser.error("--artifact and --cell must be supplied together")
  try:
    if args.artifact is not None:
      report = validate_cell_artifact(args.artifact, args.cell)
    else:
      report = validate_campaign(args.data_dir)
  except (OSError, ValueError) as error:
    parser.error(str(error))
  rendered = json.dumps(report, indent=2) + "\n"
  if args.output:
    args.output.write_text(rendered)
  print(rendered, end="")
  if not report["passed"]:
    raise SystemExit(1)


if __name__ == "__main__":
  main()
