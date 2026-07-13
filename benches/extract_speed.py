#!/usr/bin/env python3
"""Extract per-clock now()/elapsed() medians from Criterion data.

Usage: extract_speed.py <path-to-target/criterion>
       extract_speed.py --collector-bundle <path-to-bundle>

Prints JSON {clock: {"now": ns, "elapsed": ns}} for the speed-bench clocks.
Thread-CPU entries additionally carry the provider and read-cost labels encoded
by the Rust benchmark ID.
Runtime-selected ordered clocks include their selected provider and every
eligible exact direct-candidate `now()` row so dispatch overhead is explicit.
Every cell of the campaign (local, EC2, Docker-Alpine musl, Windows) funnels
through this so the extraction arithmetic is identical everywhere.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
import sys
import tempfile
from pathlib import Path, PurePosixPath

COLLECTOR_SCHEMA = "tach-speed-collector-v1"
COLLECTOR_MANIFEST_FILENAME = "tach-speed-collector.json"
COLLECTOR_CRITERION_DIRECTORY = "criterion"
COLLECTOR_HOST_DIRECTORY = "host"
RUNTIME_ATTESTATION_FILENAME = "runtime-attestation.json"
RUNTIME_ATTESTATION_SCHEMA = "tach-benchmark-runtime-v2"
THREAD_CPU_BEHAVIOR_FILENAME = "thread-cpu-behavior.json"
THREAD_CPU_BEHAVIOR_SCHEMA = "tach-thread-cpu-behavior-v2"
WALL_SELECTOR_FILENAMES = (
    "linux-x86-wall-selection.json",
    "linux-aarch64-wall-selection.json",
    "residual-wall-selection.json",
    "apple-wall-selection.json",
    "windows-wall-selection.json",
)
_SHA256 = re.compile(r"[0-9a-f]{64}")
_SOURCE_REVISION = re.compile(r"[0-9a-f]{40}|[0-9a-f]{64}")
RUNTIME_BUILD_MODE_FEATURES = {
    "default": ("bench-internal", "thread-cpu-inline"),
    "no-default": ("bench-internal",),
    "emscripten-pthreads": (
        "bench-internal",
        "emscripten-pthreads",
        "thread-cpu-inline",
    ),
}


def _reject_duplicate_json_keys(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key {key!r}")
        result[key] = value
    return result


def _json_object_from_bytes(value: bytes, description: str) -> dict:
    try:
        parsed = json.loads(
            value.decode("utf-8"),
            object_pairs_hook=_reject_duplicate_json_keys,
        )
    except (UnicodeDecodeError, json.JSONDecodeError, ValueError) as error:
        raise RuntimeError(f"could not load {description}: {error}") from error
    if not isinstance(parsed, dict):
        raise RuntimeError(f"{description} must be a JSON object")
    return parsed


def _read_regular_file_bytes(path: Path, description: str) -> bytes:
    """Read one opened regular file without following a final-component link."""

    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        raise RuntimeError(f"could not open {description} {path}: {error}") from error
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise RuntimeError(f"{description} is not a regular file: {path}")
        with os.fdopen(descriptor, "rb", closefd=True) as source:
            descriptor = -1
            return source.read()
    except OSError as error:
        raise RuntimeError(f"could not read {description} {path}: {error}") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def load_json_object_with_bytes(path: Path, description: str) -> tuple[dict, bytes]:
    """Load one JSON object and retain the exact bytes used to parse it."""

    value = _read_regular_file_bytes(path, description)
    return _json_object_from_bytes(value, f"{description} {path}"), value


def load_json_object(path: Path, description: str) -> dict:
    """Load one JSON object while rejecting duplicate keys at every depth."""

    value, _ = load_json_object_with_bytes(path, description)
    return value


def validate_runtime_attestation(value: object, context: str = "runtime attestation") -> dict:
    """Validate a runtime-emitted benchmark identity without enriching it."""

    expected_keys = {
        "schema",
        "invocation_id",
        "harness",
        "target",
        "features",
        "build_mode",
        "build_profile",
        "source_revision",
        "runner",
        "output_isolated",
    }
    if not isinstance(value, dict) or set(value) != expected_keys:
        raise RuntimeError(f"malformed {context}: unexpected shape")
    if value.get("schema") != RUNTIME_ATTESTATION_SCHEMA:
        raise RuntimeError(f"malformed {context}: unsupported schema")
    invocation_id = value.get("invocation_id")
    if not isinstance(invocation_id, str) or not invocation_id.strip():
        raise RuntimeError(f"malformed {context}: missing invocation ID")
    harness = value.get("harness")
    if not isinstance(harness, str) or not harness.strip():
        raise RuntimeError(f"malformed {context}: harness identity")

    target = value.get("target")
    if (
        not isinstance(target, dict)
        or set(target) != {"arch", "os", "env"}
        or not isinstance(target.get("arch"), str)
        or not target["arch"].strip()
        or not isinstance(target.get("os"), str)
        or not target["os"].strip()
        or not isinstance(target.get("env"), str)
    ):
        raise RuntimeError(f"malformed {context}: target identity")

    features = value.get("features")
    if (
        not isinstance(features, list)
        or not all(isinstance(feature, str) and feature for feature in features)
        or features != sorted(features)
        or len(features) != len(set(features))
    ):
        raise RuntimeError(f"malformed {context}: enabled features")
    build_mode = value.get("build_mode")
    expected_features = RUNTIME_BUILD_MODE_FEATURES.get(build_mode)
    if expected_features is None or features != list(expected_features):
        raise RuntimeError(f"malformed {context}: build mode")
    if value.get("build_profile") not in {"debug", "optimized"}:
        raise RuntimeError(f"malformed {context}: build profile")

    source_revision = value.get("source_revision")
    if source_revision is not None and (
        not isinstance(source_revision, str)
        or _SOURCE_REVISION.fullmatch(source_revision) is None
    ):
        raise RuntimeError(f"malformed {context}: source revision")
    runner = value.get("runner")
    if runner is not None and (
        not isinstance(runner, str) or not runner.strip()
    ):
        raise RuntimeError(f"malformed {context}: runner")
    if type(value.get("output_isolated")) is not bool:
        raise RuntimeError(f"malformed {context}: output isolation")
    return value


def sha256_file(path: Path, description: str = "file") -> str:
    """Return a regular file's content digest, rejecting links and special files."""

    try:
        mode = path.lstat().st_mode
    except OSError as error:
        raise RuntimeError(f"could not stat {description} {path}: {error}") from error
    if not stat.S_ISREG(mode):
        raise RuntimeError(f"{description} is not a regular file: {path}")
    digest = hashlib.sha256()
    try:
        with path.open("rb") as source:
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(chunk)
    except OSError as error:
        raise RuntimeError(f"could not hash {description} {path}: {error}") from error
    return digest.hexdigest()


