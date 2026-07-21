#!/usr/bin/env python3
"""Render summary-ordered.{svg,png} (+ -wide): GlobalInstant vs std.

The two clocks that stay correct across threads. Reuses summary.py's renderer
and reads `std` from the SAME `bench_data.CELLS` as summary.py, so the std bars
are identical across both charts by construction. Writes ONLY `summary-ordered*`
— it never touches `summary.*`.
"""

from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

import bench_data
import summary

ROOT = Path(__file__).resolve().parent

# Only the cross-thread-correct clocks. Names must keep the `<key>@...` form so
# summary._clock_key() resolves to the bench_data clock key.
CRATES = [
  ("tach_ordered@0.2.0", "#EC7A1C"),
  ("std", "#9A8A3A"),
]

# Cells carry up to six clocks; we slice two. Tighter cell height than summary.py
# (two rows, not six) so the cards aren't mostly whitespace.
CELL_H_TALL = 330
CELL_H_WIDE = 210


def _groups(kind: str):
  return [
    (header, [clocks[summary._clock_key(name)][kind] for name, _ in CRATES])
    for header, clocks in bench_data.CELLS
  ]


def _render(svg_path: Path, png_path: Path) -> None:
  svg = summary.render_svg(_groups("now"), _groups("elapsed"), CRATES)
  svg_path.write_text(svg)
  subprocess.run(["rsvg-convert", "-o", str(png_path), str(svg_path)], check=True)


def main() -> None:
  if shutil.which("rsvg-convert") is None:
    raise SystemExit("rsvg-convert is required to render the benchmark PNG")

  summary.GRID_CELL_H = CELL_H_TALL
  _render(ROOT / "summary-ordered.svg", ROOT / "summary-ordered.png")

  for key, val in summary.WIDE_OVERRIDES.items():
    setattr(summary, key, val)
  summary.GRID_CELL_H = CELL_H_WIDE
  _render(ROOT / "summary-ordered-wide.svg", ROOT / "summary-ordered-wide.png")


if __name__ == "__main__":
  main()
