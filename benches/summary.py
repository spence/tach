#!/usr/bin/env python3
"""Render the README benchmark chart with fixed pixel geometry."""

from __future__ import annotations

import html
import shutil
import subprocess
from pathlib import Path

import bench_data


ROOT = Path(__file__).resolve().parent
SVG_PATH = ROOT / "summary.svg"
PNG_PATH = ROOT / "summary.png"
SVG_WIDE_PATH = ROOT / "summary-wide.svg"
PNG_WIDE_PATH = ROOT / "summary-wide.png"

BACKGROUND = "#FBF6EC"
FONT = "Avenir Next, Helvetica, Arial, sans-serif"
MONO = "SFMono-Regular, Menlo, Consolas, monospace"
TEXT_FG = "#2E231B"
MUTED_FG = "#7A6E60"
HEADER_NOW_LABEL = "Instant::now()"
HEADER_ELAPSED_LABEL = "now() + elapsed()"
UNIT_LABEL = "times shown in nanoseconds"
DISPLAY_LABELS = {}

CRATES = [
  ("tach@0.2.0", "#D72D24"),
  ("tach_ordered@0.2.0", "#EC7A1C"),
  ("quanta@0.12.6", "#5B6472"),
  ("fastant@0.1.11", "#4F6F6A"),
  ("minstant@0.1.7", "#8B5E3C"),
  ("std", "#9A8A3A"),
]

def _clock_key(crate_name: str) -> str:
  return crate_name.split("@")[0]


def _groups(kind: str):
  return [
    (header, [clocks[_clock_key(name)][kind] for name, _ in CRATES])
    for header, clocks in bench_data.CELLS
  ]


# Built from benches/speed-*.json via bench_data.CELLS (fresh campaign).
NOW_GROUPS = _groups("now")
ELAPSED_GROUPS = _groups("elapsed")

GRID_COLS = 2
GRID_CELL_W = 740
GRID_CELL_H = 540
GRID_COL_GAP = 36
GRID_ROW_GAP = 52
GRID_MARGIN = 40
GRID_CELL_PAD = 34
GRID_TITLE_FONT_SIZE = 50
GRID_SUBTITLE_FONT_SIZE = 28
GRID_LABEL_FONT_SIZE = 30
GRID_VALUE_FONT_SIZE = 29
GRID_ROW_HEIGHT = 64
GRID_BAR_HEIGHT = 36
GRID_CRATE_LABEL_WIDTH = 215
GRID_VALUE_RESERVE = 230
GRID_LIGHTEN = 0.62

GRID_HEADER_HEIGHT = 120
GRID_HEADER_GAP = 24
GRID_HEADER_BAR_WIDTH = 720
GRID_HEADER_BAR_HEIGHT = 24
GRID_HEADER_DARK_FRACTION = 0.46
GRID_HEADER_LABEL_GAP = 10


def value_label(value: float) -> str:
  if value >= 100:
    return f"{value:.0f}"
  if value >= 10:
    return f"{value:.1f}"
  return f"{value:.2f}"


def esc(value: str) -> str:
  return html.escape(value, quote=True)


def lighten(hex_color: str, amount: float) -> str:
  h = hex_color.lstrip("#")
  bh = BACKGROUND.lstrip("#")
  fr, fg, fb = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
  br, bg, bb = int(bh[0:2], 16), int(bh[2:4], 16), int(bh[4:6], 16)
  return (
    f"#{int(fr + (br - fr) * amount):02x}"
    f"{int(fg + (bg - fg) * amount):02x}"
    f"{int(fb + (bb - fb) * amount):02x}"
  )


def styled_text(
  x: float,
  y: float,
  value: str,
  size: int,
  family: str = FONT,
  color: str = TEXT_FG,
  anchor: str = "middle",
  weight: str | None = None,
) -> str:
  weight_attr = f' font-weight="{weight}"' if weight else ""
  return (
    f'<text x="{x:g}" y="{y:g}" text-anchor="{anchor}" '
    f'font-family="{family}" font-size="{size}"{weight_attr} '
    f'fill="{color}">{esc(value)}</text>'
  )


def crate_short(name: str) -> str:
  key = name.split("@")[0]
  return DISPLAY_LABELS.get(key, key)


