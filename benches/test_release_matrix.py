"""Focused tests for release route-matrix admission."""

from __future__ import annotations

from pathlib import Path
import sys
import tomllib
import unittest


sys.path.insert(0, str(Path(__file__).resolve().parent))

import release_matrix


SOURCE_REVISION = "a" * 40
CLOSURE_A = "b" * 64
CLOSURE_B = "c" * 64


def identity(
  build_mode: str = "default",
  *,
  case_id: str = "native",
  target: str = "x86_64-unknown-linux-gnu",
  runtime_profile: str = "native_criterion",
) -> release_matrix.RouteIdentity:
  return release_matrix.RouteIdentity(case_id, target, build_mode, runtime_profile)


def frozen(
  route: release_matrix.RouteIdentity,
  closure: str = CLOSURE_A,
) -> release_matrix.FrozenExecution:
  return release_matrix.FrozenExecution(
    SOURCE_REVISION,
    route.target,
    route.runtime_profile,
    closure,
  )


def requirement(
  route: release_matrix.RouteIdentity,
  kind: release_matrix.EvidenceKind = release_matrix.EvidenceKind.FULL_SPEED,
) -> release_matrix.CoverageRequirement:
  return release_matrix.CoverageRequirement(route, kind, "test-producer", "three_timer_direct")


def matrix(*requirements: release_matrix.CoverageRequirement) -> release_matrix.RouteMatrix:
  return release_matrix.RouteMatrix(tuple(requirements))


def observed(
  artifact_id: str,
  route: release_matrix.RouteIdentity,
  kind: release_matrix.EvidenceKind = release_matrix.EvidenceKind.FULL_SPEED,
  closure: str = CLOSURE_A,
) -> release_matrix.ObservedCoverage:
  return release_matrix.ObservedCoverage(artifact_id, route, kind, frozen(route, closure))


def equivalence(
  artifact_id: str,
  measured: release_matrix.RouteIdentity,
  equivalent: release_matrix.RouteIdentity,
  *,
  measured_closure: str = CLOSURE_A,
  equivalent_closure: str = CLOSURE_A,
) -> release_matrix.ModeEquivalence:
  return release_matrix.ModeEquivalence(
    artifact_id,
    measured,
    equivalent,
    frozen(measured, measured_closure),
    frozen(equivalent, equivalent_closure),
  )


