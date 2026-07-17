#!/usr/bin/env python3
"""Validate M2 inline-parity probes: paired public/exact reads within max(1 ns, 5%).

Each converted wall family writes a `public_exact_probe` object into its
`target/criterion/<family>-selection.json` file. A probe is a tree whose leaves
each carry `public_batches_ns`, `exact_batches_ns`, and `reads_per_batch`. This
validator finds every leaf under every `public_exact_probe` (handling both the
direct `{instant, ordered}` shape and the nested `{now, elapsed}` shape),
computes the per-read delta, and fails if any leaf exceeds its band.

For one leaf:
    exact  = median(exact_batches_ns)  / reads_per_batch
    public = median(public_batches_ns) / reads_per_batch
    delta  = public - exact
    band   = max(1.0 ns, 0.05 * exact)
    PASS iff abs(delta) <= band

The band is the M2.G1 gate: max(1 ns, 5%). It is NOT widened here.
"""

import json
import statistics
import sys
from pathlib import Path

LEAF_KEYS = ("public_batches_ns", "exact_batches_ns", "reads_per_batch")


def is_leaf(node):
  return isinstance(node, dict) and all(key in node for key in LEAF_KEYS)


def collect_probes(node, out):
  """Collect every value stored under a `public_exact_probe` key, at any depth."""
  if isinstance(node, dict):
    for key, value in node.items():
      if key == "public_exact_probe":
        out.append(value)
      else:
        collect_probes(value, out)
  elif isinstance(node, list):
    for item in node:
      collect_probes(item, out)


def collect_leaves(node, path, out):
  """Collect (path, leaf) pairs beneath a probe value."""
  if is_leaf(node):
    out.append((path, node))
    return
  if isinstance(node, dict):
    for key, value in node.items():
      collect_leaves(value, f"{path}/{key}" if path else key, out)


def evaluate_leaf(leaf):
  reads = leaf["reads_per_batch"]
  if not isinstance(reads, (int, float)) or reads <= 0:
    raise ValueError(f"reads_per_batch must be a positive number, got {reads!r}")
  public_batches = leaf["public_batches_ns"]
  exact_batches = leaf["exact_batches_ns"]
  if not public_batches or not exact_batches:
    raise ValueError("public_batches_ns/exact_batches_ns must be non-empty")
  exact = statistics.median(exact_batches) / reads
  public = statistics.median(public_batches) / reads
  delta = public - exact
  band = max(1.0, 0.05 * exact)
  return exact, public, delta, band, abs(delta) <= band


def main(argv):
  paths = argv[1:]
  if not paths:
    print("usage: check-inline-parity.py <selection.json> [<selection.json> ...]", file=sys.stderr)
    return 2

  total_leaves = 0
  failures = 0
  for raw_path in paths:
    path = Path(raw_path)
    if not path.is_file():
      print(f"ERROR: {raw_path}: no such file", file=sys.stderr)
      return 2
    document = json.loads(path.read_text())
    probes = []
    collect_probes(document, probes)
    if not probes:
      print(f"ERROR: {raw_path}: no public_exact_probe found", file=sys.stderr)
      return 2

    stem = path.stem
    file_leaves = 0
    for probe in probes:
      leaves = []
      collect_leaves(probe, "", leaves)
      for probe_path, leaf in leaves:
        label = f"{stem}/{probe_path}" if probe_path else stem
        exact, public, delta, band, ok = evaluate_leaf(leaf)
        verdict = "PASS" if ok else "FAIL"
        print(
          f"{label}: exact {exact:.3f} ns  public {public:.3f} ns  "
          f"delta {delta:+.3f} ns  band {band:.3f} ns -> {verdict}"
        )
        file_leaves += 1
        total_leaves += 1
        if not ok:
          failures += 1
    if file_leaves == 0:
      print(f"ERROR: {raw_path}: public_exact_probe carried no measurable leaves", file=sys.stderr)
      return 2

  if total_leaves == 0:
    print("ERROR: no parity leaves validated", file=sys.stderr)
    return 2
  if failures:
    print(f"\nFAILED: {failures}/{total_leaves} parity leaves exceeded max(1 ns, 5%)")
    return 1
  print(f"\nOK: {total_leaves} parity leaves within max(1 ns, 5%)")
  return 0


if __name__ == "__main__":
  sys.exit(main(sys.argv))
