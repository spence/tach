#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock


BENCHES_DIR = Path(__file__).resolve().parent
if str(BENCHES_DIR) not in sys.path:
  sys.path.insert(0, str(BENCHES_DIR))
FULL_SPEED_ARTIFACT = "speed-supplemental-macos-x86_64.json"
RUNTIME_SMOKE_ARTIFACT = "speed-supplemental-wasip1-threads-smoke.json"
TAGGED_FALLBACK_ARTIFACT = "speed-supplemental-wasi-p1-wasmtime.json"


def load_script(filename: str, module_name: str):
  spec = importlib.util.spec_from_file_location(module_name, BENCHES_DIR / filename)
  if spec is None or spec.loader is None:
    raise RuntimeError(f"could not load {filename}")
  module = importlib.util.module_from_spec(spec)
  sys.modules[module_name] = module
  spec.loader.exec_module(module)
  return module


RELEASE_VALIDATOR = load_script("validate-release-evidence.py", "tach_release_evidence_validator")
SUPPLEMENTAL_VALIDATOR = load_script(
  "validate-supplemental-thread-cpu.py", "tach_supplemental_evidence_validator"
)


def write_cell(directory: Path, artifact: str, document: dict) -> Path:
  path = directory / artifact
  path.write_text(json.dumps(document), encoding="utf-8")
  return path


def full_speed_document(bundle_path: str = "collector.bundle") -> dict:
  return {"collector_bundle": {"path": bundle_path}}


def windows_kernel(handle: int = 123, attributes: int = 0):
  def provide_information(_handle, information):
    information._obj.dwFileAttributes = attributes
    return 1

  return SimpleNamespace(
    CreateFileW=mock.Mock(return_value=handle),
    GetFileInformationByHandle=mock.Mock(side_effect=provide_information),
    CloseHandle=mock.Mock(return_value=1),
  )


