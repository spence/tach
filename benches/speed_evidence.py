"""Shared evidence rules for tach's three steady-state timing contracts."""

from __future__ import annotations

import math
from pathlib import Path, PurePosixPath
import random
import re
import stat
import statistics
import subprocess


LOCAL_COMPETITORS = ("quanta", "fastant", "minstant", "std")
METRICS = ("now", "elapsed")
APPLE_AARCH64_QUANTA_INELIGIBILITY_REASON = (
  "apple_bare_cntvct_omits_xnu_wake_correction_and_may_suspend_diverge"
)
APPLE_AARCH64_QUANTA_IMPLEMENTATION = (
  "quanta reads bare CNTVCT_EL0 on aarch64-apple-darwin; unlike XNU's "
  "wake-corrected wall-clock path, that counter can time-warp or diverge "
  "across suspend"
)
EMSCRIPTEN_QUANTA_INELIGIBILITY_REASON = (
  "quanta_has_no_wasm32_unknown_emscripten_monotonic_implementation"
)
EMSCRIPTEN_QUANTA_IMPLEMENTATION = (
  "quanta 0.12.6 only defines its wasm monotonic provider for target_os=wasi; "
  "wasm32-unknown-emscripten fails to compile in quanta::clocks::monotonic"
)
WASM_UNKNOWN_STD_INELIGIBILITY_REASON = "rust_std_instant_panics_without_a_wasi_or_emscripten_host"
WASM_UNKNOWN_STD_IMPLEMENTATION = (
  "Rust 1.95 std::time::Instant::now aborts on wasm32-unknown-unknown; the target "
  "has no standard-library host clock binding"
)
WASM_UNKNOWN_QUANTA_INELIGIBILITY_REASON = (
  "quanta_wasm_performance_binding_panics_when_the_host_clock_is_unavailable"
)
WASM_UNKNOWN_QUANTA_IMPLEMENTATION = (
  "quanta 0.12.6 resolves globalThis.performance with expect and calls "
  "Performance.now without a failure guard; a missing or invalid host clock traps "
  "instead of satisfying tach's never-crash contract"
)
WASM_UNKNOWN_SYSTEM_TIME_INELIGIBILITY_REASON = (
  "wasm_fallback_uses_non_monotonic_low_resolution_date_now"
)
WASM_UNKNOWN_SYSTEM_TIME_IMPLEMENTATIONS = {
  "fastant": (
    "fastant 0.1.11 falls back to web_time::SystemTime::now, which reads Date.now"
  ),
  "minstant": (
    "minstant 0.1.7 falls back to web_time::SystemTime::now, which reads Date.now"
  ),
}
WASI_SYSTEM_TIME_INELIGIBILITY_REASON = "wasi_fallback_uses_realtime_clock_not_monotonic"
WASI_SYSTEM_TIME_IMPLEMENTATIONS = {
  "fastant": (
    "fastant 0.1.11 falls back to web_time::SystemTime::now; web-time re-exports "
    "std::time on WASI, so this reads the realtime clock rather than CLOCK_MONOTONIC"
  ),
  "minstant": (
    "minstant 0.1.7 falls back to web_time::SystemTime::now; web-time re-exports "
    "std::time on WASI, so this reads the realtime clock rather than CLOCK_MONOTONIC"
  ),
}
BENCHMARK_FEATURES = ("bench-internal", "thread-cpu-inline")
BENCHMARK_FEATURES_BY_BUILD_MODE = {
  "default": BENCHMARK_FEATURES,
  "no-default": ("bench-internal",),
  "emscripten-pthreads": (
    "bench-internal",
    "emscripten-pthreads",
    "thread-cpu-inline",
  ),
}
STRICT_THREAD_CPU_PROVIDER_COSTS = {
  "Linux perf task-clock read": "system call",
  "POSIX thread CPU clock": "system call",
  "Windows GetThreadTimes": "system call",
  "WASI thread CPU clock": "host call",
  "Node thread CPU usage": "host call",
}
THREAD_CPU_PROVIDER_KEY_METADATA = {
  "linux_perf_read": ("Linux perf task-clock read", "system call"),
  "posix_thread_cpu_clock": ("POSIX thread CPU clock", "system call"),
  "windows_thread_times": ("Windows GetThreadTimes", "system call"),
  "wasi_thread_cpu_clock": ("WASI thread CPU clock", "host call"),
  "node_thread_cpu_usage": ("Node thread CPU usage", "host call"),
}


def perf_thread_cpu_read_cost(target_triple: str | None) -> str:
  """Classify a perf mmap read by the calls tach issues on its hot path."""
  return "inline"


def strict_thread_cpu_provider_cost(
  provider: str | None,
  target_triple: str | None,
) -> str | None:
  if provider == "Linux perf task-clock mmap":
    return perf_thread_cpu_read_cost(target_triple)
  return STRICT_THREAD_CPU_PROVIDER_COSTS.get(provider)


def thread_cpu_provider_key_metadata(
  provider_key: str | None,
  target_triple: str | None,
) -> tuple[str, str] | None:
  if provider_key == "linux_perf_mmap":
    return ("Linux perf task-clock mmap", perf_thread_cpu_read_cost(target_triple))
  return THREAD_CPU_PROVIDER_KEY_METADATA.get(provider_key)
THREAD_CPU_PERF_COUNTER_CANDIDATE_SETS = {
  (
    "x86_cpuid_rdtsc_cpuid",
    "x86_lfence_rdtsc_lfence",
    "x86_mfence_rdtsc_mfence",
    "x86_rdtscp_lfence",
    "x86_serialize_rdtsc_serialize",
  ),
  ("aarch64_isb_cntvct_isb", "aarch64_cntvctss_isb"),
  ("arm_isb_mrrc_cntvct_isb",),
  ("riscv_fence_rdtime_fence",),
}
THREAD_CPU_PERF_READ_ENTRY_CANDIDATE_SETS = {
  ("raw_read_syscall",),
  ("raw_read_svc",),
  ("raw_read_ecall",),
  ("raw_read_svc_3",),
  ("raw_read_syscall_0",),
  ("raw_read_int80", "libc_syscall_read", "libc_read"),
  ("raw_read_sc", "raw_read_scv", "libc_read"),
}
LAMBDA_INVOCATIONS = 5
LAMBDA_SAMPLES_PER_INVOCATION = 31
LAMBDA_SAMPLE_COUNT = LAMBDA_INVOCATIONS * LAMBDA_SAMPLES_PER_INVOCATION
LAMBDA_BOOTSTRAP_SAMPLES = 5_000
BENCHMARK_SOURCE_PATHS = (
  "Cargo.lock",
  "Cargo.toml",
  "src",
  "tests",
  "benches/bench_data.py",
  "benches/collect-speed-bundle.py",
  "benches/collect-host-speed-bundle.py",
  "benches/compose-speed.py",
  "benches/compose-supplemental-speed.py",
  "benches/extract_speed.py",
  "benches/host_speed.py",
  "benches/instant.rs",
  "benches/lambda-speed/Cargo.toml",
  "benches/lambda-speed/Cargo.lock",
  "benches/lambda-speed/src",
  "benches/release_chart.py",
  "benches/release_matrix.py",
  "benches/route_observation.py",
  "benches/require-clean-benchmark-source.sh",
  "benches/probes/aarch64-thread-pmu.c",
  "benches/route-coverage.toml",
  "benches/run-speed-aws.sh",
  "benches/run-speed-freebsd-aws.sh",
  "benches/run-speed-criterion.sh",
  "benches/run-speed-local.sh",
  "benches/run-thread-pmu-aws.sh",
  "benches/seal-speed-source.py",
  "benches/speed_evidence.py",
  "benches/summary.py",
  "benches/summary-thread-cpu.py",
  "benches/summary-use-cases.py",
  "benches/validate-release-evidence.py",
  "benches/verify-target-providers.py",
)
PRIMARY_SPEED_SCHEMA = "tach-speed-primary-v1"
PRIMARY_SPEED_REPORT_SCHEMA = "tach-speed-primary-report-v1"
PRIMARY_EVIDENCE_KIND = "full_speed"
PRIMARY_SPEED_CELLS = {
  "speed-0-apple.json": (
    0,
    "Apple Silicon",
    "M1 Max MacBook Pro",
    "aarch64-apple-darwin",
    "criterion",
    "default",
  ),
  "speed-1-c7g.json": (
    1,
    "AWS Graviton 3",
    "c7g.large",
    "aarch64-unknown-linux-gnu",
    "criterion",
    "default",
  ),
  "speed-2-inteln.json": (
    2,
    "AWS Intel",
    "c7i.large",
    "x86_64-unknown-linux-gnu",
    "criterion",
    "default",
  ),
  "speed-4-windows.json": (
    4,
    "GitHub Windows",
    "windows-2025",
    "x86_64-pc-windows-msvc",
    "criterion",
    "default",
  ),
}
EXPECTED_PRIMARY_IDENTITIES = tuple(
  values[:5] for values in PRIMARY_SPEED_CELLS.values()
)

SUPPLEMENTAL_SPEED_CELLS = {
  "speed-supplemental-macos-x86_64.json": (
    "x86_64-apple-darwin", "criterion", "full_speed_cell", "default"
  ),
  "speed-supplemental-macos-x86_64-no-default.json": (
    "x86_64-apple-darwin", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-macos-aarch64-no-default.json": (
    "aarch64-apple-darwin", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-windows-i686.json": (
    "i686-pc-windows-msvc", "criterion", "full_speed_cell", "default"
  ),
  "speed-supplemental-windows-aarch64.json": (
    "aarch64-pc-windows-msvc", "criterion", "full_speed_cell", "default"
  ),
  "speed-supplemental-windows-x86_64-no-default.json": (
    "x86_64-pc-windows-msvc", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-windows-i686-no-default.json": (
    "i686-pc-windows-msvc", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-windows-aarch64-no-default.json": (
    "aarch64-pc-windows-msvc", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-linux-i686.json": (
    "i686-unknown-linux-gnu", "criterion", "full_speed_cell", "default"
  ),
  "speed-supplemental-linux-s390x.json": (
    "s390x-unknown-linux-gnu", "criterion", "full_speed_cell", "default"
  ),
  "speed-supplemental-linux-loongarch64.json": (
    "loongarch64-unknown-linux-gnu", "criterion", "full_speed_cell", "default"
  ),
  "speed-supplemental-linux-riscv64gc.json": (
    "riscv64gc-unknown-linux-gnu", "criterion", "full_speed_cell", "default"
  ),
  "speed-supplemental-linux-powerpc64.json": (
    "powerpc64-unknown-linux-gnu", "criterion", "full_speed_cell", "default"
  ),
  "speed-supplemental-linux-powerpc64le.json": (
    "powerpc64le-unknown-linux-gnu", "criterion", "full_speed_cell", "default"
  ),
  "speed-supplemental-linux-armv7.json": (
    "armv7-unknown-linux-gnueabihf", "criterion", "full_speed_cell", "default"
  ),
  "speed-supplemental-android-x86_64.json": (
    "x86_64-linux-android", "criterion", "full_speed_cell", "default"
  ),
  "speed-supplemental-android-aarch64.json": (
    "aarch64-linux-android", "criterion", "full_speed_cell", "default"
  ),
  "speed-supplemental-linux-x86_64-no-default.json": (
    "x86_64-unknown-linux-gnu", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-linux-aarch64-no-default.json": (
    "aarch64-unknown-linux-gnu", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-linux-musl-x86_64-no-default.json": (
    "x86_64-unknown-linux-musl", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-linux-i686-no-default.json": (
    "i686-unknown-linux-gnu", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-linux-s390x-no-default.json": (
    "s390x-unknown-linux-gnu", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-linux-loongarch64-no-default.json": (
    "loongarch64-unknown-linux-gnu", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-linux-riscv64gc-no-default.json": (
    "riscv64gc-unknown-linux-gnu", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-linux-powerpc64-no-default.json": (
    "powerpc64-unknown-linux-gnu", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-linux-powerpc64le-no-default.json": (
    "powerpc64le-unknown-linux-gnu", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-linux-armv7-no-default.json": (
    "armv7-unknown-linux-gnueabihf", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-android-x86_64-no-default.json": (
    "x86_64-linux-android", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-android-aarch64-no-default.json": (
    "aarch64-linux-android", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-freebsd-x86_64.json": (
    "x86_64-unknown-freebsd", "criterion", "full_speed_cell", "default"
  ),
  "speed-supplemental-freebsd-x86_64-no-default.json": (
    "x86_64-unknown-freebsd", "criterion", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-lambda-aarch64.json": (
    "aarch64-unknown-linux-gnu", "lambda", "full_speed_cell", "default"
  ),
  "speed-supplemental-wasm-node.json": (
    "wasm32-unknown-unknown", "node-wasm-bindgen", "full_speed_cell", "default"
  ),
  "speed-supplemental-wasm-node-no-default.json": (
    "wasm32-unknown-unknown", "node-wasm-bindgen", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-emscripten-node.json": (
    "wasm32-unknown-emscripten", "emcc-node", "full_speed_cell", "default"
  ),
  "speed-supplemental-emscripten-node-no-default.json": (
    "wasm32-unknown-emscripten", "emcc-node", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-emscripten-pthreads.json": (
    "wasm32-unknown-emscripten", "emcc-node", "full_speed_cell", "emscripten-pthreads"
  ),
  "speed-supplemental-wasi-p1-node.json": (
    "wasm32-wasip1", "node-uvwasi", "full_speed_cell", "default"
  ),
  "speed-supplemental-wasi-p1-node-no-default.json": (
    "wasm32-wasip1", "node-uvwasi", "full_speed_cell", "no-default"
  ),
  "speed-supplemental-wasi-p1-wasmtime.json": (
    "wasm32-wasip1", "wasmtime", "tagged_wall_fallback", "default"
  ),
  "speed-supplemental-wasi-p1-wasmtime-no-default.json": (
    "wasm32-wasip1", "wasmtime", "tagged_wall_fallback", "no-default"
  ),
  "speed-supplemental-wasi-p2-wasmtime.json": (
    "wasm32-wasip2", "wasmtime-component", "tagged_wall_fallback", "default"
  ),
  "speed-supplemental-wasi-p2-wasmtime-no-default.json": (
    "wasm32-wasip2", "wasmtime-component", "tagged_wall_fallback", "no-default"
  ),
  "speed-supplemental-browser-negative.json": (
    "wasm32-unknown-unknown", "browser", "tagged_wall_fallback", "default"
  ),
  "speed-supplemental-browser-negative-no-default.json": (
    "wasm32-unknown-unknown", "browser", "tagged_wall_fallback", "no-default"
  ),
  "speed-supplemental-wasip1-threads-smoke.json": (
    "wasm32-wasip1-threads", "wasi-threads-smoke", "runtime_smoke", "default"
  ),
  "speed-supplemental-wasip1-threads-no-default-smoke.json": (
    "wasm32-wasip1-threads", "wasi-threads-smoke", "runtime_smoke", "no-default"
  ),
  "speed-supplemental-wasm32v1-none-smoke.json": (
    "wasm32v1-none", "wasm32v1-none-smoke", "runtime_smoke", "default"
  ),
  "speed-supplemental-wasm32v1-none-no-default-smoke.json": (
    "wasm32v1-none", "wasm32v1-none-smoke", "runtime_smoke", "no-default"
  ),
}

# Supplemental artifacts are runtime evidence.  The route-coverage TOML is a
# separate, static admission contract and must never be used to fill in an
# observed selector or benchmark identity here.
SUPPLEMENTAL_SPEED_SCHEMA = "tach-speed-supplemental-v4"
SUPPLEMENTAL_SELECTION_PROFILES = frozenset({
  "runtime_tournament",
  "fixed_native",
  "availability_fallback",
  "fallback_only",
})

# The frozen capability-gated wall pick per architecture family (ADR-0005 gates;
# ADR-0006 Apple ordered self-sync). A converted fixed-pick cell must assert its
# `selected_provider` is one of the mode-legal picks for its family; the emitted
# JSON records the resolved provider, not the commpage/kernel mode, so each
# contract lists every mode-legal provider. The Apple `ordered` set never admits
# a bare unbarriered counter (`apple_bare_cntvct`): an unbarriered read is never
# an ordered pick (ADR-0006 invariant).
EXPECTED_WALL_PICKS = {
  "apple": {
    "instant": frozenset({
      "apple_bare_cntvct",               # commpage SPEC (1) / NOSPEC_APPLE (3)
      "apple_commpage_cntvctss_offset",  # commpage NOSPEC (2)
      "apple_mach_absolute_time",        # commpage NONE (0)
    }),
    "ordered": frozenset({
      "apple_commpage_acntvct_offset",    # NOSPEC_APPLE (3), self-synchronizing
      "apple_commpage_cntvctss_offset",   # NOSPEC (2), self-synchronizing
      "apple_commpage_isb_cntvct_offset", # SPEC (1), explicit isb barrier
      "apple_mach_absolute_time",         # NONE (0), libSystem call boundary
    }),
  },
  "linux-aarch64": {
    "instant": frozenset({"aarch64_cntvct", "linux_clock_monotonic_syscall"}),
    "ordered": frozenset({"aarch64_isb_cntvct", "linux_clock_monotonic_syscall"}),
  },
  "linux-x86": {
    "instant": frozenset({
      "linux_kernel_eligible_tsc",
      "linux_clock_monotonic_syscall_x86_64",
      "linux_clock_monotonic_syscall_i686_time32",
    }),
    "ordered": frozenset({
      "linux_kernel_eligible_tsc_x86_lfence_rdtsc",
      "linux_clock_monotonic_syscall_x86_64_x86_cpuid",
      "linux_clock_monotonic_syscall_i686_time32_x86_cpuid",
    }),
  },
  "windows": {
    "instant": frozenset({"windows_qpc"}),
    "ordered": frozenset({"windows_qpc_call_boundary"}),
  },
  "freebsd": {
    "instant": frozenset({
      "freebsd_kernel_eligible_tsc",
      "freebsd_clock_monotonic_syscall",
    }),
    "ordered": frozenset({
      "freebsd_kernel_eligible_tsc_x86_lfence_rdtsc",
      "freebsd_clock_monotonic_syscall_x86_cpuid",
    }),
  },
}
SUPPLEMENTAL_ROUTE_ROWS = {
  "instant": ("tach", "direct_selected_wall", "instant"),
  "ordered": ("tach_ordered", "direct_selected_ordered_wall", "ordered"),
  "thread_cpu": ("tach_thread_cpu", "direct_selected_thread_cpu", "thread_cpu"),
}
RUNTIME_ATTESTATION_SCHEMA = "tach-benchmark-runtime-v2"
RUNTIME_SMOKE_ATTESTATION_SCHEMA = "tach-runtime-smoke-attestation-v1"
COLLECTOR_ATTESTATION_SCHEMA = "tach-speed-collector-v1"
COLLECTOR_BUNDLE_DESCRIPTOR_SCHEMA = "tach-speed-collector-v1"
THREAD_CPU_BEHAVIOR_SCHEMA = "tach-thread-cpu-behavior-v2"
THREAD_CPU_BEHAVIOR_SAMPLE_COUNT = 3
THREAD_CPU_BEHAVIOR_PHASES = ("busy", "sleep", "sibling_isolation")
THREAD_CPU_BEHAVIOR_FIELDS = (
  "wall_delta_ns",
  "public_delta_ns",
  "direct_delta_ns",
)

RUNTIME_TARGETS = {
  "aarch64-apple-darwin": {"arch": "aarch64", "os": "macos", "env": ""},
  "aarch64-unknown-linux-gnu": {"arch": "aarch64", "os": "linux", "env": "gnu"},
  "x86_64-unknown-linux-gnu": {"arch": "x86_64", "os": "linux", "env": "gnu"},
  "x86_64-unknown-linux-musl": {"arch": "x86_64", "os": "linux", "env": "musl"},
  "s390x-unknown-linux-gnu": {"arch": "s390x", "os": "linux", "env": "gnu"},
  "loongarch64-unknown-linux-gnu": {"arch": "loongarch64", "os": "linux", "env": "gnu"},
  "riscv64gc-unknown-linux-gnu": {"arch": "riscv64", "os": "linux", "env": "gnu"},
  "powerpc64-unknown-linux-gnu": {"arch": "powerpc64", "os": "linux", "env": "gnu"},
  "powerpc64le-unknown-linux-gnu": {"arch": "powerpc64", "os": "linux", "env": "gnu"},
  "armv7-unknown-linux-gnueabihf": {"arch": "arm", "os": "linux", "env": "gnu"},
  "x86_64-linux-android": {"arch": "x86_64", "os": "android", "env": ""},
  "aarch64-linux-android": {"arch": "aarch64", "os": "android", "env": ""},
  "x86_64-pc-windows-msvc": {"arch": "x86_64", "os": "windows", "env": "msvc"},
  "x86_64-apple-darwin": {"arch": "x86_64", "os": "macos", "env": ""},
  "i686-pc-windows-msvc": {"arch": "x86", "os": "windows", "env": "msvc"},
  "aarch64-pc-windows-msvc": {"arch": "aarch64", "os": "windows", "env": "msvc"},
  "i686-unknown-linux-gnu": {"arch": "x86", "os": "linux", "env": "gnu"},
  "x86_64-unknown-freebsd": {"arch": "x86_64", "os": "freebsd", "env": ""},
  "wasm32-unknown-unknown": {"arch": "wasm32", "os": "unknown", "env": ""},
  "wasm32-unknown-emscripten": {"arch": "wasm32", "os": "emscripten", "env": ""},
  "wasm32-wasip1": {"arch": "wasm32", "os": "wasi", "env": "p1"},
  "wasm32-wasip2": {"arch": "wasm32", "os": "wasi", "env": "p2"},
  "wasm32-wasip1-threads": {"arch": "wasm32", "os": "wasi", "env": "p1"},
  "wasm32v1-none": {"arch": "wasm32", "os": "none", "env": ""},
}
SUPPLEMENTAL_RUNTIME_TARGETS = RUNTIME_TARGETS

SUPPLEMENTAL_NATIVE_THREAD_CPU_IDENTITIES = {
  ("aarch64-unknown-linux-gnu", "criterion", "no-default"): (
    "native_thread_cpu__libc_clock_gettime_clock_thread_cputime_id",
    "libc::clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
    "system call",
  ),
  ("aarch64-unknown-linux-gnu", "lambda", "default"): (
    "native_thread_cpu__raw_syscall_clock_thread_cputime_id",
    "raw SYS_clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
    "system call",
  ),
  ("x86_64-unknown-linux-gnu", "criterion", "no-default"): (
    "native_thread_cpu__libc_clock_gettime_clock_thread_cputime_id",
    "libc::clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
    "system call",
  ),
  ("x86_64-unknown-linux-gnu", "lambda", "default"): (
    "native_thread_cpu__raw_syscall_clock_thread_cputime_id",
    "raw SYS_clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
    "system call",
  ),
  ("x86_64-unknown-linux-musl", "criterion", "no-default"): (
    "native_thread_cpu__libc_clock_gettime_clock_thread_cputime_id",
    "libc::clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
    "system call",
  ),
  ("x86_64-apple-darwin", "criterion", "default"): (
    "native_thread_cpu__clock_gettime_nsec_np_clock_thread_cputime_id",
    "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)",
    "system call",
  ),
  ("i686-pc-windows-msvc", "criterion", "default"): (
    "native_thread_cpu__get_thread_times_current_thread_pseudohandle",
    "GetThreadTimes(current-thread pseudo-handle)",
    "system call",
  ),
  ("aarch64-pc-windows-msvc", "criterion", "default"): (
    "native_thread_cpu__get_thread_times_current_thread_pseudohandle",
    "GetThreadTimes(current-thread pseudo-handle)",
    "system call",
  ),
  ("i686-unknown-linux-gnu", "criterion", "default"): (
    "native_thread_cpu__libc_clock_gettime_clock_thread_cputime_id",
    "libc::clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
    "system call",
  ),
  ("i686-unknown-linux-gnu", "criterion", "no-default"): (
    "native_thread_cpu__libc_clock_gettime_clock_thread_cputime_id",
    "libc::clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
    "system call",
  ),
  **{
    (target, "criterion", build_mode): (
      "native_thread_cpu__libc_clock_gettime_clock_thread_cputime_id",
      "libc::clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
      "system call",
    )
    for target in (
      "s390x-unknown-linux-gnu",
      "loongarch64-unknown-linux-gnu",
      "riscv64gc-unknown-linux-gnu",
      "powerpc64-unknown-linux-gnu",
      "powerpc64le-unknown-linux-gnu",
      "armv7-unknown-linux-gnueabihf",
    )
    for build_mode in ("default", "no-default")
  },
  ("x86_64-linux-android", "criterion", "default"): (
    "native_thread_cpu__inline_syscall_clock_thread_cputime_id",
    "inline syscall(CLOCK_THREAD_CPUTIME_ID)",
    "system call",
  ),
  ("aarch64-linux-android", "criterion", "default"): (
    "native_thread_cpu__inline_syscall_clock_thread_cputime_id",
    "inline syscall(CLOCK_THREAD_CPUTIME_ID)",
    "system call",
  ),
  **{
    (target, "criterion", "no-default"): (
      "native_thread_cpu__libc_clock_gettime_clock_thread_cputime_id",
      "libc::clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
      "system call",
    )
    for target in ("x86_64-linux-android", "aarch64-linux-android")
  },
  ("x86_64-unknown-freebsd", "criterion", "default"): (
    "native_thread_cpu__clock_gettime_clock_thread_cputime_id",
    "clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
    "system call",
  ),
  ("x86_64-unknown-freebsd", "criterion", "no-default"): (
    "native_thread_cpu__clock_gettime_clock_thread_cputime_id",
    "clock_gettime(CLOCK_THREAD_CPUTIME_ID)",
    "system call",
  ),
  **{
    (target, "criterion", "no-default"): (
      "native_thread_cpu__clock_gettime_nsec_np_clock_thread_cputime_id",
      "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)",
      "system call",
    )
    for target in ("x86_64-apple-darwin", "aarch64-apple-darwin")
  },
  **{
    (target, "criterion", "no-default"): (
      "native_thread_cpu__get_thread_times_current_thread_pseudohandle",
      "GetThreadTimes(current-thread pseudo-handle)",
      "system call",
    )
    for target in (
      "x86_64-pc-windows-msvc",
      "i686-pc-windows-msvc",
      "aarch64-pc-windows-msvc",
    )
  },
}

WINDOWS_GET_THREAD_TIMES_AUTHORITY = (
  "https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/"
  "nf-processthreadsapi-getthreadtimes"
)
WINDOWS_QUERY_THREAD_CYCLE_TIME_AUTHORITY = (
  "https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/"
  "nf-realtimeapiset-querythreadcycletime"
)
WINDOWS_NT_QUERY_INFORMATION_THREAD_AUTHORITY = (
  "https://learn.microsoft.com/en-us/windows/win32/api/winternl/"
  "nf-winternl-ntqueryinformationthread"
)


