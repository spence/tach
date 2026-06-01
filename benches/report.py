#!/usr/bin/env python3
"""Compose a per-cell benchmark report SVG.

Two input modes:

(1) Criterion mode (default) — reads criterion's per-group violin SVGs +
    per-crate pdf_small.svg + estimates.json, builds a report with violin
    + per-crate distribution + medians table.

      python3 benches/report.py <cell-name> \\
        --title "..." --subtitle "..." [--criterion-dir <path>]

(2) Lambda mode — reads N run JSONs produced by the standalone
    tach-lambda-bench handler, builds a bar-and-whisker chart of the
    medians plus min/max ranges across runs.

      python3 benches/report.py lambda-x86_64 \\
        --title "..." --subtitle "..." \\
        --lambda-runs <dir-containing-run*.json>

Output path: written next to this script (benches/report-<cell-name>.svg).
"""

from __future__ import annotations

import argparse
import html
import json
import re
import sys
from pathlib import Path


OUTPUT_DIR = Path(__file__).resolve().parent
REPO_ROOT = OUTPUT_DIR.parent
DEFAULT_CRITERION_DIR = REPO_ROOT / "target" / "criterion"

GROUP_NOW = "Instant__now()"
GROUP_ELAPSED = "Instant__now() + elapsed()"
GROUP_ORDERED = "Ordered Instant__now()"

CRATES = ["tach", "quanta", "fastant", "minstant", "std"]

# (criterion subdir, display label, highlight?) tuples for the ordered group.
# Criterion replaces `::` with `__` in subdir names; the display labels
# restore the original form.
ORDERED_ENTRIES = [
  ("tach__OrderedInstant", "tach::OrderedInstant", True),
  ("tach__OrderedInstant (now + elapsed)", "OrderedInstant (now+elapsed)", True),
  ("tach__Instant (unordered reference)", "tach::Instant (unordered ref)", False),
  ("std__time__Instant", "std::time::Instant", False),
  ("std__time__Instant (now + elapsed)", "std::Instant (now+elapsed)", False),
]

BACKGROUND = "#FBF6EC"
FONT = "Avenir Next, Helvetica, Arial, sans-serif"
MONO = "SFMono-Regular, Menlo, Consolas, monospace"
TEXT_FG = "#2E231B"
MUTED_FG = "#7A6E60"
TACH_FG = "#D72D24"

# Parent SVG width; both gnuplot (1280) and plotters (960) violins are scaled
# uniformly into this width. Resulting violin height is set so the aspect
# ratio of each source SVG is preserved.
TARGET_WIDTH = 1280
HEADER_H = 90
SECTION_LABEL_H = 36
TABLE_ROW_H = 38
TABLE_HEADER_H = 44
PAD = 30


def find_group_dir(criterion_dir: Path, group_label: str) -> Path:
  """Find the criterion group directory, handling case-folding (Windows)."""
  candidates = [d for d in criterion_dir.iterdir() if d.is_dir() and d.name.lower() == group_label.lower()]
  if not candidates:
    raise FileNotFoundError(
      f"No criterion group dir matching {group_label!r} under {criterion_dir}.\n"
      f"Run `cargo bench --bench instant` first, or pass --criterion-dir."
    )
  return candidates[0]


def read_pdf_small(criterion_dir: Path, group_label: str, crate: str) -> tuple[str, float, float]:
  """Read criterion's per-crate pdf_small.svg. Returns (inner_content, width, height)."""
  group_dir = find_group_dir(criterion_dir, group_label)
  svg_path = group_dir / crate / "report" / "pdf_small.svg"
  if not svg_path.exists():
    raise FileNotFoundError(f"Missing pdf_small SVG at {svg_path}")
  return _extract_svg_body(svg_path.read_text(), svg_path)


def _extract_svg_body(text: str, svg_path: Path) -> tuple[str, float, float]:
  outer = re.search(r"<svg\b([^>]*)>", text)
  if not outer:
    raise ValueError(f"No <svg> root in {svg_path}")
  attrs = outer.group(1)
  w_match = re.search(r'width="([0-9.]+)"', attrs)
  h_match = re.search(r'height="([0-9.]+)"', attrs)
  if w_match and h_match:
    width = float(w_match.group(1))
    height = float(h_match.group(1))
  else:
    vb = re.search(r'viewBox="[\d.]+\s+[\d.]+\s+([\d.]+)\s+([\d.]+)"', attrs)
    if not vb:
      raise ValueError(f"Couldn't determine dimensions of {svg_path}")
    width, height = float(vb.group(1)), float(vb.group(2))
  body_start = outer.end()
  body_end = text.rfind("</svg>")
  inner = text[body_start:body_end].strip()
  return inner, width, height


