"""Admission rules for release-grade route coverage.

The static route manifest describes every timer route that *must* eventually be
proven.  This module turns that declaration into exact evidence requirements
without letting one convenient artifact silently stand in for a different
build mode or host runtime.

An artifact observes exactly one ``RouteIdentity``.  Mode equivalence is
deliberately unavailable until the project has a canonical producer-bound
closure digest; a direct observation is therefore the only current admission
path.  Smoke and tagged-fallback records are typed non-latency evidence, so
they can never satisfy a full-speed requirement.
"""

from __future__ import annotations

from collections import defaultdict
from collections.abc import Iterable, Mapping
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
import re
import tomllib


ROUTE_COVERAGE_SCHEMA = "tach-route-coverage-v1"
OBSERVED_COVERAGE_SCHEMA = "tach-route-observation-v1"
MODE_EQUIVALENCE_SCHEMA = "tach-route-mode-equivalence-v1"
DEFAULT_ROUTE_COVERAGE_PATH = Path(__file__).with_name("route-coverage.toml")

_SOURCE_REVISION_RE = re.compile(r"[0-9a-f]{40}(?:[0-9a-f]{24})?")
_SHA256_RE = re.compile(r"[0-9a-f]{64}")


class RouteMatrixError(ValueError):
  """A route manifest or serialized observation violates this protocol."""


class EvidenceKind(str, Enum):
  """The kind of claim an artifact is allowed to make."""

  FULL_SPEED = "full_speed"
  RUNTIME_SMOKE = "runtime_smoke"
  TAGGED_WALL_FALLBACK = "tagged_wall_fallback"

  @property
  def is_latency_evidence(self) -> bool:
    return self is EvidenceKind.FULL_SPEED


class FailureCode(str, Enum):
  """Machine-readable reasons a release matrix is not admissible."""

  UNKNOWN_IDENTITY = "unknown_identity"
  WRONG_HOST_PROFILE = "wrong_host_profile"
  FROZEN_IDENTITY_MISMATCH = "frozen_identity_mismatch"
  ONE_ARTIFACT_MULTIPLE_MEASURED_MODES = "one_artifact_multiple_measured_modes"
  DUPLICATE_DIRECT_MEASUREMENT = "duplicate_direct_measurement"
  DUPLICATE_COVERAGE = "duplicate_coverage"
  MISSING_COVERAGE = "missing_coverage"
  WRONG_EVIDENCE_KIND = "wrong_evidence_kind"
  NON_LATENCY_FOR_FULL_SPEED = "non_latency_for_full_speed"
  FALSE_EQUIVALENCE = "false_equivalence"
  EQUIVALENCE_PROHIBITED = "equivalence_prohibited"
  MALFORMED_RECORD = "malformed_record"


def _require_string(value: object, label: str) -> str:
  if not isinstance(value, str) or not value:
    raise RouteMatrixError(f"{label} must be a non-empty string")
  return value


def _require_exact_keys(mapping: Mapping[str, object], expected: set[str], label: str) -> None:
  actual = set(mapping)
  if actual != expected:
    raise RouteMatrixError(
      f"{label} keys differ: missing={sorted(expected - actual)!r}, "
      f"unexpected={sorted(actual - expected)!r}"
    )


def _as_mapping(value: object, label: str) -> Mapping[str, object]:
  if not isinstance(value, Mapping):
    raise RouteMatrixError(f"{label} must be an object")
  if not all(isinstance(key, str) for key in value):
    raise RouteMatrixError(f"{label} has a non-string key")
  return value