def render_grid_cell(now_group, elapsed_group, crates, x0: float, y0: float) -> list[str]:
  header, now_vals = now_group
  title, instance, triple, *notes = header
  _, elapsed_vals = elapsed_group

  parts = []
  title_x = x0 + GRID_CELL_PAD
  title_y = y0 + GRID_CELL_PAD + GRID_TITLE_FONT_SIZE - 2
  parts.append(
    styled_text(title_x, title_y, title, GRID_TITLE_FONT_SIZE, anchor="start", weight="600")
  )

  subtitle_y = title_y + GRID_SUBTITLE_FONT_SIZE + 12
  parts.append(
    styled_text(
      title_x, subtitle_y, instance, GRID_SUBTITLE_FONT_SIZE,
      family=MONO, color=MUTED_FG, anchor="start",
    )
  )
  triple_y = subtitle_y + GRID_SUBTITLE_FONT_SIZE + 4
  parts.append(
    styled_text(
      title_x, triple_y, triple, GRID_SUBTITLE_FONT_SIZE,
      family=MONO, color=MUTED_FG, anchor="start",
    )
  )

  last_subtitle_y = triple_y
  for note in notes:
    last_subtitle_y += GRID_SUBTITLE_FONT_SIZE + 4
    parts.append(
      styled_text(
        title_x, last_subtitle_y, note, GRID_SUBTITLE_FONT_SIZE,
        family=MONO, color=MUTED_FG, anchor="start",
      )
    )

  bar_area_left = title_x + GRID_CRATE_LABEL_WIDTH + 8
  bar_area_right = x0 + GRID_CELL_W - GRID_CELL_PAD - GRID_VALUE_RESERVE
  bar_area_width = bar_area_right - bar_area_left
  cell_max = max(elapsed_vals)

  rows_top = last_subtitle_y + 22
  for i, ((crate_full, color), now_v, elapsed_v) in enumerate(zip(crates, now_vals, elapsed_vals)):
    row_top = rows_top + i * GRID_ROW_HEIGHT
    bar_y = row_top + (GRID_ROW_HEIGHT - GRID_BAR_HEIGHT) / 2
    bar_center = bar_y + GRID_BAR_HEIGHT / 2
    text_baseline = bar_center + GRID_LABEL_FONT_SIZE * 0.34

    parts.append(
      styled_text(
        title_x, text_baseline, crate_short(crate_full),
        GRID_LABEL_FONT_SIZE, family=MONO, anchor="start",
      )
    )

    light_color = lighten(color, GRID_LIGHTEN)
    elapsed_w = max(1.0, elapsed_v / cell_max * bar_area_width)
    now_w = max(1.0, now_v / cell_max * bar_area_width)
    parts.append(
      f'<rect x="{bar_area_left:g}" y="{bar_y:g}" '
      f'width="{elapsed_w:g}" height="{GRID_BAR_HEIGHT}" fill="{light_color}"/>'
    )
    parts.append(
      f'<rect x="{bar_area_left:g}" y="{bar_y:g}" '
      f'width="{now_w:g}" height="{GRID_BAR_HEIGHT}" fill="{color}"/>'
    )

    value_x = x0 + GRID_CELL_W - GRID_CELL_PAD
    parts.append(
      f'<text x="{value_x:g}" y="{text_baseline:g}" text-anchor="end" '
      f'font-family="{MONO}" font-size="{GRID_VALUE_FONT_SIZE}" fill="{TEXT_FG}">'
      f'{esc(value_label(now_v))}'
      f'<tspan dx="5" fill="{MUTED_FG}">/</tspan>'
      f'<tspan dx="5">{esc(value_label(elapsed_v))}</tspan>'
      f'</text>'
    )

  return parts


def render_grid_header(width: float, y0: float) -> list[str]:
  parts = []
  content_left = GRID_MARGIN + GRID_CELL_PAD
  content_right = width - GRID_MARGIN - GRID_CELL_PAD
  bar_x = content_left
  bar_y = y0 + GRID_HEADER_HEIGHT - GRID_HEADER_BAR_HEIGHT - 12
  dark_color = TEXT_FG
  light_color = lighten(dark_color, 0.66)
  dark_w = GRID_HEADER_BAR_WIDTH * GRID_HEADER_DARK_FRACTION

  parts.append(
    f'<rect x="{bar_x:g}" y="{bar_y:g}" width="{GRID_HEADER_BAR_WIDTH:g}" '
    f'height="{GRID_HEADER_BAR_HEIGHT}" fill="{light_color}"/>'
  )
  parts.append(
    f'<rect x="{bar_x:g}" y="{bar_y:g}" width="{dark_w:g}" '
    f'height="{GRID_HEADER_BAR_HEIGHT}" fill="{dark_color}"/>'
  )

  label_baseline = bar_y - GRID_HEADER_LABEL_GAP
  parts.append(
    styled_text(
      bar_x + dark_w / 2, label_baseline, HEADER_NOW_LABEL,
      GRID_LABEL_FONT_SIZE, family=MONO, anchor="middle", weight="600",
    )
  )
  parts.append(
    styled_text(
      bar_x + dark_w + (GRID_HEADER_BAR_WIDTH - dark_w) / 2,
      label_baseline,
      HEADER_ELAPSED_LABEL,
      GRID_LABEL_FONT_SIZE, family=MONO, anchor="middle", weight="600",
    )
  )

  bar_center_y = bar_y + GRID_HEADER_BAR_HEIGHT / 2
  parts.append(
    f'<text x="{content_right:g}" y="{bar_center_y:g}" '
    f'text-anchor="end" dominant-baseline="central" '
    f'font-family="{MONO}" font-size="{GRID_LABEL_FONT_SIZE}" '
    f'fill="{MUTED_FG}">{esc(UNIT_LABEL)}</text>'
  )
  return parts