def _safe_relative_path(value: object, context: str) -> PurePosixPath:
    if not isinstance(value, str) or not value or "\\" in value:
        raise RuntimeError(f"{context} is not a safe relative path")
    path = PurePosixPath(value)
    if (
        path.is_absolute()
        or not path.parts
        or any(part in {"", ".", ".."} for part in path.parts)
        or path.as_posix() != value
    ):
        raise RuntimeError(f"{context} is not a safe relative path")
    return path


def regular_file_tree(root: Path, description: str) -> dict[str, Path]:
    """Enumerate a directory's regular-file tree without following links."""

    try:
        root_mode = root.lstat().st_mode
    except OSError as error:
        raise RuntimeError(f"could not stat {description} {root}: {error}") from error
    if not stat.S_ISDIR(root_mode):
        raise RuntimeError(f"{description} is not a directory: {root}")

    files = {}

    def visit(directory: Path) -> None:
        try:
            children = sorted(directory.iterdir(), key=lambda child: child.name)
        except OSError as error:
            raise RuntimeError(f"could not read {description} {directory}: {error}") from error
        for child in children:
            relative = child.relative_to(root).as_posix()
            _safe_relative_path(relative, f"{description} path {relative!r}")
            try:
                mode = child.lstat().st_mode
            except OSError as error:
                raise RuntimeError(f"could not stat {description} entry {child}: {error}") from error
            if stat.S_ISDIR(mode):
                visit(child)
            elif stat.S_ISREG(mode):
                if relative in files:
                    raise RuntimeError(f"duplicate {description} path {relative!r}")
                files[relative] = child
            else:
                raise RuntimeError(
                    f"{description} contains a nonregular input at {relative!r}"
                )

    visit(root)
    return files


def validate_thread_cpu_behavior_attestation(
    criterion_dir: Path,
    attestation: dict,
) -> dict | None:
    """Load the optional v2 semantic sidecar bound to this invocation.

    This checks only the sidecar's identity and top-level shape.  The evidence
    layer owns validation of its raw probe samples and derived summaries.
    """

    behavior_path = criterion_dir / THREAD_CPU_BEHAVIOR_FILENAME
    if not behavior_path.exists():
        return None
    try:
        behavior_mode = behavior_path.lstat().st_mode
    except OSError as error:
        raise RuntimeError(
            f"could not stat thread-CPU behavior sidecar {behavior_path}: {error}"
        ) from error
    if not stat.S_ISREG(behavior_mode):
        raise RuntimeError(
            f"thread-CPU behavior sidecar is not a regular file: {behavior_path}"
        )
    behavior = load_json_object(behavior_path, "thread-CPU behavior sidecar")
    expected_keys = {
        "schema",
        "runtime_attestation",
        "direct_benchmark",
        "sample_count",
        "busy",
        "sleep",
        "sibling_isolation",
    }
    if not isinstance(behavior, dict) or set(behavior) != expected_keys:
        raise RuntimeError("thread-CPU behavior sidecar has an unexpected v2 shape")
    if behavior.get("schema") != THREAD_CPU_BEHAVIOR_SCHEMA:
        raise RuntimeError("thread-CPU behavior sidecar has an unsupported schema")
    if (
        not isinstance(behavior.get("direct_benchmark"), str)
        or not behavior["direct_benchmark"]
        or type(behavior.get("sample_count")) is not int
        or behavior["sample_count"] <= 0
        or not all(
            isinstance(behavior.get(phase), dict)
            for phase in ("busy", "sleep", "sibling_isolation")
        )
    ):
        raise RuntimeError("thread-CPU behavior sidecar has malformed v2 fields")
    embedded = validate_runtime_attestation(
        behavior.get("runtime_attestation"),
        "thread-CPU behavior runtime attestation",
    )
    if embedded != attestation:
        raise RuntimeError(
            "thread-CPU behavior runtime attestation disagrees with runtime-attestation.json"
        )
    return behavior


def _expected_wall_selector_filename(attestation: dict) -> str | None:
    target = attestation["target"]
    architecture = target["arch"]
    operating_system = target["os"]
    if operating_system == "macos":
        return "apple-wall-selection.json"
    if operating_system == "windows":
        return "windows-wall-selection.json"
    if operating_system == "freebsd":
        return "residual-wall-selection.json"
    if operating_system in {"linux", "android"}:
        if architecture in {"x86", "x86_64"}:
            return "linux-x86-wall-selection.json"
        if architecture == "aarch64":
            return "linux-aarch64-wall-selection.json"
        if architecture in {"arm", "s390x", "riscv64", "loongarch64", "powerpc64"}:
            return "residual-wall-selection.json"
    return None


def select_attested_wall_selector(
    criterion_dir: Path,
    attestation: dict,
    available_files: set[str] | None = None,
) -> str | None:
    """Choose the one wall selector permitted by the runtime target identity."""

    if available_files is None:
        available_files = set(
            regular_file_tree(criterion_dir, "Criterion selector tree")
        )
    recognized = sorted(set(WALL_SELECTOR_FILENAMES) & available_files)
    expected = _expected_wall_selector_filename(attestation)
    target = attestation["target"]
    target_name = f"{target['arch']}-{target['os']}-{target['env']}"
    if expected is None:
        if recognized:
            raise RuntimeError(
                "attested target "
                f"{target_name!r} cannot use recognized wall selector sidecars: {recognized!r}"
            )
        return None
    if not recognized:
        raise RuntimeError(
            f"attested target {target_name!r} is missing wall selector {expected!r}"
        )
    if len(recognized) != 1:
        raise RuntimeError(
            f"attested target {target_name!r} has multiple wall selector sidecars: "
            f"{recognized!r}"
        )
    if recognized[0] != expected:
        raise RuntimeError(
            f"wall selector {recognized[0]!r} cannot belong to attested target "
            f"{target_name!r}; expected {expected!r}"
        )
    return expected