def local_reference_eligibility(target_triple: str | None) -> dict[str, dict]:
  """Classify public competitors against Instant's target-specific contract."""
  eligibility = {
    name: {
      "eligible": True,
      "reason": None,
      "implementation": None,
    }
    for name in LOCAL_COMPETITORS
  }
  if not isinstance(target_triple, str):
    return eligibility

  if target_triple == "aarch64-apple-darwin":
    eligibility["quanta"] = {
      "eligible": False,
      "reason": APPLE_AARCH64_QUANTA_INELIGIBILITY_REASON,
      "implementation": APPLE_AARCH64_QUANTA_IMPLEMENTATION,
    }
    return eligibility

  if target_triple == "wasm32-unknown-emscripten":
    eligibility["quanta"] = {
      "eligible": False,
      "reason": EMSCRIPTEN_QUANTA_INELIGIBILITY_REASON,
      "implementation": EMSCRIPTEN_QUANTA_IMPLEMENTATION,
    }
    for name, implementation in WASM_UNKNOWN_SYSTEM_TIME_IMPLEMENTATIONS.items():
      eligibility[name] = {
        "eligible": False,
        "reason": WASM_UNKNOWN_SYSTEM_TIME_INELIGIBILITY_REASON,
        "implementation": implementation,
      }
    return eligibility

  if target_triple in {"wasm32-wasip1", "wasm32-wasip2", "wasm32-wasip1-threads"}:
    for name, implementation in WASI_SYSTEM_TIME_IMPLEMENTATIONS.items():
      eligibility[name] = {
        "eligible": False,
        "reason": WASI_SYSTEM_TIME_INELIGIBILITY_REASON,
        "implementation": implementation,
      }
    return eligibility

  if target_triple in {"wasm32-unknown-unknown", "wasm32v1-none"}:
    eligibility["quanta"] = {
      "eligible": False,
      "reason": WASM_UNKNOWN_QUANTA_INELIGIBILITY_REASON,
      "implementation": WASM_UNKNOWN_QUANTA_IMPLEMENTATION,
    }
    for name, implementation in WASM_UNKNOWN_SYSTEM_TIME_IMPLEMENTATIONS.items():
      eligibility[name] = {
        "eligible": False,
        "reason": WASM_UNKNOWN_SYSTEM_TIME_INELIGIBILITY_REASON,
        "implementation": implementation,
      }
    eligibility["std"] = {
      "eligible": False,
      "reason": WASM_UNKNOWN_STD_INELIGIBILITY_REASON,
      "implementation": WASM_UNKNOWN_STD_IMPLEMENTATION,
    }
    return eligibility

  if "-windows-" not in target_triple:
    return eligibility

  for name in ("fastant", "minstant"):
    eligibility[name] = {
      "eligible": False,
      "reason": "windows_system_time_fallback_is_not_monotonic_qpc",
      "implementation": (
        f"{name} falls back to std::time::SystemTime on non-Linux-x86 targets"
      ),
    }

  architecture = target_triple.split("-", 1)[0]
  if architecture in ("x86_64", "aarch64"):
    raw_counter = "RDTSC" if architecture == "x86_64" else "CNTVCT_EL0"
    eligibility["quanta"] = {
      "eligible": False,
      "reason": "windows_raw_counter_does_not_meet_qpc_reliability_contract",
      "implementation": f"quanta selects calibrated {raw_counter} on this target",
    }

  return eligibility


def equivalence_allowance(reference: float) -> float:
  """Predeclared material-equivalence band for per-call nanoseconds."""
  return max(1.0, reference * 0.05)


def finite_number(value) -> bool:
  return type(value) in (int, float) and math.isfinite(value)


def exact_wall_candidate_identity(domain: str, benchmark: str) -> dict | None:
  prefix = "direct_wall__" if domain == "instant" else "direct_ordered_wall__"
  if (
    domain not in ("instant", "ordered")
    or not isinstance(benchmark, str)
    or not benchmark.startswith(prefix)
  ):
    return None
  if "vdso_direct" in benchmark or "vdso_time64_direct" in benchmark:
    read_cost = "direct vDSO call"
  elif any(
    provider in benchmark
    for provider in (
      "performance.now",
      "process.hrtime.bigint",
      "wasi_clock_monotonic",
      "emscripten_get_now",
      "emscripten_performance_epoch",
    )
  ):
    read_cost = "host call"
  elif "syscall" in benchmark:
    read_cost = "system call"
  elif "clock_monotonic" in benchmark:
    read_cost = "vDSO or system call"
  elif benchmark.removeprefix(prefix).startswith("windows_"):
    read_cost = "platform call"
  else:
    read_cost = "inline"
  return {
    "benchmark": benchmark,
    "provider": benchmark.removeprefix(prefix),
    "time_domain": f"{domain} wall",
    "read_cost": read_cost,
  }


def validate_ci(context: str, clock: str, entry: dict, failures: list[str]) -> None:
  for metric in METRICS:
    interval = entry.get(f"{metric}_ci95")
    point = entry.get(metric)
    if (
      not finite_number(point)
      or point < 0
      or not isinstance(interval, list)
      or len(interval) != 2
      or not all(finite_number(bound) for bound in interval)
      or not interval[0] <= point <= interval[1]
    ):
      failures.append(f"{context} {clock}: invalid {metric} estimate or 95% CI")


def equivalent_or_faster(subject: dict, reference: dict, metric: str) -> tuple[bool, float]:
  allowance = equivalence_allowance(reference[metric])
  subject_pair = subject.get("paired_sample_id")
  reference_pair = reference.get("paired_sample_id")
  if subject_pair is not None and reference_pair is not None:
    if (
      not isinstance(subject_pair, str)
      or not subject_pair
      or subject_pair != reference_pair
    ):
      raise TypeError("paired sample identities do not match")
    subject_samples = subject.get(f"{metric}_samples")
    reference_samples = reference.get(f"{metric}_samples")
    if (
      not isinstance(subject_samples, list)
      or not isinstance(reference_samples, list)
      or len(subject_samples) != len(reference_samples)
      or not subject_samples
      or not all(finite_number(value) for value in (*subject_samples, *reference_samples))
    ):
      raise TypeError("paired samples are malformed")
    differences = [
      subject_value - reference_value
      for subject_value, reference_value in zip(subject_samples, reference_samples)
    ]
    point, interval = lambda_median_with_ci(
      differences,
      f"paired-equivalence:{subject_pair}:{metric}",
    )
    return point <= allowance and interval[1] <= allowance, allowance
  point_passed = subject[metric] <= reference[metric] + allowance
  # This conservative comparison uses the slow edge of tach's interval and
  # the fast edge of the reference interval. Passing means the entire 95% CI
  # uncertainty still fits inside the predeclared material-equivalence band.
  ci_passed = subject[f"{metric}_ci95"][1] <= (
    reference[f"{metric}_ci95"][0] + allowance
  )
  return point_passed and ci_passed, allowance


def reproduce_material_decision(
  challenger: list[int],
  incumbent: list[int],
  reads_per_batch: int,
  required_wins: int,
  relative_denominator: int | None = 20,
  floor_ns_per_read: int = 1,
) -> dict:
  if (
    len(challenger) != 9
    or len(incumbent) != 9
    or not all(type(value) is int and value > 0 for value in challenger + incumbent)
    or type(reads_per_batch) is not int
    or reads_per_batch <= 0
    or (
      relative_denominator is not None
      and (type(relative_denominator) is not int or relative_denominator <= 0)
    )
    or type(floor_ns_per_read) is not int
    or floor_ns_per_read <= 0
  ):
    raise ValueError("malformed selector samples")
  challenger_median = int(statistics.median(challenger))
  incumbent_median = int(statistics.median(incumbent))
  relative_allowance = (
    incumbent_median // relative_denominator
    if relative_denominator is not None
    else 0
  )
  allowance = max(reads_per_batch * floor_ns_per_read, relative_allowance)
  decisive_wins = sum(
    candidate + allowance < baseline
    for candidate, baseline in zip(challenger, incumbent, strict=True)
  )
  selected = (
    challenger_median + allowance < incumbent_median
    and decisive_wins >= required_wins
  )
  return {
    "challenger_median_ns": challenger_median,
    "incumbent_median_ns": incumbent_median,
    "allowance_ns": allowance,
    "decisive_wins": decisive_wins,
    "selected": selected,
  }


def validate_apple_wall_selector(
  context: str,
  selection: dict,
  failures: list[str],
) -> dict:
  selected = selection.get("selected_provider")
  candidates = selection.get("eligible_direct_candidates")
  selected_benchmarks = selection.get("selected_native_benchmark")
  if not all(isinstance(value, dict) for value in (selected, candidates, selected_benchmarks)):
    failures.append(f"{context}: malformed Apple wall selector metadata")
    return {}
  probe = selection.get("probe")
  if (
    isinstance(probe, dict)
    and isinstance(probe.get("instant"), dict)
    and "tsc_eligible" in probe["instant"]
    and isinstance(probe.get("ordered"), dict)
  ):
    public_exact = selection.get("public_exact_probe")
    if not isinstance(public_exact, dict):
      failures.append(f"{context}: Apple x86 selector lacks paired public/exact evidence")
      public_exact = {}
    results = {}
    domain_specs = (
      (
        "instant",
        "tsc_eligible",
        "tsc_batches_ticks",
        "tsc_median_ticks",
        "tsc_selected",
        "apple_invariant_rdtsc",
        "direct_wall",
        "direct_selected_wall",
      ),
      (
        "ordered",
        "commpage_eligible",
        "commpage_batches_ticks",
        "commpage_median_ticks",
        "commpage_selected",
        "apple_commpage_lfence_rdtsc_nanotime",
        "direct_ordered_wall",
        "direct_selected_ordered_wall",
      ),
    )
    for (
      domain,
      eligible_key,
      batches_key,
      median_key,
      selected_key,
      challenger_provider,
      direct_prefix,
      selected_prefix,
    ) in domain_specs:
      domain_probe = probe[domain]
      eligible = domain_probe.get(eligible_key)
      baseline = domain_probe.get("mach_absolute_time_batches_ticks")
      challenger = domain_probe.get(batches_key)
      reads = domain_probe.get("reads_per_batch")
      required = domain_probe.get("required_decisive_wins")
      expected_candidates = [f"{direct_prefix}__apple_mach_absolute_time"]
      decision = None
      measured_winner = "apple_mach_absolute_time"
      if eligible is True:
        expected_candidates.append(f"{direct_prefix}__{challenger_provider}")
        try:
          decision = reproduce_material_decision(challenger, baseline, reads, required)
        except (TypeError, ValueError):
          failures.append(f"{context}: malformed Apple x86 {domain} selector samples")
          decision = {}
        if decision.get("selected") is True:
          measured_winner = challenger_provider
        if (
          not isinstance(baseline, list)
          or not isinstance(challenger, list)
          or domain_probe.get("mach_absolute_time_median_ticks")
          != int(statistics.median(baseline))
          or domain_probe.get(median_key) != int(statistics.median(challenger))
          or domain_probe.get("allowance_total_ticks") != decision.get("allowance_ns")
          or domain_probe.get("decisive_wins") != decision.get("decisive_wins")
          or domain_probe.get(selected_key) != decision.get("selected")
        ):
          failures.append(f"{context}: Apple x86 {domain} selector does not reproduce")
      elif eligible is False:
        if baseline != [0] * 9 or challenger != [0] * 9:
          failures.append(f"{context}: ineligible Apple x86 {domain} candidate was measured")
      else:
        failures.append(f"{context}: malformed Apple x86 {domain} eligibility")

      if reads != 4_096 or required != 8:
        failures.append(f"{context}: malformed Apple x86 {domain} decision rule")
      if candidates.get(domain) != expected_candidates:
        failures.append(f"{context}: Apple x86 {domain} candidate set is incomplete")
      selected_provider = domain_probe.get("selected_provider")
      if domain == "instant":
        if domain_probe.get("measured_winner") != measured_winner:
          failures.append(f"{context}: Apple x86 Instant measured winner changed")
        if selected_provider not in (measured_winner, "apple_mach_absolute_time"):
          failures.append(f"{context}: Apple x86 Instant selected an invalid provider")
      elif selected_provider != measured_winner:
        failures.append(f"{context}: Apple x86 Ordered selected provider does not reproduce")
      if selected.get(domain) != selected_provider:
        failures.append(f"{context}: Apple x86 {domain} selected providers disagree")
      if selected_benchmarks.get(domain) != f"{selected_prefix}__{selected_provider}":
        failures.append(f"{context}: Apple {domain} selected benchmark is mislabeled")
      domain_public_exact = public_exact.get(domain)
      if not isinstance(domain_public_exact, dict):
        failures.append(f"{context}: Apple x86 {domain} lacks metric parity evidence")
        domain_public_exact = {}
      results[domain] = {
        "winner": selected_provider,
        "measured_winner": measured_winner,
        "decision": decision,
        "public_exact": {
          metric: validate_wall_public_exact_probe(
            context, f"{domain}.{metric}", domain_public_exact.get(metric), failures
          )
          for metric in METRICS
        },
      }
    return results

  for domain, prefix in (
    ("instant", "direct_selected_wall"),
    ("ordered", "direct_selected_ordered_wall"),
  ):
    provider = selected.get(domain)
    expected_selected = f"{prefix}__{provider}"
    if selected_benchmarks.get(domain) != expected_selected:
      failures.append(f"{context}: Apple {domain} selected benchmark is mislabeled")
    declared = candidates.get(domain)
    if not isinstance(declared, list) or "native_wall__mach_absolute_time" not in declared:
      failures.append(f"{context}: Apple {domain} candidate set omits mach_absolute_time")
      continue
    if isinstance(provider, str) and "commpage" in provider:
      direct_prefix = "direct_wall" if domain == "instant" else "direct_ordered_wall"
      if f"{direct_prefix}__{provider}" not in declared:
        failures.append(f"{context}: Apple {domain} candidate set omits selected commpage path")
  return {"winner": selected, "decision": "XNU USER_TIMEBASE mode"}


