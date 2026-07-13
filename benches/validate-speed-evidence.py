#!/usr/bin/env python3
"""Validate every release evidence requirement behind tach's public claims."""

from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parent
RELEASE_VALIDATOR_PATH = ROOT / "validate-release-evidence.py"
RELEASE_VALIDATOR_MODULE = "tach_release_evidence_for_speed_claims"


def load_release_validator():
  """Load the release gate as an in-process API, preserving its snapshot types."""
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


def validate(data_dir: Path, checkout_root: Path = ROOT.parent) -> dict:
  """Validate the complete release claim, not a primary-only subset."""
  return load_release_validator().validate_release_evidence(data_dir, checkout_root)


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--data-dir", type=Path, default=ROOT)
  parser.add_argument("--output", type=Path)
  args = parser.parse_args()
  try:
    report = validate(args.data_dir)
  except ValueError as error:
    parser.error(str(error))
  rendered = json.dumps(report, indent=2) + "\n"
  if args.output:
    args.output.write_text(rendered)
  print(rendered, end="")
  if not report["passed"]:
    raise SystemExit(1)


if __name__ == "__main__":
  main()