def read_violin(criterion_dir: Path, group_label: str) -> tuple[str, float, float]:
  """Read criterion's violin SVG. Returns (inner_content, width, height).
  Handles both gnuplot (`<g id="gnuplot_canvas">…</g>` wrapped) and plotters
  (loose elements inside `<svg>`) output formats.
  """
  group_dir = find_group_dir(criterion_dir, group_label)
  svg_path = group_dir / "report" / "violin.svg"
  if not svg_path.exists():
    raise FileNotFoundError(f"Missing violin SVG at {svg_path}")
  text = svg_path.read_text()

  return _extract_svg_body(text, svg_path)


def read_estimates(criterion_dir: Path, group_label: str, crate: str) -> dict:
  """Return {median_ns, lower_ns, upper_ns} for one crate in one group."""
  group_dir = find_group_dir(criterion_dir, group_label)
  est_path = group_dir / crate / "new" / "estimates.json"
  if not est_path.exists():
    raise FileNotFoundError(f"Missing estimates at {est_path}")
  data = json.loads(est_path.read_text())
  median = data["median"]
  return {
    "median_ns": median["point_estimate"],
    "lower_ns": median["confidence_interval"]["lower_bound"],
    "upper_ns": median["confidence_interval"]["upper_bound"],
  }


def fmt_ns(value: float) -> str:
  if value >= 100:
    return f"{value:.0f}"
  if value >= 10:
    return f"{value:.1f}"
  return f"{value:.2f}"


def text_el(
  x: float,
  y: float,
  value: str,
  size: int,
  family: str = FONT,
  color: str = TEXT_FG,
  anchor: str = "start",
  weight: str | None = None,
) -> str:
  weight_attr = f' font-weight="{weight}"' if weight else ""
  return (
    f'<text x="{x:g}" y="{y:g}" text-anchor="{anchor}" '
    f'font-family="{family}" font-size="{size}"{weight_attr} '
    f'fill="{color}">{html.escape(value)}</text>'
  )


def build_table(
  now_data: dict[str, dict],
  elapsed_data: dict[str, dict],
  y_top: float,
  range_label: str = "95% CI",
) -> tuple[str, float]:
  """Build the per-crate medians table. Returns (svg_fragment, y_bottom).
  range_label is what appears in the column headers above the bracketed values
  (e.g. "95% CI" for criterion or "min–max" for Lambda)."""
  parts = []
  col_x_crate = PAD + 20
  col_x_now = PAD + 360
  col_x_now_ci = PAD + 600
  col_x_elapsed = PAD + 880
  col_x_elapsed_ci = PAD + 1120

  hy = y_top + 26
  parts.append(text_el(col_x_crate, hy, "crate", 16, family=MONO, color=MUTED_FG, weight="600"))
  parts.append(text_el(col_x_now, hy, "now() median", 16, family=MONO, color=MUTED_FG, weight="600", anchor="end"))
  parts.append(text_el(col_x_now_ci, hy, f"now() {range_label}", 16, family=MONO, color=MUTED_FG, weight="600", anchor="end"))
  parts.append(text_el(col_x_elapsed, hy, "now+elapsed median", 16, family=MONO, color=MUTED_FG, weight="600", anchor="end"))
  parts.append(text_el(col_x_elapsed_ci, hy, f"now+elapsed {range_label}", 16, family=MONO, color=MUTED_FG, weight="600", anchor="end"))

  # Underline
  underline_y = hy + 8
  parts.append(
    f'<line x1="{PAD}" y1="{underline_y:g}" x2="{TARGET_WIDTH - PAD}" y2="{underline_y:g}" '
    f'stroke="{MUTED_FG}" stroke-width="0.5" opacity="0.5"/>'
  )

  # Data rows
  for i, crate in enumerate(CRATES):
    ry = underline_y + 12 + (i + 1) * TABLE_ROW_H - 12
    color = TACH_FG if crate == "tach" else TEXT_FG
    weight = "600" if crate == "tach" else None
    parts.append(text_el(col_x_crate, ry, crate, 18, family=MONO, color=color, weight=weight))

    nd = now_data[crate]
    ed = elapsed_data[crate]

    parts.append(text_el(col_x_now, ry, f"{fmt_ns(nd['median_ns'])} ns", 18, family=MONO, color=color, weight=weight, anchor="end"))
    ci_now = f"[{fmt_ns(nd['lower_ns'])}, {fmt_ns(nd['upper_ns'])}]"
    parts.append(text_el(col_x_now_ci, ry, ci_now, 16, family=MONO, color=MUTED_FG, anchor="end"))

    parts.append(text_el(col_x_elapsed, ry, f"{fmt_ns(ed['median_ns'])} ns", 18, family=MONO, color=color, weight=weight, anchor="end"))
    ci_el = f"[{fmt_ns(ed['lower_ns'])}, {fmt_ns(ed['upper_ns'])}]"
    parts.append(text_el(col_x_elapsed_ci, ry, ci_el, 16, family=MONO, color=MUTED_FG, anchor="end"))

  table_height = TABLE_HEADER_H + len(CRATES) * TABLE_ROW_H + 12
  return "\n".join(parts), y_top + table_height


