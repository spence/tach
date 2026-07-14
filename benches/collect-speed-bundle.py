#!/usr/bin/env python3
"""Collect one sealed Criterion tree into a verified speed-input bundle.

Usage: collect-speed-bundle.py <criterion-dir> <fresh-bundle-dir>

The producer writes ``tach-speed-source-seal.json`` only after its benchmark
command exits.  This collector requires that seal, verifies the exact source
tree against it while streaming every sealed file into private staging, and
validates/manifests only those staged bytes.
"""

from __future__ import annotations

import argparse
import base64
import ctypes
import hashlib
import json
import os
import re
import shutil
import stat
import tempfile
from pathlib import Path, PurePosixPath

import extract_speed


SOURCE_SEAL_FILENAME = "tach-speed-source-seal.json"
SOURCE_SEAL_SCHEMA = "tach-speed-source-seal-v1"
_SHA256 = re.compile(r"[0-9a-f]{64}")
_COPY_CHUNK_SIZE = 1024 * 1024


def _open_windows_no_reparse(
    path: Path,
    kernel32=None,
    open_osfhandle=None,
    get_last_error=None,
) -> int:
    """Open one Windows path without traversing a final reparse point."""

    class FileInformation(ctypes.Structure):
        _fields_ = [
            ("dwFileAttributes", ctypes.c_uint32),
            ("ftCreationTimeLow", ctypes.c_uint32),
            ("ftCreationTimeHigh", ctypes.c_uint32),
            ("ftLastAccessTimeLow", ctypes.c_uint32),
            ("ftLastAccessTimeHigh", ctypes.c_uint32),
            ("ftLastWriteTimeLow", ctypes.c_uint32),
            ("ftLastWriteTimeHigh", ctypes.c_uint32),
            ("dwVolumeSerialNumber", ctypes.c_uint32),
            ("nFileSizeHigh", ctypes.c_uint32),
            ("nFileSizeLow", ctypes.c_uint32),
            ("nNumberOfLinks", ctypes.c_uint32),
            ("nFileIndexHigh", ctypes.c_uint32),
            ("nFileIndexLow", ctypes.c_uint32),
        ]

    if kernel32 is None:
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    if open_osfhandle is None:
        import msvcrt

        open_osfhandle = msvcrt.open_osfhandle
    if get_last_error is None:
        get_last_error = ctypes.get_last_error

    handle_type = ctypes.c_void_p
    kernel32.CreateFileW.argtypes = [
        ctypes.c_wchar_p,
        ctypes.c_uint32,
        ctypes.c_uint32,
        ctypes.c_void_p,
        ctypes.c_uint32,
        ctypes.c_uint32,
        handle_type,
    ]
    kernel32.CreateFileW.restype = handle_type
    kernel32.GetFileInformationByHandle.argtypes = [
        handle_type,
        ctypes.POINTER(FileInformation),
    ]
    kernel32.GetFileInformationByHandle.restype = ctypes.c_int
    kernel32.CloseHandle.argtypes = [handle_type]
    kernel32.CloseHandle.restype = ctypes.c_int

    handle = kernel32.CreateFileW(
        str(path),
        0x80000000,
        0x00000007,
        None,
        3,
        0x00000080 | 0x00200000,
        None,
    )
    if handle == ctypes.c_void_p(-1).value:
        error_code = get_last_error()
        raise OSError(error_code, f"CreateFileW failed with Windows error {error_code}", path)

    transferred = False
    try:
        information = FileInformation()
        if not kernel32.GetFileInformationByHandle(handle, ctypes.byref(information)):
            error_code = get_last_error()
            raise OSError(
                error_code,
                f"GetFileInformationByHandle failed with Windows error {error_code}",
                path,
            )
        if information.dwFileAttributes & 0x00000400:
            raise RuntimeError(f"Criterion input rejects a reparse point: {path}")
        flags = os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOINHERIT", 0)
        descriptor = open_osfhandle(handle, flags)
        if descriptor < 0:
            raise OSError("open_osfhandle returned an invalid descriptor")
        transferred = True
        return descriptor
    finally:
        if not transferred:
            kernel32.CloseHandle(handle)


def _require_directory(path: Path, description: str) -> None:
    try:
        mode = path.lstat().st_mode
    except OSError as error:
        raise RuntimeError(f"could not stat {description} {path}: {error}") from error
    if not stat.S_ISDIR(mode):
        raise RuntimeError(f"{description} is not a directory: {path}")


def _relative_destination(root: Path, relative: str) -> Path:
    return root.joinpath(*PurePosixPath(relative).parts)


def _assert_fresh_destination(criterion_dir: Path, bundle_dir: Path) -> None:
    if os.path.lexists(bundle_dir):
        raise RuntimeError(f"collector bundle destination already exists: {bundle_dir}")
    source = criterion_dir.resolve()
    destination = bundle_dir.resolve(strict=False)
    try:
        destination.relative_to(source)
    except ValueError:
        return
    raise RuntimeError("collector bundle destination must not be inside Criterion input")


