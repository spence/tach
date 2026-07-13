#!/usr/bin/env python3
"""Render one six-platform chart covering tach's three timing use cases."""

from __future__ import annotations

import argparse
import html
import importlib.util
import shutil
import subprocess
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parent
RELEASE_VALIDATOR_PATH = ROOT / "validate-release-evidence.py"
RELEASE_VALIDATOR_MODULE = "tach_release_evidence_for_use_case_chart"
BACKGROUND = "#FBF6EC"
TEXT = "#2E231B"
MUTED = "#7A6E60"
FONT = "Avenir Next, Helvetica, Arial, sans-serif"
MONO = "SFMono-Regular, Menlo, Consolas, monospace"

SECTIONS = [
  (
    "SAME-THREAD ELAPSED TIME",
    [
      ("tach", "tach::Instant", "#D72D24", True),
      ("quanta", "quanta †", "#5B6472", False),
      ("fastant", "fastant †", "#4F6F6A", False),
      ("minstant", "minstant †", "#8B5E3C", False),
      ("std", "std", "#9A8A3A", False),
    ],
  ),
  (
    "CROSS-THREAD ELAPSED TIME",
    [
      ("tach_ordered", "tach::OrderedInstant", "#EC7A1C", True),
      ("std", "std", "#9A8A3A", False),
    ],
  ),
  (
    "CURRENT-THREAD CPU TIME",
    [
      ("tach_thread_cpu", "tach::ThreadCpuInstant", "#D72D24", True),
      ("native_thread_cpu", "OS native", "#5B6472", False),
    ],
  ),
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


def esc(value) -> str:
  return html.escape(str(value), quote=True)


def text(x, y, value, size, *, family=FONT, color=TEXT, anchor="start", weight=None):
  weight_attr = f' font-weight="{weight}"' if weight else ""
  return (
    f'<text x="{x:g}" y="{y:g}" text-anchor="{anchor}" '
    f'font-family="{family}" font-size="{size}" fill="{color}"{weight_attr}>'
    f'{esc(value)}</text>'
  )


def lighten(color: str, amount: float = 0.62) -> str:
  fg = color.lstrip("#")
  bg = BACKGROUND.lstrip("#")
  values = []
  for offset in (0, 2, 4):
    foreground = int(fg[offset:offset + 2], 16)
    background = int(bg[offset:offset + 2], 16)
    values.append(int(foreground + (background - foreground) * amount))
  return "#" + "".join(f"{value:02x}" for value in values)


def value_label(value: float) -> str:
  if value >= 100:
    return f"{value:.0f}"
  if value >= 10:
    return f"{value:.1f}"
  return f"{value:.2f}"


def compact_provider(label: str) -> str:
  return (
    label
    .replace("POSIX thread CPU clock", "POSIX thread clock")
    .replace("Windows GetThreadTimes", "GetThreadTimes")
    .replace("clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)", "clock_gettime_nsec_np")
    .replace("clock_gettime(CLOCK_THREAD_CPUTIME_ID)", "clock_gettime")
    .replace("inline syscall(CLOCK_THREAD_CPUTIME_ID)", "raw syscall")
  )


def validate(cells) -> None:
  if len(cells) != 6:
    raise ValueError(f"expected the prior campaign's six cells, found {len(cells)}")
  missing = []
  for header, clocks in cells:
    for _section, rows in SECTIONS:
      for key, _label, _color, _highlight in rows:
        if key not in clocks:
          missing.append(f"{header[0]}: {key}")
          continue
        required = [field for field in ("now", "elapsed") if field not in clocks[key]]
        if key in ("tach_thread_cpu", "native_thread_cpu"):
          required += [
            field
            for field in ("provider", "read_cost", "time_domain")
            if field not in clocks[key]
          ]
        if required:
          missing.append(f"{header[0]} {key}: {', '.join(required)}")
        elif key in ("tach_thread_cpu", "native_thread_cpu"):
          if clocks[key]["time_domain"] != "thread CPU":
            missing.append(
              f"{header[0]} {key}: expected thread CPU, got {clocks[key]['time_domain']}"
            )
  if missing:
    raise ValueError(
      "use-case campaign data is incomplete; rerun these measurements:\n  "
      + "\n  ".join(missing)
    )


def render_cell(header, clocks, x0, y0, width, height, scale=1.0):
  pad = 28 * scale
  title_size = 31 * scale
  meta_size = 17 * scale
  section_size = 17 * scale
  label_size = 18 * scale
  value_size = 17 * scale
  row_h = 43 * scale
  bar_h = 22 * scale
  label_w = 285 * scale
  value_w = 164 * scale
  parts = []

  title, instance, triple = header
  x = x0 + pad
  y = y0 + pad + title_size
  parts.append(text(x, y, title, title_size, weight="600"))
  y += meta_size + 7 * scale
  parts.append(text(x, y, instance, meta_size, family=MONO, color=MUTED))
  y += meta_size + 4 * scale
  parts.append(text(x, y, triple, meta_size, family=MONO, color=MUTED))
  y += 27 * scale

  for section_index, (section_name, rows) in enumerate(SECTIONS):
    parts.append(text(x, y, section_name, section_size, family=MONO, color=MUTED, weight="600"))
    if section_index == 2:
      tach = clocks["tach_thread_cpu"]
      native = clocks["native_thread_cpu"]
      provider = compact_provider(str(tach["provider"]))
      native_provider = compact_provider(str(native["provider"]))
      detail = f"tach: {provider} · {tach['read_cost']}  |  native: {native_provider}"
      parts.append(
        text(x0 + width - pad, y, detail, 13 * scale, family=MONO, color=MUTED, anchor="end")
      )
    y += 10 * scale

    section_max = max(float(clocks[key]["elapsed"]) for key, *_ in rows)
    bar_left = x + label_w
    bar_right = x0 + width - pad - value_w
    bar_width = bar_right - bar_left
    for key, label, color, highlight in rows:
      values = clocks[key]
      row_top = y
      bar_y = row_top + (row_h - bar_h) / 2
      baseline = row_top + row_h / 2 + label_size * 0.34
      parts.append(
        text(
          x, baseline, label, label_size, family=MONO,
          color=color if highlight else TEXT, weight="600" if highlight else None,
        )
      )
      elapsed_w = max(scale, float(values["elapsed"]) / section_max * bar_width)
      now_w = max(scale, float(values["now"]) / section_max * bar_width)
      parts.append(
        f'<rect x="{bar_left:g}" y="{bar_y:g}" width="{elapsed_w:g}" '
        f'height="{bar_h:g}" fill="{lighten(color)}"/>'
      )
      parts.append(
        f'<rect x="{bar_left:g}" y="{bar_y:g}" width="{now_w:g}" '
        f'height="{bar_h:g}" fill="{color}"/>'
      )
      parts.append(
        f'<text x="{x0 + width - pad:g}" y="{baseline:g}" text-anchor="end" '
        f'font-family="{MONO}" font-size="{value_size:g}" fill="{TEXT}">'
        f'{esc(value_label(float(values["now"])))}'
        f'<tspan dx="4" fill="{MUTED}">/</tspan>'
        f'<tspan dx="4">{esc(value_label(float(values["elapsed"])))}</tspan></text>'
      )
      y += row_h
    y += 18 * scale

  return parts


def render_svg(cells, columns: int) -> str:
  if columns == 2:
    cell_w, cell_h, gap_x, gap_y, margin, header_h, scale = 850, 700, 32, 38, 38, 142, 1.0
  else:
    cell_w, cell_h, gap_x, gap_y, margin, header_h, scale = 690, 566, 24, 30, 30, 112, 0.80
  rows = (len(cells) + columns - 1) // columns
  width = margin * 2 + columns * cell_w + (columns - 1) * gap_x
  height = margin * 2 + header_h + rows * cell_h + (rows - 1) * gap_y
  parts = [
    '<?xml version="1.0" encoding="UTF-8"?>',
    f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
    f'viewBox="0 0 {width} {height}">',
    f'<rect width="{width}" height="{height}" fill="{BACKGROUND}"/>',
  ]
  parts.append(
    text(
      margin,
      margin + 42 * scale,
      "tach steady-state speed across three timing contracts",
      39 * scale,
      weight="600",
    )
  )
  parts.append(
    text(
      margin, margin + 76 * scale,
      "median nanoseconds per call · lower is better · † informational, outside the audited contract",
      20 * scale, family=MONO, color=MUTED,
    )
  )
  parts.append(
    text(
      width - margin, margin + 76 * scale,
      "dark: now()   /   light: now() + elapsed()",
      20 * scale, family=MONO, color=MUTED, anchor="end",
    )
  )

  for index, (header, clocks) in enumerate(cells):
    col = index % columns
    row = index // columns
    x = margin + col * (cell_w + gap_x)
    y = margin + header_h + row * (cell_h + gap_y)
    parts.extend(render_cell(header, clocks, x, y, cell_w, cell_h, scale))

  footer_y = height - 12 * scale
  parts.append(
    text(
      margin, footer_y,
      "tach selects the fastest eligible reliable provider · material tie: 95% CI + "
      "max(1 ns, 5%) · selector evidence retained in JSON",
      14 * scale, family=MONO, color=MUTED,
    )
  )
  parts.append("</svg>")
  return "\n".join(parts) + "\n"


def cells_from_release_snapshot(snapshot) -> list[tuple[tuple[str, str, str], dict]]:
  """Build chart cells only from the full-gate snapshot's captured primary bytes."""
  documents = snapshot.primary_chart_documents()
  cells = [
    ((document["title"], document["instance"], document["triple"]), document["clocks"])
    for _, document in sorted(documents.items(), key=lambda item: item[1].get("order", 99))
  ]
  validate(cells)
  return cells


def render_cells(cells, output_dir: Path, png: bool = True) -> None:
  """Render already-admitted chart cells without reading evidence paths."""
  output_dir.mkdir(parents=True, exist_ok=True)
  for suffix, columns in (("", 2), ("-wide", 3)):
    svg = output_dir / f"summary-use-cases{suffix}.svg"
    svg.write_text(render_svg(cells, columns))
    if png:
      subprocess.run(
        ["rsvg-convert", "-o", str(output_dir / f"summary-use-cases{suffix}.png"), str(svg)],
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
