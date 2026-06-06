#!/usr/bin/env python3
"""Extract per-clock now()/elapsed() medians from a criterion output tree.

Usage: extract_speed.py <path-to-target/criterion>

Prints JSON {clock: {"now": ns, "elapsed": ns}} for the six speed-bench clocks.
Every cell of the campaign (local, EC2, Docker-Alpine musl, Windows) funnels
through this so the extraction arithmetic is identical everywhere.
"""

import json
import sys
from pathlib import Path

FUNS = ["tach", "tach_ordered", "quanta", "fastant", "minstant", "std"]
# Criterion sanitizes the group label "Instant::now()" -> dir "Instant__now()"
# ("::" -> "__"); spaces / "+" / "()" are kept verbatim.
GROUPS = {"now": "Instant__now()", "elapsed": "Instant__now() + elapsed()"}


def median_ns(criterion_dir: Path, group_dir: str, fn: str) -> float:
    p = criterion_dir / group_dir / fn / "new" / "estimates.json"
    with open(p) as f:
        return json.load(f)["median"]["point_estimate"]


def main() -> None:
    criterion_dir = Path(sys.argv[1])
    out = {
        fn: {kind: median_ns(criterion_dir, gdir, fn) for kind, gdir in GROUPS.items()}
        for fn in FUNS
    }
    json.dump(out, sys.stdout, indent=2)
    print()


if __name__ == "__main__":
    main()