def _stat_fingerprint(value: os.stat_result) -> tuple[int, ...]:
    return (
        stat.S_IFMT(value.st_mode),
        value.st_dev,
        value.st_ino,
        value.st_nlink,
        value.st_size,
        value.st_mtime_ns,
        value.st_ctime_ns,
    )


def _path_matches_opened_file(initial: os.stat_result, opened: os.stat_result) -> bool:
    if os.name != "nt":
        return _stat_fingerprint(initial) == _stat_fingerprint(opened)
    return (
        stat.S_IFMT(initial.st_mode),
        initial.st_size,
        initial.st_mtime_ns,
    ) == (
        stat.S_IFMT(opened.st_mode),
        opened.st_size,
        opened.st_mtime_ns,
    )


def _open_source_file(path: Path) -> int:
    if os.name == "nt":
        return _open_windows_no_reparse(path)
    no_follow = getattr(os, "O_NOFOLLOW", None)
    if no_follow is None:
        raise RuntimeError("secure evidence collection requires O_NOFOLLOW")
    return os.open(path, os.O_RDONLY | no_follow)


def _read_regular_file(
    path: Path,
    relative: str,
    *,
    retain_bytes: bool,
) -> tuple[bytes | None, str, tuple[int, ...]]:
    """Read a stable regular source file through one non-following descriptor."""

    try:
        initial = path.lstat()
    except OSError as error:
        raise RuntimeError(f"could not stat Criterion input {relative!r}: {error}") from error
    if not stat.S_ISREG(initial.st_mode):
        raise RuntimeError(f"Criterion input {relative!r} is not a regular file")
    try:
        descriptor = _open_source_file(path)
    except OSError as error:
        raise RuntimeError(f"could not open Criterion input {relative!r}: {error}") from error
    try:
        opened = os.fstat(descriptor)
        expected = _stat_fingerprint(opened)
        if not stat.S_ISREG(opened.st_mode) or not _path_matches_opened_file(initial, opened):
            raise RuntimeError(f"Criterion input {relative!r} changed while opening it")
        digest = hashlib.sha256()
        chunks = []
        with os.fdopen(descriptor, "rb", closefd=True) as source:
            descriptor = -1
            for chunk in iter(lambda: source.read(_COPY_CHUNK_SIZE), b""):
                digest.update(chunk)
                if retain_bytes:
                    chunks.append(chunk)
            completed = os.fstat(source.fileno())
        if _stat_fingerprint(completed) != expected:
            raise RuntimeError(
                f"Criterion input {relative!r} changed while it was read"
            )
        return (b"".join(chunks) if retain_bytes else None), digest.hexdigest(), expected
    except OSError as error:
        raise RuntimeError(f"could not read Criterion input {relative!r}: {error}") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def _copy_sealed_file(
    source_path: Path,
    destination: Path,
    relative: str,
    expected_digest: str,
) -> str:
    """Stream one sealed source file into staging and hash those exact bytes."""

    try:
        initial = source_path.lstat()
    except OSError as error:
        raise RuntimeError(f"could not stat Criterion input {relative!r}: {error}") from error
    if not stat.S_ISREG(initial.st_mode):
        raise RuntimeError(f"Criterion input {relative!r} is not a regular file")
    try:
        descriptor = _open_source_file(source_path)
    except OSError as error:
        raise RuntimeError(f"could not open Criterion input {relative!r}: {error}") from error
    try:
        opened = os.fstat(descriptor)
        expected_state = _stat_fingerprint(opened)
        if not stat.S_ISREG(opened.st_mode) or not _path_matches_opened_file(initial, opened):
            raise RuntimeError(f"Criterion input {relative!r} changed while opening it")
        destination.parent.mkdir(parents=True, exist_ok=True)
        digest = hashlib.sha256()
        with os.fdopen(descriptor, "rb", closefd=True) as source:
            descriptor = -1
            try:
                with destination.open("xb") as copied:
                    for chunk in iter(lambda: source.read(_COPY_CHUNK_SIZE), b""):
                        digest.update(chunk)
                        copied.write(chunk)
            except OSError as error:
                raise RuntimeError(
                    f"could not write sealed Criterion input {relative!r}: {error}"
                ) from error
            completed = os.fstat(source.fileno())
        if _stat_fingerprint(completed) != expected_state:
            raise RuntimeError(
                f"Criterion input {relative!r} changed while it was copied"
            )
        actual_digest = digest.hexdigest()
        if actual_digest != expected_digest:
            raise RuntimeError(f"source seal hash mismatch for {relative!r}")
        return actual_digest
    except OSError as error:
        raise RuntimeError(f"could not copy Criterion input {relative!r}: {error}") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def _load_source_seal(path: Path) -> tuple[dict[str, str], bytes, dict, bytes]:
    if not os.path.lexists(path):
        raise RuntimeError("Criterion input is missing tach-speed-source-seal.json")
    raw, _, _ = _read_regular_file(
        path,
        SOURCE_SEAL_FILENAME,
        retain_bytes=True,
    )
    assert raw is not None
    seal = extract_speed._json_object_from_bytes(raw, "source seal")
    if set(seal) != {"schema", "runtime_attestation", "files"}:
        raise RuntimeError("malformed source seal: unexpected shape")
    if seal.get("schema") != SOURCE_SEAL_SCHEMA:
        raise RuntimeError("malformed source seal: unsupported schema")
    runtime = seal.get("runtime_attestation")
    if not isinstance(runtime, dict) or set(runtime) != {"path", "sha256", "base64"}:
        raise RuntimeError("malformed source seal: runtime attestation")
    if runtime.get("path") != extract_speed.RUNTIME_ATTESTATION_FILENAME:
        raise RuntimeError("malformed source seal: runtime attestation path")
    digest = runtime.get("sha256")
    encoded = runtime.get("base64")
    if not isinstance(digest, str) or _SHA256.fullmatch(digest) is None:
        raise RuntimeError("malformed source seal: runtime attestation hash")
    if not isinstance(encoded, str):
        raise RuntimeError("malformed source seal: runtime attestation bytes")
    try:
        runtime_bytes = base64.b64decode(encoded, validate=True)
    except (ValueError, TypeError) as error:
        raise RuntimeError("malformed source seal: runtime attestation bytes") from error
    if hashlib.sha256(runtime_bytes).hexdigest() != digest:
        raise RuntimeError("malformed source seal: runtime attestation hash mismatch")
    attestation = extract_speed._json_object_from_bytes(
        runtime_bytes,
        "sealed runtime attestation",
    )
    extract_speed.validate_runtime_attestation(
        attestation,
        "sealed runtime attestation",
    )

    files = seal.get("files")
    if not isinstance(files, dict) or not files or list(files) != sorted(files):
        raise RuntimeError("malformed source seal: file hashes")
    normalized_files = {}
    for relative, value in files.items():
        try:
            normalized = extract_speed._safe_relative_path(
                relative,
                "source seal file path",
            ).as_posix()
        except RuntimeError as error:
            raise RuntimeError(f"malformed source seal: {error}") from error
        if normalized == SOURCE_SEAL_FILENAME:
            raise RuntimeError("malformed source seal: seal must exclude itself")
        if normalized in normalized_files or not isinstance(value, str) or _SHA256.fullmatch(value) is None:
            raise RuntimeError(f"malformed source seal hash for {relative!r}")
        normalized_files[normalized] = value
    runtime_name = extract_speed.RUNTIME_ATTESTATION_FILENAME
    if normalized_files.get(runtime_name) != digest:
        raise RuntimeError("malformed source seal: runtime attestation inventory")
    return normalized_files, runtime_bytes, attestation, raw