def _relative_file_path(root: Path, relative: str) -> Path:
    return root.joinpath(*PurePosixPath(relative).parts)


def _collector_bundle_inputs(
    bundle_dir: Path,
) -> tuple[Path, str, dict, dict[str, str], bytes]:
    """Read and validate the immutable manifest inputs for one observation."""

    try:
        bundle_mode = bundle_dir.lstat().st_mode
    except OSError as error:
        raise RuntimeError(f"could not stat collector bundle {bundle_dir}: {error}") from error
    if not stat.S_ISDIR(bundle_mode):
        raise RuntimeError(f"collector bundle is not a directory: {bundle_dir}")

    try:
        entries = {entry.name: entry for entry in bundle_dir.iterdir()}
    except OSError as error:
        raise RuntimeError(f"could not read collector bundle {bundle_dir}: {error}") from error
    data_directories = {
        name for name in (COLLECTOR_CRITERION_DIRECTORY, COLLECTOR_HOST_DIRECTORY)
        if name in entries
    }
    expected_entries = {COLLECTOR_MANIFEST_FILENAME, *data_directories}
    if len(data_directories) != 1 or set(entries) != expected_entries:
        raise RuntimeError(
            "collector bundle has unexpected top-level entries: "
            "expected the manifest and exactly one observation directory, "
            f"found={sorted(entries)!r}"
        )

    manifest_path = entries[COLLECTOR_MANIFEST_FILENAME]
    data_kind = data_directories.pop()
    data_dir = entries[data_kind]
    try:
        manifest_mode = manifest_path.lstat().st_mode
    except OSError as error:
        raise RuntimeError(f"could not stat collector manifest {manifest_path}: {error}") from error
    if not stat.S_ISREG(manifest_mode):
        raise RuntimeError(f"collector manifest is not a regular file: {manifest_path}")
    manifest, manifest_bytes = load_json_object_with_bytes(
        manifest_path,
        "collector manifest",
    )
    if set(manifest) != {"schema", "runtime_attestation", "files"}:
        raise RuntimeError("malformed collector manifest: unexpected shape")
    if manifest.get("schema") != COLLECTOR_SCHEMA:
        raise RuntimeError("malformed collector manifest: unsupported schema")
    attestation = validate_runtime_attestation(
        manifest.get("runtime_attestation"),
        "collector manifest runtime attestation",
    )

    hashes = manifest.get("files")
    if not isinstance(hashes, dict) or not hashes:
        raise RuntimeError("malformed collector manifest: missing file hashes")
    if list(hashes) != sorted(hashes):
        raise RuntimeError("malformed collector manifest: file hashes are not sorted")
    file_hashes = {}
    for relative, digest in hashes.items():
        normalized = _safe_relative_path(relative, "collector manifest file path")
        normalized_text = normalized.as_posix()
        if normalized_text in file_hashes:
            raise RuntimeError(f"duplicate collector manifest file path {relative!r}")
        if not isinstance(digest, str) or _SHA256.fullmatch(digest) is None:
            raise RuntimeError(f"malformed collector manifest hash for {relative!r}")
        file_hashes[normalized_text] = digest
    if RUNTIME_ATTESTATION_FILENAME not in file_hashes:
        raise RuntimeError("collector manifest is missing runtime-attestation.json")
    return data_dir, data_kind, attestation, file_hashes, manifest_bytes


def _assert_tree_matches_manifest(
    criterion_dir: Path,
    file_hashes: dict[str, str],
    description: str,
) -> dict[str, Path]:
    files = regular_file_tree(criterion_dir, description)
    expected_files = set(file_hashes)
    actual_files = set(files)
    if actual_files != expected_files:
        raise RuntimeError(
            f"{description} does not match manifest: "
            f"missing={sorted(expected_files - actual_files)!r}, "
            f"unexpected={sorted(actual_files - expected_files)!r}"
        )
    return files


def _collector_attestation(attestation: dict, manifest_bytes: bytes) -> dict:
    return {
        "schema": COLLECTOR_SCHEMA,
        "invocation_id": attestation["invocation_id"],
        "runtime_attestation": attestation,
        "manifest_sha256": hashlib.sha256(manifest_bytes).hexdigest(),
    }


def _copy_manifest_file_to_snapshot(
    source: Path,
    destination: Path,
    expected_digest: str,
    relative: str,
) -> None:
    """Copy one manifest path while hashing the exact opened source bytes."""

    flags = os.O_RDONLY
    flags |= getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(source, flags)
    except OSError as error:
        raise RuntimeError(
            f"could not open collector Criterion input {relative!r}: {error}"
        ) from error
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise RuntimeError(
                f"collector Criterion input is not a regular file: {relative!r}"
            )
        destination.parent.mkdir(parents=True, exist_ok=True)
        digest = hashlib.sha256()
        with os.fdopen(descriptor, "rb", closefd=True) as source_file:
            descriptor = -1
            with destination.open("xb") as destination_file:
                for chunk in iter(lambda: source_file.read(1024 * 1024), b""):
                    digest.update(chunk)
                    destination_file.write(chunk)
        if digest.hexdigest() != expected_digest:
            raise RuntimeError(f"collector hash mismatch for {relative!r}")
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def extract_collector_bundle_observation(bundle_dir: Path) -> dict:
    """Snapshot a verified bundle, then return values parsed only from that snapshot."""

    data_dir, data_kind, attestation, file_hashes, manifest_bytes = _collector_bundle_inputs(
        bundle_dir
    )
    _assert_tree_matches_manifest(
        data_dir,
        file_hashes,
        f"collector {data_kind} tree",
    )
    with tempfile.TemporaryDirectory(prefix="tach-speed-collector-observation-") as directory:
        snapshot = Path(directory) / data_kind
        for relative, digest in file_hashes.items():
            _copy_manifest_file_to_snapshot(
                _relative_file_path(data_dir, relative),
                _relative_file_path(snapshot, relative),
                digest,
                relative,
            )

        snapshot_files = _assert_tree_matches_manifest(
            snapshot,
            file_hashes,
            f"collector snapshot {data_kind} tree",
        )
        copied_attestation = validate_runtime_attestation(
            load_json_object(
                snapshot / RUNTIME_ATTESTATION_FILENAME,
                "snapshot runtime attestation",
            ),
            "snapshot runtime attestation",
        )
        if copied_attestation != attestation:
            raise RuntimeError(
                "snapshot runtime attestation disagrees with the collector manifest"
            )
        if data_kind == COLLECTOR_CRITERION_DIRECTORY:
            if attestation.get("harness") != "criterion":
                raise RuntimeError("Criterion bundle carries a non-Criterion attestation")
            behavior = validate_thread_cpu_behavior_attestation(snapshot, attestation)
            selector = select_attested_wall_selector(
                snapshot,
                attestation,
                set(snapshot_files),
            )
            clocks = extract_criterion_directory(
                snapshot,
                wall_selector_filename=selector,
            )
        else:
            if attestation.get("harness") == "criterion":
                raise RuntimeError("host bundle carries a Criterion attestation")
            import host_speed

            host_observation = host_speed.extract_host_observation(snapshot, attestation)
            clocks = host_observation["clocks"]
            behavior = host_observation.get("thread_cpu_behavior")
        return {
            "clocks": clocks,
            "thread_cpu_behavior": behavior,
            "collector_attestation": _collector_attestation(
                attestation,
                manifest_bytes,
            ),
        }


