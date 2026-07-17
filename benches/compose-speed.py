#!/usr/bin/env python3
"""Compose one canonical primary cell from a retained observation bundle.

The primary four-cell campaign accepts no caller-supplied clocks or build
identity.  The collector bundle supplies the measured rows and Rust-emitted
attestation; the output filename selects one fixed campaign identity.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import extract_speed
import speed_evidence


def compose(
  artifact_id: str,
  clocks: dict,
  collector_attestation: dict,
  collector_bundle_path: str,
) -> dict:
  """Public, testable wrapper around the bundle-bound primary composer."""
  expected = speed_evidence.PRIMARY_SPEED_CELLS.get(artifact_id)
  if expected is None:
    raise ValueError(f"unknown primary artifact {artifact_id!r}")
  order, title, instance, triple, _, _ = expected
  return speed_evidence.compose_primary_speed_cell(
    artifact_id,
    title,
    instance,
    triple,
    order,
    clocks,
    collector_attestation,
    collector_bundle_path,
  )


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("output", type=Path)
  parser.add_argument("--collector-bundle", required=True, type=Path)
  args = parser.parse_args()

  artifact_id = args.output.name
  expected = speed_evidence.PRIMARY_SPEED_CELLS.get(artifact_id)
  if expected is None:
    parser.error(f"output filename is not a canonical primary artifact: {artifact_id}")
  try:
    output_directory = args.output.parent.resolve(strict=True)
  except OSError as error:
    parser.error(f"could not resolve output directory: {error}")
  try:
    observation = extract_speed.extract_collector_bundle_observation(
      args.collector_bundle
    )
    clocks = observation["clocks"]
    collector = observation["collector_attestation"]
    if not isinstance(clocks, dict) or not isinstance(collector, dict):
      parser.error("collector bundle observation has malformed clocks or attestation")
    bundle_path = args.collector_bundle.resolve().relative_to(output_directory).as_posix()
  except (OSError, RuntimeError) as error:
    parser.error(str(error))
  except ValueError:
    parser.error("--collector-bundle must be contained by the output directory")

  try:
    document = compose(artifact_id, clocks, collector, bundle_path)
  except ValueError as error:
    parser.error(str(error))
  args.output.write_text(json.dumps(document, indent=2) + "\n")


if __name__ == "__main__":
  main()
