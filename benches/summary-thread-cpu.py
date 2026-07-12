#!/usr/bin/env python3
"""Render ThreadCpuInstant versus the OS-native thread CPU primitive.

The input is the same six `speed-*.json` files used by the Instant and
OrderedInstant charts. Each cell must carry `tach_thread_cpu` and
`native_thread_cpu` measurements with explicit provider metadata.
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
from pathlib import Path

import bench_data
import speed_evidence
import summary


ROOT = Path(__file__).resolve().parent
CRATES = [
  ("tach_thread_cpu@0.2.0", "#D72D24"),
  ("native_thread_cpu", "#5B6472"),
]


def compact_provider(label: str) -> str:
  return (
    label
    .replace("POSIX thread CPU clock", "POSIX thread clock")
    .replace("Windows GetThreadTimes", "GetThreadTimes")
    .replace("clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)", "clock_gettime_nsec_np")
    .replace("clock_gettime(CLOCK_THREAD_CPUTIME_ID)", "clock_gettime")
    .replace("inline syscall(CLOCK_THREAD_CPUTIME_ID)", "raw syscall")
  )


def groups(cells, kind: str):
  output = []
  missing = []
  for header, clocks in cells:
    absent = [key for key in ("tach_thread_cpu", "native_thread_cpu") if key not in clocks]
    if absent:
      missing.append(f"{header[0]}: {', '.join(absent)}")
      continue

    tach = clocks["tach_thread_cpu"]
    native = clocks["native_thread_cpu"]
    for key, value in (("tach_thread_cpu", tach), ("native_thread_cpu", native)):
      required = [
        field
        for field in ("now", "elapsed", "provider", "read_cost", "time_domain")
        if field not in value
      ]
      if required:
        missing.append(f"{header[0]} {key}: {', '.join(required)}")
      elif value["time_domain"] != "thread CPU":
        missing.append(f"{header[0]} {key}: expected thread CPU, got {value['time_domain']}")

    provider = compact_provider(str(tach.get("provider", "unrecorded")))
    cost = str(tach.get("read_cost", "unknown"))
    native_provider = compact_provider(str(native.get("provider", "unrecorded")))
    annotated_header = (
      *header,
      f"tach: {provider} · {cost}",
      f"native: {native_provider}",
    )
    output.append((annotated_header, [tach[kind], native[kind]]))

  if missing:
    raise ValueError(
      "thread-CPU campaign data is incomplete; rerun these cells:\n  "
      + "\n  ".join(missing)
    )
  return output


def render(documents, output_dir: Path, png: bool = True) -> None:
  report = speed_evidence.validate_campaign_for_checkout(documents, ROOT.parent)
  if not report["passed"]:
    raise ValueError("benchmark evidence failed:\n  " + "\n  ".join(report["failures"]))
  cells = [
    ((document["title"], document["instance"], document["triple"]), document["clocks"])
    for document in documents
  ]
  now_groups = groups(cells, "now")
  elapsed_groups = groups(cells, "elapsed")
  if len(now_groups) != 6:
    raise ValueError(f"expected the prior campaign's six cells, found {len(now_groups)}")

  summary.HEADER_NOW_LABEL = "ThreadCpuInstant::now()"
  summary.HEADER_ELAPSED_LABEL = "now() + elapsed()"
  summary.DISPLAY_LABELS.update({
    "tach_thread_cpu": "tach",
    "native_thread_cpu": "OS native",
  })
  summary.GRID_CELL_H = 440
  summary.GRID_CRATE_LABEL_WIDTH = 215
  summary.GRID_LABEL_FONT_SIZE = 27

  output_dir.mkdir(parents=True, exist_ok=True)
  tall_svg = output_dir / "summary-thread-cpu.svg"
  tall_svg.write_text(summary.render_svg(now_groups, elapsed_groups, CRATES))
  if png:
    subprocess.run(
      ["rsvg-convert", "-o", str(output_dir / "summary-thread-cpu.png"), str(tall_svg)],
      check=True,
    )

  for key, value in summary.WIDE_OVERRIDES.items():
    setattr(summary, key, value)
  summary.GRID_CELL_H = 290
  summary.GRID_LABEL_FONT_SIZE = 18
  wide_svg = output_dir / "summary-thread-cpu-wide.svg"
  wide_svg.write_text(summary.render_svg(now_groups, elapsed_groups, CRATES))
  if png:
    subprocess.run(
      [
        "rsvg-convert",
        "-o",
        str(output_dir / "summary-thread-cpu-wide.png"),
        str(wide_svg),
      ],
      check=True,
    )


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--data-dir", type=Path, default=ROOT)
  parser.add_argument("--output-dir", type=Path, default=ROOT)
  parser.add_argument("--svg-only", action="store_true")
  args = parser.parse_args()

  if not args.svg_only and shutil.which("rsvg-convert") is None:
    raise SystemExit("rsvg-convert is required to render the benchmark PNG")
  try:
    render(
      bench_data.load_cell_documents(args.data_dir),
      args.output_dir,
      png=not args.svg_only,
    )
  except ValueError as error:
    raise SystemExit(str(error)) from error


if __name__ == "__main__":
  main()