def build_section_label(text: str, y: float) -> str:
  return text_el(PAD, y + 22, text, 22, family=FONT, color=TEXT_FG, weight="600")


def embed_violin(inner: str, src_w: float, src_h: float, y_offset: float) -> tuple[str, float]:
  """Wrap a violin's inner SVG content so it lands at (0, y_offset) scaled to
  fit TARGET_WIDTH. Returns (svg_fragment, rendered_height)."""
  scale = TARGET_WIDTH / src_w
  rendered_h = src_h * scale
  if abs(scale - 1.0) < 1e-6:
    transform = f"translate(0, {y_offset:g})"
  else:
    transform = f"translate(0, {y_offset:g}) scale({scale:g})"
  return f'<g transform="{transform}">{inner}</g>', rendered_h


def embed_pdf_row(
  criterion_dir: Path, group_label: str, y_offset: float
) -> tuple[str, float]:
  """Lay out one pdf_small per crate horizontally. Returns (svg_fragment, rendered_height)."""
  pdfs = [read_pdf_small(criterion_dir, group_label, c) for c in CRATES]
  src_w = pdfs[0][1]
  src_h = pdfs[0][2]
  n = len(CRATES)
  gap = 8
  inner_pad = PAD
  available = TARGET_WIDTH - 2 * inner_pad
  cell_w = (available - gap * (n - 1)) / n
  scale = cell_w / src_w
  cell_h = src_h * scale
  label_h = 22

  parts = []
  for i, (crate, (inner, _, _)) in enumerate(zip(CRATES, pdfs)):
    x = inner_pad + i * (cell_w + gap)
    label_color = TACH_FG if crate == "tach" else TEXT_FG
    label_weight = "600" if crate == "tach" else None
    parts.append(
      text_el(
        x + cell_w / 2, y_offset + label_h - 6, crate,
        16, family=MONO, color=label_color, anchor="middle", weight=label_weight,
      )
    )
    parts.append(
      f'<g transform="translate({x:g}, {y_offset + label_h:g}) scale({scale:g})">{inner}</g>'
    )

  total_h = label_h + cell_h
  return "\n".join(parts), total_h


def embed_pdf_row_entries(
  criterion_dir: Path,
  group_label: str,
  entries: list[tuple[str, str, bool]],
  y_offset: float,
) -> tuple[str, float]:
  """Same shape as embed_pdf_row but takes custom (subdir, label, highlight) tuples."""
  pdfs = [read_pdf_small(criterion_dir, group_label, subdir) for subdir, _, _ in entries]
  src_w = pdfs[0][1]
  src_h = pdfs[0][2]
  n = len(entries)
  gap = 8
  inner_pad = PAD
  available = TARGET_WIDTH - 2 * inner_pad
  cell_w = (available - gap * (n - 1)) / n
  scale = cell_w / src_w
  cell_h = src_h * scale
  label_h = 22

  parts = []
  for i, ((_, label, highlight), (inner, _, _)) in enumerate(zip(entries, pdfs)):
    x = inner_pad + i * (cell_w + gap)
    color = TACH_FG if highlight else TEXT_FG
    weight = "600" if highlight else None
    parts.append(
      text_el(
        x + cell_w / 2, y_offset + label_h - 6, label,
        13, family=MONO, color=color, anchor="middle", weight=weight,
      )
    )
    parts.append(
      f'<g transform="translate({x:g}, {y_offset + label_h:g}) scale({scale:g})">{inner}</g>'
    )

  return "\n".join(parts), label_h + cell_h