def validate_collector_bundle(bundle_dir: Path) -> dict:
    """Verify a bundle in place; use the observation API before extracting it."""

    data_dir, data_kind, attestation, file_hashes, manifest_bytes = _collector_bundle_inputs(
        bundle_dir
    )
    copied_files = _assert_tree_matches_manifest(
        data_dir,
        file_hashes,
        f"collector {data_kind} tree",
    )
    for relative, path in copied_files.items():
        if sha256_file(path, f"collector {data_kind} input") != file_hashes[relative]:
            raise RuntimeError(f"collector hash mismatch for {relative!r}")
    copied_attestation = validate_runtime_attestation(
        load_json_object(
            data_dir / RUNTIME_ATTESTATION_FILENAME,
            "copied runtime attestation",
        ),
        "copied runtime attestation",
    )
    if copied_attestation != attestation:
        raise RuntimeError(
            "copied runtime attestation disagrees with the collector manifest"
        )
    if data_kind == COLLECTOR_CRITERION_DIRECTORY:
        if attestation.get("harness") != "criterion":
            raise RuntimeError("Criterion bundle carries a non-Criterion attestation")
        validate_thread_cpu_behavior_attestation(data_dir, attestation)
        select_attested_wall_selector(data_dir, attestation, set(copied_files))
    else:
        if attestation.get("harness") == "criterion":
            raise RuntimeError("host bundle carries a Criterion attestation")
        import host_speed

        host_speed.extract_host_observation(data_dir, attestation)
    return {
        "observation_dir": data_dir,
        "observation_kind": data_kind,
        "collector_attestation": _collector_attestation(attestation, manifest_bytes),
    }

WALL_FUNS = ["tach", "tach_ordered", "quanta", "fastant", "minstant", "std"]
# Criterion sanitizes the group label "Instant::now()" -> dir "Instant__now()"
# ("::" -> "__"); spaces / "+" / "()" are kept verbatim.
WALL_GROUPS = {"now": "Instant__now()", "elapsed": "Instant__now() + elapsed()"}
THREAD_CPU_GROUPS = {
    "now": "ThreadCpuInstant__now()",
    "elapsed": "ThreadCpuInstant__now() + elapsed()",
}
CRITERION_GROUP_IDS = {
    WALL_GROUPS["now"]: "Instant::now()",
    WALL_GROUPS["elapsed"]: "Instant::now() + elapsed()",
    THREAD_CPU_GROUPS["now"]: "ThreadCpuInstant::now()",
    THREAD_CPU_GROUPS["elapsed"]: "ThreadCpuInstant::now() + elapsed()",
}

TACH_PROVIDER_LABELS = {
    "linux_perf_mmap": "Linux perf task-clock mmap",
    "linux_perf_read": "Linux perf task-clock read",
    "posix_thread_cpu_clock": "POSIX thread CPU clock",
    "windows_thread_times": "Windows GetThreadTimes",
    "wasi_thread_cpu_clock": "WASI thread CPU clock",
    "node_thread_cpu_usage": "Node thread CPU usage",
    "performance_now": "performance.now",
    "node_hrtime": "process.hrtime.bigint",
    "monotonic_wall_clock": "monotonic wall clock",
    "unavailable": "unavailable",
    "other": "other",
}
TACH_CPU_PROVIDERS = {
    "linux_perf_mmap",
    "linux_perf_read",
    "posix_thread_cpu_clock",
    "windows_thread_times",
    "wasi_thread_cpu_clock",
    "node_thread_cpu_usage",
}
NATIVE_PROVIDER_LABELS = {
    "clock_gettime_clock_thread_cputime_id": "clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
    "libc_clock_gettime_clock_thread_cputime_id": (
        "libc::clock_gettime(CLOCK_THREAD_CPUTIME_ID)"
    ),
    "inline_syscall_clock_thread_cputime_id": (
        "inline syscall(CLOCK_THREAD_CPUTIME_ID)"
    ),
    "clock_gettime_nsec_np_clock_thread_cputime_id": (
        "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)"
    ),
    "get_thread_times_current_thread_pseudohandle": (
        "GetThreadTimes(current-thread pseudo-handle)"
    ),
}


def criterion_benchmarks(
    criterion_dir: Path, group_dir: str
) -> list[tuple[int, str, Path]]:
    group = criterion_dir / group_dir
    if not group.is_dir():
        return []

    expected_group_id = CRITERION_GROUP_IDS.get(group_dir)
    benchmarks = []
    identities: dict[str, Path] = {}
    for directory in group.iterdir():
        estimates = directory / "new" / "estimates.json"
        if not directory.is_dir() or not estimates.exists():
            continue

        identity = directory.name
        metadata_path = directory / "new" / "benchmark.json"
        if metadata_path.exists():
            metadata = load_json_object(metadata_path, "Criterion benchmark metadata")
            group_id = metadata.get("group_id")
            function_id = metadata.get("function_id")
            full_id = metadata.get("full_id")
            if (
                not isinstance(group_id, str)
                or not isinstance(function_id, str)
                or full_id != f"{group_id}/{function_id}"
                or (expected_group_id is not None and group_id != expected_group_id)
            ):
                raise RuntimeError(
                    f"malformed Criterion benchmark identity in {metadata_path}"
                )
            identity = function_id
        existing = identities.get(identity)
        if existing is not None:
            raise RuntimeError(
                f"duplicate Criterion benchmark identity {identity!r} under {group}: "
                f"{[existing.name, directory.name]}"
            )
        identities[identity] = directory
        benchmarks.append((estimates.stat().st_mtime_ns, identity, directory))
    return benchmarks