@dataclass(frozen=True, order=True)
class RouteIdentity:
  """One required route: case, target, build mode, and host runtime profile."""

  case_id: str
  target: str
  build_mode: str
  runtime_profile: str

  def __post_init__(self) -> None:
    _require_string(self.case_id, "case_id")
    _require_string(self.target, "target")
    _require_string(self.build_mode, "build_mode")
    _require_string(self.runtime_profile, "runtime_profile")

  @classmethod
  def from_mapping(cls, value: object) -> RouteIdentity:
    mapping = _as_mapping(value, "route identity")
    _require_exact_keys(
      mapping,
      {"case_id", "target", "build_mode", "runtime_profile"},
      "route identity",
    )
    return cls(
      case_id=_require_string(mapping["case_id"], "route identity case_id"),
      target=_require_string(mapping["target"], "route identity target"),
      build_mode=_require_string(mapping["build_mode"], "route identity build_mode"),
      runtime_profile=_require_string(
        mapping["runtime_profile"], "route identity runtime_profile"
      ),
    )

  def to_mapping(self) -> dict[str, str]:
    return {
      "case_id": self.case_id,
      "target": self.target,
      "build_mode": self.build_mode,
      "runtime_profile": self.runtime_profile,
    }

  def display(self) -> str:
    return (
      f"case={self.case_id!r}, target={self.target!r}, "
      f"build_mode={self.build_mode!r}, runtime_profile={self.runtime_profile!r}"
    )


@dataclass(frozen=True)
class FrozenExecution:
  """The immutable execution facts used when sharing a measurement by equivalence."""

  source_revision: str
  target: str
  runtime_profile: str
  closure_digest: str

  def __post_init__(self) -> None:
    _require_string(self.source_revision, "source_revision")
    _require_string(self.target, "frozen target")
    _require_string(self.runtime_profile, "frozen runtime_profile")
    _require_string(self.closure_digest, "closure_digest")
    if not _SOURCE_REVISION_RE.fullmatch(self.source_revision):
      raise RouteMatrixError("source_revision must be a 40- or 64-character lowercase SHA")
    if not _SHA256_RE.fullmatch(self.closure_digest):
      raise RouteMatrixError("closure_digest must be a lowercase SHA-256")

  @classmethod
  def from_mapping(cls, value: object) -> FrozenExecution:
    mapping = _as_mapping(value, "frozen execution")
    _require_exact_keys(
      mapping,
      {"source_revision", "target", "runtime_profile", "closure_digest"},
      "frozen execution",
    )
    return cls(
      source_revision=_require_string(
        mapping["source_revision"], "frozen execution source_revision"
      ),
      target=_require_string(mapping["target"], "frozen execution target"),
      runtime_profile=_require_string(
        mapping["runtime_profile"], "frozen execution runtime_profile"
      ),
      closure_digest=_require_string(
        mapping["closure_digest"], "frozen execution closure_digest"
      ),
    )

  def to_mapping(self) -> dict[str, str]:
    return {
      "source_revision": self.source_revision,
      "target": self.target,
      "runtime_profile": self.runtime_profile,
      "closure_digest": self.closure_digest,
    }


@dataclass(frozen=True)
class CoverageRequirement:
  """One concrete observation that the route manifest requires for release."""

  identity: RouteIdentity
  required_kind: EvidenceKind
  producer: str
  route_contract: str

  def __post_init__(self) -> None:
    if not isinstance(self.identity, RouteIdentity):
      raise RouteMatrixError("coverage requirement identity must be a RouteIdentity")
    if not isinstance(self.required_kind, EvidenceKind):
      raise RouteMatrixError("coverage requirement kind must be an EvidenceKind")
    _require_string(self.producer, "coverage requirement producer")
    _require_string(self.route_contract, "coverage requirement route_contract")


@dataclass(frozen=True)
class RouteMatrix:
  """The fully expanded route-coverage manifest."""

  requirements: tuple[CoverageRequirement, ...]

  def __post_init__(self) -> None:
    if not all(isinstance(requirement, CoverageRequirement) for requirement in self.requirements):
      raise RouteMatrixError("route matrix requirements must be CoverageRequirement records")
    identities = [requirement.identity for requirement in self.requirements]
    if len(identities) != len(set(identities)):
      raise RouteMatrixError("route matrix contains duplicate identities")
    artifact_keys: dict[tuple[str, str, str], str] = {}
    for identity in identities:
      artifact_key = (
        identity.target,
        identity.build_mode,
        identity.runtime_profile,
      )
      prior_case_id = artifact_keys.get(artifact_key)
      if prior_case_id is not None and prior_case_id != identity.case_id:
        case_ids = sorted((prior_case_id, identity.case_id))
        raise RouteMatrixError(
          "route matrix has ambiguous artifact binding for "
          f"target={identity.target!r}, build_mode={identity.build_mode!r}, "
          f"runtime_profile={identity.runtime_profile!r}: case ids "
          f"{case_ids[0]!r} and {case_ids[1]!r} cannot share one artifact"
        )
      artifact_keys[artifact_key] = identity.case_id

  @property
  def by_identity(self) -> dict[RouteIdentity, CoverageRequirement]:
    return {requirement.identity: requirement for requirement in self.requirements}

  @property
  def identities(self) -> tuple[RouteIdentity, ...]:
    return tuple(requirement.identity for requirement in self.requirements)


