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


def _reject_duplicate_json_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    document: dict[str, object] = {}
    for key, value in pairs:
        if key in document:
            raise ValueError(f"duplicate JSON key {key!r}")
        document[key] = value
    return document


def load_json_document(path: Path) -> dict:
    """Load one evidence document without conflating it with static manifests."""
    document = json.loads(
        Path(path).read_text(), object_pairs_hook=_reject_duplicate_json_keys
    )
    if not isinstance(document, dict):
        raise ValueError(f"evidence document is not a JSON object: {path}")
    return document


def load_cell_documents(directory=_DIR):
    documents = []
    for p in sorted(Path(directory).glob("speed-[0-9]-*.json")):
        d = load_json_document(p)
        documents.append(d)
    documents.sort(key=lambda d: d.get("order", 99))
    return documents


def load_supplemental_speed_documents(directory=_DIR):
    documents = {}
    for path in sorted(Path(directory).glob("speed-supplemental-*.json")):
        documents[path.name] = load_json_document(path)
    return documents


def load_cells(directory=_DIR):
    return [
        ((d["title"], d["instance"], d["triple"]), d["clocks"])
        for d in load_cell_documents(directory)
    ]


# CELLS: [((title, instance, triple), {clock: {"now": ns, "elapsed": ns}}), ...]
CELLS = load_cells()