def find_exact_benchmark(
    criterion_dir: Path, group_dir: str, fn: str
) -> Path:
    group = criterion_dir / group_dir
    matches = [
        directory
        for _, identity, directory in criterion_benchmarks(criterion_dir, group_dir)
        if identity == fn
    ]
    if not matches:
        raise RuntimeError(f"expected benchmark {fn!r} under {group}, found none")
    if len(matches) != 1:
        raise RuntimeError(
            f"ambiguous benchmark {fn!r} under {group}: "
            f"{[directory.name for directory in matches]}"
        )
    return matches[0]


def median_estimate(criterion_dir: Path, group_dir: str, fn: str) -> dict:
    directory = find_exact_benchmark(criterion_dir, group_dir, fn)
    median = load_json_object(
        directory / "new" / "estimates.json",
        "Criterion estimates",
    )["median"]
    return {
        "point": median["point_estimate"],
        "ci95": [
            median["confidence_interval"]["lower_bound"],
            median["confidence_interval"]["upper_bound"],
        ],
    }


def add_estimate(entry: dict, kind: str, estimate: dict) -> None:
    entry[kind] = estimate["point"]
    entry[f"{kind}_ci95"] = estimate["ci95"]


def find_benchmark(criterion_dir: Path, group_dir: str, prefix: str) -> str:
  group = criterion_dir / group_dir
  matches = sorted(
    identity
    for _, identity, _ in criterion_benchmarks(criterion_dir, group_dir)
    if identity == prefix or identity.startswith(f"{prefix}__")
  )
  if not matches:
    raise RuntimeError(
      f"expected a {prefix!r} benchmark under {group}, found none"
    )
  if len(matches) != 1:
    raise RuntimeError(f"ambiguous {prefix!r} benchmark under {group}: {matches}")
  return matches[0]


def has_benchmark(criterion_dir: Path, group_dir: str, prefix: str) -> bool:
  return any(
    identity == prefix or identity.startswith(f"{prefix}__")
    for _, identity, _ in criterion_benchmarks(criterion_dir, group_dir)
  )


def thread_cpu_entry(criterion_dir: Path, prefix: str) -> dict:
    benchmark = find_benchmark(criterion_dir, THREAD_CPU_GROUPS["now"], prefix)
    elapsed_benchmark = find_benchmark(
        criterion_dir, THREAD_CPU_GROUPS["elapsed"], prefix
    )
    if benchmark != elapsed_benchmark:
        raise RuntimeError(
            f"provider changed between thread-CPU groups: {benchmark} vs {elapsed_benchmark}"
        )

    suffix = benchmark.removeprefix(f"{prefix}__")
    entry = {"benchmark": benchmark}
    for kind, group in THREAD_CPU_GROUPS.items():
        add_estimate(entry, kind, median_estimate(criterion_dir, group, benchmark))
    if prefix == "tach_thread_cpu":
        provider_key, separator, cost = suffix.rpartition("__")
        if not separator:
            provider_key, cost = suffix, "unknown_cost"
        entry["provider"] = TACH_PROVIDER_LABELS.get(
            provider_key, provider_key.replace("_", " ")
        )
        entry["read_cost"] = cost.replace("_", " ")
        if provider_key in TACH_CPU_PROVIDERS:
            entry["time_domain"] = "thread CPU"
        else:
            entry["time_domain"] = "monotonic wall fallback"
    else:
        entry["provider"] = NATIVE_PROVIDER_LABELS.get(
            suffix, suffix.replace("_", " ")
        )
        entry["read_cost"] = "system call"
        entry["time_domain"] = "thread CPU"
    return entry


