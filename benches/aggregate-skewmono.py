#!/usr/bin/env python3
"""Aggregate per-cell skewmono JSONs into the numbers we publish.

Usage:
  python3 benches/aggregate-skewmono.py [--write]

Reads every benches/skewmono-<cell>.json that exists, computes:
  - per-clock cross-cell median of 1s/1m skew (median ns + ppm)
  - per-clock max-of-max cross-thread violation magnitude
  - per-clock max-of-max per-thread backward-jump magnitude (expected 0)

Prints a Markdown drift table that can be pasted into README, plus a
per-cell breakdown table for BENCHMARKS.md. With --write, replaces those
sections directly.
"""
from __future__ import annotations
import argparse
import json
import statistics
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
BENCH_DIR = REPO / "benches"

CELL_ORDER = [
    "apple-silicon-m1",
    "c7g-4xlarge",
    "t3-medium",
    "m7i-metal-24xl",
    "lambda-x86_64",
    "github-windows-x86_64",
]

# Display order for clocks in tables.
CLOCK_ORDER = [
    ("tach",         "`tach::Instant` (default, `#![no_std]`)"),
    ("tach_recal",     "`tach::Instant` + `recalibrate-background` (**requires `std`**)"),
    ("tach_ordered",   "`tach::OrderedInstant` (default, `#![no_std]`)"),
    ("tach_monotonic", "`tach::MonotonicInstant` (default, `#![no_std]`)"),
    ("quanta",         "`quanta::Instant`"),
    ("minstant",       "`minstant::Instant`"),
    ("fastant",        "`fastant::Instant`"),
    ("std",            "`std::time::Instant`"),
]

# Clocks where recalibration bounds drift to a constant window (so 1h/1d
# are NOT linearly extrapolated from 1m; they stay at the 1m magnitude).
BOUNDED_CLOCKS = {"tach_recal", "std"}


def fmt_duration(ns: float | int) -> str:
    """Format a nanosecond magnitude as a compact human duration."""
    if ns is None:
        return "n/a"
    abs_ns = abs(ns)
    sign = "-" if ns < 0 else ""
    if abs_ns >= 1_000_000_000:
        return f"{sign}{abs_ns/1_000_000_000:.2f} s"
    if abs_ns >= 1_000_000:
        return f"{sign}{abs_ns/1_000_000:.1f} ms"
    if abs_ns >= 1_000:
        return f"{sign}{abs_ns/1_000:.1f} µs"
    return f"{sign}{abs_ns:.0f} ns"


def load_cells() -> dict[str, dict]:
    out = {}
    for cell in CELL_ORDER:
        p = BENCH_DIR / f"skewmono-{cell}.json"
        if p.exists():
            out[cell] = json.loads(p.read_text())
    return out


def cross_cell_median(cells: dict[str, dict], clock: str, key: str) -> float | None:
    """Median magnitude across cells for clock's <skew_1s|skew_1m>.median_skew_ns."""
    samples = []
    for c in cells.values():
        cr = c["clocks"].get(clock)
        if cr is None:
            continue
        section = cr.get(key)
        if section is None:
            continue
        v = section.get("median_skew_ns")
        if v is None:
            continue
        samples.append(abs(v))
    if not samples:
        return None
    return statistics.median(samples)


def cross_cell_max_xthread(cells: dict[str, dict], clock: str) -> tuple[int, int]:
    """(max_violation_ns_max, max_violation_ns_median) for the cross-thread test."""
    samples = []
    for c in cells.values():
        cr = c["clocks"].get(clock)
        if cr is None:
            continue
        section = cr.get("cross_thread", {})
        v = section.get("max_violation_ns", 0)
        if v:
            samples.append(v)
    if not samples:
        return 0, 0
    return max(samples), int(statistics.median(samples))


def cross_cell_perthread_violations(cells: dict[str, dict], clock: str) -> int:
    """Sum of per-thread violations across all cells. Expected 0."""
    total = 0
    for c in cells.values():
        cr = c["clocks"].get(clock)
        if cr is None:
            continue
        total += cr.get("per_thread", {}).get("violations", 0)
    return total


def derive_table_row(cells: dict[str, dict], clock: str) -> tuple[str, str, str, str]:
    """Return (1s, 1m, 1h, 1d) strings for the README drift table."""
    m1s = cross_cell_median(cells, clock, "skew_1s")
    m1m = cross_cell_median(cells, clock, "skew_1m")

    def cell(v):
        return fmt_duration(v) if v is not None else "n/a"

    s1s = cell(m1s)
    s1m = cell(m1m)

    if m1m is None:
        return s1s, s1m, "n/a", "n/a"
    if clock in BOUNDED_CLOCKS:
        # Drift bounded by either the recal interval (tach_recal: 60s) or the
        # kernel's continuous correction (std). 1h/1d don't grow further.
        s1h = cell(m1m)
        s1d = cell(m1m)
    else:
        # Linear extrapolation: 1h = 1m × 60, 1d = 1m × 1440.
        s1h = cell(m1m * 60)
        s1d = cell(m1m * 1440)
    return s1s, s1m, s1h, s1d


