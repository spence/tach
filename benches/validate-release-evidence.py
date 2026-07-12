#!/usr/bin/env python3
"""Release gate: require primary and all-three-clock supplemental evidence."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import bench_data
import speed_evidence


ROOT = Path(__file__).resolve().parent


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--data-dir", type=Path, default=ROOT)
  parser.add_argument("--output", type=Path)
  args = parser.parse_args()
  primary = speed_evidence.validate_campaign_for_checkout(
    bench_data.load_cell_documents(args.data_dir), ROOT.parent
  )
  supplemental = speed_evidence.validate_supplemental_speed_campaign(
    bench_data.load_supplemental_speed_documents(args.data_dir)
  )
  failures = [
    *(f"primary: {failure}" for failure in primary["failures"]),
    *(f"supplemental: {failure}" for failure in supplemental["failures"]),
  ]
  if (
    primary.get("source_revision") is not None
    and supplemental.get("source_revision") is not None
    and primary["source_revision"] != supplemental["source_revision"]
  ):
    failures.append("primary and supplemental campaigns use different source revisions")
  report = {
    "schema": "tach-release-speed-evidence-v2",
    "passed": not failures,
    "failures": failures,
    "primary": primary,
    "supplemental_speed": supplemental,
  }
  rendered = json.dumps(report, indent=2) + "\n"
  if args.output:
    args.output.write_text(rendered)
  print(rendered, end="")
  if failures:
    raise SystemExit(1)


if __name__ == "__main__":
  main()