def build_ordered_table(
  criterion_dir: Path,
  group_label: str,
  entries: list[tuple[str, str, bool]],
  y_top: float,
) -> tuple[str, float]:
  """Single-column table for the heterogeneous ordered-bench entries.
  Each row: bench label, median, 95% CI."""
  parts = []
  col_x_label = PAD + 20
  col_x_median = PAD + 760
  col_x_ci = PAD + 1080

  hy = y_top + 26
  parts.append(text_el(col_x_label, hy, "bench", 16, family=MONO, color=MUTED_FG, weight="600"))
  parts.append(text_el(col_x_median, hy, "median", 16, family=MONO, color=MUTED_FG, weight="600", anchor="end"))
  parts.append(text_el(col_x_ci, hy, "95% CI", 16, family=MONO, color=MUTED_FG, weight="600", anchor="end"))
  underline_y = hy + 8
  parts.append(
    f'<line x1="{PAD}" y1="{underline_y:g}" x2="{TARGET_WIDTH - PAD}" y2="{underline_y:g}" '
    f'stroke="{MUTED_FG}" stroke-width="0.5" opacity="0.5"/>'
  )

  for i, (subdir, label, highlight) in enumerate(entries):
    ry = underline_y + 12 + (i + 1) * TABLE_ROW_H - 12
    color = TACH_FG if highlight else TEXT_FG
    weight = "600" if highlight else None
    est = read_estimates(criterion_dir, group_label, subdir)
    parts.append(text_el(col_x_label, ry, label, 18, family=MONO, color=color, weight=weight))
    parts.append(text_el(col_x_median, ry, f"{fmt_ns(est['median_ns'])} ns",
                          18, family=MONO, color=color, weight=weight, anchor="end"))
    ci = f"[{fmt_ns(est['lower_ns'])}, {fmt_ns(est['upper_ns'])}]"
    parts.append(text_el(col_x_ci, ry, ci, 16, family=MONO, color=MUTED_FG, anchor="end"))

  return "\n".join(parts), y_top + TABLE_HEADER_H + len(entries) * TABLE_ROW_H + 12


def build_report(criterion_dir: Path, cell_name: str, title: str, subtitle: str) -> str:
  now_inner, now_w, now_h = read_violin(criterion_dir, GROUP_NOW)
  elapsed_inner, el_w, el_h = read_violin(criterion_dir, GROUP_ELAPSED)
  now_data = {c: read_estimates(criterion_dir, GROUP_NOW, c) for c in CRATES}
  elapsed_data = {c: read_estimates(criterion_dir, GROUP_ELAPSED, c) for c in CRATES}

  # Ordered group is optional — only present if the bench was run with the
  # OrderedInstant group enabled.
  try:
    ordered_inner, ord_w, ord_h = read_violin(criterion_dir, GROUP_ORDERED)
    has_ordered = True
  except FileNotFoundError:
    has_ordered = False

  # Header
  title_y = 36
  subtitle_y = 66

  y = HEADER_H

  # now() section: violin + per-crate distributions
  now_label_y = y
  y += SECTION_LABEL_H
  now_violin_fragment, now_rendered_h = embed_violin(now_inner, now_w, now_h, y)
  y += now_rendered_h + 8
  now_dist_label_y = y
  y += SECTION_LABEL_H
  now_pdf_fragment, now_pdf_h = embed_pdf_row(criterion_dir, GROUP_NOW, y)
  y += now_pdf_h + PAD

  # elapsed section: violin + per-crate distributions
  elapsed_label_y = y
  y += SECTION_LABEL_H
  elapsed_violin_fragment, elapsed_rendered_h = embed_violin(elapsed_inner, el_w, el_h, y)
  y += elapsed_rendered_h + 8
  elapsed_dist_label_y = y
  y += SECTION_LABEL_H
  elapsed_pdf_fragment, elapsed_pdf_h = embed_pdf_row(criterion_dir, GROUP_ELAPSED, y)
  y += elapsed_pdf_h + PAD

  # Table section
  table_label_y = y
  y += SECTION_LABEL_H
  table_fragment, table_bottom = build_table(now_data, elapsed_data, y)
  y = table_bottom + PAD

  # Ordered section: violin + per-entry distributions + ordered table
  if has_ordered:
    ordered_label_y = y
    y += SECTION_LABEL_H
    ordered_violin_fragment, ordered_rendered_h = embed_violin(ordered_inner, ord_w, ord_h, y)
    y += ordered_rendered_h + 8
    ordered_dist_label_y = y
    y += SECTION_LABEL_H
    ordered_pdf_fragment, ordered_pdf_h = embed_pdf_row_entries(
      criterion_dir, GROUP_ORDERED, ORDERED_ENTRIES, y,
    )
    y += ordered_pdf_h + PAD
    ordered_table_label_y = y
    y += SECTION_LABEL_H
    ordered_table_fragment, ordered_table_bottom = build_ordered_table(
      criterion_dir, GROUP_ORDERED, ORDERED_ENTRIES, y,
    )
    y = ordered_table_bottom + PAD

  total_height = int(y)
  width = TARGET_WIDTH

  parts = [
    '<?xml version="1.0" encoding="UTF-8"?>',
    f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" '
    f'width="{width}" height="{total_height}" viewBox="0 0 {width} {total_height}">',
    f'<rect width="{width}" height="{total_height}" fill="{BACKGROUND}"/>',

    text_el(PAD, title_y, title, 28, weight="600"),
    text_el(PAD, subtitle_y, subtitle, 14, family=MONO, color=MUTED_FG),

    build_section_label("Instant::now()", now_label_y),
    build_section_label("Instant::now() — per-crate distribution", now_dist_label_y),
    build_section_label("Instant::now() + elapsed()", elapsed_label_y),
    build_section_label("Instant::now() + elapsed() — per-crate distribution", elapsed_dist_label_y),
    build_section_label("Per-crate medians and 95% confidence intervals (nanoseconds)", table_label_y),

    now_violin_fragment,
    now_pdf_fragment,
    elapsed_violin_fragment,
    elapsed_pdf_fragment,
    table_fragment,
  ]

  if has_ordered:
    parts.extend([
      build_section_label("Ordered Instant::now() — barrier-ordered counter reads", ordered_label_y),
      build_section_label("Ordered Instant::now() — per-bench distribution", ordered_dist_label_y),
      build_section_label("Ordered bench medians and 95% confidence intervals (nanoseconds)", ordered_table_label_y),
      ordered_violin_fragment,
      ordered_pdf_fragment,
      ordered_table_fragment,
    ])

  parts.append('</svg>')

  return "\n".join(parts) + "\n"


