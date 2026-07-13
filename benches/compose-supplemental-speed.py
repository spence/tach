#!/usr/bin/env python3
"""Compose one source-bound supplemental runtime evidence cell.

The static route-coverage manifest declares what a producer must eventually
cover. This tool consumes one verified collector bundle rather than independently
chosen clocks and semantic samples, then emits one validated runtime cell.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import bench_data
import extract_speed
import speed_evidence


def _json_object(path: Path, description: str) -> dict:
  try:
    return bench_data.load_json_document(path)
  except (OSError, ValueError, json.JSONDecodeError) as error:
    raise ValueError(f"could not load {description} {path}: {error}") from error


def _provenance_fields(values: list[str]) -> dict:
  fields = {}
  for value in values:
    key, separator, field_value = value.partition("=")
    if (
      not separator
      or not key
      or not field_value
      or key in fields
      or key in {"harness", "source_revision"}
    ):
      raise ValueError(
        "--provenance-field must be a unique non-identity KEY=VALUE pair"
      )
    fields[key] = field_value
  return fields


def compose(
  artifact: str,
  clocks: dict | None,
  raw_behavior: dict | None,
  source_revision: str,
  selection_profiles: dict,
  provenance_extra: dict,
  runtime_smoke: dict | None,
  collector_bundle_path: str = "collector.bundle",
) -> dict:
  """Public, testable wrapper around the evidence-layer single-cell composer."""
  return speed_evidence.compose_supplemental_speed_cell(
    artifact,
    clocks,
    raw_behavior,
    source_revision,
    selection_profiles,
    provenance_extra,
    runtime_smoke,
    collector_bundle_path,
  )


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--artifact", required=True, choices=sorted(speed_evidence.SUPPLEMENTAL_SPEED_CELLS))
  parser.add_argument("--output", required=True, type=Path)
  parser.add_argument("--source-revision", required=True)
  parser.add_argument("--collector-bundle", type=Path)
  parser.add_argument("--smoke-attestation", type=Path)
  parser.add_argument(
    "--instant-profile", choices=sorted(speed_evidence.SUPPLEMENTAL_SELECTION_PROFILES)
  )
  parser.add_argument(
    "--ordered-profile", choices=sorted(speed_evidence.SUPPLEMENTAL_SELECTION_PROFILES)
  )
  parser.add_argument(
    "--thread-cpu-profile", choices=sorted(speed_evidence.SUPPLEMENTAL_SELECTION_PROFILES)
  )
  parser.add_argument("--provenance-field", action="append", default=[])
  args = parser.parse_args()

  _, _, mode, _ = speed_evidence.SUPPLEMENTAL_SPEED_CELLS[args.artifact]
  extra = _provenance_fields(args.provenance_field)
  if mode == "runtime_smoke":
    if args.collector_bundle or not args.smoke_attestation or any((
      args.instant_profile,
      args.ordered_profile,
      args.thread_cpu_profile,
    )):
      parser.error("runtime smoke evidence needs only --smoke-attestation")
    clocks = None
    behavior = None
    profiles = {}
    smoke = _json_object(args.smoke_attestation, "runtime smoke attestation")
  elif mode != "full_speed_cell":
    parser.error("this supplemental mode needs a producer-specific attested bundle")
  else:
    if not args.collector_bundle:
      parser.error("measured runtime evidence needs --collector-bundle")
    if not all((args.instant_profile, args.ordered_profile, args.thread_cpu_profile)):
      parser.error("measured supplemental evidence needs a selection profile for every timer")
    try:
      observation = extract_speed.extract_collector_bundle_observation(
        args.collector_bundle
      )
      extracted_clocks = observation["clocks"]
      collector = observation["collector_attestation"]
      if not isinstance(extracted_clocks, dict) or not isinstance(collector, dict):
        parser.error("collector bundle observation has malformed clocks or attestation")
      clocks = {**extracted_clocks, "collector_attestation": collector}
      behavior = observation["thread_cpu_behavior"]
      if not isinstance(behavior, dict):
        parser.error("measured runtime evidence has no thread-CPU behavior sidecar")
      bundle_path = args.collector_bundle.resolve().relative_to(
        args.output.parent.resolve()
      ).as_posix()
    except RuntimeError as error:
      parser.error(str(error))
    except ValueError:
      parser.error("--collector-bundle must be contained by the output directory")
    profiles = {
      "instant": args.instant_profile,
      "ordered": args.ordered_profile,
      "thread_cpu": args.thread_cpu_profile,
    }
    smoke = None

  try:
    document = compose(
      args.artifact,
      clocks,
      behavior,
      args.source_revision,
      profiles,
      extra,
      smoke,
      bundle_path if mode == "full_speed_cell" else "collector.bundle",
    )
  except ValueError as error:
    parser.error(str(error))
  args.output.write_text(json.dumps(document, indent=2) + "\n")


if __name__ == "__main__":
  main()