def validate_wall_public_exact_probe(
  context: str,
  domain: str,
  probe: object,
  failures: list[str],
  *,
  material_loss_is_failure: bool = True,
) -> dict:
  """Replay an alternating public-vs-selected-direct steady-state probe."""
  if not isinstance(probe, dict):
    failures.append(f"{context}: {domain} lacks paired public/exact evidence")
    return {}
  public = probe.get("public_batches_ns")
  exact = probe.get("exact_batches_ns")
  band = probe.get("equivalence_band")
  reads = probe.get("reads_per_batch")
  required = probe.get("required_decisive_losses")
  if (
    probe.get("selection_kind") != "paired_public_exact_parity"
    or reads != 65_536
    or required != 8
    or not isinstance(band, dict)
    or band.get("floor_ns_per_read") != 1
    or band.get("relative_denominator") != 20
    or probe.get("batch_order")
    != "64 alternating 1024-read chunks per batch; starting side flips by batch"
    or probe.get("call_boundary") != "symmetric dynamic FnMut boundary"
    or probe.get("measurement_clock")
    != "std::time::Instant outside the measured read loop"
    or not isinstance(public, list)
    or not isinstance(exact, list)
    or len(public) != 9
    or len(exact) != 9
    or not all(type(sample) is int and sample > 0 for sample in [*public, *exact])
  ):
    failures.append(f"{context}: malformed {domain} paired public/exact evidence")
    return {}

  public_median = int(statistics.median(public))
  exact_median = int(statistics.median(exact))
  floor = reads * band["floor_ns_per_read"]
  allowance = max(floor, exact_median // band["relative_denominator"])
  decisive_losses = sum(
    public_sample > exact_sample + max(floor, exact_sample // band["relative_denominator"])
    for public_sample, exact_sample in zip(public, exact, strict=True)
  )
  materially_slower = (
    public_median > exact_median + allowance and decisive_losses >= required
  )
  if materially_slower and material_loss_is_failure:
    failures.append(
      f"{context}: {domain} public read is repeatably slower than its selected exact route"
    )
  return {
    "reads_per_batch": reads,
    "public_median_batch_ns": public_median,
    "exact_median_batch_ns": exact_median,
    "public_ns_per_read": public_median / reads,
    "exact_ns_per_read": exact_median / reads,
    "equivalence_allowance_ns_per_read": allowance / reads,
    "decisive_losses": decisive_losses,
    "admission_role": (
      "public_exact_parity"
      if material_loss_is_failure
      else "diagnostic_dispatch_lower_bound"
    ),
    "passed": not materially_slower,
  }


def wall_public_exact_metric(reproduction: object, metric: str) -> dict | None:
  """Return metric-specific parity, accepting the legacy now-only shape."""
  if not isinstance(reproduction, dict):
    return None
  parity = reproduction.get("public_exact")
  if not isinstance(parity, dict):
    return None
  metric_parity = parity.get(metric)
  if isinstance(metric_parity, dict):
    return metric_parity
  if metric == "now" and "passed" in parity:
    return parity
  return None


# Contract for a runtime-dispatched wall route whose public read cannot reach inline
# parity with a compile-time-specialized native read that pays no per-call dispatch
# (a read tach cannot ship). The exact native route is retained as a diagnostic
# dispatch lower bound; the gate becomes the usable-public-reference winner gate
# (tach must still beat `std`). Shared by the FreeBSD residual cell and the
# barrier-exposed ordered picks below, despite the historical FREEBSD_ name.
FREEBSD_PUBLIC_EXACT_CONTRACT = "dispatch_lower_bound_with_public_winner_gate"

# Ordered picks whose read carries a pipeline-flushing barrier (an `isb` context
# synchronization) STRUCTURALLY expose the SIGILL-safe provider dispatch: the barrier
# forbids the out-of-order overlap that hides the same dispatch on the barrier-free
# `Instant` path (measured at +0.000 ns on this hardware), so ordered inline parity
# with the dispatch-free native read is unreachable by any correct optimization
# (hardcoding the pick to drop the dispatch SIGILLs a counter-disabled thread;
# ADR-0003 mandates the isb). These take the contract above for their `ordered`
# domain. Adjudicated in docs/ESCALATIONS.md ESC-APPLE-ELAPSED-DISPATCH.
BARRIER_EXPOSED_ORDERED_PICKS = frozenset({"aarch64_isb_cntvct"})


def wall_public_exact_contract(reproduction: object) -> str | None:
  """Return the admission contract for a runtime-dispatched wall route."""
  if not isinstance(reproduction, dict):
    return None
  contract = reproduction.get("public_exact_contract")
  return contract if isinstance(contract, str) else None


def public_reference_winner_gate(
  clocks: dict,
  use_case: str,
  metric: str,
  target_triple: str | None,
) -> tuple[bool, dict]:
  """Check only alternatives a caller can actually use through a public API."""
  if use_case == "instant":
    eligibility = local_reference_eligibility(target_triple)
    comparisons = {}
    passed = True
    for reference_name in LOCAL_COMPETITORS:
      policy = eligibility[reference_name]
      if not policy["eligible"]:
        continue
      reference = clocks.get(reference_name)
      if not isinstance(reference, dict):
        comparisons[reference_name] = {"measured": False, "passed": False}
        passed = False
        continue
      winner, allowance = equivalent_or_faster(clocks["tach"], reference, metric)
      comparisons[reference_name] = {
        "measured": True,
        "reference_ns": reference[metric],
        "equivalence_allowance_ns": allowance,
        "passed": winner,
      }
      passed = passed and winner
    return passed, comparisons
  if use_case == "ordered":
    reference = clocks.get("std")
    if not isinstance(reference, dict):
      return False, {"std": {"measured": False, "passed": False}}
    winner, allowance = equivalent_or_faster(clocks["tach_ordered"], reference, metric)
    return winner, {
      "std": {
        "measured": True,
        "reference_ns": reference[metric],
        "equivalence_allowance_ns": allowance,
        "passed": winner,
      }
    }
  raise ValueError(f"unsupported wall-clock use case {use_case!r}")


def thread_public_exact_metric(reproduction: object, metric: str) -> dict | None:
  """Return a fixed-native thread clock's paired public/exact result."""
  if not isinstance(reproduction, dict):
    return None
  public_exact = reproduction.get("public_exact")
  if not isinstance(public_exact, dict):
    return None
  metric_result = public_exact.get(metric)
  return metric_result if isinstance(metric_result, dict) else None


def validate_legacy_native_thread_cpu_entry_probe(
  context: str,
  probe: dict,
  failures: list[str],
) -> dict:
  raw_preferred = probe.get("selection_kind") == "raw_syscall_preferred_with_performance_audit"
  if raw_preferred and probe.get("selection_basis") != (
    "the inlined raw Linux syscall is the native primitive; libc wraps the "
    "same kernel clock and remains the failure fallback"
  ):
    failures.append(f"{context}: native raw-syscall preference lacks its selection basis")
  libc_provider = probe.get("libc_provider")
  raw_provider = probe.get("raw_provider")
  libc_available = probe.get("libc_available")
  raw_available = probe.get("raw_available")
  if (
    not isinstance(libc_provider, str)
    or not isinstance(raw_provider, str)
    or type(libc_available) is not bool
    or type(raw_available) is not bool
  ):
    failures.append(f"{context}: malformed native thread-CPU candidate identities")
    return {}

  candidates = []
  if libc_available:
    candidates.append(f"direct_thread_cpu__{libc_provider}")
  if raw_available:
    candidates.append(f"direct_thread_cpu__{raw_provider}")
  if not candidates:
    failures.append(f"{context}: thread-CPU selector found no strict native provider")
    return {}

  reads_per_batch = probe.get("reads_per_batch")
  required_wins = probe.get("required_decisive_wins")
  if reads_per_batch != 4096 or required_wins != 7:
    failures.append(f"{context}: native thread-CPU decision rule changed")
  explicit_band = "floor_ns_per_read" in probe or "relative_denominator" in probe
  floor_ns_per_read = probe.get("floor_ns_per_read", 1)
  relative_denominator = probe.get("relative_denominator", 20)
  if explicit_band and (floor_ns_per_read != 1 or relative_denominator is not None):
    failures.append(f"{context}: native thread-CPU entry band changed")

  raw_decision = None
  libc_decision = None
  if libc_available and raw_available:
    try:
      raw_decision = reproduce_material_decision(
        probe.get("raw_batches_ns"),
        probe.get("libc_batches_ns"),
        reads_per_batch,
        required_wins,
        relative_denominator=relative_denominator,
        floor_ns_per_read=floor_ns_per_read,
      )
      libc_decision = reproduce_material_decision(
        probe.get("libc_batches_ns"),
        probe.get("raw_batches_ns"),
        reads_per_batch,
        required_wins,
        relative_denominator=relative_denominator,
        floor_ns_per_read=floor_ns_per_read,
      )
    except (TypeError, ValueError):
      failures.append(f"{context}: malformed native thread-CPU selector samples")
      return {}
    expected = (
      raw_provider
      if raw_preferred or raw_decision["selected"]
      else libc_provider
    )
    for prefix, decision in (("raw", raw_decision), ("libc", libc_decision)):
      expected_fields = {
        f"{prefix}_allowance_ns": decision["allowance_ns"],
        f"{prefix}_decisive_wins": decision["decisive_wins"],
      }
      if any(probe.get(key) != value for key, value in expected_fields.items()):
        failures.append(f"{context}: native thread-CPU {prefix} decision does not reproduce")
    if probe.get("raw_selected") != raw_decision["selected"]:
      failures.append(f"{context}: native thread-CPU raw winner flag does not reproduce")
    if probe.get("libc_materially_faster") != libc_decision["selected"]:
      failures.append(f"{context}: native thread-CPU libc reverse decision does not reproduce")
    if probe.get("libc_median_ns") != libc_decision["challenger_median_ns"]:
      failures.append(f"{context}: native thread-CPU libc median does not reproduce")
    if probe.get("raw_median_ns") != raw_decision["challenger_median_ns"]:
      failures.append(f"{context}: native thread-CPU raw median does not reproduce")
  elif raw_available and raw_preferred:
    expected = raw_provider
  elif libc_available:
    expected = libc_provider
  else:
    expected = raw_provider

  if not (libc_available and raw_available):
    for field in ("libc_batches_ns", "raw_batches_ns"):
      if probe.get(field) != [0] * 9:
        failures.append(f"{context}: unavailable native tournament has stale samples")
    for field in (
      "libc_median_ns",
      "raw_median_ns",
      "raw_allowance_ns",
      "raw_decisive_wins",
      "libc_allowance_ns",
      "libc_decisive_wins",
    ):
      if probe.get(field) != 0:
        failures.append(f"{context}: unavailable native tournament has stale {field}")
    if probe.get("raw_selected") is not False or probe.get("libc_materially_faster") is not False:
      failures.append(f"{context}: unavailable native tournament reports a measured winner")

  if probe.get("selected_provider") != expected:
    failures.append(f"{context}: native thread-CPU selected provider does not reproduce")
  if probe.get("selected_read_cost") != "system call":
    failures.append(f"{context}: native thread-CPU read-cost tier does not reproduce")
  return {
    "winner": expected,
    "candidates": candidates,
    "selected_read_cost": "system call",
    "raw_decision": raw_decision,
    "libc_reverse_decision": libc_decision,
  }


def validate_generic_native_thread_cpu_entry_probe(
  context: str,
  probe: dict,
  failures: list[str],
) -> dict:
  names = probe.get("candidate_names")
  eligible = probe.get("candidate_eligible")
  measured = probe.get("candidate_measured")
  if (
    not isinstance(names, list)
    or not names
    or not all(isinstance(name, str) and name for name in names)
    or len(set(names)) != len(names)
    or not isinstance(eligible, list)
    or len(eligible) != len(names)
    or not all(type(value) is bool for value in eligible)
    or not all(eligible)
    or not isinstance(measured, list)
    or len(measured) != len(names)
    or not all(type(value) is bool for value in measured)
  ):
    failures.append(f"{context}: malformed generic native thread-CPU candidates")
    return {}
  reads_per_batch = probe.get("reads_per_batch")
  required_wins = probe.get("required_decisive_wins")
  band = probe.get("equivalence_band")
  if (
    reads_per_batch != 4096
    or required_wins != 8
    or band != {"floor_ns_per_read": 1, "relative_denominator": 20}
  ):
    failures.append(f"{context}: generic native decision rule changed")
  expected_candidates = [
    f"direct_thread_cpu__{name}" for name in names
  ]

  batches = probe.get("candidate_batches_ns")
  decisions = []
  incumbent_index = 0
  if len(names) == 1:
    if probe.get("selection_kind") != "fixed_candidate" or batches is not None:
      failures.append(f"{context}: single available native route contains tournament samples")
  else:
    if not all(measured):
      failures.append(f"{context}: native tournament contains an unmeasured candidate")
    if probe.get("selection_kind") != "tournament":
      failures.append(f"{context}: multi-candidate native selector is not a tournament")
    if not isinstance(batches, list) or len(batches) != len(names):
      failures.append(f"{context}: malformed generic native tournament samples")
      return {}
    for index, samples in enumerate(batches):
      if not isinstance(samples, list) or len(samples) != 9 or not all(
        type(value) is int and value >= 0 for value in samples
      ):
        failures.append(f"{context}: malformed generic native samples for {names[index]}")
        return {}
      if 0 in samples:
        failures.append(f"{context}: eligible native route {names[index]} has zero samples")
    for challenger_index in range(1, len(names)):
      try:
        decision = reproduce_material_decision(
          batches[challenger_index],
          batches[incumbent_index],
          reads_per_batch,
          required_wins,
          relative_denominator=band.get("relative_denominator"),
          floor_ns_per_read=band.get("floor_ns_per_read"),
        )
      except (AttributeError, TypeError, ValueError):
        failures.append(f"{context}: malformed generic native tournament decision")
        return {}
      decision["challenger"] = names[challenger_index]
      decision["incumbent"] = names[incumbent_index]
      decisions.append(decision)
      if decision["selected"]:
        incumbent_index = challenger_index

  winner = names[incumbent_index]
  if probe.get("selected_candidate") != winner:
    failures.append(f"{context}: generic native winner does not reproduce")
  return {
    "winner": winner,
    "candidates": expected_candidates,
    "selected_read_cost": "system call",
    "decisions": decisions,
    "selection_kind": probe.get("selection_kind"),
  }


def validate_native_thread_cpu_entry_probe(
  context: str,
  probe: dict,
  failures: list[str],
) -> dict:
  if "candidate_names" in probe:
    return validate_generic_native_thread_cpu_entry_probe(context, probe, failures)
  return validate_legacy_native_thread_cpu_entry_probe(context, probe, failures)


def validate_thread_cpu_perf_counter_probe(
  context: str,
  probe: dict,
  failures: list[str],
) -> dict:
  names = probe.get("candidate_names")
  eligible = probe.get("candidate_eligible")
  batches = probe.get("candidate_batches_ns")
  if (
    not isinstance(names, list)
    or not isinstance(eligible, list)
    or len(names) not in (1, 2, 5)
    or len(eligible) != len(names)
    or not all(isinstance(name, str) and name for name in names)
    or not all(type(value) is bool for value in eligible)
  ):
    failures.append(f"{context}: malformed perf counter candidate metadata")
    return {}
  if tuple(names) not in THREAD_CPU_PERF_COUNTER_CANDIDATE_SETS:
    failures.append(f"{context}: perf counter candidate identities changed")
  selection_kind = probe.get("selection_kind", "tournament")
  if len(names) == 1:
    if (
      selection_kind != "fixed_candidate"
      or eligible != [True]
      or batches not in (None, [])
      or probe.get("reads_per_batch") is not None
      or probe.get("required_decisive_wins") is not None
    ):
      failures.append(f"{context}: malformed fixed perf counter evidence")
    winner = names[0]
    if probe.get("selected_candidate") != winner:
      failures.append(f"{context}: fixed perf counter identity disagrees")
    return {
      "winner": winner,
      "candidates": [f"direct_thread_cpu__linux_perf_mmap__{winner}"],
      "decisions": [],
      "selection_kind": "fixed_candidate",
    }
  if selection_kind != "tournament" or not isinstance(batches, list) or len(batches) != len(names):
    failures.append(f"{context}: malformed perf counter tournament metadata")
    return {}
  if probe.get("reads_per_batch") != 4096 or probe.get("required_decisive_wins") != 8:
    failures.append(f"{context}: perf counter decision rule changed")
  if probe.get("equivalence_band") != {"floor_ns_per_read": 1, "relative_denominator": 20}:
    failures.append(f"{context}: perf counter equivalence band changed")
  if eligible[0] is not True:
    failures.append(f"{context}: perf counter baseline is unavailable")

  expected_candidates = []
  decisions = []
  incumbent_index = None
  eligible_names = []
  for index, (name, is_eligible, samples) in enumerate(
    zip(names, eligible, batches, strict=True)
  ):
    if not isinstance(samples, list) or len(samples) != 9 or not all(
      type(value) is int and value >= 0 for value in samples
    ):
      failures.append(f"{context}: malformed perf counter samples for {name}")
      continue
    if not is_eligible:
      continue
    if name in eligible_names:
      failures.append(f"{context}: duplicate eligible perf counter identity {name}")
      continue
    eligible_names.append(name)
    expected_candidates.append(f"direct_thread_cpu__linux_perf_mmap__{name}")
    if incumbent_index is None:
      if 0 in samples:
        failures.append(f"{context}: malformed perf counter samples for {name}")
      incumbent_index = index
      continue
    try:
      decision = reproduce_material_decision(
        samples,
        batches[incumbent_index],
        probe.get("reads_per_batch"),
        probe.get("required_decisive_wins"),
      )
    except (TypeError, ValueError):
      failures.append(f"{context}: malformed perf counter tournament for {name}")
      continue
    decision["challenger"] = name
    decision["incumbent"] = names[incumbent_index]
    decisions.append(decision)
    if decision["selected"]:
      incumbent_index = index

  if incumbent_index is None:
    failures.append(f"{context}: perf counter tournament has no eligible candidate")
    return {}
  winner = names[incumbent_index]
  if probe.get("selected_candidate") != winner:
    failures.append(f"{context}: perf counter winner does not reproduce")
  return {
    "winner": winner,
    "candidates": expected_candidates,
    "decisions": decisions,
    "selection_kind": "tournament",
  }


def validate_thread_cpu_perf_read_entry_probe(
  context: str,
  probe: dict,
  failures: list[str],
) -> dict:
  names = probe.get("candidate_names")
  eligible = probe.get("candidate_eligible")
  measured = probe.get("candidate_measured")
  batches = probe.get("candidate_batches_ns")
  if (
    not isinstance(names, list)
    or tuple(names) not in THREAD_CPU_PERF_READ_ENTRY_CANDIDATE_SETS
    or not isinstance(eligible, list)
    or not isinstance(measured, list)
    or len(eligible) != len(names)
    or len(measured) != len(names)
    or not all(type(value) is bool for value in eligible + measured)
  ):
    failures.append(f"{context}: malformed perf-read entry metadata")
    return {}
  expected_candidates = [
    f"direct_thread_cpu__linux_perf_read__{name}"
    for name, available in zip(names, eligible, strict=True)
    if available
  ]
  if len(names) == 1:
    if (
      probe.get("selection_kind") != "fixed_candidate"
      or eligible != [True]
      or measured != [False]
      or batches is not None
    ):
      failures.append(f"{context}: malformed fixed perf-read ABI evidence")
    winner = names[0]
    decisions = []
  else:
    if (
      probe.get("selection_kind") != "tournament"
      or probe.get("reads_per_batch") != 4096
      or probe.get("required_decisive_wins") != 8
      or probe.get("equivalence_band")
      != {"floor_ns_per_read": 1, "relative_denominator": 20}
      or not isinstance(batches, list)
      or len(batches) != len(names)
      or measured != eligible
    ):
      failures.append(f"{context}: malformed perf-read ABI tournament")
      return {}
    incumbent_index = None
    decisions = []
    for index, (name, available, samples) in enumerate(
      zip(names, eligible, batches, strict=True)
    ):
      if not isinstance(samples, list) or len(samples) != 9 or not all(
        type(value) is int and value >= 0 for value in samples
      ):
        failures.append(f"{context}: malformed perf-read samples for {name}")
        return {}
      if not available:
        if any(samples):
          failures.append(f"{context}: unavailable perf-read ABI {name} has samples")
        continue
      if 0 in samples:
        failures.append(f"{context}: eligible perf-read ABI {name} has zero samples")
      if incumbent_index is None:
        incumbent_index = index
        continue
      try:
        decision = reproduce_material_decision(
          samples,
          batches[incumbent_index],
          probe.get("reads_per_batch"),
          probe.get("required_decisive_wins"),
        )
      except (TypeError, ValueError):
        failures.append(f"{context}: malformed perf-read ABI decision for {name}")
        return {}
      decision.update({"challenger": name, "incumbent": names[incumbent_index]})
      decisions.append(decision)
      if decision["selected"]:
        incumbent_index = index
    if incumbent_index is None:
      failures.append(f"{context}: perf-read ABI tournament has no eligible candidate")
      return {}
    winner = names[incumbent_index]
  if probe.get("selected_candidate") != winner:
    failures.append(f"{context}: perf-read ABI winner does not reproduce")
  return {
    "winner": winner,
    "candidates": expected_candidates,
    "decisions": decisions,
    "selection_kind": probe.get("selection_kind"),
  }


def replay_thread_cpu_paths(
  names: list[str],
  eligible: list[bool],
  batches: list[list[int]],
  reads_per_batch: int,
  required_wins: int,
  excluded: str | None = None,
) -> dict:
  incumbent_index = None
  decisions = []
  for index, (name, available, samples) in enumerate(
    zip(names, eligible, batches, strict=True)
  ):
    if not available or name == excluded:
      continue
    if incumbent_index is None:
      incumbent_index = index
      continue
    decision = reproduce_material_decision(
      samples,
      batches[incumbent_index],
      reads_per_batch,
      required_wins,
    )
    decision.update({"challenger": name, "incumbent": names[incumbent_index]})
    decisions.append(decision)
    if decision["selected"]:
      incumbent_index = index
  if incumbent_index is None:
    raise ValueError("thread-CPU path tournament has no eligible candidate")
  return {"winner": names[incumbent_index], "decisions": decisions}


def validate_thread_cpu_path_probe(
  context: str,
  probe: dict,
  failures: list[str],
) -> dict:
  names = probe.get("candidate_names")
  eligible = probe.get("candidate_eligible")
  batches = probe.get("candidate_batches_ns")
  if (
    probe.get("selection_kind") != "tournament_with_measured_runner_up"
    or names != ["posix_thread_cpu", "linux_perf_mmap", "linux_perf_read"]
    or not isinstance(eligible, list)
    or len(eligible) != 3
    or eligible[0] is not True
    or eligible[2] is not True
    or not all(type(value) is bool for value in eligible)
    or not isinstance(batches, list)
    or len(batches) != 3
    or probe.get("reads_per_batch") != 4096
    or probe.get("required_decisive_wins") != 8
    or probe.get("equivalence_band")
    != {"floor_ns_per_read": 1, "relative_denominator": 20}
  ):
    failures.append(f"{context}: malformed three-way thread-CPU path tournament")
    return {}
  for name, available, samples in zip(names, eligible, batches, strict=True):
    if not isinstance(samples, list) or len(samples) != 9 or not all(
      type(value) is int and value >= 0 for value in samples
    ):
      failures.append(f"{context}: malformed thread-CPU path samples for {name}")
      return {}
    if available and 0 in samples:
      failures.append(f"{context}: eligible thread-CPU path {name} has zero samples")
    if not available and any(samples):
      failures.append(f"{context}: unavailable thread-CPU path {name} has samples")
  try:
    selected = replay_thread_cpu_paths(
      names,
      eligible,
      batches,
      probe.get("reads_per_batch"),
      probe.get("required_decisive_wins"),
    )
    fallback = replay_thread_cpu_paths(
      names,
      eligible,
      batches,
      probe.get("reads_per_batch"),
      probe.get("required_decisive_wins"),
      excluded=selected["winner"],
    )
  except (TypeError, ValueError):
    failures.append(f"{context}: malformed three-way thread-CPU path decisions")
    return {}
  if probe.get("selected_candidate") != selected["winner"]:
    failures.append(f"{context}: thread-CPU path winner does not reproduce")
  if probe.get("fallback_candidate") != fallback["winner"]:
    failures.append(f"{context}: thread-CPU measured fallback does not reproduce")
  expected_capability_loss = eligible[1] and selected["winner"] != "linux_perf_mmap"
  if probe.get("capability_was_not_profitable") is not expected_capability_loss:
    failures.append(f"{context}: capability-vs-profitability evidence does not reproduce")
  return {"selected": selected, "fallback": fallback}


def validate_thread_cpu_capability_probe(
  context: str,
  probe: dict,
  failures: list[str],
) -> dict:
  """Validate AArch64 capability selection and its non-selecting cost audit."""
  names = probe.get("candidate_names")
  eligible = probe.get("candidate_eligible")
  batches = probe.get("candidate_batches_ns")
  if (
    probe.get("selection_kind") != "capability_preferred_with_performance_audit"
    or names != ["posix_thread_cpu", "linux_perf_mmap", "linux_perf_read"]
    or not isinstance(eligible, list)
    or len(eligible) != 3
    or eligible[0] is not True
    or eligible[2] is not True
    or not all(type(value) is bool for value in eligible)
    or not isinstance(batches, list)
    or len(batches) != 3
    or probe.get("preferred_candidate") != "linux_perf_mmap"
    or probe.get("failure_fallback_candidate") != "posix_thread_cpu"
    or not isinstance(probe.get("selection_basis"), str)
    or not probe["selection_basis"].strip()
    or probe.get("reads_per_batch") != 4096
    or probe.get("required_decisive_wins") != 8
    or probe.get("equivalence_band")
    != {"floor_ns_per_read": 1, "relative_denominator": 20}
  ):
    failures.append(f"{context}: malformed capability-preferred thread-CPU audit")
    return {}
  for name, available, samples in zip(names, eligible, batches, strict=True):
    if not isinstance(samples, list) or len(samples) != 9 or not all(
      type(value) is int and value >= 0 for value in samples
    ):
      failures.append(f"{context}: malformed thread-CPU audit samples for {name}")
      return {}
    if available and 0 in samples:
      failures.append(f"{context}: eligible thread-CPU audit path {name} has zero samples")
    if not available and any(samples):
      failures.append(f"{context}: unavailable thread-CPU audit path {name} has samples")
  try:
    audit = replay_thread_cpu_paths(
      names,
      eligible,
      batches,
      probe.get("reads_per_batch"),
      probe.get("required_decisive_wins"),
    )
  except (TypeError, ValueError):
    failures.append(f"{context}: malformed capability-preferred thread-CPU audit decisions")
    return {}
  selected = "linux_perf_mmap" if eligible[1] else "posix_thread_cpu"
  if probe.get("selected_candidate") != selected:
    failures.append(f"{context}: thread-CPU capability policy does not reproduce")
  if audit["winner"] != selected:
    failures.append(
      f"{context}: capability-preferred thread-CPU provider loses its performance audit"
    )
  fallback = {"winner": "posix_thread_cpu"} if selected == "linux_perf_mmap" else None
  return {"selected": {"winner": selected}, "fallback": fallback, "audit": audit}


def validate_thread_cpu_selector(
  context: str,
  selection: dict,
  failures: list[str],
  target_triple: str | None = None,
) -> dict:
  capability_preferred = (
    selection.get("selection_kind") == "capability_preferred_with_failure_fallback"
  )
  if capability_preferred and target_triple is not None and not (
    target_triple.startswith("aarch64-") and "-linux-" in target_triple
  ):
    failures.append(f"{context}: capability-preferred thread-CPU policy used off Linux AArch64")
  if selection.get("selection_kind") == "availability_fallback":
    provider_key = selection.get("selected_provider")
    mechanism = selection.get("selected_mechanism")
    read_cost = selection.get("selected_read_cost")
    candidate = f"direct_thread_cpu__{mechanism}"
    expected = thread_cpu_provider_key_metadata(provider_key, target_triple)
    if (
      expected is None
      or expected[1] != read_cost
      or not isinstance(mechanism, str)
      or not mechanism
      or selection.get("selected_native_benchmark")
      != f"direct_selected_thread_cpu__{mechanism}"
      or selection.get("eligible_direct_candidates") != [candidate]
      or not isinstance(selection.get("failure_fallback"), dict)
      or selection.get("fallback_provider") != "monotonic_wall_clock"
      or not isinstance(selection.get("fallback_mechanism"), str)
      or not selection["fallback_mechanism"]
      or selection.get("fallback_read_cost") not in {"inline", "system call", "host call"}
      or selection.get("fallback_native_benchmark") is not None
    ):
      failures.append(f"{context}: malformed availability-fallback thread-CPU selector")
      return {}
    return {
      "winner": provider_key,
      "selected_mechanism": mechanism,
      "selected_read_cost": read_cost,
      "eligible_direct_candidates": [candidate],
      "failure_fallback": selection["failure_fallback"],
    }
  if selection.get("selection_kind") == "fixed_windows_thread_times":
    return validate_windows_thread_cpu_selector(
      context, selection, failures, target_triple
    )
  if selection.get("selection_kind") == "fixed_native":
    return validate_fixed_native_thread_cpu_selector(
      context, selection, failures, target_triple
    )

  native_probe = selection.get("native_entry_probe")
  perf = selection.get("perf")
  candidates = selection.get("eligible_direct_candidates")
  if (
    not isinstance(native_probe, dict)
    or not isinstance(perf, dict)
    or not isinstance(candidates, list)
  ):
    failures.append(f"{context}: malformed thread-CPU selector metadata")
    return {}
  native = validate_native_thread_cpu_entry_probe(context, native_probe, failures)
  if not native:
    return {}

  event_available = perf.get("event_available")
  mmap = perf.get("mmap")
  read = perf.get("read")
  if type(event_available) is not bool or not isinstance(mmap, dict) or not isinstance(read, dict):
    failures.append(f"{context}: malformed perf provider evidence")
    return {}
  architecture = target_triple.split("-", 1)[0] if isinstance(target_triple, str) else None
  linux_or_android = (
    isinstance(target_triple, str)
    and (
      "-unknown-linux-" in target_triple
      or target_triple.endswith("-linux-android")
      or target_triple.endswith("-linux-gnu")
      or target_triple.endswith("-linux-musl")
    )
  )
  expected_mmap_support = (
    linux_or_android
    and architecture in (
      "x86_64",
      "i686",
      "aarch64",
      "arm",
      "armv7",
      "riscv64gc",
      "riscv64",
    )
  ) if architecture is not None else mmap.get("supported_on_target")
  expected_read_support = (
    linux_or_android
    and architecture in (
      "x86_64", "i686", "aarch64", "arm", "armv7", "riscv64gc", "riscv64",
      "s390x", "loongarch64", "powerpc64", "powerpc64le",
    )
  ) if target_triple is not None else read.get("supported_on_target")
  if mmap.get("supported_on_target") is not expected_mmap_support:
    failures.append(f"{context}: perf-mmap target support classification changed")
  if read.get("supported_on_target") is not expected_read_support:
    failures.append(f"{context}: persistent perf-read target support classification changed")
  expected_mmap_cost = perf_thread_cpu_read_cost(target_triple)
  if mmap.get("read_cost") != expected_mmap_cost or read.get("read_cost") != "system call":
    failures.append(f"{context}: perf provider read-cost classification changed")

  counter = {}
  read_entry = {}
  path_replay = {}
  expected_mmap_candidates = []
  expected_read_candidates = []
  mmap_mechanism = None
  read_mechanism = None
  selected_path = "posix_thread_cpu"
  fallback_path = None
  if event_available:
    path_probe = perf.get("path_probe")
    read_probe = read.get("entry_probe")
    if not isinstance(path_probe, dict) or not isinstance(read_probe, dict):
      failures.append(f"{context}: available perf selector lacks path/read probes")
      return {}
    if capability_preferred:
      path_replay = validate_thread_cpu_capability_probe(context, path_probe, failures)
    else:
      path_replay = validate_thread_cpu_path_probe(context, path_probe, failures)
    read_entry = validate_thread_cpu_perf_read_entry_probe(context, read_probe, failures)
    if not path_replay or not read_entry:
      return {}
    selected_path = path_replay["selected"]["winner"]
    fallback_path = (
      path_replay["fallback"]["winner"]
      if path_replay.get("fallback") is not None
      else None
    )
    read_mechanism = f"linux_perf_read__{read_entry['winner']}"
    expected_read_candidates = read_entry["candidates"]
    mmap_available = path_probe["candidate_eligible"][1]
    if mmap.get("available") is not mmap_available:
      failures.append(f"{context}: perf-mmap capability evidence disagrees with path probe")
    if read.get("available") is not True:
      failures.append(f"{context}: open perf event lacks persistent read evidence")
    if mmap_available:
      counter_probe = mmap.get("counter_probe")
      if not isinstance(counter_probe, dict):
        failures.append(f"{context}: available perf-mmap path lacks counter evidence")
        return {}
      counter = validate_thread_cpu_perf_counter_probe(context, counter_probe, failures)
      if not counter:
        return {}
      mmap_mechanism = f"linux_perf_mmap__{counter['winner']}"
      expected_mmap_candidates = counter["candidates"]
    elif any(
      value not in (None, [], False)
      for value in (
        mmap.get("counter_probe"),
        mmap.get("selected_mechanism"),
        mmap.get("selected_candidate_benchmark"),
        mmap.get("eligible_benchmarks"),
      )
    ):
      failures.append(f"{context}: unavailable perf-mmap path contains candidate evidence")
  else:
    if (
      perf.get("path_probe") is not None
      or mmap.get("available") is not False
      or read.get("available") is not False
      or mmap.get("counter_probe") is not None
      or read.get("entry_probe") is not None
    ):
      failures.append(f"{context}: unavailable perf event contains measured provider evidence")

  expected_mmap_benchmark = (
    f"direct_thread_cpu__{mmap_mechanism}" if mmap_mechanism is not None else None
  )
  expected_read_benchmark = (
    f"direct_thread_cpu__{read_mechanism}" if read_mechanism is not None else None
  )
  expected_mmap_fields = {
    "selected_mechanism": mmap_mechanism,
    "selected_candidate_benchmark": expected_mmap_benchmark,
    "eligible_benchmarks": expected_mmap_candidates,
  }
  expected_read_fields = {
    "selected_mechanism": read_mechanism,
    "selected_candidate_benchmark": expected_read_benchmark,
    "eligible_benchmarks": expected_read_candidates,
  }
  if any(mmap.get(key) != value for key, value in expected_mmap_fields.items()):
    failures.append(f"{context}: perf-mmap exact candidate coverage does not reproduce")
  if any(read.get(key) != value for key, value in expected_read_fields.items()):
    failures.append(f"{context}: perf-read exact candidate coverage does not reproduce")

  expected_candidates = [
    *native["candidates"],
    *expected_mmap_candidates,
    *expected_read_candidates,
  ]
  if candidates != expected_candidates or len(candidates) != len(set(candidates)):
    failures.append(f"{context}: thread-CPU eligible candidate coverage is not exhaustive")

  if capability_preferred:
    expected_failure_fallback = {
      "preferred_provider": "linux_perf_mmap",
      "eligibility_gate": (
        "perf task-clock mmap exposes complete seqlock conversion metadata and a "
        "usable architectural counter"
      ),
      "fallback_provider": "posix_thread_cpu_clock",
      "fallback_mechanism": native["winner"],
      "trigger": (
        "perf event or mmap capability is unavailable, or an inline mmap read fails"
      ),
    }
    if selection.get("failure_fallback") != expected_failure_fallback:
      failures.append(f"{context}: malformed capability-preferred failure fallback")
  elif selection.get("failure_fallback") is not None:
    failures.append(f"{context}: measured thread-CPU selector declares a capability fallback")

  def identity(path: str) -> tuple[str, str, str]:
    if path == "linux_perf_mmap":
      if mmap_mechanism is None:
        raise ValueError("selected unavailable perf-mmap path")
      return ("linux_perf_mmap", mmap_mechanism, expected_mmap_cost)
    if path == "linux_perf_read":
      if read_mechanism is None:
        raise ValueError("selected unavailable perf-read path")
      return ("linux_perf_read", read_mechanism, "system call")
    if path == "posix_thread_cpu":
      return ("posix_thread_cpu_clock", native["winner"], "system call")
    raise ValueError("unknown thread-CPU path")

  try:
    selected_provider, selected_mechanism, selected_cost = identity(selected_path)
    fallback_identity = identity(fallback_path) if fallback_path is not None else None
  except ValueError:
    failures.append(f"{context}: selected thread-CPU path has no exact mechanism")
    return {}
  expected_selected = {
    "selected_provider": selected_provider,
    "selected_mechanism": selected_mechanism,
    "selected_read_cost": selected_cost,
    "selected_native_benchmark": f"direct_selected_thread_cpu__{selected_mechanism}",
  }
  if any(selection.get(key) != value for key, value in expected_selected.items()):
    failures.append(f"{context}: thread-CPU public/exact winner does not reproduce")
  if fallback_identity is None:
    expected_fallback = {
      "fallback_provider": None,
      "fallback_mechanism": None,
      "fallback_read_cost": None,
      "fallback_native_benchmark": None,
    }
  else:
    fallback_provider, fallback_mechanism, fallback_cost = fallback_identity
    expected_fallback = {
      "fallback_provider": fallback_provider,
      "fallback_mechanism": fallback_mechanism,
      "fallback_read_cost": fallback_cost,
      "fallback_native_benchmark": f"direct_fallback_thread_cpu__{fallback_mechanism}",
    }
  if any(selection.get(key) != value for key, value in expected_fallback.items()):
    failures.append(f"{context}: thread-CPU measured fallback does not reproduce")

  expected_clock = (
    "raw SYS_clock_gettime(CLOCK_MONOTONIC_RAW), never libc/vDSO or the candidate under test"
  )
  if perf.get("measurement_clock") != expected_clock:
    failures.append(f"{context}: perf selector measurement clock changed")
  if not isinstance(perf.get("decision_rule"), str) or not perf["decision_rule"]:
    failures.append(f"{context}: perf selector decision rule is missing")
  return {
    "winner": selected_provider,
    "selected_mechanism": selected_mechanism,
    "selected_read_cost": selected_cost,
    "fallback": fallback_identity,
    "native_entry": native,
    "perf_counter": counter,
    "perf_read_entry": read_entry,
    "perf_path": path_replay,
  }


def validate_fixed_native_thread_cpu_selector(
  context: str,
  selection: dict,
  failures: list[str],
  target_triple: str | None = None,
) -> dict:
  """Validate a fixed native current-thread CPU clock without a tournament."""
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
    failures.append(f"{context}: fixed-native thread-CPU identity changed")

  expected_fixed = {
    "candidate": mechanism,
    "supported_architectures": ["x86_64", "aarch64"],
    "native_primitive": "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)",
    "time_domain": "thread CPU",
  }
  fixed = selection.get("fixed_provider")
  if not isinstance(fixed, dict) or any(
    fixed.get(key) != value for key, value in expected_fixed.items()
  ) or not isinstance(fixed.get("selection_basis"), str) or not fixed["selection_basis"].strip():
    failures.append(f"{context}: fixed-native thread-CPU basis is incomplete")

  if not isinstance(selection.get("read_cost_basis"), str) or not selection["read_cost_basis"].strip():
    failures.append(f"{context}: fixed-native thread-CPU read-cost basis is missing")

  if selection.get("perf") is not None or selection.get("native_entry_probe") is not None:
    failures.append(f"{context}: fixed-native thread-CPU selector must not declare a perf tournament")
  if selection.get("failure_fallback") is not None:
    failures.append(f"{context}: fixed-native thread-CPU selector must not declare a fallback")

  public_exact = selection.get("public_exact_probe")
  if public_exact is None:
    public_exact_reproduction = {}
  elif not isinstance(public_exact, dict):
    failures.append(f"{context}: malformed fixed-native thread-CPU public/exact evidence")
    public_exact_reproduction = {}
  else:
    public_exact_reproduction = {
      metric: validate_wall_public_exact_probe(
        context, f"thread_cpu.{metric}", public_exact.get(metric), failures
      )
      for metric in METRICS
    }

  if target_triple is not None:
    architecture = target_triple.split("-", 1)[0]
    if target_triple not in {
      "x86_64-apple-darwin",
      "aarch64-apple-darwin",
    } or architecture not in expected_fixed["supported_architectures"]:
      failures.append(f"{context}: fixed-native thread-CPU selector used for unsupported target")

  return {
    "winner": "posix_thread_cpu_clock",
    "selected_mechanism": mechanism,
    "selected_read_cost": "system call",
    "fallback": None,
    "eligible_direct_candidates": [candidate],
    "public_exact": public_exact_reproduction,
  }


def validate_windows_thread_cpu_selector(
  context: str,
  selection: dict,
  failures: list[str],
  target_triple: str | None = None,
) -> dict:
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
    failures.append(f"{context}: fixed Windows thread-CPU identity changed")

  expected_guard = {
    "required_provider": "windows_thread_times",
    "required_read_cost": "system call",
    "stale_selection_removed_before_guard": True,
    "on_mismatch": "panic before thread-cpu-selection.json is written",
  }
  if selection.get("native_campaign_guard") != expected_guard:
    failures.append(f"{context}: Windows native campaign guard is incomplete")

  supported_architectures = ["x86", "x86_64", "aarch64"]
  expected_fixed = {
    "candidate": mechanism,
    "supported_architectures": supported_architectures,
    "selection_basis": (
      "GetThreadTimes is Windows' documented elapsed current-thread CPU timeline"
    ),
    "authority": WINDOWS_GET_THREAD_TIMES_AUTHORITY,
  }
  fixed = selection.get("fixed_provider")
  if fixed != expected_fixed:
    failures.append(f"{context}: malformed fixed Windows thread-CPU basis")

  if target_triple is not None:
    architecture = target_triple.split("-", 1)[0]
    normalized_architecture = "x86" if architecture == "i686" else architecture
    if (
      "-windows-" not in target_triple
      or normalized_architecture not in supported_architectures
    ):
      failures.append(
        f"{context}: fixed Windows thread-CPU selector used for unsupported target"
      )

  expected_failure_fallback = {
    "provider": "monotonic_wall_clock",
    "mechanism": "windows_selected_monotonic_wall_fallback",
    "read_cost": "system call",
    "time_domain": "monotonic wall fallback",
    "trigger": "GetThreadTimes(current-thread pseudo-handle) returns zero",
    "state_transition": "sticky process-wide fallback",
    "eligible_for_thread_cpu_speed_claim": False,
    "exact_route_measured": True,
    "exact_benchmark": (
      "direct_fallback_thread_cpu__windows_selected_monotonic_wall_fallback"
    ),
    "observed_as_public_provider_during_campaign": False,
    "campaign_behavior": (
      "an observed fallback aborts the native benchmark before extraction "
      "instead of emitting thread-CPU parity evidence"
    ),
  }
  failure_fallback = selection.get("failure_fallback")
  if (
    isinstance(failure_fallback, dict)
    and failure_fallback.get("observed_as_public_provider_during_campaign") is not False
  ):
    failures.append(
      f"{context}: observed Windows wall fallback cannot be thread-CPU speed evidence"
    )
  if failure_fallback != expected_failure_fallback:
    failures.append(f"{context}: Windows thread-CPU failure fallback is not explicit")

  expected_exclusions = {
    "query_thread_cycle_time": {
      "eligibility": "ineligible",
      "reason": (
        "implementation-dependent cycles cannot be converted to elapsed thread CPU time"
      ),
      "authority": WINDOWS_QUERY_THREAD_CYCLE_TIME_AUTHORITY,
    },
    "nt_query_information_thread": {
      "eligibility": "ineligible",
      "reason": (
        "the documented THREADINFOCLASS contract exposes no stable ThreadTimes class"
      ),
      "authority": WINDOWS_NT_QUERY_INFORMATION_THREAD_AUTHORITY,
    },
  }
  exclusions = selection.get("ineligible_direct_candidates")
  if exclusions != expected_exclusions:
    failures.append(f"{context}: Windows thread-CPU exclusions are incomplete")

  public_exact = selection.get("public_exact_probe")
  if not isinstance(public_exact, dict):
    failures.append(f"{context}: Windows thread-CPU selector lacks paired public/exact evidence")
    public_exact = {}
  public_exact_reproduction = {
    metric: validate_wall_public_exact_probe(
      context, f"thread_cpu.{metric}", public_exact.get(metric), failures
    )
    for metric in METRICS
  }

  return {
    "winner": "windows_thread_times",
    "selected_mechanism": mechanism,
    "selected_read_cost": "system call",
    "fallback": failure_fallback,
    "eligible_direct_candidates": [candidate],
    "ineligible_direct_candidates": exclusions if isinstance(exclusions, dict) else {},
    "public_exact": public_exact_reproduction,
  }


def validate_tournament(
  context: str,
  candidate_names: list[str],
  candidate_batches: list[list[int]],
  steps: list[dict],
  reads_per_batch: int,
  required_wins: int,
  selected_provider: str,
  failures: list[str],
) -> dict:
  if (
    not candidate_names
    or len(candidate_names) != len(candidate_batches)
    or len(set(candidate_names)) != len(candidate_names)
    or len(steps) != len(candidate_names) - 1
  ):
    failures.append(f"{context}: malformed or incomplete tournament")
    return {}
  samples = dict(zip(candidate_names, candidate_batches, strict=True))
  incumbent = candidate_names[0]
  decisions = []
  for index, step in enumerate(steps):
    challenger = candidate_names[index + 1]
    if not isinstance(step, dict):
      failures.append(f"{context}: malformed tournament step {index}")
      continue
    if step.get("incumbent") != incumbent or step.get("challenger") != challenger:
      failures.append(f"{context}: tournament step {index} does not follow candidate order")
      continue
    try:
      decision = reproduce_material_decision(
        samples[challenger], samples[incumbent], reads_per_batch, required_wins
      )
    except (TypeError, ValueError):
      failures.append(f"{context}: malformed tournament samples at step {index}")
      continue
    winner = challenger if decision["selected"] else incumbent
    expected = {
      "allowance_ns": decision["allowance_ns"],
      "decisive_wins": decision["decisive_wins"],
      "challenger_selected": decision["selected"],
      "winner": winner,
    }
    if any(step.get(key) != value for key, value in expected.items()):
      failures.append(f"{context}: tournament decision {index} does not reproduce")
    decision.update({"challenger": challenger, "incumbent": incumbent, "winner": winner})
    decisions.append(decision)
    incumbent = winner
  if incumbent != selected_provider:
    failures.append(
      f"{context}: reproduced tournament winner {incumbent!r} != {selected_provider!r}"
    )
  return {"winner": incumbent, "decisions": decisions}


def validate_residual_wall_metadata(
  context: str,
  selection: dict,
  clocks: dict,
  failures: list[str],
) -> tuple[dict, dict, dict] | None:
  selected = selection.get("selected_provider")
  candidates = selection.get("eligible_direct_candidates")
  selected_benchmarks = selection.get("selected_native_benchmark")
  probe = selection.get("probe")
  if not all(
    isinstance(value, dict)
    for value in (selected, candidates, selected_benchmarks, probe)
  ):
    failures.append(f"{context}: malformed residual wall selector metadata")
    return None
  for domain, direct_prefix, selected_prefix in (
    ("instant", "direct_wall", "direct_selected_wall"),
    ("ordered", "direct_ordered_wall", "direct_selected_ordered_wall"),
  ):
    provider = selected.get(domain)
    declared = candidates.get(domain)
    if (
      not isinstance(provider, str)
      or not isinstance(declared, list)
      or not declared
      or len(set(declared)) != len(declared)
      or not all(
        isinstance(candidate, str)
        and candidate.startswith(f"{direct_prefix}__")
        for candidate in declared
      )
    ):
      failures.append(f"{context}: malformed residual {domain} candidate set")
      continue
    if f"{direct_prefix}__{provider}" not in declared:
      failures.append(f"{context}: residual {domain} winner is not an eligible candidate")
    expected_selected = f"{selected_prefix}__{provider}"
    if selected_benchmarks.get(domain) != expected_selected:
      failures.append(f"{context}: residual {domain} selected benchmark is mislabeled")
    for candidate in declared:
      row = clocks.get(candidate)
      if not isinstance(row, dict):
        failures.append(f"{context}: residual selector lacks exact row {candidate}")
        continue
      validate_ci(context, candidate, row, failures)
  return selected, candidates, probe


def replay_residual_staged_domain(
  context: str,
  probe: dict,
  baseline_provider: str,
  baseline_batches_field: str,
  stages: list[tuple[str, str, str, bool]],
  selected_provider: str,
  failures: list[str],
) -> dict:
  reads = probe.get("reads_per_batch")
  required = probe.get("required_decisive_wins")
  if reads != 4096 or required != 8:
    failures.append(f"{context}: residual wall decision rule changed")
    return {}
  incumbent = baseline_provider
  incumbent_batches = probe.get(baseline_batches_field)
  decisions = []
  for challenger, batches_field, decision_prefix, eligible in stages:
    if not eligible:
      continue
    challenger_batches = probe.get(batches_field)
    try:
      decision = reproduce_material_decision(
        challenger_batches, incumbent_batches, reads, required
      )
    except (TypeError, ValueError):
      failures.append(f"{context}: malformed residual samples for {challenger}")
      continue
    if (
      probe.get(f"{decision_prefix}_allowance_ns") != decision["allowance_ns"]
      or probe.get(f"{decision_prefix}_decisive_wins")
      != decision["decisive_wins"]
    ):
      failures.append(f"{context}: residual decision for {challenger} does not reproduce")
    decision.update({"challenger": challenger, "incumbent": incumbent})
    if decision["selected"]:
      incumbent = challenger
      incumbent_batches = challenger_batches
    decision["winner"] = incumbent
    decisions.append(decision)
  if probe.get("selected_provider") != selected_provider:
    failures.append(f"{context}: residual probe and envelope winners disagree")
  if incumbent != selected_provider:
    failures.append(
      f"{context}: reproduced residual winner {incumbent!r} != {selected_provider!r}"
    )
  return {"winner": incumbent, "decisions": decisions}


def residual_declared_providers(candidates: dict, domain: str) -> list[str]:
  prefix = "direct_wall__" if domain == "instant" else "direct_ordered_wall__"
  return [candidate.removeprefix(prefix) for candidate in candidates.get(domain, [])]


def validate_loongarch_wall_selector(
  context: str,
  selected: dict,
  candidates: dict,
  probe: dict,
  failures: list[str],
) -> dict:
  output = {}
  for domain in ("instant", "ordered"):
    domain_probe = probe.get(domain)
    if not isinstance(domain_probe, dict):
      failures.append(f"{context}: malformed LoongArch {domain} wall probe")
      continue
    expected = [
      "loongarch_stable_counter",
      "linux_clock_monotonic",
      "linux_clock_monotonic_syscall",
    ]
    if domain_probe.get("clock_raw_available") is True:
      expected.append("linux_clock_monotonic_raw")
    if domain_probe.get("syscall_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_syscall")
    if domain_probe.get("vdso_available") is True:
      expected.append("linux_clock_monotonic_vdso_direct")
    if domain_probe.get("vdso_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_vdso_direct")
    if domain_probe.get("clock_boottime_available") is True:
      expected.append("linux_clock_boottime")
    if domain_probe.get("syscall_boottime_available") is True:
      expected.append("linux_clock_boottime_syscall")
    if domain_probe.get("vdso_boottime_available") is True:
      expected.append("linux_clock_boottime_vdso_direct")
    if residual_declared_providers(candidates, domain) != expected:
      failures.append(f"{context}: LoongArch {domain} candidate set is incomplete")
    if domain_probe.get("candidate_count") != len(expected):
      failures.append(f"{context}: LoongArch {domain} probe candidate count changed")
    stages = [
      (
        "linux_clock_monotonic_syscall",
        "syscall_batches_ns",
        "fallback",
        True,
      ),
      (
        "linux_clock_monotonic_raw",
        "clock_raw_batches_ns",
        "clock_raw",
        domain_probe.get("clock_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_syscall",
        "syscall_raw_batches_ns",
        "syscall_raw",
        domain_probe.get("syscall_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_vdso_direct",
        "vdso_batches_ns",
        "vdso",
        domain_probe.get("vdso_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_vdso_direct",
        "vdso_raw_batches_ns",
        "vdso_raw",
        domain_probe.get("vdso_raw_available") is True,
      ),
      (
        "linux_clock_boottime",
        "clock_boottime_batches_ns",
        "clock_boottime",
        domain_probe.get("clock_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_syscall",
        "syscall_boottime_batches_ns",
        "syscall_boottime",
        domain_probe.get("syscall_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_vdso_direct",
        "vdso_boottime_batches_ns",
        "vdso_boottime",
        domain_probe.get("vdso_boottime_available") is True,
      ),
      (
        "loongarch_stable_counter",
        "direct_batches_ns",
        "direct",
        True,
      ),
    ]
    output[domain] = replay_residual_staged_domain(
      f"{context}: LoongArch {domain}",
      domain_probe,
      "linux_clock_monotonic",
      "clock_batches_ns",
      stages,
      selected.get(domain),
      failures,
    )
  return output


def validate_riscv_wall_selector(
  context: str,
  selected: dict,
  candidates: dict,
  probe: dict,
  failures: list[str],
) -> dict:
  output = {}
  for domain in ("instant", "ordered"):
    domain_probe = probe.get(domain)
    if not isinstance(domain_probe, dict):
      failures.append(f"{context}: malformed RISC-V {domain} wall probe")
      continue
    expected = []
    if domain_probe.get("direct_eligible") is True:
      expected.append("riscv_rdtime")
    expected.extend(("linux_clock_monotonic", "linux_clock_monotonic_syscall"))
    if domain == "ordered":
      expected.append("linux_clock_monotonic_syscall_os_ordered")
    if domain_probe.get("vdso_available") is True:
      expected.append("linux_clock_monotonic_vdso_direct")
    if domain_probe.get("vdso_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_vdso_direct")
    if domain_probe.get("clock_raw_available") is True:
      expected.append("linux_clock_monotonic_raw")
    if domain_probe.get("syscall_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_syscall")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_raw_syscall_os_ordered")
    if domain_probe.get("clock_boottime_available") is True:
      expected.append("linux_clock_boottime")
    if domain_probe.get("syscall_boottime_available") is True:
      expected.append("linux_clock_boottime_syscall")
      if domain == "ordered":
        expected.append("linux_clock_boottime_syscall_os_ordered")
    if domain_probe.get("vdso_boottime_available") is True:
      expected.append("linux_clock_boottime_vdso_direct")
    if residual_declared_providers(candidates, domain) != expected:
      failures.append(f"{context}: RISC-V {domain} candidate set is incomplete")
    if domain_probe.get("candidate_count") != len(expected):
      failures.append(f"{context}: RISC-V {domain} probe candidate count changed")
    stages = [
      (
        "linux_clock_monotonic_syscall",
        "syscall_batches_ns",
        "fallback",
        True,
      ),
      (
        "linux_clock_monotonic_syscall_os_ordered",
        "syscall_os_ordered_batches_ns",
        "syscall_os_ordered",
        domain == "ordered",
      ),
      (
        "linux_clock_monotonic_raw",
        "clock_raw_batches_ns",
        "clock_raw",
        domain_probe.get("clock_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_syscall",
        "syscall_raw_batches_ns",
        "syscall_raw",
        domain_probe.get("syscall_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_syscall_os_ordered",
        "syscall_raw_os_ordered_batches_ns",
        "syscall_raw_os_ordered",
        domain == "ordered" and domain_probe.get("syscall_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_vdso_direct",
        "vdso_batches_ns",
        "vdso",
        domain_probe.get("vdso_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_vdso_direct",
        "vdso_raw_batches_ns",
        "vdso_raw",
        domain_probe.get("vdso_raw_available") is True,
      ),
      (
        "linux_clock_boottime",
        "clock_boottime_batches_ns",
        "clock_boottime",
        domain_probe.get("clock_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_syscall",
        "syscall_boottime_batches_ns",
        "syscall_boottime",
        domain_probe.get("syscall_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_syscall_os_ordered",
        "syscall_boottime_os_ordered_batches_ns",
        "syscall_boottime_os_ordered",
        domain == "ordered"
        and domain_probe.get("syscall_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_vdso_direct",
        "vdso_boottime_batches_ns",
        "vdso_boottime",
        domain_probe.get("vdso_boottime_available") is True,
      ),
      (
        "riscv_rdtime",
        "direct_batches_ns",
        "direct",
        domain_probe.get("direct_eligible") is True,
      ),
    ]
    output[domain] = replay_residual_staged_domain(
      f"{context}: RISC-V {domain}",
      domain_probe,
      "linux_clock_monotonic",
      "clock_batches_ns",
      stages,
      selected.get(domain),
      failures,
    )
  return output


def validate_power_wall_selector(
  context: str,
  selected: dict,
  candidates: dict,
  probe: dict,
  failures: list[str],
) -> dict:
  output = {}
  for domain in ("instant", "ordered"):
    domain_probe = probe.get(domain)
    if not isinstance(domain_probe, dict):
      failures.append(f"{context}: malformed Power {domain} wall probe")
      continue
    expected = ["power_timebase", "linux_clock_monotonic", "linux_clock_monotonic_sc"]
    if domain == "ordered":
      expected.append("linux_clock_monotonic_sc_os_ordered")
    if domain_probe.get("scv_eligible") is True:
      expected.append("linux_clock_monotonic_scv")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_scv_os_ordered")
    if domain_probe.get("clock_raw_available") is True:
      expected.append("linux_clock_monotonic_raw")
    if domain_probe.get("sc_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_sc")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_raw_sc_os_ordered")
    if domain_probe.get("scv_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_scv")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_raw_scv_os_ordered")
    if domain_probe.get("vdso_available") is True:
      expected.append("linux_clock_monotonic_vdso_direct")
    if domain_probe.get("vdso_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_vdso_direct")
    if domain_probe.get("clock_boottime_available") is True:
      expected.append("linux_clock_boottime")
    if domain_probe.get("sc_boottime_available") is True:
      expected.append("linux_clock_boottime_sc")
      if domain == "ordered":
        expected.append("linux_clock_boottime_sc_os_ordered")
    if domain_probe.get("scv_boottime_available") is True:
      expected.append("linux_clock_boottime_scv")
      if domain == "ordered":
        expected.append("linux_clock_boottime_scv_os_ordered")
    if domain_probe.get("vdso_boottime_available") is True:
      expected.append("linux_clock_boottime_vdso_direct")
    if residual_declared_providers(candidates, domain) != expected:
      failures.append(f"{context}: Power {domain} candidate set is incomplete")
    if domain_probe.get("candidate_count") != len(expected):
      failures.append(f"{context}: Power {domain} probe candidate count changed")
    stages = [
      ("linux_clock_monotonic_sc", "sc_batches_ns", "sc", True),
      (
        "linux_clock_monotonic_sc_os_ordered",
        "sc_os_ordered_batches_ns",
        "sc_os_ordered",
        domain == "ordered",
      ),
      (
        "linux_clock_monotonic_scv",
        "scv_batches_ns",
        "scv",
        domain_probe.get("scv_eligible") is True,
      ),
      (
        "linux_clock_monotonic_scv_os_ordered",
        "scv_os_ordered_batches_ns",
        "scv_os_ordered",
        domain == "ordered" and domain_probe.get("scv_eligible") is True,
      ),
      (
        "linux_clock_monotonic_raw",
        "clock_raw_batches_ns",
        "clock_raw",
        domain_probe.get("clock_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_sc",
        "sc_raw_batches_ns",
        "sc_raw",
        domain_probe.get("sc_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_sc_os_ordered",
        "sc_raw_os_ordered_batches_ns",
        "sc_raw_os_ordered",
        domain == "ordered" and domain_probe.get("sc_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_scv",
        "scv_raw_batches_ns",
        "scv_raw",
        domain_probe.get("scv_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_scv_os_ordered",
        "scv_raw_os_ordered_batches_ns",
        "scv_raw_os_ordered",
        domain == "ordered" and domain_probe.get("scv_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_vdso_direct",
        "vdso_batches_ns",
        "vdso",
        domain_probe.get("vdso_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_vdso_direct",
        "vdso_raw_batches_ns",
        "vdso_raw",
        domain_probe.get("vdso_raw_available") is True,
      ),
      (
        "linux_clock_boottime",
        "clock_boottime_batches_ns",
        "clock_boottime",
        domain_probe.get("clock_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_sc",
        "sc_boottime_batches_ns",
        "sc_boottime",
        domain_probe.get("sc_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_sc_os_ordered",
        "sc_boottime_os_ordered_batches_ns",
        "sc_boottime_os_ordered",
        domain == "ordered" and domain_probe.get("sc_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_scv",
        "scv_boottime_batches_ns",
        "scv_boottime",
        domain_probe.get("scv_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_scv_os_ordered",
        "scv_boottime_os_ordered_batches_ns",
        "scv_boottime_os_ordered",
        domain == "ordered" and domain_probe.get("scv_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_vdso_direct",
        "vdso_boottime_batches_ns",
        "vdso_boottime",
        domain_probe.get("vdso_boottime_available") is True,
      ),
      ("power_timebase", "direct_batches_ns", "direct", True),
    ]
    output[domain] = replay_residual_staged_domain(
      f"{context}: Power {domain}",
      domain_probe,
      "linux_clock_monotonic",
      "clock_batches_ns",
      stages,
      selected.get(domain),
      failures,
    )
  return output


def validate_linux_clock_wall_selector(
  context: str,
  selected: dict,
  candidates: dict,
  probe: dict,
  failures: list[str],
) -> dict:
  output = {}
  for domain in ("instant", "ordered"):
    domain_probe = probe.get(domain)
    if not isinstance(domain_probe, dict):
      failures.append(f"{context}: malformed Linux clock {domain} probe")
      continue
    expected = ["linux_clock_monotonic"]
    if domain_probe.get("arm_cntvct_available") is True:
      expected.append(
        "linux_arm_cntvct"
        if domain == "instant"
        else "linux_arm_dmb_ish_isb_cntvct"
      )
    if domain_probe.get("raw_available") is True:
      expected.append("linux_clock_monotonic_syscall")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_syscall_os_ordered")
    if domain_probe.get("vdso_available") is True:
      expected.append("linux_clock_monotonic_vdso_direct")
    if domain_probe.get("vdso_monotonic_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_vdso_direct")
    if domain_probe.get("vdso_time64_available") is True:
      expected.append("linux_clock_monotonic_vdso_time64_direct")
    if domain_probe.get("vdso_time64_monotonic_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_vdso_time64_direct")
    if domain_probe.get("raw_time64_available") is True:
      expected.append("linux_clock_monotonic_time64_syscall")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_time64_syscall_os_ordered")
    if domain_probe.get("libc_monotonic_raw_available") is True:
      expected.append("linux_clock_monotonic_raw")
    if domain_probe.get("raw_monotonic_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_syscall")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_raw_syscall_os_ordered")
    if domain_probe.get("raw_time64_monotonic_raw_available") is True:
      expected.append("linux_clock_monotonic_raw_time64_syscall")
      if domain == "ordered":
        expected.append("linux_clock_monotonic_raw_time64_syscall_os_ordered")
    if domain_probe.get("libc_boottime_available") is True:
      expected.append("linux_clock_boottime")
    if domain_probe.get("raw_boottime_available") is True:
      expected.append("linux_clock_boottime_syscall")
      if domain == "ordered":
        expected.append("linux_clock_boottime_syscall_os_ordered")
    if domain_probe.get("raw_time64_boottime_available") is True:
      expected.append("linux_clock_boottime_time64_syscall")
      if domain == "ordered":
        expected.append("linux_clock_boottime_time64_syscall_os_ordered")
    if domain_probe.get("vdso_boottime_available") is True:
      expected.append("linux_clock_boottime_vdso_direct")
    if domain_probe.get("vdso_time64_boottime_available") is True:
      expected.append("linux_clock_boottime_vdso_time64_direct")
    if residual_declared_providers(candidates, domain) != expected:
      failures.append(f"{context}: Linux clock {domain} candidate set is incomplete")
    if domain_probe.get("candidate_count") != len(expected):
      failures.append(f"{context}: Linux clock {domain} probe candidate count changed")
    if domain_probe.get("s390_bare_stckf_eligible") is not False:
      failures.append(f"{context}: Linux clock {domain} exposed private bare STCKF")
    if not isinstance(domain_probe.get("s390_bare_stckf_exclusion"), str):
      failures.append(f"{context}: Linux clock {domain} lacks its STCKF exclusion")
    stages = [
      (
        "linux_clock_monotonic_syscall",
        "raw_batches_ns",
        "raw",
        domain_probe.get("raw_available") is True,
      ),
      (
        "linux_clock_monotonic_syscall_os_ordered",
        "raw_os_ordered_batches_ns",
        "raw_os_ordered",
        domain == "ordered" and domain_probe.get("raw_available") is True,
      ),
      (
        "linux_clock_monotonic_time64_syscall",
        "raw_time64_batches_ns",
        "raw_time64",
        domain_probe.get("raw_time64_available") is True,
      ),
      (
        "linux_clock_monotonic_time64_syscall_os_ordered",
        "raw_time64_os_ordered_batches_ns",
        "raw_time64_os_ordered",
        domain == "ordered" and domain_probe.get("raw_time64_available") is True,
      ),
      (
        "linux_clock_monotonic_raw",
        "libc_monotonic_raw_batches_ns",
        "libc_monotonic_raw",
        domain_probe.get("libc_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_syscall",
        "raw_monotonic_raw_batches_ns",
        "raw_monotonic_raw",
        domain_probe.get("raw_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_syscall_os_ordered",
        "raw_monotonic_raw_os_ordered_batches_ns",
        "raw_monotonic_raw_os_ordered",
        domain == "ordered"
        and domain_probe.get("raw_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_time64_syscall",
        "raw_time64_monotonic_raw_batches_ns",
        "raw_time64_monotonic_raw",
        domain_probe.get("raw_time64_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_time64_syscall_os_ordered",
        "raw_time64_monotonic_raw_os_ordered_batches_ns",
        "raw_time64_monotonic_raw_os_ordered",
        domain == "ordered"
        and domain_probe.get("raw_time64_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_vdso_direct",
        "vdso_batches_ns",
        "vdso",
        domain_probe.get("vdso_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_vdso_direct",
        "vdso_monotonic_raw_batches_ns",
        "vdso_monotonic_raw",
        domain_probe.get("vdso_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_monotonic_vdso_time64_direct",
        "vdso_time64_batches_ns",
        "vdso_time64",
        domain_probe.get("vdso_time64_available") is True,
      ),
      (
        "linux_clock_monotonic_raw_vdso_time64_direct",
        "vdso_time64_monotonic_raw_batches_ns",
        "vdso_time64_monotonic_raw",
        domain_probe.get("vdso_time64_monotonic_raw_available") is True,
      ),
      (
        "linux_clock_boottime",
        "libc_boottime_batches_ns",
        "libc_boottime",
        domain_probe.get("libc_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_syscall",
        "raw_boottime_batches_ns",
        "raw_boottime",
        domain_probe.get("raw_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_syscall_os_ordered",
        "raw_boottime_os_ordered_batches_ns",
        "raw_boottime_os_ordered",
        domain == "ordered" and domain_probe.get("raw_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_time64_syscall",
        "raw_time64_boottime_batches_ns",
        "raw_time64_boottime",
        domain_probe.get("raw_time64_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_time64_syscall_os_ordered",
        "raw_time64_boottime_os_ordered_batches_ns",
        "raw_time64_boottime_os_ordered",
        domain == "ordered"
        and domain_probe.get("raw_time64_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_vdso_direct",
        "vdso_boottime_batches_ns",
        "vdso_boottime",
        domain_probe.get("vdso_boottime_available") is True,
      ),
      (
        "linux_clock_boottime_vdso_time64_direct",
        "vdso_time64_boottime_batches_ns",
        "vdso_time64_boottime",
        domain_probe.get("vdso_time64_boottime_available") is True,
      ),
      (
        "linux_arm_cntvct"
        if domain == "instant"
        else "linux_arm_dmb_ish_isb_cntvct",
        "cntvct_batches_ns",
        "cntvct",
        domain_probe.get("arm_cntvct_available") is True,
      ),
    ]
    output[domain] = replay_residual_staged_domain(
      f"{context}: Linux clock {domain}",
      domain_probe,
      "linux_clock_monotonic",
      "libc_batches_ns",
      stages,
      selected.get(domain),
      failures,
    )
  return output


def validate_freebsd_barrier_tournament(
  context: str,
  probe: dict,
  kind: str,
  failures: list[str],
) -> dict:
  count_key = (
    "ordered_barrier_candidate_count"
    if kind == "clock"
    else "ordered_syscall_barrier_candidate_count"
  )
  names_key = (
    "ordered_barrier_candidate_names"
    if kind == "clock"
    else "ordered_syscall_barrier_candidate_names"
  )
  count = probe.get(count_key)
  names = probe.get(names_key)
  batches = probe.get(f"ordered_{kind}_barrier_candidate_batches_ns")
  medians = probe.get(f"ordered_{kind}_barrier_candidate_medians_ns")
  decision_count = probe.get(f"ordered_{kind}_barrier_decision_count")
  if (
    type(count) is not int
    or count <= 0
    or not isinstance(names, list)
    or not isinstance(batches, list)
    or not isinstance(medians, list)
    or count > min(len(names), len(batches), len(medians))
    or decision_count != count - 1
  ):
    failures.append(f"{context}: malformed FreeBSD {kind} barrier candidates")
    return {}
  names = names[:count]
  batches = batches[:count]
  for name, samples, recorded_median in zip(names, batches, medians[:count], strict=True):
    if isinstance(samples, list) and len(samples) == 9:
      if recorded_median != int(statistics.median(samples)):
        failures.append(f"{context}: FreeBSD {kind} barrier median changed for {name}")
  steps = [
    {
      "challenger": challenger,
      "incumbent": incumbent,
      "winner": winner,
      "allowance_ns": allowance,
      "decisive_wins": wins,
      "challenger_selected": challenger_selected,
    }
    for challenger, incumbent, winner, allowance, wins, challenger_selected in zip(
      probe.get(f"ordered_{kind}_barrier_challengers", [])[:decision_count],
      probe.get(f"ordered_{kind}_barrier_incumbents", [])[:decision_count],
      probe.get(f"ordered_{kind}_barrier_winners", [])[:decision_count],
      probe.get(f"ordered_{kind}_barrier_allowances_ns", [])[:decision_count],
      probe.get(f"ordered_{kind}_barrier_tournament_decisive_wins", [])[:decision_count],
      probe.get(f"ordered_{kind}_barrier_challenger_selected", [])[:decision_count],
      strict=True,
    )
  ]
  return validate_tournament(
    f"{context}: FreeBSD {kind} barrier",
    names,
    batches,
    steps,
    probe.get("ordered", {}).get("reads_per_batch"),
    probe.get("ordered", {}).get("required_decisive_wins"),
    steps[-1]["winner"] if steps else names[0],
    failures,
  )


def validate_freebsd_wall_selector(
  context: str,
  selected: dict,
  candidates: dict,
  probe: dict,
  failures: list[str],
) -> dict:
  instant = probe.get("instant")
  ordered = probe.get("ordered")
  if not isinstance(instant, dict) or not isinstance(ordered, dict):
    failures.append(f"{context}: malformed FreeBSD wall probe")
    return {}

  instant_expected = ["freebsd_clock_monotonic", "freebsd_clock_monotonic_syscall"]
  if instant.get("timekeep_available") is True:
    instant_expected.append("freebsd_at_timekeep")
  if instant.get("direct_eligible") is True:
    instant_expected.append("freebsd_kernel_eligible_tsc")
  if residual_declared_providers(candidates, "instant") != instant_expected:
    failures.append(f"{context}: FreeBSD Instant candidate set is incomplete")
  if instant.get("candidate_count") != len(instant_expected):
    failures.append(f"{context}: FreeBSD Instant probe candidate count changed")
  instant_result = replay_residual_staged_domain(
    f"{context}: FreeBSD Instant",
    instant,
    "freebsd_clock_monotonic",
    "clock_batches_ns",
    [
      (
        "freebsd_clock_monotonic_syscall",
        "syscall_batches_ns",
        "fallback",
        True,
      ),
      (
        "freebsd_at_timekeep",
        "timekeep_batches_ns",
        "timekeep",
        instant.get("timekeep_available") is True,
      ),
      (
        "freebsd_kernel_eligible_tsc",
        "direct_batches_ns",
        "direct",
        instant.get("direct_eligible") is True,
      ),
    ],
    selected.get("instant"),
    failures,
  )

  clock_barriers = validate_freebsd_barrier_tournament(
    context, probe, "clock", failures
  )
  syscall_barriers = validate_freebsd_barrier_tournament(
    context, probe, "syscall", failures
  )
  if not clock_barriers or not syscall_barriers:
    return {"instant": instant_result}
  clock_provider = f"freebsd_clock_monotonic_{clock_barriers['winner']}"
  syscall_provider = f"freebsd_clock_monotonic_syscall_{syscall_barriers['winner']}"
  declared_ordered = residual_declared_providers(candidates, "ordered")
  tsc_candidates = [
    provider
    for provider in declared_ordered
    if provider.startswith("freebsd_kernel_eligible_tsc_x86_")
  ]
  if ordered.get("direct_eligible") is True:
    if len(tsc_candidates) != 1:
      failures.append(f"{context}: FreeBSD Ordered TSC identity is incomplete")
      tsc_provider = "freebsd_kernel_eligible_tsc_x86_cpuid_rdtsc"
    else:
      tsc_provider = tsc_candidates[0]
  else:
    if tsc_candidates:
      failures.append(f"{context}: FreeBSD Ordered retained an ineligible TSC")
    tsc_provider = "freebsd_kernel_eligible_tsc_x86_cpuid_rdtsc"
  expected_ordered = ([] if ordered.get("direct_eligible") is not True else [tsc_provider])
  if ordered.get("timekeep_available") is True:
    expected_ordered.insert(0, "freebsd_at_timekeep_os_owned")
  expected_ordered.extend(
    f"freebsd_clock_monotonic_{barrier}"
    for barrier in probe.get("ordered_barrier_candidate_names", [])[
      : probe.get("ordered_barrier_candidate_count", 0)
    ]
  )
  expected_ordered.extend(
    f"freebsd_clock_monotonic_syscall_{barrier}"
    for barrier in probe.get("ordered_syscall_barrier_candidate_names", [])[
      : probe.get("ordered_syscall_barrier_candidate_count", 0)
    ]
  )
  if declared_ordered != expected_ordered:
    failures.append(f"{context}: FreeBSD Ordered candidate set is incomplete")
  expected_outer_count = (
    2
    + int(ordered.get("timekeep_available") is True)
    + int(ordered.get("direct_eligible") is True)
  )
  if ordered.get("candidate_count") != expected_outer_count:
    failures.append(f"{context}: FreeBSD Ordered probe candidate count changed")
  ordered_result = replay_residual_staged_domain(
    f"{context}: FreeBSD Ordered",
    ordered,
    clock_provider,
    "clock_batches_ns",
    [
      (syscall_provider, "syscall_batches_ns", "fallback", True),
      (
        "freebsd_at_timekeep_os_owned",
        "timekeep_batches_ns",
        "timekeep",
        ordered.get("timekeep_available") is True,
      ),
      (
        tsc_provider,
        "direct_batches_ns",
        "direct",
        ordered.get("direct_eligible") is True,
      ),
    ],
    selected.get("ordered"),
    failures,
  )
  outer_decisions = ordered_result.get("decisions", [])
  syscall_selected = bool(outer_decisions and outer_decisions[0].get("selected"))
  fallback_winner = (
    syscall_barriers["winner"] if syscall_selected else clock_barriers["winner"]
  )
  if probe.get("ordered_os_barrier") != fallback_winner:
    failures.append(f"{context}: FreeBSD complete Ordered fallback identity changed")
  return {
    "instant": instant_result,
    "ordered": ordered_result,
    "clock_barrier": clock_barriers,
    "syscall_barrier": syscall_barriers,
  }


def validate_residual_wall_selector(
  context: str,
  selection: dict,
  clocks: dict,
  failures: list[str],
) -> dict:
  metadata = validate_residual_wall_metadata(context, selection, clocks, failures)
  if metadata is None:
    return {}
  selected, candidates, probe = metadata
  architecture = selection.get("architecture")
  if architecture == "wasm32-host":
    return validate_wasm_wall_selector(
      context,
      selected,
      candidates,
      probe,
      failures,
      selection.get("probe_observations"),
    )
  if architecture == "emscripten-host":
    return validate_emscripten_wall_selector(
      context,
      selected,
      candidates,
      probe,
      failures,
      selection.get("probe_observations"),
    )
  if architecture in ("armv7-linux", "s390x-linux"):
    return validate_linux_clock_wall_selector(
      context, selected, candidates, probe, failures
    )
  if architecture == "riscv64-linux":
    return validate_riscv_wall_selector(context, selected, candidates, probe, failures)
  if architecture == "loongarch64-linux":
    return validate_loongarch_wall_selector(context, selected, candidates, probe, failures)
  if architecture == "powerpc64-linux-gnu":
    return validate_power_wall_selector(context, selected, candidates, probe, failures)
  if architecture == "x86_64-freebsd":
    result = validate_freebsd_wall_selector(context, selected, candidates, probe, failures)
    public_exact = selection.get("public_exact_probe")
    if not isinstance(public_exact, dict):
      failures.append(f"{context}: FreeBSD selector lacks paired public/exact evidence")
      public_exact = {}
    for domain in ("instant", "ordered"):
      domain_result = result.get(domain)
      domain_public_exact = public_exact.get(domain)
      if not isinstance(domain_result, dict):
        continue
      if not isinstance(domain_public_exact, dict):
        failures.append(f"{context}: FreeBSD {domain} lacks metric parity evidence")
        domain_public_exact = {}
      domain_result["public_exact_contract"] = FREEBSD_PUBLIC_EXACT_CONTRACT
      domain_result["public_exact"] = {
        metric: validate_wall_public_exact_probe(
          context,
          f"{domain}.{metric}",
          domain_public_exact.get(metric),
          failures,
          material_loss_is_failure=False,
        )
        for metric in METRICS
      }
    return result
  failures.append(f"{context}: unknown residual wall architecture {architecture!r}")
  return {}


def validate_wasm_wall_selector(
  context: str,
  selected: dict,
  candidates: dict,
  probe: dict,
  failures: list[str],
  probe_observations: object = None,
) -> dict:
  if probe_observations is None:
    observations = [probe]
  elif (
    not isinstance(probe_observations, list)
    or len(probe_observations) != LAMBDA_INVOCATIONS
    or probe_observations[0] != probe
    or not all(isinstance(value, dict) for value in probe_observations)
  ):
    failures.append(f"{context}: malformed Wasm wall probe observations")
    return {}
  else:
    observations = probe_observations

  output = {"observations": []}
  for observation_index, observed_probe in enumerate(observations, start=1):
    reads = observed_probe.get("reads_per_batch")
    required = observed_probe.get("required_decisive_wins")
    if reads != 4_096 or required != 8:
      failures.append(
        f"{context}: Wasm wall decision rule changed in observation {observation_index}"
      )
      continue
    observation_output = {}
    for domain, direct_prefix in (
      ("instant", "direct_wall"),
      ("ordered", "direct_ordered_wall"),
    ):
      domain_probe = observed_probe.get(domain)
      declared = candidates.get(domain)
      if not isinstance(domain_probe, dict) or not isinstance(declared, list):
        failures.append(f"{context}: malformed Wasm {domain} selector evidence")
        continue
      expected_candidates = {
        f"{direct_prefix}__performance.now": "performance.now",
        f"{direct_prefix}__process.hrtime.bigint": "process.hrtime.bigint",
      }
      if (
        not declared
        or len(declared) != len(set(declared))
        or not set(declared).issubset(expected_candidates)
      ):
        failures.append(f"{context}: Wasm {domain} candidate set is invalid")
        continue
      expected_selected = expected_candidates.get(declared[0])
      performance_key = f"{direct_prefix}__performance.now"
      hrtime_key = f"{direct_prefix}__process.hrtime.bigint"
      performance_batches = domain_probe.get("performance_batches_ns")
      hrtime_batches = domain_probe.get("hrtime_batches_ns")
      both_eligible = performance_key in declared and hrtime_key in declared
      decision = None
      if both_eligible:
        try:
          decision = reproduce_material_decision(
            hrtime_batches,
            performance_batches,
            reads,
            required,
          )
        except (TypeError, ValueError):
          failures.append(f"{context}: malformed Wasm {domain} paired samples")
          continue
        expected_selected = (
          "process.hrtime.bigint" if decision["selected"] else "performance.now"
        )
        reported = {
          "performance_median_ns": decision["incumbent_median_ns"],
          "hrtime_median_ns": decision["challenger_median_ns"],
          "allowance_ns": decision["allowance_ns"],
          "hrtime_decisive_wins": decision["decisive_wins"],
        }
        for field, expected in reported.items():
          if domain_probe.get(field) != expected:
            failures.append(
              f"{context}: Wasm {domain} {field} does not reproduce"
            )
      elif any(
        domain_probe.get(field) not in (0, [0] * 9)
        for field in (
          "performance_median_ns",
          "hrtime_median_ns",
          "performance_batches_ns",
          "hrtime_batches_ns",
          "allowance_ns",
          "hrtime_decisive_wins",
        )
      ):
        failures.append(
          f"{context}: Wasm {domain} single-candidate route retained tournament data"
        )
      if selected.get(domain) != expected_selected:
        failures.append(
          f"{context}: Wasm {domain} selected provider does not reproduce"
        )
      observation_output[domain] = {
        "winner": expected_selected,
        "decision": decision,
      }
    output["observations"].append(observation_output)
  if len(output["observations"]) == 1:
    output.update(output["observations"][0])
  else:
    for domain in ("instant", "ordered"):
      domain_observations = [
        value[domain]
        for value in output["observations"]
        if domain in value
      ]
      output[domain] = {
        "winner": selected.get(domain),
        "observations": domain_observations,
      }
  return output


def validate_emscripten_wall_selector(
  context: str,
  selected: dict,
  candidates: dict,
  probe: dict,
  failures: list[str],
  probe_observations: object = None,
) -> dict:
  if probe_observations is None:
    observations = [probe]
  elif (
    not isinstance(probe_observations, list)
    or len(probe_observations) != LAMBDA_INVOCATIONS
    or probe_observations[0] != probe
    or not all(isinstance(value, dict) for value in probe_observations)
  ):
    failures.append(f"{context}: malformed Emscripten wall probe observations")
    return {}
  else:
    observations = probe_observations

  output = {"observations": []}
  for observation_index, observed_probe in enumerate(observations, start=1):
    reads = observed_probe.get("reads_per_batch")
    required = observed_probe.get("required_decisive_wins")
    if reads != 4_096 or required != 8:
      failures.append(
        f"{context}: Emscripten wall decision rule changed in observation "
        f"{observation_index}"
      )
      continue
    observation_output = {}
    for domain, direct_prefix in (
      ("instant", "direct_wall"),
      ("ordered", "direct_ordered_wall"),
    ):
      domain_probe = observed_probe.get(domain)
      declared = candidates.get(domain)
      if not isinstance(domain_probe, dict) or not isinstance(declared, list):
        failures.append(f"{context}: malformed Emscripten {domain} selector evidence")
        continue
      pthread_ordered = domain == "ordered" and "epoch_eligible" in domain_probe
      if pthread_ordered:
        candidate_map = {
          f"{direct_prefix}__emscripten_performance_epoch_atomic_max": (
            "emscripten_performance_epoch_atomic_max",
            domain_probe.get("epoch_eligible"),
          ),
          f"{direct_prefix}__emscripten_get_now_atomic_max": (
            "emscripten_get_now_atomic_max",
            domain_probe.get("get_now_eligible"),
          ),
        }
        incumbent_samples = domain_probe.get("epoch_batches_ns")
        challenger_samples = domain_probe.get("get_now_batches_ns")
        incumbent_provider = "emscripten_performance_epoch_atomic_max"
        challenger_provider = "emscripten_get_now_atomic_max"
        decisive_wins_field = "get_now_decisive_wins"
        zero_fields = (
          "epoch_batches_ns",
          "get_now_batches_ns",
          "allowance_ns",
          decisive_wins_field,
        )
        if (
          domain_probe.get("shared_memory") is not True
          or domain_probe.get("pthread_build") is not True
          or type(domain_probe.get("get_now_offset_ns")) is not int
        ):
          failures.append(f"{context}: malformed Emscripten pthread Ordered substrate")
      else:
        candidate_map = {
          f"{direct_prefix}__performance.now": (
            "performance.now",
            domain_probe.get("performance_eligible"),
          ),
          f"{direct_prefix}__process.hrtime.bigint": (
            "process.hrtime.bigint",
            domain_probe.get("hrtime_eligible"),
          ),
        }
        incumbent_samples = domain_probe.get("performance_batches_ns")
        challenger_samples = domain_probe.get("hrtime_batches_ns")
        incumbent_provider = "performance.now"
        challenger_provider = "process.hrtime.bigint"
        decisive_wins_field = "hrtime_decisive_wins"
        zero_fields = (
          "performance_batches_ns",
          "hrtime_batches_ns",
          "allowance_ns",
          decisive_wins_field,
        )
      expected_candidates = [
        name for name, (_, eligible) in candidate_map.items() if eligible is True
      ]
      if any(eligible not in (True, False) for _, eligible in candidate_map.values()):
        failures.append(f"{context}: malformed Emscripten {domain} eligibility")
        continue
      if declared != expected_candidates or not declared:
        failures.append(f"{context}: Emscripten {domain} candidate set is incomplete")
        continue
      expected_selected = candidate_map[declared[0]][0]
      decision = None
      if len(declared) == 2:
        try:
          decision = reproduce_material_decision(
            challenger_samples,
            incumbent_samples,
            reads,
            required,
          )
        except (TypeError, ValueError):
          failures.append(f"{context}: malformed Emscripten {domain} paired samples")
          continue
        expected_selected = challenger_provider if decision["selected"] else incumbent_provider
        if domain_probe.get("allowance_ns") != decision["allowance_ns"]:
          failures.append(f"{context}: Emscripten {domain} allowance does not reproduce")
        if domain_probe.get(decisive_wins_field) != decision["decisive_wins"]:
          failures.append(
            f"{context}: Emscripten {domain} decisive wins do not reproduce"
          )
      elif any(
        domain_probe.get(field) not in (0, [0] * 9)
        for field in zero_fields
      ):
        failures.append(
          f"{context}: Emscripten {domain} single-candidate route retained tournament data"
        )
      if selected.get(domain) != expected_selected:
        failures.append(
          f"{context}: Emscripten {domain} selected provider does not reproduce"
        )
      observation_output[domain] = {
        "winner": expected_selected,
        "decision": decision,
      }
    output["observations"].append(observation_output)

  for domain in ("instant", "ordered"):
    domain_observations = [value[domain] for value in output["observations"] if domain in value]
    if len(output["observations"]) == 1 and domain_observations:
      output[domain] = domain_observations[0]
    else:
      output[domain] = {
        "winner": selected.get(domain),
        "observations": domain_observations,
      }
  return output


def lambda_median_with_ci(samples: list[float], seed: str) -> tuple[float, list[float]]:
  point = statistics.median(samples)
  rng = random.Random(seed)
  bootstrap = sorted(
    statistics.median(rng.choices(samples, k=len(samples)))
    for _ in range(LAMBDA_BOOTSTRAP_SAMPLES)
  )
  return point, [bootstrap[124], bootstrap[4_874]]


def validate_lambda_samples(context: str, clocks: dict, failures: list[str]) -> None:
  for clock, entry in clocks.items():
    if clock not in {
      "tach", "tach_ordered", "quanta", "fastant", "minstant", "std",
      "tach_thread_cpu", "native_thread_cpu", "direct_selected_wall",
      "direct_selected_ordered_wall", "direct_selected_thread_cpu",
      "direct_fallback_thread_cpu",
    } and not clock.startswith((
      "direct_thread_cpu__",
      "direct_wall__",
      "direct_ordered_wall__",
    )):
      continue
    if not isinstance(entry, dict):
      failures.append(f"{context} {clock}: malformed clock evidence")
      continue
    for metric in METRICS:
      samples = entry.get(f"{metric}_samples")
      if (
        not isinstance(samples, list)
        or len(samples) != LAMBDA_SAMPLE_COUNT
        or not all(finite_number(sample) and sample >= 0 for sample in samples)
      ):
        failures.append(
          f"{context} {clock}: expected {LAMBDA_SAMPLE_COUNT} finite {metric} samples"
        )
        continue
      point, interval = lambda_median_with_ci(samples, f"{clock}:{metric}")
      if entry.get(metric) != point or entry.get(f"{metric}_ci95") != interval:
        failures.append(f"{context} {clock}: {metric} aggregate does not reproduce")


def git_command(root: Path, *args: str) -> subprocess.CompletedProcess[str]:
  return subprocess.run(
    ["git", "--no-replace-objects", *args],
    cwd=root,
    capture_output=True,
    text=True,
    check=False,
  )


def validate_checkout_binding(root: Path, revision: str) -> tuple[list[str], dict]:
  failures = []
  root = root.resolve()
  commit = git_command(root, "cat-file", "-e", f"{revision}^{{commit}}")
  if commit.returncode:
    return [f"campaign source commit is unavailable: {revision}"], {}

  ancestor = git_command(root, "merge-base", "--is-ancestor", revision, "HEAD")
  if ancestor.returncode:
    failures.append("campaign source revision is not an ancestor of HEAD")

  head_result = git_command(root, "rev-parse", "HEAD")
  head = head_result.stdout.strip() if head_result.returncode == 0 else None

  # First compare two committed trees. This keeps a later evidence-only commit
  # valid while requiring every executable, dependency lock, harness, runner,
  # extractor, and claim rule to be byte-identical to the measured revision.
  # A source file cannot pin its own eventual commit hash; the four generated
  # cell documents are the authority for `revision`.
  committed_diff = git_command(
    root,
    "diff",
    "--name-only",
    "--no-renames",
    revision,
    "HEAD",
    "--",
    *BENCHMARK_SOURCE_PATHS,
  )
  if committed_diff.returncode:
    committed_inputs_unchanged = False
    committed_changes = []
    failures.append("could not compare committed benchmark inputs with the campaign revision")
  else:
    committed_changes = [line for line in committed_diff.stdout.splitlines() if line]
    committed_inputs_unchanged = not committed_changes
    if committed_changes:
      failures.append(
        "committed benchmark inputs changed since the campaign: "
        + ", ".join(committed_changes)
      )

  # Then audit the index, worktree, and untracked files. Comparing only the
  # campaign commit with a dirty worktree can hide which side of the binding
  # failed; keeping the committed-tree and local-state checks separate makes
  # both failure modes explicit.
  working_status = git_command(
    root,
    "status",
    "--porcelain=v1",
    "--untracked-files=all",
    "--",
    *BENCHMARK_SOURCE_PATHS,
  )
  if working_status.returncode:
    working_tree_clean = False
    working_changes = []
    failures.append("could not inspect local benchmark inputs")
  else:
    working_changes = [line for line in working_status.stdout.splitlines() if line]
    working_tree_clean = not working_changes
    if working_changes:
      failures.append("local benchmark inputs differ from HEAD: " + ", ".join(working_changes))

  return failures, {
    "campaign_revision": revision,
    "checkout_head": head,
    "committed_tree_inputs_unchanged": committed_inputs_unchanged,
    "committed_tree_changes": committed_changes,
    "working_tree_inputs_clean": working_tree_clean,
    "working_tree_changes": working_changes,
  }


def _wall_family_for_triple(triple: object) -> str | None:
  """Map a target triple to its fixed-pick wall family (see EXPECTED_WALL_PICKS)."""
  if not isinstance(triple, str):
    return None
  if triple.endswith("-apple-darwin"):
    # Only aarch64 Apple converts to a fixed pick; x86 Apple stays a tournament.
    return "apple" if triple.startswith("aarch64") else None
  if "freebsd" in triple:
    return "freebsd"
  if "windows" in triple:
    return "windows"
  if "linux" in triple or "android" in triple:
    if triple.startswith("aarch64"):
      return "linux-aarch64"
    if triple.startswith(("x86_64", "i686", "i586", "x86")):
      return "linux-x86"
  return None


def expected_wall_picks_for_triple(triple: object) -> dict:
  """Frozen mode-legal picks for a triple's family, or the cross-family union.

  Falling back to the union keeps the assertion meaningful for an unrecognized
  triple: the pick must still be some family's mode-legal provider, and no
  family's `ordered` set admits a bare unbarriered counter (ADR-0006).
  """
  family = _wall_family_for_triple(triple)
  if family is not None:
    return EXPECTED_WALL_PICKS[family]
  return {
    "instant": frozenset().union(*(picks["instant"] for picks in EXPECTED_WALL_PICKS.values())),
    "ordered": frozenset().union(*(picks["ordered"] for picks in EXPECTED_WALL_PICKS.values())),
  }


def is_fixed_pick_wall_selection(selection: object) -> bool:
  """True for a converted wall selection: one capability-gated pick per contract.

  The emitters in ``benches/instant.rs`` write ``selected_provider``,
  ``selected_native_benchmark``, and ``public_exact_probe`` with no
  ``eligible_direct_candidates`` and no runtime ``probe``/``instant_probe``
  candidate arrays. Every old runtime-tournament shape carries one of those, so
  their presence excludes the fixed-pick shape.
  """
  if not isinstance(selection, dict):
    return False
  if any(key in selection for key in ("eligible_direct_candidates", "probe", "instant_probe")):
    return False
  return all(
    isinstance(selection.get(key), dict)
    for key in ("selected_provider", "selected_native_benchmark", "public_exact_probe")
  )


def validate_fixed_pick_wall_selector(
  context: str,
  selection: dict,
  failures: list[str],
) -> dict:
  """Validate a converted fixed-pick wall selection and reproduce its parity.

  The frozen-pick assertion (``selected_provider`` equals a mode-legal pick for
  the cell's family) belongs to the caller that knows the target triple:
  ``validate_cell`` for primary cells and
  ``_validate_supplemental_selection_profile`` for supplemental cells. Here we
  validate the minimal shape and replay the paired public/selected-exact probe so
  ``wall_public_exact_metric`` still feeds the selected-native comparison.
  """
  selected = selection.get("selected_provider")
  selected_benchmarks = selection.get("selected_native_benchmark")
  public_exact = selection.get("public_exact_probe")
  if not all(isinstance(value, dict) for value in (selected, selected_benchmarks, public_exact)):
    failures.append(f"{context}: malformed fixed-pick wall selector metadata")
    return {}
  if not isinstance(selection.get("decision_rule"), str) or not selection["decision_rule"].strip():
    failures.append(f"{context}: fixed-pick wall selector lacks a decision rule")
  results = {}
  for domain, selected_prefix in (
    ("instant", "direct_selected_wall"),
    ("ordered", "direct_selected_ordered_wall"),
  ):
    provider = selected.get(domain)
    if not isinstance(provider, str) or not provider.strip():
      failures.append(f"{context}: fixed-pick {domain} lacks a selected provider")
      continue
    if selected_benchmarks.get(domain) != f"{selected_prefix}__{provider}":
      failures.append(f"{context}: fixed-pick {domain} selected benchmark is mislabeled")
    domain_public_exact = public_exact.get(domain)
    if not isinstance(domain_public_exact, dict):
      failures.append(f"{context}: fixed-pick {domain} lacks metric parity evidence")
      domain_public_exact = {}
    # Every converted wall family nests {now, elapsed} under each domain. A missing
    # leaf becomes None, which validate_wall_public_exact_probe fails closed, so an
    # incomplete probe can never pass as a partial reading.
    metric_probes = {metric: domain_public_exact.get(metric) for metric in METRICS}
    # A pipeline-flushing ordered barrier exposes the SIGILL-safe dispatch that the
    # barrier-free path hides, so inline parity with a non-shippable dispatch-free
    # native read is structurally unreachable; disclose the exact route as a
    # diagnostic dispatch lower bound and gate on the usable public reference rather
    # than hard-failing this one pick (see BARRIER_EXPOSED_ORDERED_PICKS).
    barrier_exposed = domain == "ordered" and provider in BARRIER_EXPOSED_ORDERED_PICKS
    domain_result = {
      "winner": provider,
      "public_exact": {
        metric: validate_wall_public_exact_probe(
          context,
          f"{domain}.{metric}",
          probe,
          failures,
          material_loss_is_failure=not barrier_exposed,
        )
        for metric, probe in metric_probes.items()
      },
    }
    if barrier_exposed:
      domain_result["public_exact_contract"] = FREEBSD_PUBLIC_EXACT_CONTRACT
    results[domain] = domain_result
  return results


def validate_wall_selector_reproduction(
  context: str,
  selection: object,
  clocks: dict,
  failures: list[str],
) -> dict:
  """Validate any supported wall selector and return its measured decisions."""

  if not isinstance(selection, dict):
    return {}
  selected = selection.get("selected_provider")
  # Converted families emit one capability-gated pick per contract with no
  # tournament candidates; route every such shape through the single fixed-pick
  # validator (ADR-0005/0006). Runtime-tournament shapes remain only for a family
  # that still races a frozen flip (Apple x86 supplemental) or carries an
  # `architecture` residual descriptor (FreeBSD/RISC-V/wasm/... pre-conversion).
  if is_fixed_pick_wall_selection(selection):
    return validate_fixed_pick_wall_selector(context, selection, failures)
  if isinstance(selection.get("architecture"), str):
    return validate_residual_wall_selector(context, selection, clocks, failures)
  if (
    isinstance(selected, dict)
    and isinstance(selected.get("instant"), str)
    and selected["instant"].startswith("apple_")
  ):
    return validate_apple_wall_selector(context, selection, failures)
  return {}


def validate_cell(
  context: str,
  clocks: dict,
  target_triple: str | None = None,
) -> tuple[list[str], dict]:
  failures: list[str] = []
  reference_eligibility = local_reference_eligibility(target_triple)
  required = {
    "tach", "tach_ordered", "tach_thread_cpu", "native_thread_cpu",
    *(
      name
      for name, policy in reference_eligibility.items()
      if policy["eligible"]
    ),
  }
  missing = sorted(required - clocks.keys())
  if missing:
    return [f"{context}: missing clocks: {', '.join(missing)}"], {}

  present_references = set(LOCAL_COMPETITORS) & set(clocks)
  for clock in sorted(required | present_references):
    validate_ci(context, clock, clocks[clock], failures)
  if failures:
    return failures, {}

  same_thread = {}
  for metric in METRICS:
    comparisons = {}
    for reference_name in LOCAL_COMPETITORS:
      policy = reference_eligibility[reference_name]
      reference = clocks.get(reference_name)
      if not isinstance(reference, dict):
        comparisons[reference_name] = {
          "reference_ns": None,
          "equivalence_allowance_ns": None,
          "eligible_for_reliable_contract": policy["eligible"],
          "eligibility_reason": policy["reason"],
          "implementation": policy["implementation"],
          "measured": False,
          "passed": False,
        }
        continue
      passed, allowance = equivalent_or_faster(
        clocks["tach"], reference, metric
      )
      comparisons[reference_name] = {
        "reference_ns": reference[metric],
        "equivalence_allowance_ns": allowance,
        "eligible_for_reliable_contract": policy["eligible"],
        "eligibility_reason": policy["reason"],
        "implementation": policy["implementation"],
        "measured": True,
        "passed": passed,
      }
      if not passed and policy["eligible"]:
        failures.append(
          f"{context}: Instant {metric} is materially slower than {reference_name}"
        )
    same_thread[metric] = {
      "tach_ns": clocks["tach"][metric],
      "comparisons": comparisons,
      "passed": all(
        item["passed"]
        for item in comparisons.values()
        if item["eligible_for_reliable_contract"]
      ),
    }

  cross_thread = {}
  for metric in METRICS:
    std_reference = clocks.get("std")
    if not isinstance(std_reference, dict):
      policy = reference_eligibility["std"]
      cross_thread[metric] = {
        "tach_ns": clocks["tach_ordered"][metric],
        "std_ns": None,
        "equivalence_allowance_ns": None,
        "eligible_for_reliable_contract": policy["eligible"],
        "eligibility_reason": policy["reason"],
        "measured": False,
        "passed": not policy["eligible"],
      }
      if policy["eligible"]:
        failures.append(f"{context}: OrderedInstant {metric} lacks its eligible std reference")
      continue
    passed, allowance = equivalent_or_faster(
      clocks["tach_ordered"], std_reference, metric
    )
    cross_thread[metric] = {
      "tach_ns": clocks["tach_ordered"][metric],
      "std_ns": std_reference[metric],
      "equivalence_allowance_ns": allowance,
      "eligible_for_reliable_contract": True,
      "eligibility_reason": None,
      "measured": True,
      "passed": passed,
    }
    if not passed:
      failures.append(f"{context}: OrderedInstant {metric} is materially slower than std")

  selected_wall_parity = {}
  wall_selection = clocks["tach"].get("selection")
  selector_reproduction = validate_wall_selector_reproduction(
    context, wall_selection, clocks, failures
  )
  selected_pairs = (
    ("instant", "tach", "direct_selected_wall"),
    ("ordered", "tach_ordered", "direct_selected_ordered_wall"),
  )
  if wall_selection is not None:
    selected = wall_selection.get("selected_provider")
    if not isinstance(selected, dict):
      failures.append(f"{context}: wall selector lacks selected-provider mapping")
      selected = {}
    fixed_pick_wall = is_fixed_pick_wall_selection(wall_selection)
    expected_wall_picks = (
      expected_wall_picks_for_triple(target_triple) if fixed_pick_wall else None
    )
    for domain, public_name, direct_name in selected_pairs:
      domain_reproduction = selector_reproduction.get(domain)
      public_exact_contract = wall_public_exact_contract(domain_reproduction)
      selected_direct_for_candidates = clocks.get(direct_name)
      candidate_results = {}
      declared_candidates = wall_selection.get("eligible_direct_candidates", {})
      if isinstance(declared_candidates, dict):
        domain_candidates = declared_candidates.get(domain)
      else:
        domain_candidates = None
      if not isinstance(domain_candidates, list) or not domain_candidates:
        # A converted fixed-pick selection has no eligible-candidate tournament;
        # it is validated through the selected-native path plus the frozen-pick
        # assertion below, not through a candidate loop.
        if not fixed_pick_wall:
          failures.append(f"{context}: {domain} wall selector lacks eligible candidates")
        domain_candidates = []
      for candidate in domain_candidates:
        expected_identity = exact_wall_candidate_identity(domain, candidate)
        if expected_identity is None:
          failures.append(
            f"{context}: {domain} wall selector declared invalid candidate key {candidate!r}"
          )
          continue
        candidate_row = clocks.get(candidate)
        if not isinstance(candidate_row, dict):
          failures.append(f"{context}: {domain} wall selector lacks exact row {candidate}")
          continue
        validate_ci(context, candidate, candidate_row, failures)
        identity_passed = all(
          candidate_row.get(field) == expected
          for field, expected in expected_identity.items()
        )
        if not identity_passed:
          failures.append(
            f"{context}: {domain} wall candidate {candidate} identity does not match its key"
          )
        metrics = {}
        for metric in METRICS:
          paired_public_exact = wall_public_exact_metric(domain_reproduction, metric)
          comparison_basis = "public Criterion estimate"
          public_reference_gate = None
          selected_candidate = candidate_row.get("provider") == selected.get(domain)
          if public_exact_contract == FREEBSD_PUBLIC_EXACT_CONTRACT:
            passed, public_reference_gate = public_reference_winner_gate(
              clocks, domain, metric, target_triple
            )
            allowance = None
            comparison_basis = (
              "runtime selector plus usable public-reference winner gate; "
              "exact route retained as a diagnostic lower bound"
            )
          elif isinstance(paired_public_exact, dict) and paired_public_exact.get("passed") is not None:
            if selected_candidate:
              passed = paired_public_exact.get("passed") is True
              allowance = paired_public_exact.get("equivalence_allowance_ns_per_read")
              comparison_basis = "alternating paired public/selected-exact probe"
            elif (
              paired_public_exact.get("passed") is True
              and isinstance(selected_direct_for_candidates, dict)
            ):
              passed, allowance = equivalent_or_faster(
                selected_direct_for_candidates, candidate_row, metric
              )
              comparison_basis = (
                "paired public/exact parity plus selected-exact candidate estimate"
              )
            else:
              passed = False
              allowance = paired_public_exact.get("equivalence_allowance_ns_per_read")
              comparison_basis = "failed paired public/selected-exact probe"
          else:
            passed, allowance = equivalent_or_faster(
              clocks[public_name], candidate_row, metric
            )
          metrics[metric] = {
            "tach_ns": clocks[public_name][metric],
            "candidate_ns": candidate_row[metric],
            "equivalence_allowance_ns": allowance,
            "comparison_basis": comparison_basis,
            "public_reference_gate": public_reference_gate,
            "paired_probe": paired_public_exact,
            "passed": passed,
          }
          if not passed:
            failures.append(
              f"{context}: {public_name} {metric} is materially slower than "
              f"eligible exact route {candidate}"
            )
        candidate_results[candidate] = {
          "metrics": metrics,
          "identity_passed": identity_passed,
          "passed": (
            identity_passed
            and all(result["passed"] for result in metrics.values())
          ),
        }

      direct_wall = clocks.get(direct_name)
      if direct_wall is None:
        failures.append(f"{context}: {domain} wall selector lacks selected-native evidence")
        continue
      validate_ci(context, direct_name, direct_wall, failures)
      expected_provider = selected.get(domain)
      if (
        fixed_pick_wall
        and expected_wall_picks is not None
        and expected_provider not in expected_wall_picks[domain]
      ):
        failures.append(
          f"{context}: {domain} fixed pick {expected_provider!r} is not a frozen "
          f"expected pick for {target_triple or 'this target'}"
        )
      if direct_wall.get("provider") != expected_provider:
        failures.append(
          f"{context}: {domain} selected-native provider "
          f"{direct_wall.get('provider')!r} != selector {expected_provider!r}"
        )
        continue
      metrics = {}
      for metric in METRICS:
        paired_public_exact = wall_public_exact_metric(domain_reproduction, metric)
        comparison_basis = "Criterion estimate"
        public_reference_gate = None
        if public_exact_contract == FREEBSD_PUBLIC_EXACT_CONTRACT:
          passed, public_reference_gate = public_reference_winner_gate(
            clocks, domain, metric, target_triple
          )
          allowance = None
          comparison_basis = (
            "usable public-reference winner gate; exact route is a diagnostic "
            "dispatch lower bound"
          )
        elif (
          isinstance(paired_public_exact, dict)
          and paired_public_exact.get("passed") is not None
        ):
          passed = paired_public_exact.get("passed") is True
          allowance = paired_public_exact.get("equivalence_allowance_ns_per_read")
          comparison_basis = "alternating paired public/exact probe"
        else:
          passed, allowance = equivalent_or_faster(
            clocks[public_name], direct_wall, metric
          )
        metrics[metric] = {
          "tach_ns": clocks[public_name][metric],
          "selected_native_ns": direct_wall[metric],
          "equivalence_allowance_ns": allowance,
          "comparison_basis": comparison_basis,
          "public_reference_gate": public_reference_gate,
          "paired_probe": paired_public_exact,
          "passed": passed,
        }
        if not passed:
          failures.append(
            f"{context}: {public_name} {metric} is materially slower than its "
            "selected native path"
          )
      selected_wall_parity[domain] = {
        "selected_provider": direct_wall.get("provider"),
        "public_exact_contract": public_exact_contract,
        "metrics": metrics,
        "eligible_candidates": candidate_results,
        "passed": (
          all(result["passed"] for result in metrics.values())
          and all(result["passed"] for result in candidate_results.values())
        ),
      }

  tach_thread = clocks["tach_thread_cpu"]
  native = clocks["native_thread_cpu"]
  for name, entry in (("tach_thread_cpu", tach_thread), ("native_thread_cpu", native)):
    if entry.get("time_domain") != "thread CPU":
      failures.append(f"{context}: {name} did not measure current-thread CPU time")
  provider = tach_thread.get("provider")
  read_cost = tach_thread.get("read_cost")
  if strict_thread_cpu_provider_cost(provider, target_triple) != read_cost:
    failures.append(
      f"{context}: ThreadCpuInstant provider/read-cost pair is not a strict "
      "native provider"
    )

  thread_selector_reproduction = {}
  selected_thread_parity = {}
  thread_selection = tach_thread.get("selection")
  if thread_selection is not None:
    if not isinstance(thread_selection, dict):
      failures.append(f"{context}: malformed thread-CPU selection evidence")
    else:
      thread_selector_reproduction = validate_thread_cpu_selector(
        context, thread_selection, failures, target_triple
      )
      declared_candidates = thread_selection.get("eligible_direct_candidates", [])
      candidate_parity = {}
      for candidate in declared_candidates:
        candidate_row = clocks.get(candidate)
        if not isinstance(candidate_row, dict):
          failures.append(f"{context}: thread-CPU selector lacks exact row {candidate}")
          continue
        validate_ci(context, candidate, candidate_row, failures)
        if candidate_row.get("benchmark") != candidate:
          failures.append(f"{context}: thread-CPU exact candidate {candidate} is mislabeled")
        if candidate_row.get("time_domain") != "thread CPU":
          failures.append(f"{context}: thread-CPU exact candidate {candidate} changed time domain")
        metrics = {}
        for metric in METRICS:
          paired_public_exact = thread_public_exact_metric(
            thread_selector_reproduction, metric
          )
          comparison_basis = "Criterion estimate"
          if (
            isinstance(paired_public_exact, dict)
            and paired_public_exact.get("passed") is not None
          ):
            passed = paired_public_exact.get("passed") is True
            allowance = paired_public_exact.get("equivalence_allowance_ns_per_read")
            comparison_basis = "alternating paired public/exact probe"
          else:
            passed, allowance = equivalent_or_faster(tach_thread, candidate_row, metric)
          metrics[metric] = {
            "tach_ns": tach_thread[metric],
            "candidate_ns": candidate_row[metric],
            "equivalence_allowance_ns": allowance,
            "comparison_basis": comparison_basis,
            "paired_probe": paired_public_exact,
            "passed": passed,
          }
          if not passed:
            failures.append(
              f"{context}: ThreadCpuInstant {metric} is materially slower than "
              f"eligible exact route {candidate}"
            )
        candidate_parity[candidate] = {
          "metrics": metrics,
          "passed": all(result["passed"] for result in metrics.values()),
        }
      selected_key = thread_selection.get("selected_provider")
      selected_metadata = thread_cpu_provider_key_metadata(selected_key, target_triple)
      if selected_metadata != (provider, read_cost):
        failures.append(
          f"{context}: thread-CPU selector public provider/read-cost disagrees "
          "with introspection"
        )
      selected_thread = clocks.get("direct_selected_thread_cpu")
      if not isinstance(selected_thread, dict):
        failures.append(f"{context}: thread-CPU selector lacks selected-native evidence")
      else:
        validate_ci(context, "direct_selected_thread_cpu", selected_thread, failures)
        expected_mechanism = thread_selection.get("selected_mechanism")
        if selected_thread.get("provider") != expected_mechanism:
          failures.append(
            f"{context}: selected thread-CPU mechanism "
            f"{selected_thread.get('provider')!r} != selector {expected_mechanism!r}"
          )
        if selected_thread.get("read_cost") != thread_selection.get("selected_read_cost"):
          failures.append(f"{context}: selected thread-CPU exact-row read cost disagrees")
        if selected_thread.get("benchmark") != thread_selection.get("selected_native_benchmark"):
          failures.append(f"{context}: selected thread-CPU exact row is mislabeled")
        if selected_thread.get("time_domain") != "thread CPU":
          failures.append(f"{context}: selected thread-CPU exact row changed time domain")
        metrics = {}
        for metric in METRICS:
          paired_public_exact = thread_public_exact_metric(
            thread_selector_reproduction, metric
          )
          comparison_basis = "Criterion estimate"
          if (
            isinstance(paired_public_exact, dict)
            and paired_public_exact.get("passed") is not None
          ):
            passed = paired_public_exact.get("passed") is True
            allowance = paired_public_exact.get("equivalence_allowance_ns_per_read")
            comparison_basis = "alternating paired public/exact probe"
          else:
            passed, allowance = equivalent_or_faster(tach_thread, selected_thread, metric)
          metrics[metric] = {
            "tach_ns": tach_thread[metric],
            "selected_native_ns": selected_thread[metric],
            "equivalence_allowance_ns": allowance,
            "comparison_basis": comparison_basis,
            "paired_probe": paired_public_exact,
            "passed": passed,
          }
          if not passed:
            failures.append(
              f"{context}: ThreadCpuInstant {metric} is materially slower than its "
              "selected native path"
            )
        selected_thread_parity = {
          "selected_provider": selected_key,
          "selected_mechanism": selected_thread.get("provider"),
          "selected_read_cost": selected_thread.get("read_cost"),
          "metrics": metrics,
          "eligible_candidates": candidate_parity,
          "passed": (
            all(result["passed"] for result in metrics.values())
            and all(result["passed"] for result in candidate_parity.values())
          ),
        }
        fallback_benchmark = thread_selection.get("fallback_native_benchmark")
        if fallback_benchmark is not None:
          fallback_thread = clocks.get("direct_fallback_thread_cpu")
          if not isinstance(fallback_thread, dict):
            failures.append(f"{context}: selector lacks measured fallback exact row")
          else:
            validate_ci(context, "direct_fallback_thread_cpu", fallback_thread, failures)
            if (
              fallback_thread.get("benchmark") != fallback_benchmark
              or fallback_thread.get("provider")
              != thread_selection.get("fallback_mechanism")
              or fallback_thread.get("read_cost")
              != thread_selection.get("fallback_read_cost")
              or fallback_thread.get("time_domain") != "thread CPU"
            ):
              failures.append(f"{context}: measured fallback exact row is mislabeled")
            runner_up_metrics = {}
            for metric in METRICS:
              passed, allowance = equivalent_or_faster(
                selected_thread, fallback_thread, metric
              )
              runner_up_metrics[metric] = {
                "selected_ns": selected_thread[metric],
                "fallback_ns": fallback_thread[metric],
                "equivalence_allowance_ns": allowance,
                "passed": passed,
              }
              if not passed:
                failures.append(
                  f"{context}: selected thread-CPU {metric} is materially slower "
                  "than its measured runner-up"
                )
            selected_thread_parity["measured_runner_up"] = {
              "provider": fallback_thread.get("provider"),
              "metrics": runner_up_metrics,
              "passed": all(result["passed"] for result in runner_up_metrics.values()),
            }

      if thread_selection.get("selection_kind") == "fixed_windows_thread_times":
        failure_fallback = thread_selection.get("failure_fallback")
        failure_row = clocks.get("direct_failure_fallback_thread_cpu")
        if not isinstance(failure_fallback, dict) or not isinstance(failure_row, dict):
          failures.append(f"{context}: Windows wall fallback lacks exact benchmark evidence")
        else:
          validate_ci(
            context,
            "direct_failure_fallback_thread_cpu",
            failure_row,
            failures,
          )
          if (
            failure_row.get("benchmark") != failure_fallback.get("exact_benchmark")
            or failure_row.get("provider") != failure_fallback.get("mechanism")
            or failure_row.get("read_cost") != failure_fallback.get("read_cost")
            or failure_row.get("time_domain") != "monotonic wall fallback"
            or failure_row.get("eligible_for_thread_cpu_speed_claim") is not False
          ):
            failures.append(
              f"{context}: Windows wall fallback exact row is mislabeled or eligible"
            )
          selected_thread_parity["failure_fallback"] = {
            "provider": failure_row.get("provider"),
            "read_cost": failure_row.get("read_cost"),
            "time_domain": failure_row.get("time_domain"),
            "eligible_for_thread_cpu_speed_claim": False,
            "now_ns": failure_row.get("now"),
            "elapsed_ns": failure_row.get("elapsed"),
            "measured": True,
          }

  current_thread = {}
  for metric in METRICS:
    paired_public_exact = thread_public_exact_metric(
      thread_selector_reproduction, metric
    )
    comparison_basis = "Criterion estimate"
    if (
      isinstance(paired_public_exact, dict)
      and paired_public_exact.get("passed") is not None
    ):
      passed = paired_public_exact.get("passed") is True
      allowance = paired_public_exact.get("equivalence_allowance_ns_per_read")
      comparison_basis = "alternating paired public/exact probe"
    else:
      passed, allowance = equivalent_or_faster(tach_thread, native, metric)
    current_thread[metric] = {
      "tach_ns": tach_thread[metric],
      "native_ns": native[metric],
      "equivalence_allowance_ns": allowance,
      "comparison_basis": comparison_basis,
      "paired_probe": paired_public_exact,
      "passed": passed,
    }
    if not passed:
      failures.append(
        f"{context}: ThreadCpuInstant {metric} is materially slower than native"
      )

  return failures, {
    "same_thread_elapsed": same_thread,
    "same_thread_reference_eligibility": reference_eligibility,
    "cross_thread_elapsed": cross_thread,
    "wall_selector_reproduction": selector_reproduction,
    "selected_wall_provider_parity": selected_wall_parity,
    "thread_cpu_selector_reproduction": thread_selector_reproduction,
    "selected_thread_cpu_provider_parity": selected_thread_parity,
    "current_thread_cpu": current_thread,
    "provider": tach_thread.get("provider"),
    "read_cost": tach_thread.get("read_cost"),
  }


def _valid_full_revision(value: object) -> bool:
  return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", value) is not None


def benchmark_features_for_build_mode(build_mode: object) -> tuple[str, ...] | None:
  """Return the exact Cargo feature identity for one advertised build mode."""
  return (
    BENCHMARK_FEATURES_BY_BUILD_MODE.get(build_mode)
    if isinstance(build_mode, str)
    else None
  )


def runtime_smoke_features_for_build_mode(build_mode: object) -> tuple[str, ...] | None:
  """Return the public tach features exercised by a runtime-smoke build."""
  return {
    "default": ("thread-cpu-inline",),
    "no-default": (),
  }.get(build_mode) if isinstance(build_mode, str) else None


def validate_runtime_attestation(
  context: str,
  attestation: object,
  expected_triple: str,
  expected_harness: str,
  expected_build_mode: str,
  expected_revision: object,
  failures: list[str],
  *,
  runtime_smoke: bool = False,
) -> dict:
  """Validate target/build identity emitted by the measured runtime itself."""
  target = RUNTIME_TARGETS.get(expected_triple)
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
  if not isinstance(attestation, dict) or set(attestation) != expected_keys:
    failures.append(f"{context}: runtime attestation schema changed")
    return {}
  if attestation.get("schema") != RUNTIME_ATTESTATION_SCHEMA:
    failures.append(f"{context}: runtime attestation version changed")
  invocation_id = attestation.get("invocation_id")
  if not isinstance(invocation_id, str) or not re.fullmatch(r"[A-Za-z0-9._:-]+", invocation_id):
    failures.append(f"{context}: runtime attestation has no opaque invocation ID")
  if attestation.get("harness") != expected_harness:
    failures.append(f"{context}: runtime attestation harness disagrees with artifact")
  if target is None or attestation.get("target") != target:
    failures.append(f"{context}: runtime attestation target disagrees with artifact")
  features = attestation.get("features")
  expected_features = (
    runtime_smoke_features_for_build_mode(expected_build_mode)
    if runtime_smoke
    else benchmark_features_for_build_mode(expected_build_mode)
  )
  if (
    not isinstance(features, list)
    or features != sorted(features)
    or len(set(features)) != len(features)
    or not all(isinstance(feature, str) and feature for feature in features)
    or attestation.get("build_mode") != expected_build_mode
    or expected_features is None
    or features != list(expected_features)
  ):
    failures.append(
      f"{context}: runtime attestation feature set differs from build mode "
      f"{expected_build_mode!r}"
    )
  if attestation.get("build_profile") != "optimized":
    failures.append(f"{context}: runtime attestation is not an optimized benchmark build")
  revision = attestation.get("source_revision")
  if not _valid_full_revision(revision) or revision != expected_revision:
    failures.append(f"{context}: runtime attestation source revision is not bound")
  if not isinstance(attestation.get("runner"), str) or not attestation["runner"].strip():
    failures.append(f"{context}: runtime attestation has no runner identity")
  if attestation.get("output_isolated") is not True:
    failures.append(f"{context}: runtime attestation did not isolate Criterion output")
  return attestation


def validate_collector_attestation(
  context: str,
  collector: object,
  expected_triple: str,
  expected_harness: str,
  expected_build_mode: str,
  expected_revision: object,
  failures: list[str],
) -> dict:
  """Validate a digest-bound Criterion collection before accepting its clocks."""
  expected_keys = {
    "schema",
    "invocation_id",
    "runtime_attestation",
    "manifest_sha256",
  }
  if not isinstance(collector, dict) or set(collector) != expected_keys:
    failures.append(f"{context}: collector attestation schema changed")
    return {}
  if collector.get("schema") != COLLECTOR_ATTESTATION_SCHEMA:
    failures.append(f"{context}: collector attestation version changed")
  if not isinstance(collector.get("manifest_sha256"), str) or not re.fullmatch(
    r"[0-9a-f]{64}", collector["manifest_sha256"]
  ):
    failures.append(f"{context}: collector manifest digest is malformed")
  attestation = validate_runtime_attestation(
    context,
    collector.get("runtime_attestation"),
    expected_triple,
    expected_harness,
    expected_build_mode,
    expected_revision,
    failures,
  )
  if collector.get("invocation_id") != attestation.get("invocation_id"):
    failures.append(f"{context}: collector and runtime invocation IDs disagree")
  return attestation


def validate_collector_bundle_descriptor(
  context: str,
  descriptor: object,
  collector: object,
  failures: list[str],
) -> None:
  """Bind a serialized cell to the immutable bundle it was extracted from."""
  expected_keys = {"schema", "path", "manifest_sha256"}
  if not isinstance(descriptor, dict) or set(descriptor) != expected_keys:
    failures.append(f"{context}: collector bundle descriptor schema changed")
    return
  relative = descriptor.get("path")
  path = PurePosixPath(relative) if isinstance(relative, str) else None
  if (
    not isinstance(relative, str)
    or not relative
    or "\\" in relative
    or path is None
    or path.is_absolute()
    or any(part in {"", ".", ".."} for part in path.parts)
    or path.as_posix() != relative
  ):
    failures.append(f"{context}: collector bundle path is not a safe relative path")
  if (
    descriptor.get("schema") != COLLECTOR_BUNDLE_DESCRIPTOR_SCHEMA
    or not isinstance(collector, dict)
    or descriptor.get("manifest_sha256") != collector.get("manifest_sha256")
  ):
    failures.append(f"{context}: collector bundle descriptor disagrees with clocks")


def validate_provenance_runtime_binding(
  context: str,
  provenance: object,
  attestation: object,
  failures: list[str],
) -> None:
  """Require document provenance to repeat the collector-emitted build identity."""
  if not isinstance(provenance, dict) or not isinstance(attestation, dict):
    failures.append(f"{context}: provenance cannot bind runtime identity")
    return
  expected = {
    "harness": attestation.get("harness"),
    "source_revision": attestation.get("source_revision"),
    "runner": attestation.get("runner"),
    "build_profile": attestation.get("build_profile"),
    "build_mode": attestation.get("build_mode"),
    "features": attestation.get("features"),
  }
  if any(provenance.get(field) != value for field, value in expected.items()):
    failures.append(f"{context}: document provenance disagrees with runtime identity")


def runtime_identity_provenance(attestation: object) -> dict:
  """Copy only collector-emitted identity fields into a composed document."""
  if not isinstance(attestation, dict):
    return {}
  return {
    field: attestation.get(field)
    for field in (
      "harness",
      "source_revision",
      "runner",
      "build_profile",
      "build_mode",
      "features",
    )
  }


def _primary_cell_result(
  artifact_id: str,
  document: object,
  failures: list[str],
  *,
  three_clock_speed: object = None,
  bound_observation: bool = False,
  bundle_path: Path | None = None,
) -> dict:
  """Render one stable primary-cell result, including failed partial identities."""
  provenance = document.get("provenance") if isinstance(document, dict) else None
  provenance = provenance if isinstance(provenance, dict) else {}
  return {
    "artifact_id": artifact_id,
    "source_revision": provenance.get("source_revision"),
    "triple": document.get("triple") if isinstance(document, dict) else None,
    "build_mode": document.get("build_mode") if isinstance(document, dict) else None,
    "evidence_kind": document.get("evidence_kind") if isinstance(document, dict) else None,
    "bound_observation": bound_observation,
    "bundle_path": str(bundle_path) if bundle_path is not None else None,
    "three_clock_speed": three_clock_speed if three_clock_speed is not None else {},
    "passed": not failures,
    "failures": failures,
  }


def validate_primary_speed_cell(artifact_id: str, document: object) -> dict:
  """Validate one primary speed cell before its retained bundle is re-extracted."""
  context = f"primary {artifact_id}"
  failures: list[str] = []
  expected = PRIMARY_SPEED_CELLS.get(artifact_id)
  if expected is None:
    return _primary_cell_result(
      artifact_id,
      document,
      [f"{context}: unknown primary artifact"],
    )
  order, title, instance, triple, harness, expected_build_mode = expected
  if not isinstance(document, dict):
    return _primary_cell_result(
      artifact_id,
      document,
      [f"{context}: document is not an object"],
    )

  expected_document_keys = {
    "schema",
    "artifact_id",
    "title",
    "instance",
    "triple",
    "order",
    "build_mode",
    "evidence_kind",
    "provenance",
    "collector_bundle",
    "collector_attestation",
    "clocks",
  }
  if set(document) != expected_document_keys:
    failures.append(f"{context}: document schema changed")
  provenance = document.get("provenance")
  provenance_fields = {
    "harness",
    "source_revision",
    "runner",
    "build_profile",
    "build_mode",
    "features",
  }
  if not isinstance(provenance, dict) or set(provenance) != provenance_fields:
    failures.append(f"{context}: provenance schema changed")
    provenance = {}
  identity = (
    document.get("order"),
    document.get("title"),
    document.get("instance"),
    document.get("triple"),
    provenance.get("harness"),
  )
  if (
    document.get("schema") != PRIMARY_SPEED_SCHEMA
    or document.get("artifact_id") != artifact_id
    or identity != (order, title, instance, triple, harness)
    or document.get("evidence_kind") != PRIMARY_EVIDENCE_KIND
  ):
    failures.append(f"{context}: identity does not match the primary campaign")
  if expected_build_mode is not None and document.get("build_mode") != expected_build_mode:
    failures.append(
      f"{context}: build mode does not match the primary campaign "
      f"{expected_build_mode!r}"
    )
  revision = provenance.get("source_revision")
  if not _valid_full_revision(revision):
    failures.append(f"{context}: invalid full source revision")
  runtime_attestation = validate_collector_attestation(
    context,
    document.get("collector_attestation"),
    triple,
    harness,
    expected_build_mode if expected_build_mode is not None else document.get("build_mode"),
    revision,
    failures,
  )
  validate_collector_bundle_descriptor(
    context,
    document.get("collector_bundle"),
    document.get("collector_attestation"),
    failures,
  )
  validate_provenance_runtime_binding(context, provenance, runtime_attestation, failures)

  clocks = document.get("clocks")
  three_clock_speed = {}
  if not isinstance(clocks, dict):
    failures.append(f"{context}: missing clocks object")
  elif "collector_attestation" in clocks:
    failures.append(f"{context}: clocks must not carry a second collector attestation")
  elif not failures:
    cell_failures, three_clock_speed = validate_cell(context, clocks, triple)
    failures.extend(cell_failures)
    if harness == "lambda":
      validate_lambda_samples(context, clocks, failures)
  return _primary_cell_result(
    artifact_id,
    document,
    failures,
    three_clock_speed=three_clock_speed,
  )


def compose_primary_speed_cell(
  artifact_id: str,
  title: str,
  instance: str,
  triple: str,
  order: int,
  clocks: object,
  collector_attestation: object,
  collector_bundle_path: object,
) -> dict:
  """Compose one canonical primary cell from a verified retained observation."""
  expected = PRIMARY_SPEED_CELLS.get(artifact_id)
  if expected is None:
    raise ValueError(f"unknown primary artifact {artifact_id!r}")
  runtime_attestation = (
    collector_attestation.get("runtime_attestation")
    if isinstance(collector_attestation, dict)
    else None
  )
  document = {
    "schema": PRIMARY_SPEED_SCHEMA,
    "artifact_id": artifact_id,
    "title": title,
    "instance": instance,
    "triple": triple,
    "order": order,
    "build_mode": (
      runtime_attestation.get("build_mode")
      if isinstance(runtime_attestation, dict)
      else None
    ),
    "evidence_kind": PRIMARY_EVIDENCE_KIND,
    "provenance": runtime_identity_provenance(runtime_attestation),
    "collector_bundle": {
      "schema": COLLECTOR_BUNDLE_DESCRIPTOR_SCHEMA,
      "path": collector_bundle_path,
      "manifest_sha256": (
        collector_attestation.get("manifest_sha256")
        if isinstance(collector_attestation, dict)
        else None
      ),
    },
    "collector_attestation": collector_attestation,
    "clocks": clocks,
  }
  report = validate_primary_speed_cell(artifact_id, document)
  if not report["passed"]:
    raise ValueError("primary cell does not validate:\n  " + "\n  ".join(report["failures"]))
  return document


def retained_primary_collector_bundle_path(
  artifact_id: str,
  cell_path: Path,
  document: object,
) -> tuple[Path | None, str | None]:
  """Resolve a primary collector bundle without permitting path escapes or links."""
  descriptor = document.get("collector_bundle") if isinstance(document, dict) else None
  relative = descriptor.get("path") if isinstance(descriptor, dict) else None
  if not isinstance(relative, str) or not relative or "\\" in relative:
    return None, f"primary {artifact_id}: collector bundle descriptor has no safe relative path"
  bundle_parts = PurePosixPath(relative)
  if (
    bundle_parts.is_absolute()
    or not bundle_parts.parts
    or any(part in {"", ".", ".."} for part in bundle_parts.parts)
    or bundle_parts.as_posix() != relative
  ):
    return None, f"primary {artifact_id}: collector bundle descriptor has no safe relative path"
  try:
    cell_mode = cell_path.lstat().st_mode
  except OSError as error:
    return None, f"primary {artifact_id}: could not stat cell document: {error}"
  if not stat.S_ISREG(cell_mode):
    return None, f"primary {artifact_id}: cell document is not a regular file"
  if cell_path.name != artifact_id:
    return None, f"primary {artifact_id}: cell path does not match artifact ID"
  try:
    cell_directory = cell_path.parent.resolve(strict=True)
  except OSError as error:
    return None, f"primary {artifact_id}: could not resolve cell directory: {error}"
  candidate = cell_directory.joinpath(*bundle_parts.parts)
  try:
    candidate.resolve(strict=False).relative_to(cell_directory)
  except ValueError:
    return None, f"primary {artifact_id}: collector bundle escapes the cell directory"
  current = cell_directory
  bundle_mode = None
  for part in bundle_parts.parts:
    current /= part
    try:
      bundle_mode = current.lstat().st_mode
    except FileNotFoundError:
      return None, f"primary {artifact_id}: retained collector bundle is missing"
    except OSError as error:
      return None, f"primary {artifact_id}: could not stat retained collector bundle: {error}"
    if stat.S_ISLNK(bundle_mode):
      return None, f"primary {artifact_id}: retained collector bundle must not contain symbolic links"
  if bundle_mode is None or not stat.S_ISDIR(bundle_mode):
    return None, f"primary {artifact_id}: retained collector bundle is not a directory"
  return current, None


def validate_primary_speed_cell_from_bundle(
  artifact_id: str,
  document: object,
  bundle_dir: Path,
) -> dict:
  """Re-extract a primary cell's retained bundle before accepting its clocks."""
  report = validate_primary_speed_cell(artifact_id, document)
  failures = list(report["failures"])
  binding_failures: list[str] = []
  expected = PRIMARY_SPEED_CELLS.get(artifact_id)
  if expected is None:
    binding_failures.append(f"primary {artifact_id}: unknown primary artifact")
  else:
    try:
      import extract_speed

      observation = extract_speed.extract_collector_bundle_observation(bundle_dir)
    except (OSError, RuntimeError, ValueError) as error:
      binding_failures.append(
        f"primary {artifact_id}: cannot verify retained collector bundle: {error}"
      )
    else:
      observed_clocks = observation.get("clocks")
      observed_collector = observation.get("collector_attestation")
      if document.get("clocks") != observed_clocks:
        binding_failures.append(
          f"primary {artifact_id}: serialized clocks differ from retained collector bundle"
        )
      if document.get("collector_attestation") != observed_collector:
        binding_failures.append(
          f"primary {artifact_id}: serialized collector attestation differs from retained collector bundle"
        )
      descriptor = document.get("collector_bundle") if isinstance(document, dict) else None
      if isinstance(descriptor, dict) and isinstance(observed_collector, dict):
        if descriptor.get("manifest_sha256") != observed_collector.get("manifest_sha256"):
          binding_failures.append(
            f"primary {artifact_id}: retained collector bundle digest changed"
          )

  all_failures = [*failures, *binding_failures]
  report["bundle_binding"] = {
    "path": str(bundle_dir),
    "passed": not binding_failures,
  }
  report["bound_observation"] = not all_failures
  report["passed"] = not all_failures
  report["failures"] = all_failures
  return report


def validate_primary_speed_campaign(
  documents: dict[str, dict],
  cell_paths: dict[str, Path] | None = None,
) -> dict:
  """Validate the exact primary four-cell campaign through retained observations."""
  failures: list[str] = []
  results = []
  if not isinstance(documents, dict):
    return {
      "schema": PRIMARY_SPEED_REPORT_SCHEMA,
      "source_revision": None,
      "passed": False,
      "failures": ["primary campaign documents must be keyed by artifact ID"],
      "cells": [],
    }
  expected_names = set(PRIMARY_SPEED_CELLS)
  actual_names = set(documents)
  if actual_names != expected_names:
    failures.append(
      "primary three-clock artifacts differ: "
      f"missing={sorted(expected_names - actual_names)!r}, "
      f"unexpected={sorted(actual_names - expected_names)!r}"
    )
  identities = []
  revisions = set()
  for artifact_id in PRIMARY_SPEED_CELLS:
    if artifact_id not in documents:
      continue
    document = documents[artifact_id]
    provenance = document.get("provenance") if isinstance(document, dict) else None
    provenance = provenance if isinstance(provenance, dict) else {}
    identities.append((
      document.get("order") if isinstance(document, dict) else None,
      document.get("title") if isinstance(document, dict) else None,
      document.get("instance") if isinstance(document, dict) else None,
      document.get("triple") if isinstance(document, dict) else None,
      provenance.get("harness"),
    ))
    cell_path = cell_paths.get(artifact_id) if isinstance(cell_paths, dict) else None
    bundle_path = None
    bundle_error = None
    if isinstance(cell_path, Path):
      bundle_path, bundle_error = retained_primary_collector_bundle_path(
        artifact_id, cell_path, document
      )
    if bundle_path is not None:
      result = validate_primary_speed_cell_from_bundle(
        artifact_id, document, bundle_path
      )
    else:
      result = validate_primary_speed_cell(artifact_id, document)
      result["failures"].append(
        bundle_error or f"primary {artifact_id}: missing retained collector bundle path"
      )
      result["passed"] = False
      result["bound_observation"] = False
    failures.extend(result["failures"])
    revision = result.get("source_revision")
    if _valid_full_revision(revision):
      revisions.add(revision)
    results.append(result)
  if tuple(identities) != EXPECTED_PRIMARY_IDENTITIES:
    failures.append(
      "primary campaign environments differ from the exact four-platform contract: "
      f"{identities!r}"
    )
  if len(revisions) != 1:
    failures.append(f"primary campaign must use one source revision: {sorted(revisions)}")
  return {
    "schema": PRIMARY_SPEED_REPORT_SCHEMA,
    "claim": (
      "each tach timer selects the fastest audited eligible reliable steady-state provider "
      "for its contract in every measured primary environment"
    ),
    "equivalence_rule": (
      "tach is faster than or materially tied with every eligible reference: its point "
      "estimate and conservative 95% CI comparison fit within max(1 ns, 5%)"
    ),
    "source_revision": next(iter(revisions)) if len(revisions) == 1 else None,
    "passed": not failures,
    "failures": failures,
    "cells": results,
  }


def validate_campaign(
  documents: dict[str, dict],
  cell_paths: dict[str, Path] | None = None,
) -> dict:
  """Compatibility entry point for the now bundle-bound primary campaign."""
  return validate_primary_speed_campaign(documents, cell_paths)


def _semantic_summary(samples: object) -> dict:
  """Return the reproducible median summary for one semantic probe phase."""
  if not isinstance(samples, list) or len(samples) != THREAD_CPU_BEHAVIOR_SAMPLE_COUNT:
    raise ValueError("semantic phase needs exactly three raw samples")
  normalized = []
  for sample in samples:
    if (
      not isinstance(sample, dict)
      or set(sample) != set(THREAD_CPU_BEHAVIOR_FIELDS)
      or not all(
        type(sample[field]) is int and sample[field] >= 0
        for field in THREAD_CPU_BEHAVIOR_FIELDS
      )
    ):
      raise ValueError("semantic samples must contain only non-negative integer deltas")
    normalized.append({field: sample[field] for field in THREAD_CPU_BEHAVIOR_FIELDS})
  return {
    field: statistics.median(sample[field] for sample in normalized)
    for field in THREAD_CPU_BEHAVIOR_FIELDS
  }


def _fallback_advances_during_sleep(summary: dict) -> bool:
  tolerance = max(100_000.0, summary["wall_delta_ns"] * 0.01)
  return (
    summary["public_delta_ns"] > tolerance
    and summary["direct_delta_ns"] > tolerance
  )


def _validate_raw_thread_cpu_behavior(raw: object) -> dict:
  """Validate the agreed sidecar without changing its collector-owned fields."""
  expected_keys = {
    "schema",
    "direct_benchmark",
    "sample_count",
    "runtime_attestation",
    *THREAD_CPU_BEHAVIOR_PHASES,
  }
  if not isinstance(raw, dict) or set(raw) != expected_keys:
    raise ValueError("thread-CPU behavior sidecar has an unexpected shape")
  if raw.get("schema") != THREAD_CPU_BEHAVIOR_SCHEMA:
    raise ValueError("thread-CPU behavior sidecar schema changed")
  if not isinstance(raw.get("direct_benchmark"), str) or not raw["direct_benchmark"]:
    raise ValueError("thread-CPU behavior sidecar has no direct benchmark identity")
  sample_count = raw.get("sample_count")
  if sample_count != THREAD_CPU_BEHAVIOR_SAMPLE_COUNT:
    raise ValueError("thread-CPU behavior sidecar has an invalid sample count")
  summaries = {}
  for phase in THREAD_CPU_BEHAVIOR_PHASES:
    probe = raw.get(phase)
    if not isinstance(probe, dict) or set(probe) != {*THREAD_CPU_BEHAVIOR_FIELDS, "samples"}:
      raise ValueError(f"{phase} semantic sidecar shape changed")
    samples = probe.get("samples")
    if not isinstance(samples, list) or len(samples) != sample_count:
      raise ValueError(f"{phase} sample count does not match the sidecar")
    reproduced = _semantic_summary(samples)
    if any(probe.get(field) != value for field, value in reproduced.items()):
      raise ValueError(f"{phase} semantic summary does not reproduce")
    summaries[phase] = reproduced
  return summaries


def build_thread_cpu_behavior(raw: object, time_domain: object) -> dict:
  """Preserve the sidecar and add only classifications derived from clocks."""
  summaries = _validate_raw_thread_cpu_behavior(raw)
  if time_domain not in ("thread CPU", "monotonic wall fallback"):
    raise ValueError("extracted public clock has an unknown thread time domain")
  return {
    **raw,
    "time_domain": time_domain,
    "tagged_wall_fallback": time_domain == "monotonic wall fallback",
    "wall_time_advanced_during_sleep": _fallback_advances_during_sleep(
      summaries["sleep"]
    ),
  }


def validate_thread_cpu_behavior(
  context: str,
  behavior: object,
  expected_time_domain: str,
  expected_direct_benchmark: object,
  expected_runtime_attestation: object,
  failures: list[str],
) -> dict:
  """Validate raw semantic samples, their derived summary, and their contract."""
  if not isinstance(behavior, dict):
    failures.append(f"{context}: missing current-thread CPU semantic probes")
    return {}
  expected_keys = {
    "schema",
    "direct_benchmark",
    "sample_count",
    "runtime_attestation",
    *THREAD_CPU_BEHAVIOR_PHASES,
    "time_domain",
    "tagged_wall_fallback",
    "wall_time_advanced_during_sleep",
  }
  if set(behavior) != expected_keys or behavior.get("schema") != THREAD_CPU_BEHAVIOR_SCHEMA:
    failures.append(f"{context}: thread-CPU semantic evidence schema changed")
    return {}
  if behavior.get("time_domain") != expected_time_domain:
    failures.append(f"{context}: semantic probes changed the declared thread timeline")
    return {}
  if (
    not isinstance(expected_direct_benchmark, str)
    or not expected_direct_benchmark
    or behavior.get("direct_benchmark") != expected_direct_benchmark
  ):
    failures.append(f"{context}: semantic probes are not bound to native_thread_cpu")
  if behavior.get("runtime_attestation") != expected_runtime_attestation:
    failures.append(f"{context}: semantic probes came from another runtime invocation")
  expected_tag = expected_time_domain == "monotonic wall fallback"
  if behavior.get("tagged_wall_fallback") is not expected_tag:
    failures.append(f"{context}: semantic fallback tag disagrees with the time domain")

  raw_sidecar = {
    key: behavior[key]
    for key in (
      "schema",
      "direct_benchmark",
      "sample_count",
      "runtime_attestation",
      *THREAD_CPU_BEHAVIOR_PHASES,
    )
    if key in behavior
  }
  try:
    summaries = _validate_raw_thread_cpu_behavior(raw_sidecar)
  except ValueError as error:
    failures.append(f"{context}: {error}")
    return {}

  results = {}
  for phase in THREAD_CPU_BEHAVIOR_PHASES:
    reproduced = summaries[phase]
    wall_delta = reproduced["wall_delta_ns"]
    public_delta = reproduced["public_delta_ns"]
    direct_delta = reproduced["direct_delta_ns"]
    if phase == "busy":
      tolerance = max(100_000.0, direct_delta * 0.05)
      passed = (
        public_delta > 0
        and direct_delta > 0
        and abs(public_delta - direct_delta) <= tolerance
      )
    elif expected_time_domain == "thread CPU":
      tolerance = max(100_000.0, wall_delta * 0.01)
      passed = public_delta <= tolerance and direct_delta <= tolerance
    else:
      tolerance = max(100_000.0, wall_delta * 0.01)
      passed = public_delta > tolerance and direct_delta > tolerance
    results[phase] = {
      "summary": reproduced,
      "tolerance_ns": tolerance,
      "passed": passed,
    }
    if not passed:
      failures.append(f"{context}: {phase} {expected_time_domain} semantic probe failed")

  sleep = summaries["sleep"]
  expected_sleep_advance = _fallback_advances_during_sleep(sleep)
  if behavior.get("wall_time_advanced_during_sleep") is not expected_sleep_advance:
    failures.append(f"{context}: sleep fallback classification does not reproduce")
  if expected_tag and not expected_sleep_advance:
    failures.append(f"{context}: fallback did not advance during sleep")
  return results


def _supplemental_benchmark_matches_key(row_name: str, benchmark: object) -> bool:
  if not isinstance(benchmark, str) or not benchmark:
    return False
  if row_name == "native_thread_cpu":
    return benchmark.startswith("native_thread_cpu__") and len(benchmark) > len(
      "native_thread_cpu__"
    )
  if row_name == "direct_failure_fallback_thread_cpu":
    return benchmark.startswith("direct_fallback_thread_cpu__")
  if row_name.startswith((
    "direct_wall__",
    "direct_ordered_wall__",
    "direct_thread_cpu__",
    "direct_fallback_thread_cpu__",
  )):
    return benchmark == row_name
  return benchmark == row_name or benchmark.startswith(f"{row_name}__")


def validate_supplemental_benchmark_identities(
  context: str,
  clocks: object,
  coverage: object,
  target_triple: str,
  harness: str,
  build_mode: str,
  failures: list[str],
) -> None:
  """Exact/direct rows and the semantic native reference carry benchmark IDs."""
  if not isinstance(clocks, dict) or not isinstance(coverage, dict):
    failures.append(f"{context}: missing extracted clocks")
    return
  required_rows = {"native_thread_cpu"}
  for route in coverage.values():
    if not isinstance(route, dict):
      continue
    selected = route.get("selected_row")
    candidates = route.get("eligible_exact_rows")
    if isinstance(selected, str):
      required_rows.add(selected)
    if isinstance(candidates, list):
      required_rows.update(candidate for candidate in candidates if isinstance(candidate, str))
  for row_name in sorted(required_rows):
    row = clocks.get(row_name)
    if not isinstance(row, dict):
      failures.append(f"{context}: missing benchmark row {row_name}")
      continue
    if not _supplemental_benchmark_matches_key(row_name, row.get("benchmark")):
      failures.append(f"{context}: benchmark identity is missing or mislabeled for {row_name}")
  native_expected = SUPPLEMENTAL_NATIVE_THREAD_CPU_IDENTITIES.get(
    (target_triple, harness, build_mode)
  )
  native = clocks.get("native_thread_cpu")
  if native_expected is not None:
    expected_benchmark, expected_provider, expected_cost = native_expected
    if not isinstance(native, dict) or any(
      native.get(field) != value
      for field, value in (
        ("benchmark", expected_benchmark),
        ("provider", expected_provider),
        ("read_cost", expected_cost),
        ("time_domain", "thread CPU"),
      )
    ):
      failures.append(f"{context}: native thread-CPU benchmark identity changed")


def _wall_selector_for_route(clocks: dict, use_case: str) -> tuple[object, object, object, object]:
  public_name, _, domain = SUPPLEMENTAL_ROUTE_ROWS[use_case]
  public = clocks.get(public_name)
  if not isinstance(public, dict):
    return None, None, None, None
  key = "selection" if use_case == "instant" else "wall_selection"
  selection = public.get(key)
  if not isinstance(selection, dict):
    return selection, None, None, None
  candidates = selection.get("eligible_direct_candidates")
  selected = selection.get("selected_provider")
  selected_benchmarks = selection.get("selected_native_benchmark")
  if not isinstance(candidates, dict) or not isinstance(selected, dict) or not isinstance(
    selected_benchmarks, dict
  ):
    return selection, None, None, None
  return selection, candidates.get(domain), selected.get(domain), selected_benchmarks.get(domain)


def _thread_cpu_selector_for_route(clocks: dict) -> tuple[object, object, object, object]:
  public = clocks.get("tach_thread_cpu")
  if not isinstance(public, dict):
    return None, None, None, None
  selection = public.get("selection")
  if not isinstance(selection, dict):
    return selection, None, None, None
  return (
    selection,
    selection.get("eligible_direct_candidates"),
    selection.get("selected_mechanism"),
    selection.get("selected_native_benchmark"),
  )


def observed_supplemental_selection_profiles(clocks: object) -> dict:
  """Classify profiles from collector-observed selector metadata alone."""
  if not isinstance(clocks, dict):
    raise ValueError("supplemental selector profiles need extracted clocks")
  profiles = {}
  for use_case in ("instant", "ordered"):
    selection, _, _, _ = _wall_selector_for_route(clocks, use_case)
    if not isinstance(selection, dict):
      raise ValueError(f"{use_case} selector metadata is incomplete")
    if isinstance(selection.get("fixed_provider"), dict):
      profiles[use_case] = "fixed_native"
    elif is_fixed_pick_wall_selection(selection):
      # A converted wall route: one capability-gated pick per contract, no probe.
      profiles[use_case] = "fixed_native"
    elif any(key in selection for key in ("probe", "instant_probe", "architecture")):
      profiles[use_case] = "runtime_tournament"
    else:
      raise ValueError(f"{use_case} selector profile is not observable")

  selection, _, _, _ = _thread_cpu_selector_for_route(clocks)
  if not isinstance(selection, dict):
    raise ValueError("thread_cpu selector metadata is incomplete")
  kind = selection.get("selection_kind")
  if kind in ("runtime_tournament", "tournament", "tournament_with_measured_runner_up"):
    profiles["thread_cpu"] = "runtime_tournament"
  elif kind in ("fixed_native", "fixed_candidate"):
    profiles["thread_cpu"] = "fixed_native"
  elif kind in (
    "availability_fallback",
    "fixed_windows_thread_times",
    "capability_preferred_with_failure_fallback",
  ):
    profiles["thread_cpu"] = "availability_fallback"
  elif kind == "fallback_only":
    profiles["thread_cpu"] = "fallback_only"
  else:
    raise ValueError("thread_cpu selector profile is not observable")
  return profiles


def _validate_supplemental_selection_profile(
  context: str,
  use_case: str,
  profile: object,
  selection: object,
  target_triple: str,
  failures: list[str],
) -> None:
  if profile not in SUPPLEMENTAL_SELECTION_PROFILES:
    failures.append(f"{context}: {use_case} has an unknown selection profile")
    return
  if not isinstance(selection, dict):
    failures.append(f"{context}: {use_case} route lacks observed selector metadata")
    return

  if use_case in ("instant", "ordered"):
    if profile not in ("runtime_tournament", "fixed_native"):
      failures.append(f"{context}: {use_case} uses a thread-only selection profile")
      return
    if profile == "runtime_tournament" and not any(
      key in selection for key in ("probe", "instant_probe", "architecture")
    ):
      failures.append(f"{context}: {use_case} runtime tournament has no selector evidence")
    if profile == "fixed_native":
      # A converted wall route is the minimal fixed-pick shape (no fixed_provider
      # dict): assert the selected provider is a mode-legal frozen pick for the
      # target family and that its selected-native benchmark is labeled to match.
      selected = selection.get("selected_provider")
      selected = selected.get(use_case) if isinstance(selected, dict) else selected
      benchmarks = selection.get("selected_native_benchmark")
      benchmark = benchmarks.get(use_case) if isinstance(benchmarks, dict) else None
      prefix = "direct_selected_wall" if use_case == "instant" else "direct_selected_ordered_wall"
      expected_picks = expected_wall_picks_for_triple(target_triple)
      if (
        not is_fixed_pick_wall_selection(selection)
        or not isinstance(selected, str)
        or benchmark != f"{prefix}__{selected}"
        or selected not in expected_picks[use_case]
      ):
        failures.append(f"{context}: {use_case} fixed-native route has no frozen fixed pick")
    return

  selection_kind = selection.get("selection_kind")
  if profile == "runtime_tournament":
    if selection_kind not in ("runtime_tournament", "tournament", "tournament_with_measured_runner_up"):
      failures.append(f"{context}: thread CPU runtime tournament has the wrong selector kind")
    if "perf" in selection and "native_entry_probe" in selection:
      validate_thread_cpu_selector(context, selection, failures, target_triple)
  elif profile == "fixed_native":
    fixed = selection.get("fixed_provider")
    if (
      selection_kind != "fixed_native"
      or not isinstance(fixed, dict)
      or fixed.get("candidate") != selection.get("selected_mechanism")
      or fixed.get("time_domain") != "thread CPU"
      or not isinstance(fixed.get("native_primitive"), str)
      or not fixed["native_primitive"].strip()
      or not isinstance(fixed.get("selection_basis"), str)
      or not fixed["selection_basis"].strip()
      or not isinstance(selection.get("read_cost_basis"), str)
      or not selection["read_cost_basis"].strip()
    ):
      failures.append(f"{context}: thread CPU fixed-native selector is incomplete")
    if target_triple.endswith("-apple-darwin"):
      validate_thread_cpu_selector(context, selection, failures, target_triple)
  elif profile == "availability_fallback":
    if selection_kind not in (
      "availability_fallback",
      "fixed_windows_thread_times",
      "capability_preferred_with_failure_fallback",
    ):
      failures.append(f"{context}: thread CPU availability selector has the wrong kind")
    if not isinstance(selection.get("failure_fallback"), dict):
      failures.append(f"{context}: thread CPU availability selector lacks a fallback branch")
    if selection_kind == "fixed_windows_thread_times":
      validate_thread_cpu_selector(context, selection, failures, target_triple)
    if selection_kind == "capability_preferred_with_failure_fallback":
      validate_thread_cpu_selector(context, selection, failures, target_triple)
  else:
    if selection_kind != "fallback_only" or selection.get("time_domain") != "monotonic wall fallback":
      failures.append(f"{context}: thread CPU fallback-only selector is incomplete")


def validate_supplemental_route_coverage(
  context: str,
  document: dict,
  failures: list[str],
) -> dict:
  clocks = document.get("clocks")
  coverage = document.get("route_coverage")
  profiles = document.get("selection_profiles")
  if (
    not isinstance(clocks, dict)
    or not isinstance(coverage, dict)
    or not isinstance(profiles, dict)
  ):
    failures.append(f"{context}: missing three-use-case route coverage")
    return {}
  if set(coverage) != set(SUPPLEMENTAL_ROUTE_ROWS) or set(profiles) != set(
    SUPPLEMENTAL_ROUTE_ROWS
  ):
    failures.append(f"{context}: route coverage must contain exactly all three public timers")
    return {}
  try:
    observed_profiles = observed_supplemental_selection_profiles(clocks)
  except ValueError as error:
    failures.append(f"{context}: {error}")
  else:
    if profiles != observed_profiles:
      failures.append(f"{context}: declared selection profiles disagree with observed selectors")

  results = {}
  mode = document.get("mode")
  target_triple = document.get("triple")
  if not isinstance(target_triple, str):
    target_triple = ""
  tach_wall = clocks.get("tach")
  wall_selection = tach_wall.get("selection") if isinstance(tach_wall, dict) else None
  wall_selector_reproduction = validate_wall_selector_reproduction(
    context, wall_selection, clocks, failures
  )
  tach_thread = clocks.get("tach_thread_cpu")
  thread_selection = (
    tach_thread.get("selection") if isinstance(tach_thread, dict) else None
  )
  thread_selector_failures: list[str] = []
  thread_selector_reproduction = (
    validate_thread_cpu_selector(
      context, thread_selection, thread_selector_failures, target_triple
    )
    if (
      isinstance(thread_selection, dict)
      and isinstance(thread_selection.get("public_exact_probe"), dict)
    )
    else {}
  )
  for failure in thread_selector_failures:
    if failure not in failures:
      failures.append(failure)
  for use_case, (expected_public, expected_selected, _) in SUPPLEMENTAL_ROUTE_ROWS.items():
    route = coverage.get(use_case)
    profile = profiles.get(use_case)
    if not isinstance(route, dict):
      failures.append(f"{context}: malformed {use_case} route coverage")
      continue
    public_name = route.get("public_row")
    selected_name = route.get("selected_row")
    candidates = route.get("eligible_exact_rows")
    if (
      set(route) != {"public_row", "selected_row", "eligible_exact_rows"}
      or public_name != expected_public
      or selected_name != expected_selected
      or not isinstance(candidates, list)
      or not candidates
      or len(set(candidates)) != len(candidates)
      or not all(isinstance(candidate, str) and candidate for candidate in candidates)
    ):
      failures.append(f"{context}: malformed {use_case} route identity")
      continue

    public = clocks.get(public_name)
    selected = clocks.get(selected_name)
    if not isinstance(public, dict) or not isinstance(selected, dict):
      failures.append(f"{context}: {use_case} public/selected rows are missing")
      continue
    validate_ci(context, public_name, public, failures)
    validate_ci(context, selected_name, selected, failures)

    if use_case in ("instant", "ordered"):
      selection, declared, selected_provider, selected_benchmark = _wall_selector_for_route(
        clocks, use_case
      )
    else:
      selection, declared, selected_provider, selected_benchmark = _thread_cpu_selector_for_route(
        clocks
      )
    _validate_supplemental_selection_profile(
      context, use_case, profile, selection, target_triple, failures
    )
    if declared != candidates:
      failures.append(f"{context}: {use_case} selector candidates disagree with exact rows")
    if selected_provider != selected.get("provider"):
      failures.append(f"{context}: {use_case} selected route identity disagrees")
    if selected_benchmark != selected.get("benchmark"):
      failures.append(f"{context}: {use_case} selected benchmark identity disagrees")
    if use_case == "thread_cpu" and isinstance(selection, dict):
      if selection.get("selected_read_cost") != selected.get("read_cost"):
        failures.append(f"{context}: thread_cpu selected read cost disagrees")

    expected_domain = (
      "monotonic wall fallback"
      if use_case == "thread_cpu" and mode == "tagged_wall_fallback"
      else ("thread CPU" if use_case == "thread_cpu" else f"{use_case} wall")
    )
    for row_name, row in ((public_name, public), (selected_name, selected)):
      if row.get("time_domain") != expected_domain:
        failures.append(f"{context}: {row_name} changed the declared {use_case} timeline")

    candidate_rows = {}
    for candidate in candidates:
      row = clocks.get(candidate)
      if not isinstance(row, dict):
        failures.append(f"{context}: {use_case} lacks eligible exact row {candidate}")
        continue
      validate_ci(context, candidate, row, failures)
      if row.get("benchmark") != candidate:
        failures.append(f"{context}: {use_case} exact row {candidate} is mislabeled")
      if row.get("time_domain") != expected_domain:
        failures.append(f"{context}: {use_case} exact row {candidate} changed time domain")
      if use_case in ("instant", "ordered"):
        expected_identity = exact_wall_candidate_identity(use_case, candidate)
        if expected_identity is None or any(
          row.get(field) != value for field, value in expected_identity.items()
        ):
          failures.append(f"{context}: {use_case} exact row {candidate} changed provider identity")
      else:
        mechanism = candidate.removeprefix("direct_thread_cpu__")
        if mechanism == candidate or row.get("provider") != mechanism:
          failures.append(f"{context}: thread_cpu exact row {candidate} changed provider identity")
        if isinstance(selection, dict) and mechanism == selection.get("selected_mechanism"):
          expected_cost = selection.get("selected_read_cost")
        elif mechanism.startswith("linux_perf_mmap__"):
          expected_cost = "inline"
        else:
          expected_cost = "system call"
        if row.get("read_cost") != expected_cost:
          failures.append(f"{context}: thread_cpu exact row {candidate} changed read cost")
      candidate_rows[candidate] = row

    route_results = {}
    for candidate, row in candidate_rows.items():
      metrics = {}
      comparison = public if row.get("provider") == selected.get("provider") else selected
      default_comparison_basis = (
        "public versus its selected exact route"
        if comparison is public
        else "selected exact route versus another eligible route"
      )
      for metric in METRICS:
        comparison_basis = default_comparison_basis
        public_reference_gate = None
        domain_reproduction = wall_selector_reproduction.get(use_case)
        public_exact_contract = wall_public_exact_contract(domain_reproduction)
        if comparison is public and use_case in ("instant", "ordered"):
          paired_public_exact = wall_public_exact_metric(domain_reproduction, metric)
        elif comparison is public and use_case == "thread_cpu":
          paired_public_exact = thread_public_exact_metric(
            thread_selector_reproduction, metric
          )
        else:
          paired_public_exact = None
        if (
          comparison is public
          and public_exact_contract == FREEBSD_PUBLIC_EXACT_CONTRACT
        ):
          passed, public_reference_gate = public_reference_winner_gate(
            clocks, use_case, metric, target_triple
          )
          allowance = None
          comparison_basis = (
            "runtime selector plus usable public-reference winner gate; "
            "exact route retained as a diagnostic lower bound"
          )
        elif isinstance(paired_public_exact, dict) and paired_public_exact.get("passed") is not None:
          passed = paired_public_exact.get("passed") is True
          allowance = paired_public_exact.get("equivalence_allowance_ns_per_read")
          comparison_basis = "alternating paired public/exact probe"
        else:
          try:
            passed, allowance = equivalent_or_faster(comparison, row, metric)
          except (KeyError, TypeError):
            failures.append(f"{context}: malformed {use_case} estimate for {candidate}")
            continue
        metrics[metric] = {
          "public_ns": public[metric],
          "comparison_ns": comparison[metric],
          "exact_ns": row[metric],
          "equivalence_allowance_ns": allowance,
          "comparison_basis": comparison_basis,
          "public_reference_gate": public_reference_gate,
          "paired_probe": paired_public_exact,
          "passed": passed,
        }
        if not passed:
          failures.append(
            f"{context}: {use_case} public {metric} loses to eligible exact route {candidate}"
          )
      route_results[candidate] = {
        "metrics": metrics,
        "passed": len(metrics) == len(METRICS) and all(
          result["passed"] for result in metrics.values()
        ),
      }
    results[use_case] = {
      "selection_profile": profile,
      "selected_row": selected_name,
      "eligible_exact_routes": route_results,
      "selector_reproduction": (
        wall_selector_reproduction.get(use_case)
        if use_case in ("instant", "ordered")
        else None
      ),
      "passed": (
        len(route_results) == len(candidates)
        and all(result["passed"] for result in route_results.values())
      ),
    }
  return results


def _selector_route_inputs(clocks: dict, use_case: str) -> tuple[list[str], str]:
  if use_case in ("instant", "ordered"):
    selection, candidates, _, selected_benchmark = _wall_selector_for_route(clocks, use_case)
  else:
    selection, candidates, _, selected_benchmark = _thread_cpu_selector_for_route(clocks)
  if not isinstance(selection, dict) or not isinstance(candidates, list) or not isinstance(
    selected_benchmark, str
  ):
    raise ValueError(f"{use_case} selector metadata is incomplete")
  return candidates, selected_benchmark


def supplemental_route_coverage_from_clocks(
  clocks: object,
  selection_profiles: object,
) -> dict:
  """Derive route rows solely from observed extractor output and typed profiles."""
  if not isinstance(clocks, dict):
    raise ValueError("supplemental composer needs an extracted clocks object")
  if not isinstance(selection_profiles, dict) or set(selection_profiles) != set(
    SUPPLEMENTAL_ROUTE_ROWS
  ) or any(profile not in SUPPLEMENTAL_SELECTION_PROFILES for profile in selection_profiles.values()):
    raise ValueError("supplemental composer needs one known profile per timer")
  observed_profiles = observed_supplemental_selection_profiles(clocks)
  if selection_profiles != observed_profiles:
    raise ValueError("supplemental composer profiles do not match observed selectors")
  routes = {}
  for use_case, (public_name, selected_name, _) in SUPPLEMENTAL_ROUTE_ROWS.items():
    candidates, selected_benchmark = _selector_route_inputs(clocks, use_case)
    selected = clocks.get(selected_name)
    if not isinstance(selected, dict) or selected.get("benchmark") != selected_benchmark:
      raise ValueError(f"{use_case} selected row does not match observed selector metadata")
    routes[use_case] = {
      "public_row": public_name,
      "selected_row": selected_name,
      "eligible_exact_rows": list(candidates),
    }
  return routes


def _selector_free_clocks(clocks: dict) -> dict:
  """Remove wall profiles while retaining thread public/exact parity evidence."""
  result = {}
  for name, entry in clocks.items():
    if not isinstance(entry, dict):
      result[name] = entry
      continue
    copied = dict(entry)
    if name == "tach":
      copied.pop("selection", None)
    elif name == "tach_ordered":
      copied.pop("wall_selection", None)
    elif name == "tach_thread_cpu":
      selection = copied.get("selection")
      if not isinstance(selection, dict) or not isinstance(
        selection.get("public_exact_probe"), dict
      ):
        copied.pop("selection", None)
    result[name] = copied
  return result


def validate_supplemental_speed_cell(name: str, document: object) -> dict:
  """Validate one externally-produced supplemental evidence cell."""
  failures: list[str] = []
  expected = SUPPLEMENTAL_SPEED_CELLS.get(name)
  context = f"supplemental {name}"
  if expected is None:
    return {
      "artifact": name,
      "passed": False,
      "failures": [f"{context}: unknown supplemental artifact"],
    }
  expected_triple, expected_harness, mode, expected_build_mode = expected
  if not isinstance(document, dict):
    return {
      "artifact": name,
      "passed": False,
      "failures": [f"{context}: document is not an object"],
    }
  expected_document_keys = {
    "schema",
    "triple",
    "mode",
    "build_mode",
    "provenance",
    "evidence_class",
  }
  if mode == "runtime_smoke":
    expected_document_keys.update({
      "passed",
      "smoke_schema",
      "runtime_attestation",
      "assertions",
    })
  else:
    expected_document_keys.update({
      "collector_bundle",
      "clocks",
      "selection_profiles",
      "route_coverage",
      "thread_cpu_behavior",
    })
  if set(document) != expected_document_keys:
    failures.append(f"{context}: document schema changed")
  provenance = document.get("provenance")
  if (
    document.get("schema") != SUPPLEMENTAL_SPEED_SCHEMA
    or document.get("triple") != expected_triple
    or document.get("mode") != mode
    or document.get("build_mode") != expected_build_mode
    or not isinstance(provenance, dict)
    or provenance.get("harness") != expected_harness
  ):
    failures.append(f"{context}: identity/provenance does not match the checked cell")
  revision = provenance.get("source_revision") if isinstance(provenance, dict) else None
  if not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", revision):
    failures.append(f"{context}: invalid full source revision")

  if mode == "runtime_smoke":
    assertions = document.get("assertions")
    if (
      document.get("evidence_class") != "runtime_smoke"
      or document.get("passed") is not True
      or document.get("smoke_schema") != RUNTIME_SMOKE_ATTESTATION_SCHEMA
      or not isinstance(assertions, list)
      or not assertions
      or not all(isinstance(assertion, str) and assertion for assertion in assertions)
    ):
      failures.append(f"{context}: malformed runtime smoke evidence")
    runtime_attestation = validate_runtime_attestation(
      context,
      document.get("runtime_attestation"),
      expected_triple,
      expected_harness,
      expected_build_mode,
      revision,
      failures,
      runtime_smoke=True,
    )
    validate_provenance_runtime_binding(context, provenance, runtime_attestation, failures)
    return {
      "artifact": name,
      "mode": mode,
      "build_mode": expected_build_mode,
      "source_revision": revision,
      "passed": not failures,
      "failures": failures,
    }

  clocks = document.get("clocks")
  if document.get("evidence_class") != "measured_external_runtime" or not isinstance(
    clocks, dict
  ):
    failures.append(f"{context}: malformed measured three-clock evidence")
    return {
      "artifact": name,
      "mode": mode,
      "build_mode": expected_build_mode,
      "source_revision": revision,
      "passed": False,
      "failures": failures,
    }
  runtime_attestation = validate_collector_attestation(
    context,
    clocks.get("collector_attestation"),
    expected_triple,
    expected_harness,
    expected_build_mode,
    revision,
    failures,
  )
  validate_collector_bundle_descriptor(
    context, document.get("collector_bundle"), clocks.get("collector_attestation"), failures
  )
  validate_provenance_runtime_binding(context, provenance, runtime_attestation, failures)
  route_results = validate_supplemental_route_coverage(context, document, failures)
  validate_supplemental_benchmark_identities(
    context,
    clocks,
    document.get("route_coverage"),
    expected_triple,
    expected_harness,
    expected_build_mode,
    failures,
  )
  expected_domain = "monotonic wall fallback" if mode == "tagged_wall_fallback" else "thread CPU"
  native = clocks.get("native_thread_cpu")
  direct_benchmark = native.get("benchmark") if isinstance(native, dict) else None
  semantic_results = validate_thread_cpu_behavior(
    context,
    document.get("thread_cpu_behavior"),
    expected_domain,
    direct_benchmark,
    runtime_attestation,
    failures,
  )

  cell_report = {}
  if mode == "full_speed_cell":
    reference_eligibility = local_reference_eligibility(expected_triple)
    required_clocks = {
      "tach", "tach_ordered", "tach_thread_cpu", "native_thread_cpu",
      *(
        name
        for name, policy in reference_eligibility.items()
        if policy["eligible"]
      ),
    }
    if not required_clocks <= set(clocks):
      failures.append(f"{context}: full speed cell omits public/reference clocks")
    else:
      cell_failures, cell_report = validate_cell(
        context, _selector_free_clocks(clocks), expected_triple
      )
      failures.extend(cell_failures)

  if mode == "tagged_wall_fallback":
    public = clocks.get("tach_thread_cpu")
    if not isinstance(public, dict) or public.get("time_domain") != expected_domain:
      failures.append(f"{context}: fallback runtime was not explicitly tagged as wall time")
  return {
    "artifact": name,
    "mode": mode,
    "build_mode": expected_build_mode,
    "source_revision": revision,
    "three_clock_speed": cell_report,
    "route_coverage": route_results,
    "semantics": semantic_results,
    "passed": not failures,
    "failures": failures,
  }


def compose_supplemental_speed_cell(
  name: str,
  clocks: object,
  raw_behavior: object,
  source_revision: str,
  selection_profiles: object,
  provenance_extra: object = None,
  runtime_smoke: object = None,
  collector_bundle_path: object = "collector.bundle",
) -> dict:
  """Compose one validated supplemental cell without consulting static declarations."""
  expected = SUPPLEMENTAL_SPEED_CELLS.get(name)
  if expected is None:
    raise ValueError(f"unknown supplemental artifact {name!r}")
  triple, harness, mode, build_mode = expected
  extra = {} if provenance_extra is None else provenance_extra
  if not isinstance(extra, dict) or any(
    key in {
      "harness",
      "source_revision",
      "runner",
      "build_profile",
      "build_mode",
      "features",
    }
    for key in extra
  ):
    raise ValueError("supplemental provenance extras cannot replace identity fields")
  if mode == "runtime_smoke":
    if not isinstance(runtime_smoke, dict) or set(runtime_smoke) != {
      "schema", "runtime_attestation", "assertions"
    } or runtime_smoke.get("schema") != RUNTIME_SMOKE_ATTESTATION_SCHEMA:
      raise ValueError("runtime smoke composition needs a producer attestation")
    runtime_attestation = runtime_smoke["runtime_attestation"]
    provenance = runtime_identity_provenance(runtime_attestation)
    if provenance.get("source_revision") != source_revision:
      raise ValueError("runtime smoke source revision does not match its attestation")
    document = {
      "schema": SUPPLEMENTAL_SPEED_SCHEMA,
      "triple": triple,
      "mode": mode,
      "build_mode": build_mode,
      "provenance": {**provenance, **extra},
    }
    document.update({
      "evidence_class": "runtime_smoke",
      "passed": True,
      "smoke_schema": runtime_smoke["schema"],
      "runtime_attestation": runtime_smoke["runtime_attestation"],
      "assertions": runtime_smoke["assertions"],
    })
  else:
    collector = clocks.get("collector_attestation") if isinstance(clocks, dict) else None
    runtime_attestation = (
      collector.get("runtime_attestation") if isinstance(collector, dict) else None
    )
    provenance = runtime_identity_provenance(runtime_attestation)
    if provenance.get("source_revision") != source_revision:
      raise ValueError("collector source revision does not match the requested revision")
    document = {
      "schema": SUPPLEMENTAL_SPEED_SCHEMA,
      "triple": triple,
      "mode": mode,
      "build_mode": build_mode,
      "provenance": {**provenance, **extra},
    }
    public_thread_cpu = clocks.get("tach_thread_cpu") if isinstance(clocks, dict) else None
    public_time_domain = (
      public_thread_cpu.get("time_domain")
      if isinstance(public_thread_cpu, dict)
      else None
    )
    document.update({
      "evidence_class": "measured_external_runtime",
      "collector_bundle": {
        "schema": COLLECTOR_BUNDLE_DESCRIPTOR_SCHEMA,
        "path": collector_bundle_path,
        "manifest_sha256": collector.get("manifest_sha256")
        if isinstance(collector, dict) else None,
      },
      "clocks": clocks,
      "selection_profiles": selection_profiles,
      "route_coverage": supplemental_route_coverage_from_clocks(
        clocks, selection_profiles
      ),
      "thread_cpu_behavior": build_thread_cpu_behavior(raw_behavior, public_time_domain),
    })
  report = validate_supplemental_speed_cell(name, document)
  if not report["passed"]:
    raise ValueError("supplemental cell does not validate:\n  " + "\n  ".join(report["failures"]))
  return document


def validate_supplemental_speed_cell_from_bundle(
  name: str,
  document: object,
  bundle_dir: Path,
) -> dict:
  """Re-extract a cell from its retained bundle before accepting a speed claim."""
  report = validate_supplemental_speed_cell(name, document)
  failures = list(report["failures"])
  binding_failures: list[str] = []
  context = f"supplemental {name}"
  expected = SUPPLEMENTAL_SPEED_CELLS.get(name)
  if expected is None or not isinstance(document, dict):
    return report
  _, _, mode, _ = expected
  if mode == "runtime_smoke":
    binding_failures.append(f"{context}: runtime smoke evidence has no collector bundle")
  else:
    try:
      import extract_speed

      observation = extract_speed.extract_collector_bundle_observation(bundle_dir)
    except (OSError, RuntimeError, ValueError) as error:
      binding_failures.append(f"{context}: cannot verify retained collector bundle: {error}")
    else:
      extracted_clocks = observation.get("clocks")
      collector = observation.get("collector_attestation")
      observed_clocks = (
        {**extracted_clocks, "collector_attestation": collector}
        if isinstance(extracted_clocks, dict) and isinstance(collector, dict)
        else extracted_clocks
      )
      observed_behavior = observation.get("thread_cpu_behavior")
      if document.get("clocks") != observed_clocks:
        binding_failures.append(
          f"{context}: serialized clocks differ from retained collector bundle"
        )
      if not isinstance(observed_clocks, dict) or not isinstance(observed_behavior, dict):
        binding_failures.append(
          f"{context}: retained collector bundle lacks measured thread CPU data"
        )
      else:
        public = observed_clocks.get("tach_thread_cpu")
        time_domain = public.get("time_domain") if isinstance(public, dict) else None
        try:
          observed_semantics = build_thread_cpu_behavior(observed_behavior, time_domain)
        except ValueError as error:
          binding_failures.append(
            f"{context}: retained thread-CPU behavior is malformed: {error}"
          )
        else:
          if document.get("thread_cpu_behavior") != observed_semantics:
            binding_failures.append(
              f"{context}: serialized thread-CPU behavior differs from retained collector bundle"
            )
      descriptor = document.get("collector_bundle")
      if isinstance(descriptor, dict) and isinstance(collector, dict):
        if descriptor.get("manifest_sha256") != collector.get("manifest_sha256"):
          binding_failures.append(f"{context}: retained collector bundle digest changed")

  report["bundle_binding"] = {
    "path": str(bundle_dir),
    "passed": not binding_failures,
  }
  report["failures"] = [*failures, *binding_failures]
  report["passed"] = not report["failures"]
  return report


def validate_supplemental_speed_campaign(
  documents: dict[str, dict],
  cell_paths: dict[str, Path] | None = None,
  require_bound_observations: bool = True,
  expected_artifact_ids: set[str] | None = None,
) -> dict:
  """Validate supplemental evidence, requiring retained observations by default.

  Measured cells are release evidence only when their serialized claims
  reproduce from a retained collector bundle. Runtime-smoke cells are the one
  intentionally data-free mode: their producer attestation is the evidence.
  `require_bound_observations=False` is limited to structural unit tests.
  """
  failures = []
  results = []
  expected_names = (
    set(SUPPLEMENTAL_SPEED_CELLS)
    if expected_artifact_ids is None
    else set(expected_artifact_ids)
  )
  unknown_names = expected_names - set(SUPPLEMENTAL_SPEED_CELLS)
  if unknown_names:
    failures.append(
      f"supplemental campaign requires unknown artifacts: {sorted(unknown_names)!r}"
    )
  actual_names = set(documents)
  if actual_names != expected_names:
    failures.append(
      "supplemental three-clock artifacts differ: "
      f"missing={sorted(expected_names - actual_names)!r}, "
      f"unexpected={sorted(actual_names - expected_names)!r}"
    )
  revisions = set()
  for name in sorted(expected_names & actual_names):
    document = documents[name]
    expected = SUPPLEMENTAL_SPEED_CELLS[name]
    _, _, mode, _ = expected
    cell_path = cell_paths.get(name) if cell_paths is not None else None
    descriptor = document.get("collector_bundle") if isinstance(document, dict) else None
    relative = descriptor.get("path") if isinstance(descriptor, dict) else None
    bundle_path = PurePosixPath(relative) if isinstance(relative, str) else None
    safe_bundle_path = (
      bundle_path is not None
      and relative
      and "\\" not in relative
      and not bundle_path.is_absolute()
      and all(part not in {"", ".", ".."} for part in bundle_path.parts)
      and bundle_path.as_posix() == relative
    )
    if mode == "runtime_smoke":
      result = validate_supplemental_speed_cell(name, document)
    elif cell_path is not None and safe_bundle_path:
      result = validate_supplemental_speed_cell_from_bundle(
        name, document, cell_path.parent / bundle_path
      )
    elif require_bound_observations:
      result = validate_supplemental_speed_cell(name, document)
      result["failures"].append(
        f"supplemental {name}: missing retained collector bundle path"
      )
      result["passed"] = False
    else:
      result = validate_supplemental_speed_cell(name, document)
    failures.extend(result["failures"])
    revision = result.get("source_revision")
    if isinstance(revision, str) and re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", revision):
      revisions.add(revision)
    results.append({key: value for key, value in result.items() if key != "failures"})
  source_revisions = sorted(revisions)
  return {
    "schema": "tach-speed-supplemental-report-v2",
    "claim_scope": (
      "runtime decision-boundary coverage for all three public timing contracts on the "
      "declared target/build-mode/host-runtime identities"
    ),
    "evidence_class_invariant": (
      "measured runtime, tagged wall fallback, and runtime smoke evidence remain distinct; "
      "the static route manifest and compile/codegen proof are never accepted as latency evidence"
    ),
    "source_revision": source_revisions[0] if len(source_revisions) == 1 else None,
    "source_revisions": source_revisions,
    "passed": not failures,
    "failures": failures,
    "cells": results,
  }


def validate_campaign_for_checkout(
  documents: dict[str, dict],
  root: Path,
  cell_paths: dict[str, Path] | None = None,
) -> dict:
  """Validate a retained primary campaign and bind its common source checkout."""
  report = validate_primary_speed_campaign(
    documents,
    cell_paths,
  )
  revision = report.get("source_revision")
  if isinstance(revision, str):
    binding_failures, binding = validate_checkout_binding(root, revision)
  else:
    binding_failures = ["campaign has no single source revision to bind"]
    binding = {}
  report["checkout_binding"] = {"passed": not binding_failures, **binding}
  report["failures"].extend(binding_failures)
  report["passed"] = not report["failures"]
  return report