def add_thread_cpu_selector_evidence(criterion_dir: Path, out: dict) -> None:
    path = criterion_dir / "thread-cpu-selection.json"
    if not path.exists():
        return

    selection = load_json_object(path, "thread-CPU selector")
    out["tach_thread_cpu"]["selection"] = selection
    if selection.get("selection_kind") == "fixed_native":
        add_fixed_native_thread_cpu_selector_evidence(
            criterion_dir, out, selection, path
        )
        return
    if selection.get("selection_kind") == "fixed_windows_thread_times":
        add_windows_thread_cpu_selector_evidence(criterion_dir, out, selection, path)
        return

    candidates = selection.get("eligible_direct_candidates")
    if not isinstance(candidates, list):
        raise RuntimeError(f"malformed thread-CPU candidates in {path}")
    native_probe = selection.get("native_entry_probe")
    perf = selection.get("perf")
    if not isinstance(native_probe, dict) or not isinstance(perf, dict):
        raise RuntimeError(f"malformed thread-CPU selector layers in {path}")
    mmap = perf.get("mmap")
    perf_read = perf.get("read")
    if not isinstance(mmap, dict) or not isinstance(perf_read, dict):
        raise RuntimeError(f"malformed perf thread-CPU provider layers in {path}")
    mmap_read_cost = mmap.get("read_cost")
    read_read_cost = perf_read.get("read_cost")
    if mmap_read_cost not in ("inline", "system call") or read_read_cost != "system call":
        raise RuntimeError(f"malformed perf thread-CPU read costs in {path}")
    native_candidates = []
    if "candidate_names" in native_probe:
        names = native_probe.get("candidate_names")
        eligible = native_probe.get("candidate_eligible")
        if (
            not isinstance(names, list)
            or not isinstance(eligible, list)
            or len(names) != len(eligible)
            or not all(isinstance(name, str) for name in names)
            or not all(type(available) is bool for available in eligible)
        ):
            raise RuntimeError(f"malformed generic native thread-CPU candidates in {path}")
        native_candidates.extend(
            f"direct_thread_cpu__{provider}"
            for provider, available in zip(names, eligible, strict=True)
            if available
        )
    else:
        for available, provider in (
            (native_probe.get("libc_available"), native_probe.get("libc_provider")),
            (native_probe.get("raw_available"), native_probe.get("raw_provider")),
        ):
            if type(available) is not bool or not isinstance(provider, str):
                raise RuntimeError(f"malformed native thread-CPU candidates in {path}")
            if available:
                native_candidates.append(f"direct_thread_cpu__{provider}")
    mmap_candidates = mmap.get("eligible_benchmarks")
    read_candidates = perf_read.get("eligible_benchmarks")
    if not all(
        isinstance(layer, list)
        and all(isinstance(candidate, str) for candidate in layer)
        for layer in (mmap_candidates, read_candidates)
    ):
        raise RuntimeError(f"malformed perf thread-CPU candidates in {path}")
    if candidates != [*native_candidates, *mmap_candidates, *read_candidates]:
        raise RuntimeError(f"thread-CPU candidate union disagrees with selector layers in {path}")

    selected_benchmark = selection.get("selected_native_benchmark")
    if not isinstance(selected_benchmark, str):
        raise RuntimeError(f"malformed selected thread-CPU benchmark in {path}")
    selected_provider = selection.get("selected_provider")
    selected_mechanism = selection.get("selected_mechanism")
    selected_cost = selection.get("selected_read_cost")
    if (
        selected_provider not in TACH_PROVIDER_LABELS
        or not isinstance(selected_mechanism, str)
        or selected_cost not in ("inline", "system call", "host call")
        or selected_benchmark != f"direct_selected_thread_cpu__{selected_mechanism}"
    ):
        raise RuntimeError(f"malformed selected thread-CPU identity in {path}")
    if (
        out["tach_thread_cpu"].get("provider") != TACH_PROVIDER_LABELS[selected_provider]
        or out["tach_thread_cpu"].get("read_cost") != selected_cost
    ):
        raise RuntimeError("thread-CPU introspection disagrees with selector metadata")

    fallback_benchmark = selection.get("fallback_native_benchmark")
    fallback_mechanism = selection.get("fallback_mechanism")
    fallback_cost = selection.get("fallback_read_cost")
    if fallback_benchmark is not None and (
        not isinstance(fallback_mechanism, str)
        or fallback_cost not in ("inline", "system call")
        or fallback_benchmark != f"direct_fallback_thread_cpu__{fallback_mechanism}"
    ):
        raise RuntimeError(f"malformed fallback thread-CPU identity in {path}")

    benchmarks = list(
        dict.fromkeys(
            [*candidates, selected_benchmark]
            + ([fallback_benchmark] if fallback_benchmark is not None else [])
        )
    )
    for benchmark in benchmarks:
        provider = benchmark.removeprefix("direct_selected_thread_cpu__").removeprefix(
            "direct_fallback_thread_cpu__"
        ).removeprefix(
            "direct_thread_cpu__"
        )
        if benchmark == selected_benchmark:
            read_cost = selected_cost
        elif benchmark == fallback_benchmark:
            read_cost = fallback_cost
        elif provider.startswith("linux_perf_mmap__"):
            read_cost = mmap_read_cost
        elif provider.startswith("linux_perf_read__"):
            read_cost = read_read_cost
        else:
            read_cost = "system call"
        entry = {
            "provider": provider,
            "read_cost": read_cost,
            "time_domain": "thread CPU",
            "benchmark": benchmark,
        }
        for metric, group in THREAD_CPU_GROUPS.items():
            add_estimate(
                entry,
                metric,
                median_estimate(criterion_dir, group, benchmark),
            )
        if benchmark == selected_benchmark:
            key = "direct_selected_thread_cpu"
        elif benchmark == fallback_benchmark:
            key = "direct_fallback_thread_cpu"
        else:
            key = benchmark
        out[key] = entry

    if out["direct_selected_thread_cpu"]["provider"] != selected_mechanism:
        raise RuntimeError("selected thread-CPU benchmark disagrees with selector metadata")
    if fallback_benchmark is not None and (
        out["direct_fallback_thread_cpu"]["provider"] != fallback_mechanism
    ):
        raise RuntimeError("fallback thread-CPU benchmark disagrees with selector metadata")


def add_fixed_native_thread_cpu_selector_evidence(
    criterion_dir: Path,
    out: dict,
    selection: dict,
    path: Path,
) -> None:
    mechanism = "macos_clock_gettime_nsec_np_thread_cpu"
    candidate = f"direct_thread_cpu__{mechanism}"
    selected_benchmark = f"direct_selected_thread_cpu__{mechanism}"
    expected_identity = {
        "selected_provider": "posix_thread_cpu_clock",
        "selected_mechanism": mechanism,
        "selected_read_cost": "system call",
        "selected_native_benchmark": selected_benchmark,
        "fallback_provider": None,
        "fallback_mechanism": None,
        "fallback_read_cost": None,
        "fallback_native_benchmark": None,
        "eligible_direct_candidates": [candidate],
    }
    if any(selection.get(key) != value for key, value in expected_identity.items()):
        raise RuntimeError(f"malformed fixed-native thread-CPU identity in {path}")

    fixed = selection.get("fixed_provider")
    expected_fixed = {
        "candidate": mechanism,
        "supported_architectures": ["x86_64", "aarch64"],
        "native_primitive": "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)",
        "time_domain": "thread CPU",
    }
    if (
        not isinstance(fixed, dict)
        or any(fixed.get(key) != value for key, value in expected_fixed.items())
        or not isinstance(fixed.get("selection_basis"), str)
        or not fixed["selection_basis"].strip()
        or not isinstance(selection.get("read_cost_basis"), str)
        or not selection["read_cost_basis"].strip()
        or selection.get("perf") is not None
        or selection.get("native_entry_probe") is not None
        or selection.get("failure_fallback") is not None
    ):
        raise RuntimeError(f"malformed fixed-native thread-CPU basis in {path}")

    public = out.get("tach_thread_cpu")
    if (
        not isinstance(public, dict)
        or public.get("provider") != TACH_PROVIDER_LABELS["posix_thread_cpu_clock"]
        or public.get("read_cost") != "system call"
        or public.get("time_domain") != "thread CPU"
    ):
        raise RuntimeError(
            "macOS thread-CPU introspection disagrees with fixed selector metadata"
        )

    for benchmark in (candidate, selected_benchmark):
        entry = {
            "provider": mechanism,
            "read_cost": "system call",
            "time_domain": "thread CPU",
            "benchmark": benchmark,
        }
        for metric, group in THREAD_CPU_GROUPS.items():
            add_estimate(
                entry,
                metric,
                median_estimate(criterion_dir, group, benchmark),
            )
        key = (
            "direct_selected_thread_cpu"
            if benchmark == selected_benchmark
            else benchmark
        )
        out[key] = entry