class SnapshotReaderTests(unittest.TestCase):
  @unittest.skipUnless(
    hasattr(RELEASE_VALIDATOR.os, "O_NOFOLLOW"),
    "POSIX O_NOFOLLOW is unavailable",
  )
  def test_posix_snapshot_keeps_no_follow_descriptor_path(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      path = Path(directory) / "evidence.json"
      path.write_bytes(b'{"schema":"example"}')
      real_open = RELEASE_VALIDATOR.os.open
      with (
        mock.patch.object(RELEASE_VALIDATOR.os, "open", wraps=real_open) as opened,
        mock.patch.object(RELEASE_VALIDATOR, "_open_windows_no_reparse") as windows,
      ):
        raw = RELEASE_VALIDATOR._read_regular_file_bytes_once(path, "evidence")

      self.assertEqual(raw, b'{"schema":"example"}')
      opened.assert_called_once_with(
        path,
        RELEASE_VALIDATOR.os.O_RDONLY | RELEASE_VALIDATOR.os.O_NOFOLLOW,
      )
      windows.assert_not_called()

  def test_windows_reparse_point_is_rejected_before_transfer(self) -> None:
    path = Path("evidence.json")
    kernel = windows_kernel(attributes=0x00000400)
    transfer = mock.Mock(return_value=71)

    with self.assertRaisesRegex(ValueError, "reject a reparse point"):
      RELEASE_VALIDATOR._open_windows_no_reparse(
        path,
        kernel32=kernel,
        open_osfhandle=transfer,
        get_last_error=lambda: 0,
      )

    transfer.assert_not_called()
    kernel.CloseHandle.assert_called_once_with(123)

  def test_windows_invalid_create_error_is_wrapped_by_snapshot_reader(self) -> None:
    path = Path("evidence.json")
    invalid_handle = RELEASE_VALIDATOR.ctypes.c_void_p(-1).value
    kernel = windows_kernel(handle=invalid_handle)
    real_windows_open = RELEASE_VALIDATOR._open_windows_no_reparse

    def open_windows(candidate):
      return real_windows_open(
        candidate,
        kernel32=kernel,
        open_osfhandle=mock.Mock(),
        get_last_error=lambda: 5,
      )

    with (
      mock.patch.object(RELEASE_VALIDATOR.os, "name", "nt"),
      mock.patch.object(
        RELEASE_VALIDATOR,
        "_open_windows_no_reparse",
        side_effect=open_windows,
      ),
    ):
      with self.assertRaisesRegex(ValueError, "CreateFileW failed.*error 5"):
        RELEASE_VALIDATOR._read_regular_file_bytes_once(path, "evidence")

    kernel.GetFileInformationByHandle.assert_not_called()
    kernel.CloseHandle.assert_not_called()

  def test_windows_conversion_failure_closes_handle_once(self) -> None:
    kernel = windows_kernel()
    transfer = mock.Mock(side_effect=OSError("conversion failed"))

    with self.assertRaisesRegex(OSError, "conversion failed"):
      RELEASE_VALIDATOR._open_windows_no_reparse(
        Path("evidence.json"),
        kernel32=kernel,
        open_osfhandle=transfer,
        get_last_error=lambda: 0,
      )

    transfer.assert_called_once()
    kernel.CloseHandle.assert_called_once_with(123)

  def test_windows_transfer_gives_descriptor_single_close_ownership(self) -> None:
    class DescriptorSource:
      def __init__(self):
        self.close_count = 0

      def __enter__(self):
        return self

      def __exit__(self, _kind, _error, _traceback):
        self.close_count += 1

      def read(self):
        return b'{"schema":"example"}'

    path = Path("evidence.json")
    kernel = windows_kernel()
    transfer = mock.Mock(return_value=71)
    source = DescriptorSource()
    real_windows_open = RELEASE_VALIDATOR._open_windows_no_reparse

    def open_windows(candidate):
      return real_windows_open(
        candidate,
        kernel32=kernel,
        open_osfhandle=transfer,
        get_last_error=lambda: 0,
      )

    with (
      mock.patch.object(RELEASE_VALIDATOR.os, "name", "nt"),
      mock.patch.object(
        RELEASE_VALIDATOR,
        "_open_windows_no_reparse",
        side_effect=open_windows,
      ),
      mock.patch.object(
        RELEASE_VALIDATOR.os,
        "fstat",
        return_value=SimpleNamespace(st_mode=RELEASE_VALIDATOR.stat.S_IFREG),
      ),
      mock.patch.object(RELEASE_VALIDATOR.os, "fdopen", return_value=source) as fdopen,
      mock.patch.object(RELEASE_VALIDATOR.os, "close") as close,
    ):
      raw = RELEASE_VALIDATOR._read_regular_file_bytes_once(path, "evidence")

    self.assertEqual(raw, b'{"schema":"example"}')
    kernel.CloseHandle.assert_not_called()
    fdopen.assert_called_once_with(71, "rb", closefd=True)
    self.assertEqual(source.close_count, 1)
    close.assert_not_called()

  @unittest.skipUnless(
    RELEASE_VALIDATOR.os.name == "nt",
    "native Windows snapshot check requires Windows",
  )
  def test_windows_native_regular_file_snapshot(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      path = Path(directory) / "evidence.json"
      path.write_bytes(b'{"schema":"example"}')

      raw = RELEASE_VALIDATOR._read_regular_file_bytes_once(path, "evidence")

      self.assertEqual(raw, b'{"schema":"example"}')


class RetainedBundlePathTests(unittest.TestCase):
  def test_full_cells_map_to_a_regular_sibling_bundle(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      cell = write_cell(root, FULL_SPEED_ARTIFACT, full_speed_document())
      bundle = root / "collector.bundle"
      bundle.mkdir()

      resolved, error = RELEASE_VALIDATOR.retained_collector_bundle_path(
        root, full_speed_document()
      )
      self.assertIsNone(error)
      self.assertEqual(resolved, bundle.resolve())

      resolved, error = SUPPLEMENTAL_VALIDATOR.retained_collector_bundle_path(
        cell, full_speed_document()
      )
      self.assertIsNone(error)
      self.assertEqual(resolved, bundle.resolve())

  def test_full_cells_reject_bundle_paths_outside_the_cell_directory(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      cell = write_cell(root, FULL_SPEED_ARTIFACT, full_speed_document("../outside"))

      resolved, error = RELEASE_VALIDATOR.retained_collector_bundle_path(
        root, full_speed_document("../outside")
      )
      self.assertIsNone(resolved)
      self.assertEqual(error, "collector bundle descriptor has no safe relative path")

      resolved, error = SUPPLEMENTAL_VALIDATOR.retained_collector_bundle_path(
        cell, full_speed_document("../outside")
      )
      self.assertIsNone(resolved)
      self.assertEqual(error, "collector bundle descriptor has no safe relative path")


class SupplementalValidatorTests(unittest.TestCase):
  def test_campaign_passes_safe_cell_paths_to_bound_validator(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      cell = write_cell(root, FULL_SPEED_ARTIFACT, full_speed_document())
      (root / "collector.bundle").mkdir()
      report = {"passed": True, "failures": []}
      with mock.patch.object(
        RELEASE_VALIDATOR.speed_evidence,
        "validate_supplemental_speed_campaign",
        return_value=report,
      ) as validate:
        result = RELEASE_VALIDATOR.validate_supplemental_campaign(root)

      self.assertTrue(result["passed"])
      documents, cell_paths = validate.call_args.args[:2]
      self.assertEqual(documents[FULL_SPEED_ARTIFACT], full_speed_document())
      self.assertEqual(cell_paths[FULL_SPEED_ARTIFACT], cell.resolve())
      self.assertEqual(validate.call_args.kwargs, {"require_bound_observations": True})

  def test_campaign_does_not_map_an_escaping_bundle_path(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      write_cell(root, FULL_SPEED_ARTIFACT, full_speed_document("../outside"))
      report = {"passed": True, "failures": []}
      with mock.patch.object(
        RELEASE_VALIDATOR.speed_evidence,
        "validate_supplemental_speed_campaign",
        return_value=report,
      ) as validate:
        result = RELEASE_VALIDATOR.validate_supplemental_campaign(root)

      self.assertFalse(result["passed"])
      _, cell_paths = validate.call_args.args[:2]
      self.assertNotIn(FULL_SPEED_ARTIFACT, cell_paths)
      self.assertTrue(any("safe relative path" in failure for failure in result["failures"]))

  def test_runtime_smoke_never_uses_a_collector_bundle(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      cell = write_cell(
        Path(directory),
        RUNTIME_SMOKE_ARTIFACT,
        {"collector_bundle": {"path": "generic.bundle"}},
      )
      report = {"artifact": RUNTIME_SMOKE_ARTIFACT, "passed": True, "failures": []}
      with mock.patch.object(
        SUPPLEMENTAL_VALIDATOR.speed_evidence,
        "validate_supplemental_speed_cell",
        return_value=report,
      ) as direct, mock.patch.object(
        SUPPLEMENTAL_VALIDATOR.speed_evidence,
        "validate_supplemental_speed_cell_from_bundle",
      ) as bound:
        result = SUPPLEMENTAL_VALIDATOR.validate_cell_artifact(
          RUNTIME_SMOKE_ARTIFACT, cell
        )

      self.assertTrue(result["passed"])
      direct.assert_called_once_with(RUNTIME_SMOKE_ARTIFACT, {"collector_bundle": {"path": "generic.bundle"}})
      bound.assert_not_called()

  def test_full_cell_uses_the_retained_bundle_observation(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      document = full_speed_document()
      cell = write_cell(root, FULL_SPEED_ARTIFACT, document)
      bundle = root / "collector.bundle"
      bundle.mkdir()
      report = {"artifact": FULL_SPEED_ARTIFACT, "passed": True, "failures": []}
      with mock.patch.object(
        SUPPLEMENTAL_VALIDATOR.speed_evidence,
        "validate_supplemental_speed_cell_from_bundle",
        return_value=report,
      ) as bound:
        result = SUPPLEMENTAL_VALIDATOR.validate_cell_artifact(FULL_SPEED_ARTIFACT, cell)

      self.assertTrue(result["passed"])
      bound.assert_called_once_with(FULL_SPEED_ARTIFACT, document, bundle.resolve())

  def test_tagged_fallback_uses_the_retained_bundle_observation(self) -> None:
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      document = full_speed_document()
      cell = write_cell(root, TAGGED_FALLBACK_ARTIFACT, document)
      bundle = root / "collector.bundle"
      bundle.mkdir()
      report = {"artifact": TAGGED_FALLBACK_ARTIFACT, "passed": True, "failures": []}
      with mock.patch.object(
        SUPPLEMENTAL_VALIDATOR.speed_evidence,
        "validate_supplemental_speed_cell_from_bundle",
        return_value=report,
      ) as bound:
        result = SUPPLEMENTAL_VALIDATOR.validate_cell_artifact(
          TAGGED_FALLBACK_ARTIFACT, cell
        )

      self.assertTrue(result["passed"])
      bound.assert_called_once_with(TAGGED_FALLBACK_ARTIFACT, document, bundle.resolve())


if __name__ == "__main__":
  unittest.main()