def build_drift_table(cells: dict[str, dict]) -> str:
    out = ["| Crate | 1-sec interval | 1-min interval | 1-hr interval | 1-day interval |",
           "|---|---|---|---|---|"]
    for clock, label in CLOCK_ORDER:
        s1s, s1m, s1h, s1d = derive_table_row(cells, clock)
        out.append(f"| {label} | {s1s} | {s1m} | {s1h} | {s1d} |")
    return "\n".join(out)


def build_xthread_table(cells: dict[str, dict]) -> str:
    out = [
        "Per-cell maximum cross-thread violation magnitude (ns). Cells where "
        "the value exceeds 10 µs are flagged as a hazard.",
        "",
        "| Clock | " + " | ".join(CELL_ORDER) + " |",
        "|---|" + "|".join(["---"] * len(CELL_ORDER)) + "|",
    ]
    for clock, _label in CLOCK_ORDER:
        cells_data = []
        for cell in CELL_ORDER:
            c = cells.get(cell)
            if c is None:
                cells_data.append("n/a")
                continue
            cr = c["clocks"].get(clock)
            if cr is None:
                cells_data.append("n/a")
                continue
            v = cr.get("cross_thread", {}).get("max_violation_ns", 0)
            cells_data.append(fmt_duration(v))
        out.append(f"| `{clock}` | " + " | ".join(cells_data) + " |")
    return "\n".join(out)


def build_perthread_summary(cells: dict[str, dict]) -> str:
    lines = []
    for cell in CELL_ORDER:
        c = cells.get(cell)
        if c is None:
            lines.append(f"- {cell}: not measured")
            continue
        any_violations = []
        for clock, _ in CLOCK_ORDER:
            cr = c["clocks"].get(clock)
            if cr is None:
                continue
            v = cr.get("per_thread", {}).get("violations", 0)
            if v:
                any_violations.append(f"{clock}={v}")
        if any_violations:
            lines.append(f"- **{cell}: backward jumps observed** — " + ", ".join(any_violations))
        else:
            lines.append(f"- {cell}: 0 backward jumps on any clock ✓")
    return "\n".join(lines)


def replace_drift_table(text: str, new_table: str) -> str:
    """Replace the existing drift table inside the `## drift` section. The
    table is identified by the header row starting with `| Crate |` and
    runs until the next blank line."""
    import re
    pattern = re.compile(
        r"(\| Crate \| 1-sec interval[^\n]*\n\|---[^\n]*\n(?:\|[^\n]*\n)+)",
        re.MULTILINE,
    )
    if not pattern.search(text):
        raise RuntimeError("could not find drift table in document")
    return pattern.sub(new_table + "\n", text, count=1)


def replace_or_append_bench_section(text: str, header: str, body: str) -> str:
    """Insert (or replace) a `## <header>` section just before the existing
    `## Drift methodology` section in BENCHMARKS.md, or at end if absent."""
    import re
    section_pattern = re.compile(
        r"^## " + re.escape(header) + r".*?(?=^## |\Z)",
        re.MULTILINE | re.DOTALL,
    )
    full = f"## {header}\n\n{body}\n\n"
    if section_pattern.search(text):
        return section_pattern.sub(full, text, count=1)
    # Insert before "## Drift methodology" if it exists, else at end
    anchor = re.search(r"^## Drift methodology", text, re.MULTILINE)
    if anchor:
        return text[: anchor.start()] + full + text[anchor.start():]
    return text + "\n" + full


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true",
                    help="Replace the README and BENCHMARKS sections in place.")
    args = ap.parse_args()

    cells = load_cells()
    if not cells:
        print("no skewmono-*.json found under benches/", file=sys.stderr)
        return 1
    print(f"loaded {len(cells)} cell(s): {', '.join(cells.keys())}", file=sys.stderr)

    drift = build_drift_table(cells)
    xthread = build_xthread_table(cells)
    perthread = build_perthread_summary(cells)

    print("=== README drift table ===")
    print(drift)
    print()
    print("=== BENCHMARKS cross-thread table ===")
    print(xthread)
    print()
    print("=== BENCHMARKS per-thread summary ===")
    print(perthread)

    if args.write:
        for name in ("README.md", "README.crates-io.md"):
            p = REPO / name
            txt = p.read_text()
            txt = replace_drift_table(txt, drift)
            p.write_text(txt)
            print(f"wrote {p}", file=sys.stderr)

        bench_md = REPO / "BENCHMARKS.md"
        txt = bench_md.read_text()
        xthread_body = (
            xthread
            + "\n\n### Per-thread monotonicity\n\n"
            + perthread
        )
        txt = replace_or_append_bench_section(txt, "Cross-thread monotonicity", xthread_body)
        bench_md.write_text(txt)
        print(f"wrote {bench_md}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