def add_windows_thread_cpu_selector_evidence(
    criterion_dir: Path,
    out: dict,
    selection: dict,
    path: Path,
) -> None:
    mechanism = "get_thread_times_current_thread_pseudohandle"
    candidate = f"direct_thread_cpu__{mechanism}"
    selected_benchmark = f"direct_selected_thread_cpu__{mechanism}"
    expected_identity = {
        "selected_provider": "windows_thread_times",
        "selected_mechanism": mechanism,
        "selected_read_cost": "system call",
        "selected_native_benchmark": selected_benchmark,
        "fallback_provider": None,
        "fallback_mechanism": None,
        "fallback_read_cost": None,
        "fallback_native_benchmark": None,
        "eligible_direct_candidates": [candidate],
    }
    if any(selection.get(key) != value for key, value in expected_identity.items()):
        raise RuntimeError(f"malformed fixed Windows thread-CPU identity in {path}")

    expected_guard = {
        "required_provider": "windows_thread_times",
        "required_read_cost": "system call",
        "stale_selection_removed_before_guard": True,
        "on_mismatch": "panic before thread-cpu-selection.json is written",
    }
    if selection.get("native_campaign_guard") != expected_guard:
        raise RuntimeError(f"malformed Windows native campaign guard in {path}")

    fixed = selection.get("fixed_provider")
    if (
        not isinstance(fixed, dict)
        or fixed.get("candidate") != mechanism
        or fixed.get("supported_architectures") != ["x86", "x86_64", "aarch64"]
        or not isinstance(fixed.get("selection_basis"), str)
        or not fixed["selection_basis"].strip()
        or not isinstance(fixed.get("authority"), str)
        or not fixed["authority"].startswith("https://learn.microsoft.com/")
    ):
        raise RuntimeError(f"malformed fixed Windows thread-CPU basis in {path}")

    failure_fallback = selection.get("failure_fallback")
    fallback_mechanism = "windows_selected_monotonic_wall_fallback"
    fallback_benchmark = f"direct_fallback_thread_cpu__{fallback_mechanism}"
    expected_fallback = {
        "provider": "monotonic_wall_clock",
        "mechanism": fallback_mechanism,
        "read_cost": "system call",
        "time_domain": "monotonic wall fallback",
        "trigger": "GetThreadTimes(current-thread pseudo-handle) returns zero",
        "state_transition": "sticky process-wide fallback",
        "eligible_for_thread_cpu_speed_claim": False,
        "exact_route_measured": True,
        "exact_benchmark": fallback_benchmark,
        "observed_as_public_provider_during_campaign": False,
        "campaign_behavior": (
            "an observed fallback aborts the native benchmark before extraction "
            "instead of emitting thread-CPU parity evidence"
        ),
    }
    if not isinstance(failure_fallback, dict):
        raise RuntimeError(f"malformed Windows thread-CPU failure fallback in {path}")
    if failure_fallback.get("observed_as_public_provider_during_campaign") is not False:
        raise RuntimeError(
            "observed Windows wall fallback cannot be extracted as thread-CPU speed evidence"
        )
    if failure_fallback != expected_fallback:
        raise RuntimeError(f"malformed Windows thread-CPU failure fallback in {path}")

    exclusions = selection.get("ineligible_direct_candidates")
    if not isinstance(exclusions, dict) or set(exclusions) != {
        "query_thread_cycle_time",
        "nt_query_information_thread",
    }:
        raise RuntimeError(f"malformed Windows thread-CPU exclusions in {path}")
    for exclusion in exclusions.values():
        if (
            not isinstance(exclusion, dict)
            or exclusion.get("eligibility") != "ineligible"
            or not isinstance(exclusion.get("reason"), str)
            or not exclusion["reason"].strip()
            or not isinstance(exclusion.get("authority"), str)
            or not exclusion["authority"].startswith("https://learn.microsoft.com/")
        ):
            raise RuntimeError(f"malformed Windows thread-CPU exclusion in {path}")

    public = out.get("tach_thread_cpu")
    if (
        not isinstance(public, dict)
        or public.get("provider") != TACH_PROVIDER_LABELS["windows_thread_times"]
        or public.get("read_cost") != "system call"
        or public.get("time_domain") != "thread CPU"
    ):
        raise RuntimeError("Windows thread-CPU introspection disagrees with fixed selector metadata")

    for benchmark in (candidate, selected_benchmark):
        entry = {
            "provider": mechanism,
            "read_cost": "system call",
            "time_domain": "thread CPU",
            "benchmark": benchmark,
        }
        for metric, group in THREAD_CPU_GROUPS.items():
            add_estimate(
                entry,
                metric,
                median_estimate(criterion_dir, group, benchmark),
            )
        key = "direct_selected_thread_cpu" if benchmark == selected_benchmark else benchmark
        out[key] = entry

    fallback_entry = {
        "provider": fallback_mechanism,
        "read_cost": "system call",
        "time_domain": "monotonic wall fallback",
        "benchmark": fallback_benchmark,
        "eligible_for_thread_cpu_speed_claim": False,
    }
    for metric, group in THREAD_CPU_GROUPS.items():
        add_estimate(
            fallback_entry,
            metric,
            median_estimate(criterion_dir, group, fallback_benchmark),
        )
    out["direct_failure_fallback_thread_cpu"] = fallback_entry


def wall_candidate_read_cost(benchmark: str) -> str:
    if "vdso_direct" in benchmark or "vdso_time64_direct" in benchmark:
        return "direct vDSO call"
    if "syscall" in benchmark:
        return "system call"
    if "clock_monotonic" in benchmark:
        return "vDSO or system call"
    return "inline"


