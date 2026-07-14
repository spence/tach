"""Select the six native speed cells rendered on tach's public claim charts."""

from __future__ import annotations


CHART_CELLS = (
  (
    "speed-0-apple.json",
    ("Apple Silicon", "M1 Max MacBook Pro", "aarch64-apple-darwin"),
  ),
  (
    "speed-supplemental-macos-x86_64.json",
    ("GitHub Intel macOS", "macos-15-intel", "x86_64-apple-darwin"),
  ),
  (
    "speed-1-c7g.json",
    ("AWS Graviton 3", "c7g.large", "aarch64-unknown-linux-gnu"),
  ),
  (
    "speed-2-inteln.json",
    ("AWS Intel Linux", "c7i.large", "x86_64-unknown-linux-gnu"),
  ),
  (
    "speed-4-windows.json",
    ("GitHub Windows", "windows-2025", "x86_64-pc-windows-msvc"),
  ),
  (
    "speed-supplemental-freebsd-x86_64.json",
    ("AWS FreeBSD", "c7i.large", "x86_64-unknown-freebsd"),
  ),
)


def cells_from_release_snapshot(snapshot):
  """Build display cells only from bytes admitted by the complete release gate."""
  artifact_ids = tuple(artifact_id for artifact_id, _header in CHART_CELLS)
  documents = snapshot.claim_chart_documents(artifact_ids)
  cells = []
  for artifact_id, header in CHART_CELLS:
    document = documents[artifact_id]
    if document.get("triple") != header[2]:
      raise ValueError(
        f"release chart {artifact_id} target changed: "
        f"expected {header[2]!r}, got {document.get('triple')!r}"
      )
    clocks = document.get("clocks")
    if not isinstance(clocks, dict):
      raise ValueError(f"release chart {artifact_id} has no clocks object")
    cells.append((header, clocks))
  return cells