@dataclass(frozen=True)
class ObservedCoverage:
  """A single artifact's direct measurement of exactly one route identity."""

  artifact_id: str
  identity: RouteIdentity
  evidence_kind: EvidenceKind
  frozen: FrozenExecution

  def __post_init__(self) -> None:
    _require_string(self.artifact_id, "artifact_id")
    if not isinstance(self.identity, RouteIdentity):
      raise RouteMatrixError("observed coverage identity must be a RouteIdentity")
    if not isinstance(self.evidence_kind, EvidenceKind):
      raise RouteMatrixError("observed coverage evidence_kind must be an EvidenceKind")
    if not isinstance(self.frozen, FrozenExecution):
      raise RouteMatrixError("observed coverage frozen execution must be a FrozenExecution")

  @classmethod
  def from_mapping(cls, value: object) -> ObservedCoverage:
    mapping = _as_mapping(value, "observed coverage")
    _require_exact_keys(
      mapping,
      {"schema", "artifact_id", "identity", "evidence_kind", "frozen"},
      "observed coverage",
    )
    if mapping["schema"] != OBSERVED_COVERAGE_SCHEMA:
      raise RouteMatrixError("observed coverage schema is not tach-route-observation-v1")
    try:
      evidence_kind = EvidenceKind(mapping["evidence_kind"])
    except (TypeError, ValueError) as error:
      raise RouteMatrixError("observed coverage has an unknown evidence_kind") from error
    return cls(
      artifact_id=_require_string(mapping["artifact_id"], "observed coverage artifact_id"),
      identity=RouteIdentity.from_mapping(mapping["identity"]),
      evidence_kind=evidence_kind,
      frozen=FrozenExecution.from_mapping(mapping["frozen"]),
    )

  def to_mapping(self) -> dict[str, object]:
    return {
      "schema": OBSERVED_COVERAGE_SCHEMA,
      "artifact_id": self.artifact_id,
      "identity": self.identity.to_mapping(),
      "evidence_kind": self.evidence_kind.value,
      "frozen": self.frozen.to_mapping(),
    }


@dataclass(frozen=True)
class ModeEquivalence:
  """An explicit, digest-bound request to reuse one artifact for another mode."""

  artifact_id: str
  measured_identity: RouteIdentity
  equivalent_identity: RouteIdentity
  measured_frozen: FrozenExecution
  equivalent_frozen: FrozenExecution

  def __post_init__(self) -> None:
    _require_string(self.artifact_id, "mode equivalence artifact_id")
    if not isinstance(self.measured_identity, RouteIdentity):
      raise RouteMatrixError("mode equivalence measured identity must be a RouteIdentity")
    if not isinstance(self.equivalent_identity, RouteIdentity):
      raise RouteMatrixError("mode equivalence target identity must be a RouteIdentity")
    if not isinstance(self.measured_frozen, FrozenExecution):
      raise RouteMatrixError("mode equivalence measured frozen execution is invalid")
    if not isinstance(self.equivalent_frozen, FrozenExecution):
      raise RouteMatrixError("mode equivalence target frozen execution is invalid")

  @classmethod
  def from_mapping(cls, value: object) -> ModeEquivalence:
    mapping = _as_mapping(value, "mode equivalence")
    _require_exact_keys(
      mapping,
      {
        "schema",
        "artifact_id",
        "measured_identity",
        "equivalent_identity",
        "measured_frozen",
        "equivalent_frozen",
      },
      "mode equivalence",
    )
    if mapping["schema"] != MODE_EQUIVALENCE_SCHEMA:
      raise RouteMatrixError("mode equivalence schema is not tach-route-mode-equivalence-v1")
    return cls(
      artifact_id=_require_string(mapping["artifact_id"], "mode equivalence artifact_id"),
      measured_identity=RouteIdentity.from_mapping(mapping["measured_identity"]),
      equivalent_identity=RouteIdentity.from_mapping(mapping["equivalent_identity"]),
      measured_frozen=FrozenExecution.from_mapping(mapping["measured_frozen"]),
      equivalent_frozen=FrozenExecution.from_mapping(mapping["equivalent_frozen"]),
    )

  def to_mapping(self) -> dict[str, object]:
    return {
      "schema": MODE_EQUIVALENCE_SCHEMA,
      "artifact_id": self.artifact_id,
      "measured_identity": self.measured_identity.to_mapping(),
      "equivalent_identity": self.equivalent_identity.to_mapping(),
      "measured_frozen": self.measured_frozen.to_mapping(),
      "equivalent_frozen": self.equivalent_frozen.to_mapping(),
    }


