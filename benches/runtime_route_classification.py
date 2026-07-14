#!/usr/bin/env python3
"""Classify every declared runtime identity before release collection."""

from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import tomllib

import release_matrix
import speed_evidence


SCHEMA = "tach-runtime-route-classification-v1"
SOURCE_REVISION_RE = re.compile(r"[0-9a-f]{40}(?:[0-9a-f]{24})?")


@dataclass(frozen=True)
class ArtifactRoute:
  artifact_id: str
  target: str
  build_mode: str
  runtime_profile: str
  evidence_kind: release_matrix.EvidenceKind

  @property
  def key(self) -> tuple[str, str, str]:
    return self.target, self.build_mode, self.runtime_profile


def runtime_profile(target: str, harness: str) -> str:
  if harness == "criterion":
    if target.endswith("-apple-darwin"):
      return "macos_criterion"
    if target.endswith("-pc-windows-msvc"):
      return "windows_criterion"
    if target.endswith("-unknown-freebsd"):
      return "freebsd_criterion"
    if "linux" in target or "android" in target:
      return "native_criterion"
  profiles = {
    "lambda": {
      "x86_64-unknown-linux-gnu": "aws_lambda_x86_64",
      "aarch64-unknown-linux-gnu": "aws_lambda_aarch64",
    }.get(target),
    "node-wasm-bindgen": "node_wasm_bindgen",
    "browser": "browser_wasm_bindgen",
    "emcc-node": "emcc_node",
    "node-uvwasi": "node_uvwasi",
    "wasmtime": "wasmtime",
    "wasmtime-component": "wasmtime_component",
    "wasi-threads-smoke": "wasi_threads_smoke",
    "wasm32v1-none-smoke": "wasm32v1_none_smoke",
  }
  profile = profiles.get(harness)
  if not isinstance(profile, str):
    raise ValueError(f"no runtime profile for target={target!r}, harness={harness!r}")
  return profile


def artifact_routes() -> tuple[ArtifactRoute, ...]:
  routes = []
  for artifact_id, values in speed_evidence.PRIMARY_SPEED_CELLS.items():
    target, harness, build_mode = values[3], values[4], values[5]
    routes.append(ArtifactRoute(
      artifact_id,
      target,
      build_mode or "default",
      runtime_profile(target, harness),
      release_matrix.EvidenceKind.FULL_SPEED,
    ))
  supplemental_kinds = {
    "full_speed_cell": release_matrix.EvidenceKind.FULL_SPEED,
    "runtime_smoke": release_matrix.EvidenceKind.RUNTIME_SMOKE,
    "tagged_wall_fallback": release_matrix.EvidenceKind.TAGGED_WALL_FALLBACK,
  }
  for artifact_id, values in speed_evidence.SUPPLEMENTAL_SPEED_CELLS.items():
    target, harness, mode, build_mode = values
    try:
      evidence_kind = supplemental_kinds[mode]
    except KeyError as error:
      raise ValueError(
        f"supplemental artifact {artifact_id!r} has unknown evidence mode {mode!r}"
      ) from error
    routes.append(ArtifactRoute(
      artifact_id,
      target,
      build_mode,
      runtime_profile(target, harness),
      evidence_kind,
    ))
  keys = [route.key for route in routes]
  if len(keys) != len(set(keys)):
    duplicates = sorted(key for key, count in Counter(keys).items() if count > 1)
    raise ValueError(f"artifact catalog duplicates route identities: {duplicates!r}")
  return tuple(sorted(routes, key=lambda route: route.artifact_id))


def classify(
  source_revision: str,
  coverage_path: Path = release_matrix.DEFAULT_ROUTE_COVERAGE_PATH,
) -> dict[str, object]:
  if not SOURCE_REVISION_RE.fullmatch(source_revision):
    raise ValueError("source revision must be a 40- or 64-character lowercase SHA")
  coverage_bytes = coverage_path.read_bytes()
  coverage = tomllib.loads(coverage_bytes.decode("utf-8"))
  matrix = release_matrix.parse_route_coverage(coverage)
  producers = {
    producer["id"]: producer
    for producer in coverage.get("producer", [])
    if isinstance(producer, dict) and isinstance(producer.get("id"), str)
  }
  artifacts = {route.key: route for route in artifact_routes()}
  requirement_keys = {
    (
      requirement.identity.target,
      requirement.identity.build_mode,
      requirement.identity.runtime_profile,
    )
    for requirement in matrix.requirements
  }
  unmatched = sorted(
    route.artifact_id for key, route in artifacts.items() if key not in requirement_keys
  )
  if unmatched:
    raise ValueError(f"artifact catalog contains undeclared route identities: {unmatched!r}")

  routes = []
  states = Counter()
  kinds = Counter()
  for requirement in matrix.requirements:
    identity = requirement.identity
    producer = producers.get(requirement.producer)
    if not isinstance(producer, dict):
      raise ValueError(f"route requirement references missing producer {requirement.producer!r}")
    producer_state = producer.get("state")
    entrypoints = producer.get("entrypoints")
    if producer_state != "ready" or not isinstance(entrypoints, list) or not entrypoints:
      state = "open_producer_gap"
      gap_reason = "producer is not ready with concrete entrypoints"
    else:
      artifact = artifacts.get((identity.target, identity.build_mode, identity.runtime_profile))
      if artifact is None:
        state = "open_artifact_binding_gap"
        gap_reason = "no exact retained artifact identity is declared"
      elif artifact.evidence_kind is not requirement.required_kind:
        raise ValueError(
          f"artifact {artifact.artifact_id!r} is {artifact.evidence_kind.value!r}, "
          f"but {identity.display()} requires {requirement.required_kind.value!r}"
        )
      else:
        state = "producer_ready_artifact_declared"
        gap_reason = None
    artifact = artifacts.get((identity.target, identity.build_mode, identity.runtime_profile))
    states[state] += 1
    kinds[requirement.required_kind.value] += 1
    routes.append({
      "identity": identity.to_mapping(),
      "required_evidence_kind": requirement.required_kind.value,
      "producer": {
        "id": requirement.producer,
        "kind": producer.get("kind"),
        "state": producer_state,
        "entrypoints": entrypoints,
      },
      "route_contract": requirement.route_contract,
      "artifact_id": artifact.artifact_id if artifact is not None else None,
      "classification": state,
      "gap_reason": gap_reason,
    })

  return {
    "schema": SCHEMA,
    "source_revision": source_revision,
    "route_manifest_sha256": hashlib.sha256(coverage_bytes).hexdigest(),
    "counts": {
      "runtime_identities": len(routes),
      "by_evidence_kind": dict(sorted(kinds.items())),
      "by_classification": dict(sorted(states.items())),
    },
    "routes": routes,
  }


def write_exclusive(path: Path, document: dict[str, object]) -> None:
  """Write one immutable classification without replacing prior evidence."""
  with path.open("x", encoding="utf-8") as destination:
    json.dump(document, destination, indent=2)
    destination.write("\n")


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--source-revision", required=True)
  parser.add_argument("--output", type=Path)
  arguments = parser.parse_args()
  document = classify(arguments.source_revision)
  rendered = json.dumps(document, indent=2) + "\n"
  if arguments.output is None:
    print(rendered, end="")
  else:
    write_exclusive(arguments.output, document)


if __name__ == "__main__":
  main()