def add_wall_selector_evidence(
    criterion_dir: Path,
    out: dict,
    wall_selector_filename: str | None = None,
) -> None:
    filenames = (
        (wall_selector_filename,)
        if wall_selector_filename is not None
        else WALL_SELECTOR_FILENAMES
    )
    for filename in filenames:
        path = criterion_dir / filename
        if not path.exists():
            continue

        selection = load_json_object(path, "wall selector")
        out["tach"]["selection"] = selection
        # `tach_ordered.selection` retains the architecture-protocol evidence
        # emitted by ordered-selection.json; this record proves the complete
        # OS/direct provider choice for the OrderedInstant domain.
        out["tach_ordered"]["wall_selection"] = selection
        candidates = selection.get("eligible_direct_candidates", {})
        if isinstance(candidates, list):
            candidates = {"instant": candidates, "ordered": []}
        if not isinstance(candidates, dict):
            raise RuntimeError(f"malformed eligible candidates in {path}")

        for domain in ("instant", "ordered"):
            domain_candidates = candidates.get(domain, [])
            if not isinstance(domain_candidates, list):
                raise RuntimeError(f"malformed {domain} candidates in {path}")
            for candidate in domain_candidates:
                if candidate in out:
                    entry = out[candidate]
                    entry["benchmark"] = candidate
                    if "now" not in entry:
                        add_estimate(
                            entry,
                            "now",
                            median_estimate(
                                criterion_dir, WALL_GROUPS["now"], candidate
                            ),
                        )
                    if "elapsed" not in entry:
                        add_estimate(
                            entry,
                            "elapsed",
                            median_estimate(
                                criterion_dir, WALL_GROUPS["elapsed"], candidate
                            ),
                        )
                    continue
                estimate = median_estimate(criterion_dir, WALL_GROUPS["now"], candidate)
                entry = {
                    "provider": candidate.removeprefix("direct_wall__").removeprefix(
                        "direct_ordered_wall__"
                    ).removeprefix("direct_ordered__"),
                    "read_cost": wall_candidate_read_cost(candidate),
                    "time_domain": f"{domain} wall",
                    "benchmark": candidate,
                }
                add_estimate(entry, "now", estimate)
                add_estimate(
                    entry,
                    "elapsed",
                    median_estimate(
                        criterion_dir, WALL_GROUPS["elapsed"], candidate
                    ),
                )
                out[candidate] = entry

        return


def add_selected_wall_evidence(criterion_dir: Path, out: dict) -> None:
    for prefix, domain in (
        ("direct_selected_wall", "instant"),
        ("direct_selected_ordered_wall", "ordered"),
    ):
        present = {
            metric: has_benchmark(criterion_dir, group, prefix)
            for metric, group in WALL_GROUPS.items()
        }
        if not any(present.values()):
            continue
        if not all(present.values()):
            raise RuntimeError(
                f"selected native {prefix!r} must exist in both wall groups"
            )
        benchmarks = {
            metric: find_benchmark(criterion_dir, group, prefix)
            for metric, group in WALL_GROUPS.items()
        }
        if benchmarks["now"] != benchmarks["elapsed"]:
            raise RuntimeError(
                "selected wall provider changed between groups: "
                f"{benchmarks['now']} vs {benchmarks['elapsed']}"
            )
        benchmark = benchmarks["now"]
        provider = benchmark.removeprefix(f"{prefix}__")
        if "vdso_direct" in provider or "vdso_time64_direct" in provider:
            read_cost = "direct vDSO call"
        elif "syscall" in provider:
            read_cost = "system call"
        elif "clock_monotonic" in provider or provider == "windows_qpc":
            read_cost = "platform call"
        else:
            read_cost = "inline"
        entry = {
            "provider": provider,
            "read_cost": read_cost,
            "time_domain": f"{domain} wall",
            "benchmark": benchmark,
        }
        for metric, group in WALL_GROUPS.items():
            add_estimate(
                entry,
                metric,
                median_estimate(criterion_dir, group, benchmark),
            )
        out[prefix] = entry


def extract_criterion_directory(
    criterion_dir: Path,
    wall_selector_filename: str | None = None,
) -> dict:
    """Extract clocks from a raw Criterion directory without collector checks."""

    out = {}
    for fn in WALL_FUNS:
        entry = {}
        for kind, group_dir in WALL_GROUPS.items():
            add_estimate(entry, kind, median_estimate(criterion_dir, group_dir, fn))
        out[fn] = entry
    out["tach"]["time_domain"] = "instant wall"
    out["tach_ordered"]["time_domain"] = "ordered wall"
    ordered_selection = criterion_dir / "ordered-selection.json"
    if ordered_selection.exists():
        selection_data = load_json_object(ordered_selection, "ordered selector")
        out["tach_ordered"]["selection"] = selection_data
        for candidate in selection_data.get("eligible_direct_candidates", []):
            estimate = median_estimate(
                criterion_dir, WALL_GROUPS["now"], candidate
            )
            entry = {
                "provider": candidate.removeprefix("direct_ordered_wall__").removeprefix(
                    "direct_ordered__"
                ),
                "read_cost": "inline",
                "time_domain": "ordered wall",
                "benchmark": candidate,
            }
            add_estimate(entry, "now", estimate)
            out[candidate] = entry
    add_wall_selector_evidence(criterion_dir, out, wall_selector_filename)
    add_selected_wall_evidence(criterion_dir, out)
    out["tach_thread_cpu"] = thread_cpu_entry(criterion_dir, "tach_thread_cpu")
    out["native_thread_cpu"] = thread_cpu_entry(criterion_dir, "native_thread_cpu")
    add_thread_cpu_selector_evidence(criterion_dir, out)
    return out


def extract_collector_bundle(bundle_dir: Path) -> dict:
    """Extract a collector bundle through an isolated verified observation."""

    observation = extract_collector_bundle_observation(bundle_dir)
    out = observation["clocks"]
    out["collector_attestation"] = observation["collector_attestation"]
    return out


def main() -> None:
    parser = argparse.ArgumentParser(
        description="extract tach speed clocks from Criterion data"
    )
    parser.add_argument(
        "criterion_dir",
        nargs="?",
        type=Path,
        help="raw Criterion output directory",
    )
    parser.add_argument(
        "--collector-bundle",
        type=Path,
        help="verified tach-speed-collector bundle directory",
    )
    args = parser.parse_args()
    if (args.criterion_dir is None) == (args.collector_bundle is None):
        parser.error("provide exactly one raw Criterion directory or --collector-bundle")
    try:
        if args.collector_bundle is not None:
            out = extract_collector_bundle(args.collector_bundle)
        else:
            out = extract_criterion_directory(args.criterion_dir)
    except RuntimeError as error:
        parser.error(str(error))
    json.dump(out, sys.stdout, indent=2)
    print()


if __name__ == "__main__":
    main()