@dataclass(frozen=True)
class AdmissionFailure:
  """A structured reason the route matrix cannot be admitted to a release."""

  code: FailureCode
  detail: str
  identity: RouteIdentity | None = None

  def render(self) -> str:
    if self.identity is None:
      return f"{self.code.value}: {self.detail}"
    return f"{self.code.value}: {self.identity.display()}: {self.detail}"

  def to_mapping(self) -> dict[str, object]:
    return {
      "code": self.code.value,
      "detail": self.detail,
      "identity": self.identity.to_mapping() if self.identity is not None else None,
    }


@dataclass(frozen=True)
class CoverageDecision:
  """The artifact admitted for a requirement, directly or through equivalence."""

  requirement: CoverageRequirement
  artifact_id: str
  via_equivalence: bool

  def to_mapping(self) -> dict[str, object]:
    return {
      "identity": self.requirement.identity.to_mapping(),
      "required_kind": self.requirement.required_kind.value,
      "producer": self.requirement.producer,
      "route_contract": self.requirement.route_contract,
      "artifact_id": self.artifact_id,
      "via_equivalence": self.via_equivalence,
    }


@dataclass(frozen=True)
class AdmissionReport:
  """Complete route-matrix admission result."""

  decisions: tuple[CoverageDecision, ...]
  failures: tuple[AdmissionFailure, ...]

  @property
  def passed(self) -> bool:
    return not self.failures

  @property
  def failure_codes(self) -> tuple[FailureCode, ...]:
    return tuple(failure.code for failure in self.failures)

  def rendered_failures(self) -> list[str]:
    return [failure.render() for failure in self.failures]

  def to_mapping(self) -> dict[str, object]:
    return {
      "schema": "tach-release-route-matrix-report-v1",
      "passed": self.passed,
      "decisions": [decision.to_mapping() for decision in self.decisions],
      "failures": [failure.to_mapping() for failure in self.failures],
    }


def _case_evidence_kind(case: Mapping[str, object], case_id: str) -> EvidenceKind:
  """Map the manifest's declared proof class to a typed evidence requirement."""
  runtime_proof = case.get("runtime_proof")
  implicit = {
    "not_collected": EvidenceKind.FULL_SPEED,
    "runtime_smoke": EvidenceKind.RUNTIME_SMOKE,
    "tagged_wall_fallback": EvidenceKind.TAGGED_WALL_FALLBACK,
  }.get(runtime_proof)
  if implicit is None:
    raise RouteMatrixError(
      f"case {case_id!r} has no release evidence requirement for runtime_proof "
      f"{runtime_proof!r}"
    )
  explicit = case.get("evidence_requirement")
  if explicit is None:
    return implicit
  try:
    declared = EvidenceKind(explicit)
  except (TypeError, ValueError) as error:
    raise RouteMatrixError(
      f"case {case_id!r} has an unknown evidence_requirement {explicit!r}"
    ) from error
  if declared is not implicit:
    raise RouteMatrixError(
      f"case {case_id!r} contradicts runtime_proof with evidence_requirement {explicit!r}"
    )
  return declared


def _string_list(value: object, label: str) -> tuple[str, ...]:
  if not isinstance(value, list) or not value:
    raise RouteMatrixError(f"{label} must be a non-empty list")
  values = tuple(_require_string(item, label) for item in value)
  if len(values) != len(set(values)):
    raise RouteMatrixError(f"{label} contains duplicates")
  return values