def _source_files_matching_seal(
    criterion_dir: Path,
    sealed_files: dict[str, str],
) -> dict[str, Path]:
    files = extract_speed.regular_file_tree(criterion_dir, "Criterion input")
    expected = set(sealed_files) | {SOURCE_SEAL_FILENAME}
    actual = set(files)
    if actual != expected:
        raise RuntimeError(
            "Criterion input does not match source seal: "
            f"missing={sorted(expected - actual)!r}, "
            f"unexpected={sorted(actual - expected)!r}"
        )
    return files


def _verify_source_matches_seal(
    criterion_dir: Path,
    sealed_files: dict[str, str],
    sealed_runtime_bytes: bytes,
    sealed_source_seal_bytes: bytes,
) -> dict[str, Path]:
    """Verify all current source bytes against the producer's completion seal."""

    files = _source_files_matching_seal(criterion_dir, sealed_files)
    current_source_seal, _, _ = _read_regular_file(
        files[SOURCE_SEAL_FILENAME],
        SOURCE_SEAL_FILENAME,
        retain_bytes=True,
    )
    if current_source_seal != sealed_source_seal_bytes:
        raise RuntimeError("source seal changed while collecting")
    runtime_name = extract_speed.RUNTIME_ATTESTATION_FILENAME
    runtime_bytes, runtime_digest, _ = _read_regular_file(
        files[runtime_name],
        runtime_name,
        retain_bytes=True,
    )
    if runtime_bytes != sealed_runtime_bytes or runtime_digest != sealed_files[runtime_name]:
        raise RuntimeError("source runtime attestation disagrees with source seal")
    for relative, expected_digest in sealed_files.items():
        if relative == runtime_name:
            continue
        _, actual_digest, _ = _read_regular_file(
            files[relative],
            relative,
            retain_bytes=False,
        )
        if actual_digest != expected_digest:
            raise RuntimeError(f"source seal hash mismatch for {relative!r}")
    return files


