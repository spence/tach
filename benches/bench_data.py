"""Single shared source of per-cell speed medians (ns), feeding both charts.

Each cell is a self-contained `benches/speed-<cell>.json` written by the
campaign:

    {"title": ..., "instance": ..., "triple": ..., "order": N,
     "clocks": {clock: {"now": ns, "elapsed": ns}}}

clocks: tach, tach_ordered, quanta, fastant, minstant, std,
tach_thread_cpu, native_thread_cpu. Thread-CPU clocks also include `provider`
and `read_cost` strings alongside their numeric measurements.

`summary.py` and `summary-ordered.py` both `from bench_data import CELLS` and
slice the SAME objects, so `std` (and every shared clock) is byte-identical
across the two charts by construction, not by re-typing the number twice.
"""

import json
from pathlib import Path

_DIR = Path(__file__).resolve().parent


def load_cell_documents(directory=_DIR):
    documents = []
    for p in sorted(Path(directory).glob("speed-[0-9]-*.json")):
        d = json.loads(p.read_text())
        documents.append(d)
    documents.sort(key=lambda d: d.get("order", 99))
    return documents


def load_cells(directory=_DIR):
    return [
        ((d["title"], d["instance"], d["triple"]), d["clocks"])
        for d in load_cell_documents(directory)
    ]


# CELLS: [((title, instance, triple), {clock: {"now": ns, "elapsed": ns}}), ...]
CELLS = load_cells()