def parse_route_coverage(document: object) -> RouteMatrix:
  """Expand a ``tach-route-coverage-v1`` document into exact requirements."""
  manifest = _as_mapping(document, "route coverage manifest")
  if manifest.get("schema") != ROUTE_COVERAGE_SCHEMA:
    raise RouteMatrixError("route coverage schema is not tach-route-coverage-v1")
  cases = manifest.get("case")
  if not isinstance(cases, list) or not cases:
    raise RouteMatrixError("route coverage manifest has no cases")

  requirements: list[CoverageRequirement] = []
  seen_case_ids: set[str] = set()
  seen_identities: set[RouteIdentity] = set()
  for case_index, raw_case in enumerate(cases):
    case = _as_mapping(raw_case, f"route coverage case {case_index}")
    case_id = _require_string(case.get("id"), f"route coverage case {case_index} id")
    if case_id in seen_case_ids:
      raise RouteMatrixError(f"route coverage duplicates case id {case_id!r}")
    seen_case_ids.add(case_id)
    targets = _string_list(case.get("targets"), f"case {case_id!r} targets")
    modes = _string_list(case.get("modes"), f"case {case_id!r} modes")
    runtime_profile = _require_string(
      case.get("runtime_profile"), f"case {case_id!r} runtime_profile"
    )
    producer = _require_string(case.get("producer"), f"case {case_id!r} producer")
    route_contract = _require_string(
      case.get("route_contract"), f"case {case_id!r} route_contract"
    )
    required_kind = _case_evidence_kind(case, case_id)
    for target in targets:
      for build_mode in modes:
        identity = RouteIdentity(case_id, target, build_mode, runtime_profile)
        if identity in seen_identities:
          raise RouteMatrixError(f"route coverage duplicates identity: {identity.display()}")
        seen_identities.add(identity)
        requirements.append(
          CoverageRequirement(identity, required_kind, producer, route_contract)
        )
  return RouteMatrix(tuple(requirements))


def parse_route_coverage_bytes(raw: bytes, source: str = "route coverage manifest") -> RouteMatrix:
  """Parse an exact retained route-coverage byte stream into requirements."""
  if not isinstance(raw, bytes):
    raise RouteMatrixError(f"{source} bytes must be bytes")
  try:
    document = tomllib.loads(raw.decode("utf-8"))
  except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
    raise RouteMatrixError(f"could not parse route coverage manifest {source}: {error}") from error
  return parse_route_coverage(document)


def load_route_matrix(path: Path = DEFAULT_ROUTE_COVERAGE_PATH) -> RouteMatrix:
  """Load and expand a route coverage manifest from a live path.

  Release admission must instead call :func:`parse_route_coverage_bytes` with
  bytes read from the campaign's source revision.
  """
  try:
    raw = Path(path).read_bytes()
  except OSError as error:
    raise RouteMatrixError(f"could not load route coverage manifest {path}: {error}") from error
  return parse_route_coverage_bytes(raw, str(path))


def _frozen_matches_identity(frozen: FrozenExecution, identity: RouteIdentity) -> bool:
  return frozen.target == identity.target and frozen.runtime_profile == identity.runtime_profile


def _wrong_identity_code(
  identity: RouteIdentity,
  requirements: Iterable[CoverageRequirement],
) -> FailureCode:
  for requirement in requirements:
    expected = requirement.identity
    if (
      expected.case_id == identity.case_id
      and expected.target == identity.target
      and expected.build_mode == identity.build_mode
      and expected.runtime_profile != identity.runtime_profile
    ):
      return FailureCode.WRONG_HOST_PROFILE
  return FailureCode.UNKNOWN_IDENTITY


def _kind_failure(
  requirement: CoverageRequirement,
  observed_kind: EvidenceKind,
) -> FailureCode:
  if (
    requirement.required_kind is EvidenceKind.FULL_SPEED
    and not observed_kind.is_latency_evidence
  ):
    return FailureCode.NON_LATENCY_FOR_FULL_SPEED
  return FailureCode.WRONG_EVIDENCE_KIND