def _validate_sealed_snapshot(
    criterion_dir: Path,
    sealed_files: dict[str, str],
    sealed_runtime_bytes: bytes,
    expected_attestation: dict,
) -> dict[str, str]:
    copied_files = extract_speed.regular_file_tree(
        criterion_dir,
        "sealed Criterion snapshot",
    )
    if set(copied_files) != set(sealed_files):
        raise RuntimeError("sealed Criterion snapshot does not match source seal")
    files = {}
    for relative, copied in sorted(copied_files.items()):
        digest = extract_speed.sha256_file(copied, "sealed Criterion input")
        if digest != sealed_files[relative]:
            raise RuntimeError(f"sealed Criterion input {relative!r} diverged after copy")
        files[relative] = digest
    runtime_path = criterion_dir / extract_speed.RUNTIME_ATTESTATION_FILENAME
    runtime_bytes, _, _ = _read_regular_file(
        runtime_path,
        extract_speed.RUNTIME_ATTESTATION_FILENAME,
        retain_bytes=True,
    )
    if runtime_bytes != sealed_runtime_bytes:
        raise RuntimeError("sealed runtime attestation differs from source seal bytes")
    attestation = extract_speed.validate_runtime_attestation(
        extract_speed.load_json_object(runtime_path, "sealed runtime attestation"),
        "sealed runtime attestation",
    )
    if attestation != expected_attestation:
        raise RuntimeError("sealed runtime attestation disagrees with source seal")
    extract_speed.validate_thread_cpu_behavior_attestation(criterion_dir, attestation)
    return files


def collect_criterion_bundle(criterion_dir: Path, bundle_dir: Path) -> Path:
    """Copy one completed, source-sealed Criterion tree into a fresh bundle."""

    _require_directory(criterion_dir, "Criterion input")
    _assert_fresh_destination(criterion_dir, bundle_dir)
    source_seal_path = criterion_dir / SOURCE_SEAL_FILENAME
    (
        sealed_files,
        sealed_runtime_bytes,
        attestation,
        sealed_source_seal_bytes,
    ) = _load_source_seal(source_seal_path)
    source_files = _verify_source_matches_seal(
        criterion_dir,
        sealed_files,
        sealed_runtime_bytes,
        sealed_source_seal_bytes,
    )

    parent = bundle_dir.parent
    _require_directory(parent, "collector bundle parent")
    try:
        staging_dir = Path(tempfile.mkdtemp(prefix=f".{bundle_dir.name}.", dir=parent))
    except OSError as error:
        raise RuntimeError(
            f"could not create collector bundle staging directory under {parent}: {error}"
        ) from error

    published = False
    try:
        copied_criterion = staging_dir / extract_speed.COLLECTOR_CRITERION_DIRECTORY
        copied_criterion.mkdir()
        for relative, expected_digest in sorted(sealed_files.items()):
            _copy_sealed_file(
                source_files[relative],
                _relative_destination(copied_criterion, relative),
                relative,
                expected_digest,
            )
        _verify_source_matches_seal(
            criterion_dir,
            sealed_files,
            sealed_runtime_bytes,
            sealed_source_seal_bytes,
        )
        files = _validate_sealed_snapshot(
            copied_criterion,
            sealed_files,
            sealed_runtime_bytes,
            attestation,
        )
        manifest = {
            "schema": extract_speed.COLLECTOR_SCHEMA,
            "runtime_attestation": attestation,
            "files": files,
        }
        (staging_dir / extract_speed.COLLECTOR_MANIFEST_FILENAME).write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )

        if os.path.lexists(bundle_dir):
            raise RuntimeError(
                f"collector bundle destination appeared while collecting: {bundle_dir}"
            )
        try:
            staging_dir.rename(bundle_dir)
        except OSError as error:
            raise RuntimeError(
                f"could not publish collector bundle {bundle_dir}: {error}"
            ) from error
        published = True
    finally:
        if not published:
            shutil.rmtree(staging_dir, ignore_errors=True)
    return bundle_dir


def main() -> None:
    parser = argparse.ArgumentParser(
        description="copy one source-sealed Criterion tree into a fresh tach speed bundle"
    )
    parser.add_argument("criterion_dir", type=Path)
    parser.add_argument("bundle_dir", type=Path)
    args = parser.parse_args()
    try:
        bundle = collect_criterion_bundle(args.criterion_dir, args.bundle_dir)
    except RuntimeError as error:
        parser.error(str(error))
    print(bundle / extract_speed.COLLECTOR_MANIFEST_FILENAME)


if __name__ == "__main__":
    main()
