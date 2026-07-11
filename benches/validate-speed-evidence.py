#!/usr/bin/env python3
"""Validate the six-cell evidence behind tach's three-use-case speed claim."""

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
  report = speed_evidence.validate_campaign(bench_data.load_cell_documents(args.data_dir))
  rendered = json.dumps(report, indent=2) + "\n"
  if args.output:
    args.output.write_text(rendered)
  print(rendered, end="")
  if not report["passed"]:
    raise SystemExit(1)


if __name__ == "__main__":
  main()
