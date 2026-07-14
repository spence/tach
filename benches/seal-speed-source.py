#!/usr/bin/env python3
"""Seal one completed Criterion tree before it becomes speed evidence.

Usage:
    seal-speed-source.py <criterion-dir> -- <cargo bench command ...>

The command is run first.  Only after it exits successfully does this script
write ``tach-speed-source-seal.json`` into the Criterion directory.  The seal
records the exact runtime-attestation bytes and a sorted SHA-256 inventory of
every regular Criterion input.  The collector consumes that inventory later;
it never treats an unsealed Criterion directory as evidence input.
"""

from __future__ import annotations

import argparse
import base64
import ctypes
import hashlib
import json
import os
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

import extract_speed


SOURCE_SEAL_FILENAME = "tach-speed-source-seal.json"
SOURCE_SEAL_SCHEMA = "tach-speed-source-seal-v1"
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
    # CPython's path-stat and descriptor-stat Windows backends can report
    # different file-index/link/ctime metadata for the same handle. Content
    # metadata remains comparable, while the no-reparse handle and the stable
    # pre/post descriptor fingerprints own identity and mutation safety.
    return (
        stat.S_IFMT(initial.st_mode),
        initial.st_size,
        initial.st_mtime_ns,
    ) == (
        stat.S_IFMT(opened.st_mode),
        opened.st_size,
        opened.st_mtime_ns,
    )


def _read_regular_file(
    path: Path,
    relative: str,
    *,
    retain_bytes: bool,
) -> tuple[bytes | None, str, tuple[int, ...]]:
    """Read and hash one stable, non-link source file through one descriptor."""

    try:
        initial = path.lstat()
    except OSError as error:
        raise RuntimeError(f"could not stat Criterion input {relative!r}: {error}") from error
    if not stat.S_ISREG(initial.st_mode):
        raise RuntimeError(f"Criterion input {relative!r} is not a regular file")
    try:
        if os.name == "nt":
            descriptor = _open_windows_no_reparse(path)
        else:
            no_follow = getattr(os, "O_NOFOLLOW", None)
            if no_follow is None:
                raise RuntimeError("secure source sealing requires O_NOFOLLOW")
            descriptor = os.open(path, os.O_RDONLY | no_follow)
    except OSError as error:
        raise RuntimeError(f"could not open Criterion input {relative!r}: {error}") from error
    try:
        opened = os.fstat(descriptor)
        if not stat.S_ISREG(opened.st_mode):
            raise RuntimeError(f"Criterion input {relative!r} is not a regular file")
        initial_state = _stat_fingerprint(initial)
        opened_state = _stat_fingerprint(opened)
        if not _path_matches_opened_file(initial, opened):
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
        if _stat_fingerprint(completed) != opened_state:
            raise RuntimeError(
                f"Criterion input {relative!r} changed while it was sealed"
            )
        return (b"".join(chunks) if retain_bytes else None), digest.hexdigest(), initial_state
    except OSError as error:
        raise RuntimeError(f"could not read Criterion input {relative!r}: {error}") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def _source_tree_states(
    criterion_dir: Path,
) -> tuple[dict[str, Path], dict[str, tuple[int, ...]]]:
    files = extract_speed.regular_file_tree(criterion_dir, "Criterion input")
    if SOURCE_SEAL_FILENAME in files:
        raise RuntimeError(
            "Criterion input already has tach-speed-source-seal.json; use fresh output"
        )
    if any(path.startswith(f".{SOURCE_SEAL_FILENAME}.") for path in files):
        raise RuntimeError("Criterion input has an unfinished source-seal temporary file")
    states = {}
    for relative, path in files.items():
        try:
            value = path.lstat()
        except OSError as error:
            raise RuntimeError(
                f"could not stat Criterion input {relative!r}: {error}"
            ) from error
        if not stat.S_ISREG(value.st_mode):
            raise RuntimeError(f"Criterion input {relative!r} is not a regular file")
        states[relative] = _stat_fingerprint(value)
    return files, states


def _write_seal_atomically(criterion_dir: Path, payload: bytes) -> Path:
    destination = criterion_dir / SOURCE_SEAL_FILENAME
    if os.path.lexists(destination):
        raise RuntimeError("Criterion input already has tach-speed-source-seal.json")
    descriptor = -1
    temporary_path: Path | None = None
    try:
        descriptor, temporary = tempfile.mkstemp(
            prefix=f".{SOURCE_SEAL_FILENAME}.",
            dir=criterion_dir,
        )
        temporary_path = Path(temporary)
        with os.fdopen(descriptor, "wb", closefd=True) as output:
            descriptor = -1
            output.write(payload)
            output.flush()
            os.fsync(output.fileno())
        if os.path.lexists(destination):
            raise RuntimeError("Criterion input gained a source seal while sealing")
        os.replace(temporary_path, destination)
        temporary_path = None
    except OSError as error:
        raise RuntimeError(f"could not write source seal: {error}") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        if temporary_path is not None:
            try:
                temporary_path.unlink()
            except FileNotFoundError:
                pass
    return destination


def seal_criterion_source(criterion_dir: Path) -> Path:
    """Create a source inventory only after the caller has completed a run."""

    _require_directory(criterion_dir, "Criterion input")
    files, initial_states = _source_tree_states(criterion_dir)
    runtime_name = extract_speed.RUNTIME_ATTESTATION_FILENAME
    if runtime_name not in files:
        raise RuntimeError("Criterion input is missing runtime-attestation.json")

    hashes = {}
    runtime_bytes = None
    for relative, path in sorted(files.items()):
        value, digest, state = _read_regular_file(
            path,
            relative,
            retain_bytes=relative == runtime_name,
        )
        if state != initial_states[relative]:
            raise RuntimeError(f"Criterion input {relative!r} changed while sealing")
        hashes[relative] = digest
        if relative == runtime_name:
            assert value is not None
            runtime_bytes = value
    assert runtime_bytes is not None
    runtime_attestation = extract_speed._json_object_from_bytes(
        runtime_bytes,
        "runtime attestation",
    )
    extract_speed.validate_runtime_attestation(runtime_attestation, "runtime attestation")

    _, final_states = _source_tree_states(criterion_dir)
    if final_states != initial_states:
        raise RuntimeError("Criterion input changed while sealing source inventory")

    payload = {
        "schema": SOURCE_SEAL_SCHEMA,
        "runtime_attestation": {
            "path": runtime_name,
            "sha256": hashes[runtime_name],
            "base64": base64.b64encode(runtime_bytes).decode("ascii"),
        },
        "files": hashes,
    }
    return _write_seal_atomically(
        criterion_dir,
        json.dumps(payload, indent=2, sort_keys=True).encode("utf-8") + b"\n",
    )


def main() -> None:
    parser = argparse.ArgumentParser(
        description="run cargo bench, then seal its complete Criterion output"
    )
    parser.add_argument("criterion_dir", type=Path)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if not args.command:
        parser.error("pass the benchmark command after '--'; the seal is post-command only")
    try:
        completed = subprocess.run(args.command, check=False)
    except OSError as error:
        parser.error(f"could not run benchmark command: {error}")
    if completed.returncode:
        raise SystemExit(completed.returncode)
    try:
        seal = seal_criterion_source(args.criterion_dir)
    except RuntimeError as error:
        parser.error(str(error))
    print(seal)


if __name__ == "__main__":
    main()