def _add_failure(
  failures: list[AdmissionFailure],
  code: FailureCode,
  detail: str,
  identity: RouteIdentity | None = None,
) -> None:
  failures.append(AdmissionFailure(code, detail, identity))


def validate_route_matrix(
  matrix: RouteMatrix,
  observations: Iterable[ObservedCoverage],
  equivalences: Iterable[ModeEquivalence] = (),
) -> AdmissionReport:
  """Admit observed artifacts against an expanded route matrix.

  The validator is intentionally exact.  A direct artifact covers only the
  identity written on its observation.  Mode equivalence is fail-closed until
  canonical producer-bound closure digests exist.
  """
  if not isinstance(matrix, RouteMatrix):
    raise RouteMatrixError("validate_route_matrix requires a RouteMatrix")

  requirements = matrix.by_identity
  failures: list[AdmissionFailure] = []
  candidates: dict[RouteIdentity, list[CoverageDecision]] = defaultdict(list)
  artifacts: dict[str, ObservedCoverage] = {}
  direct_counts: dict[RouteIdentity, int] = defaultdict(int)

  for raw_observation in observations:
    if not isinstance(raw_observation, ObservedCoverage):
      _add_failure(
        failures,
        FailureCode.MALFORMED_RECORD,
        "observations must be ObservedCoverage records",
      )
      continue
    observation = raw_observation
    existing = artifacts.get(observation.artifact_id)
    if existing is not None:
      _add_failure(
        failures,
        FailureCode.ONE_ARTIFACT_MULTIPLE_MEASURED_MODES,
        (
          f"artifact {observation.artifact_id!r} is already bound to "
          f"{existing.identity.display()} and cannot directly measure "
          f"{observation.identity.display()}"
        ),
        observation.identity,
      )
      continue
    artifacts[observation.artifact_id] = observation

    requirement = requirements.get(observation.identity)
    if requirement is None:
      _add_failure(
        failures,
        _wrong_identity_code(observation.identity, requirements.values()),
        f"artifact {observation.artifact_id!r} observes an undeclared route identity",
        observation.identity,
      )
      continue
    if not _frozen_matches_identity(observation.frozen, observation.identity):
      _add_failure(
        failures,
        FailureCode.FROZEN_IDENTITY_MISMATCH,
        (
          f"artifact {observation.artifact_id!r} says target/profile "
          f"{observation.frozen.target!r}/{observation.frozen.runtime_profile!r}, "
          "not its declared identity"
        ),
        observation.identity,
      )
      continue
    direct_counts[observation.identity] += 1
    if direct_counts[observation.identity] > 1:
      _add_failure(
        failures,
        FailureCode.DUPLICATE_DIRECT_MEASUREMENT,
        "more than one artifact directly observes this exact identity",
        observation.identity,
      )
    if observation.evidence_kind is not requirement.required_kind:
      _add_failure(
        failures,
        _kind_failure(requirement, observation.evidence_kind),
        (
          f"artifact {observation.artifact_id!r} is {observation.evidence_kind.value!r}, "
          f"but this route requires {requirement.required_kind.value!r}"
        ),
        observation.identity,
      )
      continue
    candidates[observation.identity].append(
      CoverageDecision(requirement, observation.artifact_id, False)
    )

  for raw_equivalence in equivalences:
    identity = (
      raw_equivalence.equivalent_identity
      if isinstance(raw_equivalence, ModeEquivalence)
      else None
    )
    _add_failure(
      failures,
      FailureCode.EQUIVALENCE_PROHIBITED,
      "mode equivalence is prohibited until a canonical producer-bound closure digest exists",
      identity,
    )

  decisions: list[CoverageDecision] = []
  for requirement in matrix.requirements:
    identity = requirement.identity
    matching = candidates.get(identity, [])
    if not matching:
      _add_failure(
        failures,
        FailureCode.MISSING_COVERAGE,
        f"missing required {requirement.required_kind.value!r} evidence",
        identity,
      )
      continue
    if len(matching) > 1:
      _add_failure(
        failures,
        FailureCode.DUPLICATE_COVERAGE,
        "more than one direct or equivalent artifact claims this identity",
        identity,
      )
      continue
    decisions.append(matching[0])

  return AdmissionReport(tuple(decisions), tuple(failures))
