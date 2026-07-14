#!/usr/bin/env python3
"""Render ThreadCpuInstant versus native across six admitted native environments."""

from __future__ import annotations

import argparse
import importlib.util
import shutil
import subprocess
from pathlib import Path
import sys

import summary
import release_chart


ROOT = Path(__file__).resolve().parent
RELEASE_VALIDATOR_PATH = ROOT / "validate-release-evidence.py"
RELEASE_VALIDATOR_MODULE = "tach_release_evidence_for_thread_cpu_chart"
CRATES = [
  ("tach_thread_cpu@0.2.0", "#D72D24"),
  ("native_thread_cpu", "#5B6472"),
]


def load_release_validator():
  """Load the release gate as an in-process snapshot provider."""
  if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))
  module = sys.modules.get(RELEASE_VALIDATOR_MODULE)
  if module is not None:
    return module
  spec = importlib.util.spec_from_file_location(
    RELEASE_VALIDATOR_MODULE,
    RELEASE_VALIDATOR_PATH,
  )
  if spec is None or spec.loader is None:
    raise RuntimeError("could not load validate-release-evidence.py")
  module = importlib.util.module_from_spec(spec)
  sys.modules[RELEASE_VALIDATOR_MODULE] = module
  spec.loader.exec_module(module)
  return module


def compact_provider(label: str) -> str:
  return (
    label
    .replace("POSIX thread CPU clock", "POSIX thread clock")
    .replace("Windows GetThreadTimes", "GetThreadTimes")
    .replace("clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)", "clock_gettime_nsec_np")
    .replace("clock_gettime(CLOCK_THREAD_CPUTIME_ID)", "clock_gettime")
    .replace("inline syscall(CLOCK_THREAD_CPUTIME_ID)", "raw syscall")
    .replace("GetThreadTimes(current-thread pseudo-handle)", "GetThreadTimes")
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


def cells_from_release_snapshot(snapshot) -> list[tuple[tuple[str, str, str], dict]]:
  """Build chart cells only from the full-gate snapshot's captured bytes."""
  return release_chart.cells_from_release_snapshot(snapshot)


def render_cells(cells, output_dir: Path, png: bool = True) -> None:
  """Render already-admitted chart cells without reading evidence paths."""
  now_groups = groups(cells, "now")
  elapsed_groups = groups(cells, "elapsed")
  if len(now_groups) != 6:
    raise ValueError(f"expected six admitted native chart cells, found {len(now_groups)}")

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


def render(snapshot, output_dir: Path, png: bool = True) -> None:
  """Render a release claim from one successful full-release snapshot."""
  render_cells(cells_from_release_snapshot(snapshot), output_dir, png)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--data-dir", type=Path, default=ROOT)
  parser.add_argument("--output-dir", type=Path, default=ROOT)
  parser.add_argument("--svg-only", action="store_true")
  args = parser.parse_args()

  if not args.svg_only and shutil.which("rsvg-convert") is None:
    raise SystemExit("rsvg-convert is required to render the benchmark PNG")
  try:
    snapshot = load_release_validator().require_validated_release_snapshot(
      args.data_dir,
      ROOT.parent,
    )
    render(
      snapshot,
      args.output_dir,
      png=not args.svg_only,
    )
  except ValueError as error:
    raise SystemExit(str(error)) from error


if __name__ == "__main__":
  main()
