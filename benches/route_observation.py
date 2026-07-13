"""Canonical retained route-observation bindings."""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json

import release_matrix


CLOSURE_SCHEMA = "tach-route-observation-closure-v1"
MANIFEST_SCHEMA = "tach-release-route-observations-v1"


@dataclass(frozen=True)
class ArtifactBindingInput:
  """Validated bytes and the committed requirement they directly observe."""

  artifact_id: str
  document_sha256: str
  requirement: release_matrix.CoverageRequirement


def closure_digest(
  artifact_id: str,
  document_sha256: str,
  source_revision: str,
  requirement: release_matrix.CoverageRequirement,
) -> str:
  """Hash every fact that defines one direct, producer-bound observation."""
  payload = {
    "schema": CLOSURE_SCHEMA,
    "artifact_id": artifact_id,
    "document_sha256": document_sha256,
    "source_revision": source_revision,
    "identity": requirement.identity.to_mapping(),
    "evidence_kind": requirement.required_kind.value,
    "producer": requirement.producer,
    "route_contract": requirement.route_contract,
  }
  canonical = json.dumps(
    payload,
    ensure_ascii=True,
    separators=(",", ":"),
    sort_keys=True,
  ).encode("utf-8")
  return hashlib.sha256(canonical).hexdigest()


def compose_manifest(
  source_revision: str,
  artifacts: list[ArtifactBindingInput],
) -> dict[str, object]:
  """Compose a deterministic manifest without accepting caller-shaped identities."""
  if not artifacts:
    raise ValueError("route-observation manifest requires at least one artifact")
  if len({artifact.artifact_id for artifact in artifacts}) != len(artifacts):
    raise ValueError("route-observation manifest contains duplicate artifacts")
  bindings = []
  for artifact in sorted(artifacts, key=lambda item: item.artifact_id):
    requirement = artifact.requirement
    observation = release_matrix.ObservedCoverage(
      artifact.artifact_id,
      requirement.identity,
      requirement.required_kind,
      release_matrix.FrozenExecution(
        source_revision,
        requirement.identity.target,
        requirement.identity.runtime_profile,
        closure_digest(
          artifact.artifact_id,
          artifact.document_sha256,
          source_revision,
          requirement,
        ),
      ),
    )
    bindings.append({
      "artifact_id": artifact.artifact_id,
      "document_sha256": artifact.document_sha256,
      "route_observation": observation.to_mapping(),
    })
  return {
    "schema": MANIFEST_SCHEMA,
    "source_revision": source_revision,
    "bindings": bindings,
    "equivalences": [],
  }