def render_svg(now_groups, elapsed_groups, crates) -> str:
  rows = (len(now_groups) + GRID_COLS - 1) // GRID_COLS
  width = GRID_COLS * GRID_CELL_W + (GRID_COLS - 1) * GRID_COL_GAP + 2 * GRID_MARGIN
  cells_top = GRID_MARGIN + GRID_HEADER_HEIGHT + GRID_HEADER_GAP
  height = cells_top + rows * GRID_CELL_H + (rows - 1) * GRID_ROW_GAP + GRID_MARGIN

  parts = [
    '<?xml version="1.0" encoding="UTF-8"?>',
    (
      f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
      f'viewBox="0 0 {width} {height}">'
    ),
    f'<rect width="{width}" height="{height}" fill="{BACKGROUND}"/>',
    '<g shape-rendering="crispEdges">',
  ]

  parts.extend(render_grid_header(width, GRID_MARGIN))

  for i, (ng, eg) in enumerate(zip(now_groups, elapsed_groups)):
    col = i % GRID_COLS
    row = i // GRID_COLS
    x = GRID_MARGIN + col * (GRID_CELL_W + GRID_COL_GAP)
    y = cells_top + row * (GRID_CELL_H + GRID_ROW_GAP)
    parts.extend(render_grid_cell(ng, eg, crates, x, y))

  parts.append("</g>")
  parts.append("</svg>")
  return "\n".join(parts) + "\n"


# Wide layout overrides: 3 columns × 2 rows with smaller fonts. Used for
# GitHub's wider rendering column; the tall 2×3 default fits crates.io
# better. Keys are module-global names mutated before the second render.
WIDE_OVERRIDES = {
  "GRID_COLS": 3,
  "GRID_CELL_W": 620,
  "GRID_CELL_H": 360,
  "GRID_COL_GAP": 28,
  "GRID_ROW_GAP": 36,
  "GRID_CELL_PAD": 24,
  "GRID_TITLE_FONT_SIZE": 32,
  "GRID_SUBTITLE_FONT_SIZE": 18,
  "GRID_LABEL_FONT_SIZE": 20,
  "GRID_VALUE_FONT_SIZE": 19,
  "GRID_ROW_HEIGHT": 42,
  "GRID_BAR_HEIGHT": 24,
  "GRID_CRATE_LABEL_WIDTH": 150,
  "GRID_VALUE_RESERVE": 150,
  "GRID_HEADER_HEIGHT": 80,
  "GRID_HEADER_BAR_WIDTH": 480,
  "GRID_HEADER_BAR_HEIGHT": 18,
}


def main() -> None:
  rsvg_convert = shutil.which("rsvg-convert")
  if rsvg_convert is None:
    raise SystemExit("rsvg-convert is required to render the benchmark PNG")

  # Tall 2×3 layout (crates.io)
  SVG_PATH.write_text(render_svg(NOW_GROUPS, ELAPSED_GROUPS, CRATES))
  subprocess.run([rsvg_convert, "-o", str(PNG_PATH), str(SVG_PATH)], check=True)

  # Apply wide-layout overrides and render again for GitHub
  globals().update(WIDE_OVERRIDES)
  SVG_WIDE_PATH.write_text(render_svg(NOW_GROUPS, ELAPSED_GROUPS, CRATES))
  subprocess.run([rsvg_convert, "-o", str(PNG_WIDE_PATH), str(SVG_WIDE_PATH)], check=True)


if __name__ == "__main__":
  main()