class RouteMatrixTests(unittest.TestCase):
  def assert_failure(
    self,
    report: release_matrix.AdmissionReport,
    code: release_matrix.FailureCode,
  ) -> None:
    self.assertIn(code, report.failure_codes, report.rendered_failures())

  def test_checked_in_manifest_expands_each_case_target_mode_profile(self) -> None:
    manifest_path = Path(__file__).with_name("route-coverage.toml")
    with manifest_path.open("rb") as handle:
      manifest = tomllib.load(handle)
    route_matrix = release_matrix.load_route_matrix(manifest_path)

    expected = {
      (case["id"], target, build_mode, case["runtime_profile"])
      for case in manifest["case"]
      for target in case["targets"]
      for build_mode in case["modes"]
    }
    actual = {
      (
        requirement.identity.case_id,
        requirement.identity.target,
        requirement.identity.build_mode,
        requirement.identity.runtime_profile,
      )
      for requirement in route_matrix.requirements
    }

    self.assertEqual(actual, expected)
    self.assertTrue(any(item[2] == "no-default" for item in actual))
    self.assertTrue(any(item[2] == "emscripten-pthreads" for item in actual))
    artifact_keys = {(target, build_mode, runtime_profile) for _, target, build_mode, runtime_profile in actual}
    self.assertEqual(len(artifact_keys), len(actual))
    kinds_by_case = {
      case_id: {requirement.required_kind for requirement in route_matrix.requirements
                if requirement.identity.case_id == case_id}
      for case_id, *_ in actual
    }
    for case_id in (
      "wasm_browser_fallback",
      "wasi_p1_wasmtime_fallback",
      "wasi_p2_fallback_only",
    ):
      self.assertEqual(
        kinds_by_case[case_id],
        {release_matrix.EvidenceKind.TAGGED_WALL_FALLBACK},
      )

  def test_manifest_rejects_cases_that_share_one_artifact_binding_key(self) -> None:
    manifest = {
      "schema": release_matrix.ROUTE_COVERAGE_SCHEMA,
      "case": [
        {
          "id": "first-case",
          "targets": ["x86_64-unknown-linux-gnu"],
          "modes": ["default"],
          "runtime_profile": "native_criterion",
          "producer": "test-producer",
          "route_contract": "three_timer_direct",
          "runtime_proof": "not_collected",
        },
        {
          "id": "second-case",
          "targets": ["x86_64-unknown-linux-gnu"],
          "modes": ["default"],
          "runtime_profile": "native_criterion",
          "producer": "test-producer",
          "route_contract": "three_timer_direct",
          "runtime_proof": "not_collected",
        },
      ],
    }

    with self.assertRaisesRegex(
      release_matrix.RouteMatrixError,
      r"ambiguous artifact binding.*case ids 'first-case' and 'second-case'",
    ):
      release_matrix.parse_route_coverage(manifest)

  def test_direct_full_speed_artifact_covers_only_its_exact_identity(self) -> None:
    route = identity()
    report = release_matrix.validate_route_matrix(
      matrix(requirement(route)), [observed("artifact-default", route)]
    )

    self.assertTrue(report.passed, report.rendered_failures())
    self.assertEqual(len(report.decisions), 1)
    self.assertFalse(report.decisions[0].via_equivalence)

  def test_missing_no_default_and_pthreads_remain_separate_requirements(self) -> None:
    default = identity("default", case_id="emscripten")
    no_default = identity("no-default", case_id="emscripten")
    pthreads = identity("emscripten-pthreads", case_id="emscripten")
    report = release_matrix.validate_route_matrix(
      matrix(*(requirement(route) for route in (default, no_default, pthreads))),
      [observed("artifact-default", default)],
    )

    missing = {
      failure.identity.build_mode
      for failure in report.failures
      if failure.code is release_matrix.FailureCode.MISSING_COVERAGE
      and failure.identity is not None
    }
    self.assertEqual(missing, {"no-default", "emscripten-pthreads"})

  def test_one_artifact_cannot_directly_measure_two_modes(self) -> None:
    default = identity("default")
    no_default = identity("no-default")
    report = release_matrix.validate_route_matrix(
      matrix(requirement(default), requirement(no_default)),
      [
        observed("one-artifact", default),
        observed("one-artifact", no_default),
      ],
    )

    self.assert_failure(
      report, release_matrix.FailureCode.ONE_ARTIFACT_MULTIPLE_MEASURED_MODES
    )
    self.assert_failure(report, release_matrix.FailureCode.MISSING_COVERAGE)

  def test_duplicate_direct_measurements_are_not_admitted(self) -> None:
    route = identity()
    report = release_matrix.validate_route_matrix(
      matrix(requirement(route)),
      [observed("artifact-a", route), observed("artifact-b", route)],
    )

    self.assert_failure(report, release_matrix.FailureCode.DUPLICATE_DIRECT_MEASUREMENT)
    self.assert_failure(report, release_matrix.FailureCode.DUPLICATE_COVERAGE)

  def test_wrong_host_profile_is_not_relabelled_as_coverage(self) -> None:
    expected = identity(runtime_profile="node_wasm_bindgen")
    wrong_host = identity(runtime_profile="browser_wasm_bindgen")
    report = release_matrix.validate_route_matrix(
      matrix(requirement(expected)), [observed("browser-artifact", wrong_host)]
    )

    self.assert_failure(report, release_matrix.FailureCode.WRONG_HOST_PROFILE)
    self.assert_failure(report, release_matrix.FailureCode.MISSING_COVERAGE)

  def test_smoke_and_fallback_can_never_satisfy_full_speed(self) -> None:
    route = identity()
    for non_latency_kind in (
      release_matrix.EvidenceKind.RUNTIME_SMOKE,
      release_matrix.EvidenceKind.TAGGED_WALL_FALLBACK,
    ):
      with self.subTest(kind=non_latency_kind):
        report = release_matrix.validate_route_matrix(
          matrix(requirement(route)), [observed("non-latency", route, non_latency_kind)]
        )
        self.assert_failure(report, release_matrix.FailureCode.NON_LATENCY_FOR_FULL_SPEED)
        self.assert_failure(report, release_matrix.FailureCode.MISSING_COVERAGE)

  def test_matching_freeze_cannot_admit_mode_equivalence_yet(self) -> None:
    default = identity("default")
    no_default = identity("no-default")
    report = release_matrix.validate_route_matrix(
      matrix(requirement(default), requirement(no_default)),
      [observed("artifact-default", default)],
      [equivalence("artifact-default", default, no_default)],
    )

    self.assert_failure(report, release_matrix.FailureCode.EQUIVALENCE_PROHIBITED)
    self.assert_failure(report, release_matrix.FailureCode.MISSING_COVERAGE)

  def test_closure_drift_makes_equivalence_false(self) -> None:
    default = identity("default")
    no_default = identity("no-default")
    report = release_matrix.validate_route_matrix(
      matrix(requirement(default), requirement(no_default)),
      [observed("artifact-default", default, closure=CLOSURE_A)],
      [
        equivalence(
          "artifact-default",
          default,
          no_default,
          measured_closure=CLOSURE_A,
          equivalent_closure=CLOSURE_B,
        )
      ],
    )

    self.assert_failure(report, release_matrix.FailureCode.EQUIVALENCE_PROHIBITED)
    self.assert_failure(report, release_matrix.FailureCode.MISSING_COVERAGE)

  def test_source_revision_drift_makes_equivalence_false(self) -> None:
    default = identity("default")
    no_default = identity("no-default")
    observed_default = observed("artifact-default", default)
    different_revision = release_matrix.FrozenExecution(
      "d" * 40,
      no_default.target,
      no_default.runtime_profile,
      CLOSURE_A,
    )
    report = release_matrix.validate_route_matrix(
      matrix(requirement(default), requirement(no_default)),
      [observed_default],
      [
        release_matrix.ModeEquivalence(
          "artifact-default",
          default,
          no_default,
          observed_default.frozen,
          different_revision,
        )
      ],
    )

    self.assert_failure(report, release_matrix.FailureCode.EQUIVALENCE_PROHIBITED)
    self.assert_failure(report, release_matrix.FailureCode.MISSING_COVERAGE)

  def test_equivalence_cannot_cross_case_target_or_host_profile(self) -> None:
    measured = identity("default", case_id="native")
    different_case = identity("no-default", case_id="lambda")
    report = release_matrix.validate_route_matrix(
      matrix(requirement(measured), requirement(different_case)),
      [observed("native-artifact", measured)],
      [equivalence("native-artifact", measured, different_case)],
    )

    self.assert_failure(report, release_matrix.FailureCode.EQUIVALENCE_PROHIBITED)
    self.assert_failure(report, release_matrix.FailureCode.MISSING_COVERAGE)

  def test_typed_non_latency_requirement_is_admitted_only_by_its_own_kind(self) -> None:
    route = identity(target="wasm32v1-none", runtime_profile="wasm32v1_none_smoke")
    smoke_requirement = requirement(route, release_matrix.EvidenceKind.RUNTIME_SMOKE)
    smoke_report = release_matrix.validate_route_matrix(
      matrix(smoke_requirement),
      [observed("smoke-artifact", route, release_matrix.EvidenceKind.RUNTIME_SMOKE)],
    )
    full_speed_report = release_matrix.validate_route_matrix(
      matrix(smoke_requirement), [observed("speed-artifact", route)]
    )

    self.assertTrue(smoke_report.passed, smoke_report.rendered_failures())
    self.assert_failure(full_speed_report, release_matrix.FailureCode.WRONG_EVIDENCE_KIND)

  def test_serialized_records_have_exact_schema(self) -> None:
    route = identity()
    observation = observed("artifact", route)
    parsed_observation = release_matrix.ObservedCoverage.from_mapping(observation.to_mapping())
    self.assertEqual(parsed_observation, observation)

    encoded_equivalence = equivalence("artifact", route, identity("no-default")).to_mapping()
    parsed_equivalence = release_matrix.ModeEquivalence.from_mapping(encoded_equivalence)
    self.assertEqual(parsed_equivalence.to_mapping(), encoded_equivalence)

    encoded_observation = observation.to_mapping()
    encoded_observation["unbound"] = True
    with self.assertRaisesRegex(release_matrix.RouteMatrixError, "keys differ"):
      release_matrix.ObservedCoverage.from_mapping(encoded_observation)


if __name__ == "__main__":
  unittest.main()