def read_lambda_runs(runs_dir: Path) -> list[dict]:
  """Read all run*.json files in a directory. Returns list of run dicts."""
  files = sorted(runs_dir.glob("run*.json"))
  if not files:
    raise FileNotFoundError(f"No run*.json files found in {runs_dir}")
  return [json.loads(p.read_text()) for p in files]


def build_lambda_bar_chart(
  runs: list[dict], metric: str, y_offset: float
) -> tuple[str, float]:
  """Horizontal bar chart with whiskers. One row per crate, bar = median across
  runs, whiskers = [min, max] range. metric in {"now", "elapsed"}.
  Returns (svg_fragment, rendered_height)."""
  parts = []
  row_h = 36
  bar_h = 18
  left = PAD + 100  # space for crate label
  right = TARGET_WIDTH - PAD - 200  # space for value label on right
  bar_area_w = right - left

  # Collect per-crate {min, median, max}
  stats = {}
  for crate in CRATES:
    samples = sorted(r[crate][metric] for r in runs)
    stats[crate] = {
      "min": samples[0],
      "median": samples[len(samples) // 2],
      "max": samples[-1],
    }
  global_max = max(s["max"] for s in stats.values())

  for i, crate in enumerate(CRATES):
    row_y = y_offset + i * row_h
    bar_center_y = row_y + row_h / 2
    bar_top_y = bar_center_y - bar_h / 2

    color = TACH_FG if crate == "tach" else "#5B6472"
    if crate == "fastant":
      color = "#4F6F6A"
    elif crate == "minstant":
      color = "#8B5E3C"
    elif crate == "std":
      color = "#9A8A3A"

    # Crate label (left)
    parts.append(
      text_el(left - 12, bar_center_y + 5, crate, 16, family=MONO, anchor="end",
              color=(TACH_FG if crate == "tach" else TEXT_FG),
              weight=("600" if crate == "tach" else None))
    )

    s = stats[crate]
    median_x = left + (s["median"] / global_max) * bar_area_w
    min_x = left + (s["min"] / global_max) * bar_area_w
    max_x = left + (s["max"] / global_max) * bar_area_w

    # Bar (0 → median)
    parts.append(
      f'<rect x="{left:g}" y="{bar_top_y:g}" width="{median_x - left:g}" '
      f'height="{bar_h}" fill="{color}"/>'
    )

    # Whiskers (min → max), drawn over the bar area
    whisker_y = bar_center_y
    cap_half = 5
    parts.append(
      f'<line x1="{min_x:g}" y1="{whisker_y:g}" x2="{max_x:g}" y2="{whisker_y:g}" '
      f'stroke="{TEXT_FG}" stroke-width="1.2"/>'
    )
    # Min cap
    parts.append(
      f'<line x1="{min_x:g}" y1="{whisker_y - cap_half:g}" x2="{min_x:g}" y2="{whisker_y + cap_half:g}" '
      f'stroke="{TEXT_FG}" stroke-width="1.2"/>'
    )
    # Max cap
    parts.append(
      f'<line x1="{max_x:g}" y1="{whisker_y - cap_half:g}" x2="{max_x:g}" y2="{whisker_y + cap_half:g}" '
      f'stroke="{TEXT_FG}" stroke-width="1.2"/>'
    )

    # Value label on right
    label = f"{fmt_ns(s['median'])} ns  [{fmt_ns(s['min'])}–{fmt_ns(s['max'])}]"
    parts.append(
      text_el(right + 8, bar_center_y + 5, label, 14, family=MONO, anchor="start",
              color=(TACH_FG if crate == "tach" else TEXT_FG),
              weight=("600" if crate == "tach" else None))
    )

  total_h = len(CRATES) * row_h + 10
  return "\n".join(parts), total_h


def build_lambda_report(runs: list[dict], cell_name: str, title: str, subtitle: str) -> str:
  """Build the Lambda variant of the per-cell SVG."""
  title_y = 36
  subtitle_y = 66
  y = HEADER_H

  now_label_y = y
  y += SECTION_LABEL_H
  now_chart, now_h = build_lambda_bar_chart(runs, "now", y)
  y += now_h + PAD

  elapsed_label_y = y
  y += SECTION_LABEL_H
  elapsed_chart, elapsed_h = build_lambda_bar_chart(runs, "elapsed", y)
  y += elapsed_h + PAD

  # Re-shape runs into the form build_table expects: {crate: {median_ns, lower_ns, upper_ns}}
  # Use the run-median for median_ns, min/max as the bounds.
  def aggregate(metric: str) -> dict[str, dict]:
    out = {}
    for crate in CRATES:
      samples = sorted(r[crate][metric] for r in runs)
      out[crate] = {
        "median_ns": samples[len(samples) // 2],
        "lower_ns": samples[0],
        "upper_ns": samples[-1],
      }
    return out

  table_label_y = y
  y += SECTION_LABEL_H
  table_fragment, table_bottom = build_table(aggregate("now"), aggregate("elapsed"), y, range_label="min–max")
  y = table_bottom + PAD

  total_height = int(y)
  width = TARGET_WIDTH

  parts = [
    '<?xml version="1.0" encoding="UTF-8"?>',
    f'<svg xmlns="http://www.w3.org/2000/svg" '
    f'width="{width}" height="{total_height}" viewBox="0 0 {width} {total_height}">',
    f'<rect width="{width}" height="{total_height}" fill="{BACKGROUND}"/>',

    text_el(PAD, title_y, title, 28, weight="600"),
    text_el(PAD, subtitle_y, subtitle, 14, family=MONO, color=MUTED_FG),

    build_section_label(f"Instant::now() — median (bar) and min–max across {len(runs)} runs", now_label_y),
    build_section_label(f"Instant::now() + elapsed() — median (bar) and min–max across {len(runs)} runs", elapsed_label_y),
    build_section_label("Per-crate aggregate (nanoseconds)", table_label_y),

    now_chart,
    elapsed_chart,
    table_fragment,

    '</svg>',
  ]
  return "\n".join(parts) + "\n"


SKEWMONO_CLOCKS = [
  ("tach", "tach::Instant", True),
  ("tach_ordered", "tach::OrderedInstant", True),
  ("tach_recal", "tach::Instant + recal-bg", True),
  ("std", "std::time::Instant", False),
  ("quanta", "quanta::Instant", False),
  ("minstant", "minstant::Instant", False),
  ("fastant", "fastant::Instant", False),
]


def _fmt_skew_ns(ns: int | float) -> str:
  if ns is None:
    return "n/a"
  abs_ns = abs(ns)
  sign = "-" if ns < 0 else ""
  if abs_ns >= 1_000_000:
    return f"{sign}{abs_ns/1_000_000:.2f} ms"
  if abs_ns >= 1_000:
    return f"{sign}{abs_ns/1_000:.2f} µs"
  return f"{sign}{abs_ns:.0f} ns"


def _fmt_count(n: int | float) -> str:
  if n is None:
    return "n/a"
  if n == 0:
    return "0"
  if n >= 1_000_000_000:
    return f"{n/1_000_000_000:.2f}B"
  if n >= 1_000_000:
    return f"{n/1_000_000:.2f}M"
  if n >= 1_000:
    return f"{n/1_000:.1f}k"
  return f"{n:.0f}"


def build_skewmono_report(data: dict, cell_name: str, title: str, subtitle: str) -> str:
  """Render a single-cell skew+monotonicity SVG.

  Layout:
    - header (cell, target_triple, host info)
    - per-thread monotonicity table (expected all-zero; flagged red if not)
    - cross-thread monotonicity table (max violation magnitude per clock)
    - skew table (1s + 1m median per clock; ppm)
    - footer (methodology one-liner)
  """
  width = 1100
  margin = 32
  row_h = 26
  header_h = 130
  body = []

  body.append(
    f'<rect width="{width}" height="900" fill="{BACKGROUND}"/>'
  )
  body.append(text_el(margin, 40, title, 26, weight="bold"))
  if subtitle:
    body.append(text_el(margin, 64, subtitle, 14, color=MUTED_FG))

  host = data.get("host", {})
  meta_lines = [
    f"target: {data.get('target_triple', '?')}  ·  cpu: {host.get('cpu_model','?')}  ·  cpus: {host.get('num_cpus','?')}  ·  kernel: {host.get('kernel','?')}",
    f"tach freq: {data.get('tach_freq_hz', 0):,} Hz  ·  CPUID 15h: {data.get('tach_used_cpuid_15h')}",
  ]
  for i, line in enumerate(meta_lines):
    body.append(text_el(margin, 90 + i * 16, line, 11, color=MUTED_FG, family="monospace"))

  clocks = data.get("clocks", {})

  # Per-thread monotonicity
  y = header_h + 28
  body.append(text_el(margin, y, "Per-thread monotonicity (backward jumps in tight single-thread loop)", 14, weight="bold"))
  y += 8
  cols = [(margin, "Clock"), (380, "Violations"), (520, "Total reads"), (660, "Max jump"), (800, "Duration")]
  body.append(f'<line x1="{margin}" y1="{y+8}" x2="{width-margin}" y2="{y+8}" stroke="{MUTED_FG}" stroke-width="0.5"/>')
  for x, label in cols:
    body.append(text_el(x, y, label, 11, color=MUTED_FG, weight="bold"))
  y += row_h

  for key, label, highlight in SKEWMONO_CLOCKS:
    cr = clocks.get(key)
    if not cr:
      continue
    pt = cr.get("per_thread", {})
    violations = pt.get("violations", 0)
    color = TEXT_FG if violations == 0 else "#cc4444"
    weight = "bold" if highlight else None
    body.append(text_el(margin, y, label, 12, weight=weight))
    body.append(text_el(380, y, _fmt_count(violations), 12, family="monospace", color=color))
    body.append(text_el(520, y, _fmt_count(pt.get("total_reads", 0)), 12, family="monospace"))
    body.append(text_el(660, y, _fmt_skew_ns(pt.get("max_violation_ns", 0)), 12, family="monospace"))
    body.append(text_el(800, y, f"{pt.get('duration_ns',0)/1e9:.1f} s", 12, family="monospace"))
    y += row_h

  # Cross-thread monotonicity
  y += 20
  body.append(text_el(margin, y, "Cross-thread observation consistency (10s, N threads, atomic-max race)", 14, weight="bold"))
  y += 8
  cols = [(margin, "Clock"), (380, "Max viol"), (520, "Total viol"), (660, "Threads"), (800, "Preempt dropped")]
  body.append(f'<line x1="{margin}" y1="{y+8}" x2="{width-margin}" y2="{y+8}" stroke="{MUTED_FG}" stroke-width="0.5"/>')
  for x, label in cols:
    body.append(text_el(x, y, label, 11, color=MUTED_FG, weight="bold"))
  y += row_h

  for key, label, highlight in SKEWMONO_CLOCKS:
    cr = clocks.get(key)
    if not cr:
      continue
    ct = cr.get("cross_thread", {})
    max_v = ct.get("max_violation_ns", 0)
    color = TEXT_FG if max_v <= 1000 else ("#cc8844" if max_v <= 10000 else "#cc4444")
    weight = "bold" if highlight else None
    body.append(text_el(margin, y, label, 12, weight=weight))
    body.append(text_el(380, y, _fmt_skew_ns(max_v), 12, family="monospace", color=color))
    body.append(text_el(520, y, _fmt_count(ct.get("total_violations", 0)), 12, family="monospace"))
    body.append(text_el(660, y, str(ct.get("threads", 0)), 12, family="monospace"))
    body.append(text_el(800, y, _fmt_count(ct.get("preemption_dropped", 0)), 12, family="monospace"))
    y += row_h

  # Skew table
  y += 20
  body.append(text_el(margin, y, "Drift vs std::Instant reference clock", 14, weight="bold"))
  y += 8
  cols = [(margin, "Clock"), (380, "1s median"), (520, "1s ppm"), (660, "1m median"), (800, "1m ppm")]
  body.append(f'<line x1="{margin}" y1="{y+8}" x2="{width-margin}" y2="{y+8}" stroke="{MUTED_FG}" stroke-width="0.5"/>')
  for x, label in cols:
    body.append(text_el(x, y, label, 11, color=MUTED_FG, weight="bold"))
  y += row_h

  for key, label, highlight in SKEWMONO_CLOCKS:
    cr = clocks.get(key)
    if not cr:
      continue
    s1 = cr.get("skew_1s") or {}
    s60 = cr.get("skew_1m") or {}
    weight = "bold" if highlight else None
    body.append(text_el(margin, y, label, 12, weight=weight))
    body.append(text_el(380, y, _fmt_skew_ns(s1.get("median_skew_ns", 0)), 12, family="monospace"))
    body.append(text_el(520, y, f"{s1.get('median_skew_ppm', 0):+.2f}", 12, family="monospace"))
    body.append(text_el(660, y, _fmt_skew_ns(s60.get("median_skew_ns")) if s60 else "n/a", 12, family="monospace"))
    body.append(text_el(800, y, f"{s60.get('median_skew_ppm', 0):+.2f}" if s60 else "n/a", 12, family="monospace"))
    y += row_h

  # Footer
  y += 32
  footer = (
    "Methodology: per-thread = tight loop for 10s; cross-thread = N threads "
    "racing on shared atomic-max with bracket-read filter for preemption; "
    "skew vs std::Instant; medians of 30 (1s) / 5 (1m) samples."
  )
  body.append(text_el(margin, y, footer, 11, color=MUTED_FG))

  total_h = y + 40
  svg_open = (
    f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {total_h}" '
    f'width="{width}" height="{total_h}">'
  )
  return svg_open + "\n".join(body) + "</svg>"


def main() -> int:
  ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
  ap.add_argument("cell_name", help="Cell identifier used as output filename")
  ap.add_argument("--title", help="Header title (defaults to cell name)")
  ap.add_argument("--subtitle", default="", help="Header subtitle (defaults to empty)")
  ap.add_argument(
    "--criterion-dir",
    type=Path,
    default=DEFAULT_CRITERION_DIR,
    help=f"Criterion output directory (criterion mode; default: {DEFAULT_CRITERION_DIR})",
  )
  ap.add_argument(
    "--lambda-runs",
    type=Path,
    help="Directory with run*.json files from the Lambda harness (selects Lambda mode)",
  )
  ap.add_argument(
    "--skewmono-json",
    type=Path,
    help="Path to a tach-skew-bench/v1 JSON file (selects skewmono mode)",
  )
  args = ap.parse_args()

  title = args.title if args.title else args.cell_name
  OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

  if args.skewmono_json is not None:
    if not args.skewmono_json.exists():
      print(f"error: {args.skewmono_json} not found.", file=sys.stderr)
      return 2
    output_path = OUTPUT_DIR / f"report-skewmono-{args.cell_name}.svg"
    data = json.loads(args.skewmono_json.read_text())
    output_path.write_text(build_skewmono_report(data, args.cell_name, title, args.subtitle))
  elif args.lambda_runs is not None:
    output_path = OUTPUT_DIR / f"report-{args.cell_name}.svg"
    if not args.lambda_runs.exists():
      print(f"error: {args.lambda_runs} not found.", file=sys.stderr)
      return 2
    runs = read_lambda_runs(args.lambda_runs)
    output_path.write_text(build_lambda_report(runs, args.cell_name, title, args.subtitle))
  else:
    output_path = OUTPUT_DIR / f"report-{args.cell_name}.svg"
    if not args.criterion_dir.exists():
      print(f"error: {args.criterion_dir} not found. Run `cargo bench --bench instant` first.", file=sys.stderr)
      return 2
    output_path.write_text(build_report(args.criterion_dir, args.cell_name, title, args.subtitle))

  print(f"wrote {output_path} ({output_path.stat().st_size:,} bytes)")
  return 0


if __name__ == "__main__":
  sys.exit(main())
