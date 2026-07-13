#!/usr/bin/env python3
"""Copy one attested host observation into an immutable collector bundle."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import tempfile

import extract_speed
import host_speed


def collect_host_bundle(host_dir: Path, bundle_dir: Path) -> Path:
  if os.path.lexists(bundle_dir):
    raise RuntimeError(f"collector bundle destination already exists: {bundle_dir}")
  try:
    bundle_dir.resolve(strict=False).relative_to(host_dir.resolve(strict=True))
  except ValueError:
    pass
  else:
    raise RuntimeError("collector bundle destination must not be inside host observation")
  files = extract_speed.regular_file_tree(host_dir, "host observation")
  attestation_path = files.get(extract_speed.RUNTIME_ATTESTATION_FILENAME)
  if attestation_path is None:
    raise RuntimeError("host observation is missing runtime-attestation.json")
  attestation = extract_speed.validate_runtime_attestation(
    extract_speed.load_json_object(attestation_path, "host runtime attestation"),
    "host runtime attestation",
  )
  if attestation.get("harness") == "criterion":
    raise RuntimeError("host collector cannot relabel Criterion evidence")
  host_speed.extract_host_observation(host_dir, attestation)
  hashes = {
    relative: extract_speed.sha256_file(path, "host observation input")
    for relative, path in sorted(files.items())
  }

  parent = bundle_dir.parent
  if not parent.is_dir():
    raise RuntimeError(f"collector bundle parent is not a directory: {parent}")
  staging = Path(tempfile.mkdtemp(prefix=f".{bundle_dir.name}.", dir=parent))
  published = False
  try:
    copied = staging / extract_speed.COLLECTOR_HOST_DIRECTORY
    copied.mkdir()
    for relative, digest in hashes.items():
      extract_speed._copy_manifest_file_to_snapshot(
        host_dir.joinpath(*PurePosixPath(relative).parts),
        copied.joinpath(*PurePosixPath(relative).parts),
        digest,
        relative,
      )
    copied_files = extract_speed.regular_file_tree(copied, "copied host observation")
    if set(copied_files) != set(hashes):
      raise RuntimeError("copied host observation file set changed")
    copied_attestation = extract_speed.validate_runtime_attestation(
      extract_speed.load_json_object(
        copied / extract_speed.RUNTIME_ATTESTATION_FILENAME,
        "copied host runtime attestation",
      ),
      "copied host runtime attestation",
    )
    if copied_attestation != attestation:
      raise RuntimeError("copied host runtime attestation changed")
    host_speed.extract_host_observation(copied, copied_attestation)
    manifest = {
      "schema": extract_speed.COLLECTOR_SCHEMA,
      "runtime_attestation": attestation,
      "files": hashes,
    }
    (staging / extract_speed.COLLECTOR_MANIFEST_FILENAME).write_text(
      json.dumps(manifest, indent=2, sort_keys=True) + "\n",
      encoding="utf-8",
    )
    staging.rename(bundle_dir)
    published = True
  finally:
    if not published:
      shutil.rmtree(staging, ignore_errors=True)
  return bundle_dir


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("host_dir", type=Path)
  parser.add_argument("bundle_dir", type=Path)
  args = parser.parse_args()
  try:
    bundle = collect_host_bundle(args.host_dir, args.bundle_dir)
  except (OSError, RuntimeError) as error:
    parser.error(str(error))
  print(bundle / extract_speed.COLLECTOR_MANIFEST_FILENAME)


if __name__ == "__main__":
  main()
