#!/usr/bin/env python3
"""Wrap extracted clock medians in one campaign-cell JSON document."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import speed_evidence


WALL_CLOCKS = {"tach", "tach_ordered", "quanta", "fastant", "minstant", "std"}
THREAD_CPU_CLOCKS = {"tach_thread_cpu", "native_thread_cpu"}


def validate(clocks: dict) -> None:
  missing = sorted((WALL_CLOCKS | THREAD_CPU_CLOCKS) - clocks.keys())
  if missing:
    raise ValueError(f"missing clocks: {', '.join(missing)}")
  for key in sorted(WALL_CLOCKS | THREAD_CPU_CLOCKS):
    entry = clocks[key]
    missing_fields = [
      field
      for field in ("now", "elapsed", "now_ci95", "elapsed_ci95")
      if field not in entry
    ]
    if key in THREAD_CPU_CLOCKS:
      missing_fields += [
        field for field in ("provider", "read_cost", "time_domain") if field not in entry
      ]
    if missing_fields:
      raise ValueError(f"{key} missing fields: {', '.join(missing_fields)}")
    for field in ("now", "elapsed"):
      value = entry[field]
      if not isinstance(value, (int, float)) or value < 0:
        raise ValueError(f"{key}.{field} must be a non-negative number")
      interval = entry[f"{field}_ci95"]
      if (
        not isinstance(interval, list)
        or len(interval) != 2
        or not all(isinstance(bound, (int, float)) for bound in interval)
        or not interval[0] <= value <= interval[1]
      ):
        raise ValueError(f"{key}.{field}_ci95 must contain the point estimate")
    if key in THREAD_CPU_CLOCKS and entry["time_domain"] != "thread CPU":
      raise ValueError(f"{key} did not measure current-thread CPU time")
  if "direct_thread_cpu" in clocks:
    direct = clocks["direct_thread_cpu"]
    required = (
      "now", "elapsed", "now_ci95", "elapsed_ci95",
      "provider", "read_cost", "time_domain",
    )
    missing_direct = [field for field in required if field not in direct]
    if missing_direct:
      raise ValueError(f"direct_thread_cpu missing fields: {', '.join(missing_direct)}")


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("clocks", type=Path)
  parser.add_argument("output", type=Path)
  parser.add_argument("--title", required=True)
  parser.add_argument("--instance", required=True)
  parser.add_argument("--triple", required=True)
  parser.add_argument("--order", required=True, type=int)
  parser.add_argument("--source-revision", required=True)
  parser.add_argument("--rustc-version", required=True)
  parser.add_argument("--harness", required=True, choices=("criterion", "lambda"))
  parser.add_argument("--cargo-profile", required=True, choices=("bench", "release"))
  args = parser.parse_args()

  clocks = json.loads(args.clocks.read_text())
  validate(clocks)
  if "linux" in args.triple and args.triple.startswith(("x86_64", "aarch64")):
    selection = clocks["tach_thread_cpu"].get("selection")
    if selection is None:
      raise ValueError("Linux inline target is missing selector measurement evidence")
    if clocks["tach_thread_cpu"]["provider"] == "Linux perf mmap":
      if "direct_thread_cpu" not in clocks:
        raise ValueError("selected Linux perf target is missing direct-handle evidence")
  failures, _ = speed_evidence.validate_cell(args.title, clocks)
  if failures:
    raise ValueError("cell does not support the speed claim:\n  " + "\n  ".join(failures))
  cell = {
    "title": args.title,
    "instance": args.instance,
    "triple": args.triple,
    "order": args.order,
    "provenance": {
      "source_revision": args.source_revision,
      "rustc": args.rustc_version,
      "cargo_profile": args.cargo_profile,
      "features": ["bench-internal", "thread-cpu-inline"],
      "harness": args.harness,
    },
    "clocks": clocks,
  }
  args.output.write_text(json.dumps(cell, indent=2) + "\n")


if __name__ == "__main__":
  main()
