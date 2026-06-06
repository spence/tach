"""Single shared source of per-cell speed medians (ns), feeding both charts.

Each cell is a self-contained `benches/speed-<cell>.json` written by the
campaign:

    {"title": ..., "instance": ..., "triple": ..., "order": N,
     "clocks": {clock: {"now": ns, "elapsed": ns}}}

clocks: tach, tach_ordered, quanta, fastant, minstant, std.

`summary.py` and `summary-ordered.py` both `from bench_data import CELLS` and
slice the SAME objects, so `std` (and every shared clock) is byte-identical
across the two charts by construction, not by re-typing the number twice.
"""

import json
from pathlib import Path

_DIR = Path(__file__).resolve().parent


def _load():
    cells = []
    for p in sorted(_DIR.glob("speed-*.json")):
        d = json.loads(p.read_text())
        header = (d["title"], d["instance"], d["triple"])
        cells.append((d.get("order", 99), header, d["clocks"]))
    cells.sort(key=lambda c: c[0])
    return [(header, clocks) for _, header, clocks in cells]


# CELLS: [((title, instance, triple), {clock: {"now": ns, "elapsed": ns}}), ...]
CELLS = _load()
