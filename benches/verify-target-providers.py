#!/usr/bin/env python3
"""Compile every supported target and verify public clock-route closures."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]

TARGETS = (
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
  "i686-unknown-linux-gnu",
  "x86_64-pc-windows-msvc",
  "i686-pc-windows-msvc",
  "aarch64-pc-windows-msvc",
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
  "s390x-unknown-linux-gnu",
  "loongarch64-unknown-linux-gnu",
  "riscv64gc-unknown-linux-gnu",
  "powerpc64-unknown-linux-gnu",
  "powerpc64le-unknown-linux-gnu",
  "armv7-unknown-linux-gnueabihf",
  "x86_64-unknown-linux-musl",
  "x86_64-linux-android",
  "aarch64-linux-android",
  "x86_64-unknown-freebsd",
  "wasm32-unknown-unknown",
  "wasm32-wasip1",
  "wasm32-wasip2",
  "wasm32-unknown-emscripten",
  "wasm32-wasip1-threads",
  "wasm32v1-none",
)

BENCHMARKED_TARGETS = {
  "aarch64-apple-darwin",
  "aarch64-unknown-linux-gnu",
  "x86_64-pc-windows-msvc",
  "x86_64-unknown-linux-gnu",
  "x86_64-unknown-linux-musl",
}

RELEASE_REQUIRED_HOSTED_SPEED_TARGETS = {
  "x86_64-apple-darwin",
  "i686-unknown-linux-gnu",
  "i686-pc-windows-msvc",
  "aarch64-pc-windows-msvc",
  "x86_64-unknown-freebsd",
  "wasm32-unknown-unknown",
  "wasm32-unknown-emscripten",
  "wasm32-wasip1",
  "wasm32-wasip1-threads",
  "wasm32-wasip2",
  "wasm32v1-none",
}

LINUX_X86_WALL_TARGETS = {
  "i686-unknown-linux-gnu",
  "x86_64-linux-android",
  "x86_64-unknown-linux-gnu",
  "x86_64-unknown-linux-musl",
}

PERF_MMAP_THREAD_CPU_TARGETS = {
  "armv7-unknown-linux-gnueabihf",
  "i686-unknown-linux-gnu",
  "x86_64-linux-android",
  "x86_64-unknown-linux-gnu",
  "x86_64-unknown-linux-musl",
  "aarch64-linux-android",
  "aarch64-unknown-linux-gnu",
  "riscv64gc-unknown-linux-gnu",
}

PERF_READ_THREAD_CPU_TARGETS = {
  "armv7-unknown-linux-gnueabihf",
  "i686-unknown-linux-gnu",
  "x86_64-linux-android",
  "x86_64-unknown-linux-gnu",
  "x86_64-unknown-linux-musl",
  "aarch64-linux-android",
  "aarch64-unknown-linux-gnu",
  "riscv64gc-unknown-linux-gnu",
  "s390x-unknown-linux-gnu",
  "loongarch64-unknown-linux-gnu",
  "powerpc64-unknown-linux-gnu",
  "powerpc64le-unknown-linux-gnu",
}

POWERPC64_TARGETS = {
  "powerpc64-unknown-linux-gnu",
  "powerpc64le-unknown-linux-gnu",
}

LINUX_VDSO_TARGETS = {
  *LINUX_X86_WALL_TARGETS,
  "aarch64-linux-android",
  "aarch64-unknown-linux-gnu",
  "armv7-unknown-linux-gnueabihf",
  "loongarch64-unknown-linux-gnu",
  "powerpc64-unknown-linux-gnu",
  "powerpc64le-unknown-linux-gnu",
  "riscv64gc-unknown-linux-gnu",
  "s390x-unknown-linux-gnu",
}

# `OrderedInstant::elapsed_unordered()` deliberately uses a weaker endpoint
# contract than `OrderedInstant::elapsed()`. The M2 public `now() + elapsed()`
# proof covers the ordinary ordered endpoint; a future claim about the weaker
# endpoint needs its own provider specification rather than inheriting this one.
ORDERED_UNORDERED_ELAPSED_EXCLUSION = (
  "OrderedInstant::elapsed_unordered() has a deliberately weaker end-read ordering "
  "contract and is outside the ordinary now-plus-elapsed route proof."
)

# These are the Emscripten imports emitted by the guarded `em_js` shims. The
# first reaches `globalThis.performance.now()` and the second reaches Node's
# `process.hrtime.bigint()`; both are local-wall candidates selected by the
# startup tournament.
EMSCRIPTEN_LOCAL_WALL_IMPORTS = (
  "tach_emscripten_performance_hot_millis",
  "tach_emscripten_node_hrtime_hot_millis",
)
EMSCRIPTEN_ORDERED_PTHREAD_IMPORTS = (
  "tach_emscripten_performance_epoch_now",
  "tach_emscripten_get_now_millis",
  "tach_emscripten_shared_memory",
  "tach_emscripten_pthread_build",
)
EMSCRIPTEN_NODE_THREAD_CPU_IMPORT = "tach_node_thread_cpu_usage_micros"

PROBE_SOURCE = """#![no_std]

use tach::{Instant, OrderedInstant, ThreadCpuInstant};

#[unsafe(no_mangle)]
#[inline(never)]
pub fn tach_probe_instant_now() -> Instant {
  Instant::now()
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn tach_probe_instant_elapsed(start: Instant) -> u64 {
  start.elapsed().as_nanos() as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn tach_probe_instant_now_elapsed() -> u64 {
  tach_probe_instant_elapsed(tach_probe_instant_now())
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn tach_probe_ordered_instant_now() -> OrderedInstant {
  OrderedInstant::now()
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn tach_probe_ordered_instant_elapsed(start: OrderedInstant) -> u64 {
  start.elapsed().as_nanos() as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn tach_probe_ordered_instant_now_elapsed() -> u64 {
  tach_probe_ordered_instant_elapsed(tach_probe_ordered_instant_now())
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn tach_probe_thread_cpu_now() -> ThreadCpuInstant {
  ThreadCpuInstant::now()
}

#[unsafe(no_mangle)]
#[inline(never)]
pub fn tach_probe_thread_cpu_elapsed(start: ThreadCpuInstant) -> u64 {
  start.elapsed().as_nanos() as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn tach_probe_thread_cpu_now_elapsed() -> u64 {
  tach_probe_thread_cpu_elapsed(tach_probe_thread_cpu_now())
}

#[unsafe(no_mangle)]
pub extern "C" fn tach_probe_public_api() -> u64 {
  let local = Instant::now();
  let ordered = OrderedInstant::now();
  let thread = ThreadCpuInstant::now();
  let provider = ThreadCpuInstant::provider();
  let _ = ThreadCpuInstant::read_cost_hint();
  let _ = thread.checked_duration_since(thread);
  let _ = thread.partial_cmp(&thread);
  local.elapsed().as_nanos() as u64
    + ordered.elapsed().as_nanos() as u64
    + thread.elapsed().as_nanos() as u64
    + u64::from(provider.measures_thread_cpu_time())
}
"""


def parse_args() -> argparse.Namespace:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("--output-dir", type=Path, default=ROOT / "target/provider-proof")
  parser.add_argument("--toolchain", default="stable")
  parser.add_argument("--jobs", type=int, default=min(4, os.cpu_count() or 1))
  parser.add_argument("--install-targets", action="store_true")
  return parser.parse_args()


def command_text(command: list[str]) -> str:
  return " ".join(command)


def run(command: list[str], log: Path, env: dict[str, str] | None = None) -> None:
  result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
  with log.open("a") as output:
    output.write(f"$ {command_text(command)}\n")
    output.write(result.stdout)
    output.write(result.stderr)
  if result.returncode:
    raise RuntimeError(f"command failed ({result.returncode}): {command_text(command)}")


def make_probe(directory: Path, toolchain: str) -> Path:
  probe = directory / "probe"
  (probe / "src").mkdir(parents=True)
  manifest = f"""[package]
name = "tach-architecture-probe"
version = "0.0.0"
edition = "2024"

[lib]
crate-type = ["rlib"]

[features]
default = ["thread-cpu-inline"]
thread-cpu-inline = ["tach/thread-cpu-inline"]
emscripten-pthreads = ["tach/emscripten-pthreads"]

[dependencies]
tach = {{ path = {json.dumps(ROOT.as_posix())}, default-features = false }}

[profile.release]
codegen-units = 1
lto = "fat"
panic = "abort"
"""
  (probe / "Cargo.toml").write_text(manifest)
  (probe / "src/lib.rs").write_text(PROBE_SOURCE)
  run(
    ["cargo", f"+{toolchain}", "generate-lockfile", "--manifest-path", str(probe / "Cargo.toml")],
    directory / "probe-lock.log",
  )
  return probe


def ensure_targets(toolchain: str, install: bool) -> None:
  command = ["rustup", "target", "list", "--installed", "--toolchain", toolchain]
  installed = set(subprocess.check_output(command, text=True).splitlines())
  missing = sorted(set(TARGETS) - installed)
  if not missing:
    return
  if not install:
    rendered = " ".join(missing)
    raise RuntimeError(f"missing targets: {rendered}; rerun with --install-targets")
  subprocess.run(
    ["rustup", "target", "add", "--toolchain", toolchain, *missing],
    cwd=ROOT,
    check=True,
  )


def feature_args(mode: str) -> list[str]:
  if mode == "default":
    return []
  if mode == "no-default":
    return ["--no-default-features"]
  if mode == "emscripten-pthreads":
    return ["--features", "emscripten-pthreads"]
  raise ValueError(f"unknown provider-proof mode: {mode}")


def target_modes(target: str) -> tuple[str, ...]:
  modes = ("default", "no-default")
  if target == "wasm32-unknown-emscripten":
    return (*modes, "emscripten-pthreads")
  return modes


def mode_rustflags(mode: str) -> str:
  if mode == "emscripten-pthreads":
    return "-D warnings -C target-feature=+atomics,+bulk-memory,+mutable-globals"
  return "-D warnings"


def check_target(
  target: str,
  toolchain: str,
  output: Path,
  probe: Path,
) -> dict[str, Path]:
  log = output / "logs" / f"{target}.log"
  log.parent.mkdir(parents=True, exist_ok=True)
  env = os.environ.copy()
  env["CARGO_TERM_COLOR"] = "never"
  artifacts = {}
  for mode in target_modes(target):
    mode_env = env.copy()
    mode_env["RUSTFLAGS"] = mode_rustflags(mode)
    args = feature_args(mode)
    crate_target = output / "build" / target / f"crate-{mode}"
    run(
      [
        "cargo",
        f"+{toolchain}",
        "check",
        "--locked",
        "--lib",
        "--target",
        target,
        "--target-dir",
        str(crate_target),
        *args,
      ],
      log,
      mode_env,
    )
    api_target = output / "build" / target / f"api-{mode}"
    run(
      [
        "cargo",
        f"+{toolchain}",
        "check",
        "--locked",
        "--manifest-path",
        str(probe / "Cargo.toml"),
        "--lib",
        "--target",
        target,
        "--target-dir",
        str(api_target),
        *args,
      ],
      log,
      mode_env,
    )
    codegen_target = output / "build" / target / f"codegen-{mode}"
    run(
      [
        "cargo",
        f"+{toolchain}",
        "rustc",
        "--locked",
        "--manifest-path",
        str(probe / "Cargo.toml"),
        "--lib",
        "--release",
        "--target",
        target,
        "--target-dir",
        str(codegen_target),
        *args,
        "--",
        "--emit=llvm-ir",
      ],
      log,
      mode_env,
    )
    candidates = list(
      (codegen_target / target / "release/deps").glob("tach_architecture_probe-*.ll")
    )
    if len(candidates) != 1:
      raise RuntimeError(f"{target} {mode}: expected one LLVM IR file, found {len(candidates)}")
    artifacts[mode] = candidates[0]
    implementation_target = output / "build" / target / f"codegen-implementation-{mode}"
    run(
      [
        "cargo",
        f"+{toolchain}",
        "rustc",
        "--locked",
        "--lib",
        "--release",
        "--target",
        target,
        "--target-dir",
        str(implementation_target),
        *args,
        "--",
        "--emit=llvm-ir",
      ],
      log,
      mode_env,
    )
    implementation_candidates = list(
      (implementation_target / target / "release/deps").glob("tach-*.ll")
    )
    if len(implementation_candidates) != 1:
      raise RuntimeError(
        f"{target} {mode}: expected one implementation LLVM IR file, "
        f"found {len(implementation_candidates)}"
      )
    artifacts[f"{mode}-implementation"] = implementation_candidates[0]
  if target in LINUX_VDSO_TARGETS:
    resolver_target = output / "build" / target / "codegen-vdso-resolver"
    resolver_env = env.copy()
    resolver_env["RUSTFLAGS"] = mode_rustflags("default")
    run(
      [
        "cargo",
        f"+{toolchain}",
        "rustc",
        "--locked",
        "--lib",
        "--release",
        "--target",
        target,
        "--target-dir",
        str(resolver_target),
        "--",
        "--emit=llvm-ir",
      ],
      log,
      resolver_env,
    )
    candidates = list((resolver_target / target / "release/deps").glob("tach-*.ll"))
    if len(candidates) != 1:
      raise RuntimeError(
        f"{target} vDSO resolver: expected one LLVM IR file, found {len(candidates)}"
      )
    artifacts["vdso-resolver"] = candidates[0]
  return artifacts


def llvm_functions(ir: str) -> dict[str, str]:
  definitions = {}
  lines = ir.splitlines()
  index = 0
  pattern = re.compile(r'^define\b.*@(?:"([^"]+)"|([-A-Za-z0-9_.$]+))\(')
  while index < len(lines):
    match = pattern.match(lines[index])
    if not match:
      index += 1
      continue
    name = match.group(1) or match.group(2)
    body = [lines[index]]
    index += 1
    while index < len(lines):
      body.append(lines[index])
      if lines[index] == "}":
        break
      index += 1
    definitions[name] = "\n".join(body)
    index += 1
  return definitions


def normalize_tach_ir_symbols(ir: str) -> str:
  return re.sub(r"Cs[-A-Za-z0-9_]+_4tach", "CsTACH_4tach", ir)


def llvm_aliases(ir: str) -> dict[str, str]:
  aliases = {}
  pattern = re.compile(
    r'^@(?:"([^"]+)"|([-A-Za-z0-9_.$]+))\s*=.*\balias\b.*'
    r'@(?:"([^"]+)"|([-A-Za-z0-9_.$]+))'
  )
  for line in ir.splitlines():
    match = pattern.match(line)
    if match:
      source = match.group(1) or match.group(2)
      destination = match.group(3) or match.group(4)
      aliases[source] = destination
  return aliases


def llvm_globals(ir: str) -> dict[str, str]:
  definitions = {}
  pattern = re.compile(r'^@(?:"([^"]+)"|([-A-Za-z0-9_.$]+))\s*=')
  for line in ir.splitlines():
    match = pattern.match(line)
    if match:
      definitions[match.group(1) or match.group(2)] = line
  return definitions


def resolve_alias(name: str, aliases: dict[str, str]) -> str:
  visited = set()
  while name in aliases:
    if name in visited:
      raise RuntimeError(f"LLVM alias cycle contains {name}")
    visited.add(name)
    name = aliases[name]
  return name


def reachable_ir(ir: str, root: str) -> str:
  definitions = llvm_functions(ir)
  aliases = llvm_aliases(ir)
  resolved_root = resolve_alias(root, aliases)
  if resolved_root not in definitions:
    raise RuntimeError(f"optimized probe is missing {root}")
  symbol = re.compile(r'@(?:"([^"]+)"|([-A-Za-z0-9_.$]+))')
  pending = [resolved_root]
  visited = set()
  bodies = []
  while pending:
    name = pending.pop()
    if name in visited:
      continue
    visited.add(name)
    body = definitions.get(name)
    if body is None:
      continue
    bodies.append(body)
    for match in symbol.finditer(body):
      called = match.group(1) or match.group(2)
      called = resolve_alias(called, aliases)
      if called in definitions and called not in visited:
        pending.append(called)
  return "\n".join(bodies)


def direct_callees(ir: str, root: str) -> set[str]:
  definitions = llvm_functions(ir)
  aliases = llvm_aliases(ir)
  resolved_root = resolve_alias(root, aliases)
  body = definitions.get(resolved_root)
  if body is None:
    raise RuntimeError(f"optimized probe is missing {root}")
  symbol = re.compile(r'@(?:"([^"]+)"|([-A-Za-z0-9_.$]+))')
  return {
    resolve_alias(match.group(1) or match.group(2), aliases)
    for match in symbol.finditer(body)
  }


def require_composed_phase_roots(
  ir: str,
  route_name: str,
  root: str,
  phase_roots: dict[str, str],
) -> dict[str, str]:
  if len(set(phase_roots.values())) != len(phase_roots):
    raise ValueError(f"{route_name} phase roots must be distinct")
  aliases = llvm_aliases(ir)
  callees = direct_callees(ir, root)
  missing = [
    phase
    for phase, phase_root in phase_roots.items()
    if resolve_alias(phase_root, aliases) not in callees
  ]
  if missing:
    raise RuntimeError(
      f"{route_name} route proof failed: missing direct phase roots {missing}"
    )
  return {
    phase: resolve_alias(phase_root, aliases)
    for phase, phase_root in phase_roots.items()
  }


def reachable_ir_with_globals(ir: str, root: str) -> str:
  closure = reachable_ir(ir, root)
  aliases = llvm_aliases(ir)
  definitions = llvm_globals(ir)
  symbol = re.compile(r'@(?:"([^"]+)"|([-A-Za-z0-9_.$]+))')
  pending = [
    resolve_alias(match.group(1) or match.group(2), aliases)
    for match in symbol.finditer(closure)
  ]
  visited = set()
  globals_reached = []
  while pending:
    name = pending.pop()
    if name in visited:
      continue
    visited.add(name)
    definition = definitions.get(name)
    if definition is None:
      continue
    globals_reached.append(definition)
    pending.extend(
      resolve_alias(match.group(1) or match.group(2), aliases)
      for match in symbol.finditer(definition)
    )
  return "\n".join([closure, *globals_reached])


def clock_gettime(clock_id: int) -> str:
  return rf"@clock_gettime\([^,\n]*\b{clock_id},"


def linux_thread_clock_syscall(target: str) -> str | None:
  if target.startswith("x86_64"):
    return r'asm sideeffect inteldialect "syscall"[^\n]*\(i64 228, i32 3,'
  if target.startswith("aarch64"):
    return r'asm sideeffect "svc 0"[^\n]*\(i64 3, ptr [^\n]*, i64 113\)'
  return None


def perf_read_entry_patterns(target: str) -> list[str]:
  if target.startswith("x86_64"):
    return [r'asm sideeffect inteldialect "syscall"[^\n]*\(i64 0, i64 [^,]+, ptr [^,]+, i64 8']
  if target.startswith("aarch64"):
    return [r'asm sideeffect "svc 0"[^\n]*\(i64 [^,]+, ptr [^,]+, i64 8, i64 63']
  if target == "armv7-unknown-linux-gnueabihf":
    return [r'asm sideeffect alignstack "push \{r7\}.*mov r7, .*svc 0.*\(i32 3,']
  if target == "riscv64gc-unknown-linux-gnu":
    return [r'asm sideeffect "ecall"[^\n]*\(i64 63, i64 [^,]+, ptr [^,]+, i64 8']
  if target == "loongarch64-unknown-linux-gnu":
    return [r'asm sideeffect "syscall 0"[^\n]*\(i64 63, i64 [^,]+, ptr [^,]+, i64 8']
  if target == "s390x-unknown-linux-gnu":
    return [r'asm sideeffect "svc 3"']
  if target == "i686-unknown-linux-gnu":
    return [
      r'asm sideeffect alignstack inteldialect "push ebx\\0Amov ebx, .*\\0Aint 0x80',
      r"@syscall\([^,\n]*\b3,",
      r"@read\(",
    ]
  if target in POWERPC64_TARGETS:
    return [
      r'asm sideeffect "sc\\0Amfcr 6"',
      r'asm sideeffect ".machine push\\0A.machine power9\\0Ascv 0',
      r"@read\(",
    ]
  raise ValueError(f"{target} has no persistent perf-read entry contract")


def direct_vdso_hot_patterns(target: str) -> list[str]:
  patterns = ["linux_vdso13CLOCK_GETTIME"]
  if target in ("i686-unknown-linux-gnu", "armv7-unknown-linux-gnueabihf"):
    patterns.append("linux_vdso15CLOCK_GETTIME64")
  if target in POWERPC64_TARGETS:
    patterns.append(r'asm sideeffect alignstack "mtctr 0\\0Abctrl"')
  else:
    patterns.append(
      r"(?:linux_vdso18call_clock_gettime|"
      r"(%[-A-Za-z0-9_.]+) = inttoptr i(?:32|64) %[-A-Za-z0-9_.]+ to ptr"
      r"[\s\S]{0,240}call noundef i32 \1\(i32)"
    )
  return patterns


def vdso_resolver_spec(target: str) -> dict:
  if target in LINUX_X86_WALL_TARGETS:
    detectors = {
      "instant": "linux_x86_wall23detect_instant_provider",
      "ordered_instant": "linux_x86_wall23detect_ordered_provider",
    }
    version = "LINUX_2.6"
    symbol = "__vdso_clock_gettime"
  elif target in ("aarch64-unknown-linux-gnu", "aarch64-linux-android"):
    detectors = {
      "instant": "linux_aarch64_wall23detect_instant_provider",
      "ordered_instant": "linux_aarch64_wall23detect_ordered_provider",
    }
    version = "LINUX_2.6.39"
    symbol = "__kernel_clock_gettime"
  elif target == "armv7-unknown-linux-gnueabihf":
    detectors = {"shared_wall_selector": "linux_clock_wall15detect_provider"}
    version = "LINUX_2.6"
    symbol = "__vdso_clock_gettime"
  elif target == "s390x-unknown-linux-gnu":
    detectors = {"shared_wall_selector": "linux_clock_wall15detect_provider"}
    version = "LINUX_2.6.29"
    symbol = "__kernel_clock_gettime"
  elif target == "riscv64gc-unknown-linux-gnu":
    detectors = {"shared_wall_selector": "riscv6415detect_provider"}
    version = "LINUX_4.15"
    symbol = "__vdso_clock_gettime"
  elif target == "loongarch64-unknown-linux-gnu":
    detectors = {"shared_wall_selector": "loongarch6415detect_provider"}
    version = "LINUX_5.10"
    symbol = "__vdso_clock_gettime"
  elif target in POWERPC64_TARGETS:
    detectors = {"shared_wall_selector": "powerpc6415detect_provider"}
    version = "LINUX_2.6.15"
    symbol = "__kernel_clock_gettime"
  else:
    raise ValueError(f"{target} has no direct vDSO resolver contract")

  required_patterns = [
    "linux_vdso7install",
    r"@getauxval\([^,\n]*\b33\)",
    "linux_vdso13CLOCK_GETTIME",
    r"store atomic i(?:32|64)[^\n]*linux_vdso13CLOCK_GETTIME[^\n]* release",
    re.escape(f'c"{version}"'),
    re.escape(f'c"{symbol}"'),
  ]
  if target in ("i686-unknown-linux-gnu", "armv7-unknown-linux-gnueabihf"):
    required_patterns += [
      "linux_vdso15CLOCK_GETTIME64",
      r"store atomic i32[^\n]*linux_vdso15CLOCK_GETTIME64[^\n]* release",
      re.escape('c"__vdso_clock_gettime64"'),
    ]
  return {
    "detectors": detectors,
    "version": version,
    "symbol": symbol,
    "time64_symbol": (
      "__vdso_clock_gettime64"
      if target in ("i686-unknown-linux-gnu", "armv7-unknown-linux-gnueabihf")
      else None
    ),
    "required_patterns": required_patterns,
  }


def instant_route(target: str) -> dict:
  if target.endswith("pc-windows-msvc"):
    return {
      "provider": "Windows performance counter",
      "native_primitive": "QueryPerformanceCounter",
      "ordering": "unordered local platform-clock read",
      "required_patterns": ["@QueryPerformanceCounter"],
      "forbidden_patterns": ["llvm.x86.rdtsc", r"\brdtscp\b", "cntvct_el0"],
    }

  if target == "x86_64-apple-darwin":
    return {
      "provider": "measured XNU Mach system or commpage absolute-time provider",
      "native_primitive": "mach_absolute_time or commpage seqlock + LFENCE/RDTSC/LFENCE",
      "ordering": "unordered API over XNU's ordered absolute-time protocol",
      "required_patterns": [
        "apple_x86_64.*ticks_after_selection",
        "@mach_absolute_time",
        r"\\0Alfence\\0Ardtsc\\0Alfence\\0A",
      ],
      "forbidden_patterns": [r"\brdtscp\b"],
    }

  if target == "x86_64-unknown-freebsd":
    return {
      "provider": "independently measured kernel-eligible TSC, FreeBSD libc clock, or raw syscall",
      "native_primitive": "RDTSC, clock_gettime(CLOCK_MONOTONIC=4), or syscall 232",
      "ordering": "unordered local read from the selected wall timeline",
      "required_patterns": [
        "freebsd_x86_64.*ticks_after_selection",
        "llvm.x86.rdtsc",
        clock_gettime(4),
      ],
      "forbidden_patterns": [],
    }

  if target in LINUX_X86_WALL_TARGETS:
    syscall_patterns = (
      [
        r'asm sideeffect inteldialect "syscall"',
        r"\(i64 228, i32 1,",
        r"\(i64 228, i32 4,",
      ]
      if not target.startswith("i686")
      else [
        r'asm sideeffect alignstack inteldialect "push ebx.*int 0x80',
        r"\(i32 1, i32 265,",
        r"\(i32 4, i32 265,",
        r"\(i32 1, i32 403,",
        r"\(i32 4, i32 403,",
      ]
    )
    return {
      "provider": "measured Linux x86 complete wall provider",
      "native_primitive": (
        "kernel-eligible RDTSC or MONOTONIC/MONOTONIC_RAW through libc, "
        "a direct versioned vDSO export, an x86_64 syscall, or independent "
        "i686 time32/time64 vDSO/syscall ABIs"
      ),
      "ordering": "unordered local read from the selected wall timeline",
      "required_patterns": [
        "linux_x86_wall.*ticks_after_selection",
        "llvm.x86.rdtsc",
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        *syscall_patterns,
      ],
      "forbidden_patterns": [],
    }

  if target in ("aarch64-unknown-linux-gnu", "aarch64-linux-android"):
    return {
      "provider": "measured Linux aarch64 complete wall provider",
      "native_primitive": (
        "CNTVCT_EL0 or MONOTONIC/MONOTONIC_RAW through libc, the direct "
        "versioned vDSO export, or a raw syscall"
      ),
      "ordering": "unordered local read from the selected wall timeline",
      "required_patterns": [
        "linux_aarch64_wall.*ticks_after_selection",
        "cntvct_el0",
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        r'asm sideeffect "svc 0"',
        r"\(i64 1, ptr [^\n]*, i64 113\)",
        r"\(i64 4, ptr [^\n]*, i64 113\)",
      ],
      "forbidden_patterns": [],
    }

  if target.startswith(("x86_64", "i686")):
    return {
      "provider": "architectural monotonic counter",
      "native_primitive": "RDTSC",
      "ordering": "unordered local read",
      "required_patterns": ["llvm.x86.rdtsc"],
      "forbidden_patterns": [r"\brdtscp\b"],
    }

  if target.startswith("aarch64"):
    return {
      "provider": "architectural monotonic counter",
      "native_primitive": "CNTVCT_EL0",
      "ordering": "unordered local read",
      "required_patterns": ["cntvct_el0"],
      "forbidden_patterns": [r"\bisb sy\b"],
    }

  if target.startswith("riscv64"):
    return {
      "provider": (
        "measured TIME CSR or Linux MONOTONIC/MONOTONIC_RAW libc, direct "
        "versioned vDSO, or raw clock"
      ),
      "native_primitive": "rdtime or clock_gettime(clock id 1 or 4) by each eligible ABI",
      "ordering": "unordered local read from the independently selected timeline",
      "required_patterns": [
        "riscv64.*ticks_after_selection",
        r"\brdtime\b",
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        r'asm sideeffect "ecall"',
        r"\(i64 4, ptr [^\n]*, i64 113\)",
      ],
      "forbidden_patterns": [r"\bfence r, i\b"],
    }

  if target.startswith("loongarch64"):
    return {
      "provider": (
        "measured StableCounter or Linux MONOTONIC/MONOTONIC_RAW libc, direct "
        "versioned vDSO, or raw clock"
      ),
      "native_primitive": "rdtime.d or clock_gettime(clock id 1 or 4) by each eligible ABI",
      "ordering": "unordered local read from the independently selected timeline",
      "required_patterns": [
        "loongarch64.*ticks_after_selection",
        r"\brdtime\.d\b",
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        r'asm sideeffect "syscall 0"',
        r"\(i64 4, ptr [^\n]*, i64 113\)",
      ],
      "forbidden_patterns": [r"\bdbar 0\b"],
    }

  if target == "armv7-unknown-linux-gnueabihf":
    return {
      "provider": (
        "independently measured MONOTONIC/MONOTONIC_RAW libc, direct versioned "
        "time32/time64 vDSO, or raw time32/time64 clock"
      ),
      "native_primitive": (
        "clock_gettime clock id 1 or 4 through libc, the vDSO, syscall 263, "
        "or syscall 403"
      ),
      "ordering": "unordered local read from the independently selected timeline",
      "required_patterns": [
        "linux_clock_wall.*ticks_after_selection",
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        r'asm sideeffect alignstack "push \{r7\}.*svc 0',
        r"\(i32 263, i32 1, ptr",
        r"\(i32 403, i32 1, ptr",
        r"\(i32 263, i32 4, ptr",
        r"\(i32 403, i32 4, ptr",
      ],
      "forbidden_patterns": [
        r"@syscall\([^,\n]*\b(?:263|403),\s*i32 noundef 1,",
      ],
    }

  if target == "s390x-unknown-linux-gnu":
    return {
      "provider": (
        "independently measured MONOTONIC/MONOTONIC_RAW libc, direct versioned "
        "vDSO, or raw syscall clock"
      ),
      "native_primitive": (
        "clock_gettime clock id 1 or 4 through libc, the vDSO, or svc 0 syscall 260"
      ),
      "ordering": "unordered local read from the independently selected timeline",
      "required_patterns": [
        "linux_clock_wall.*ticks_after_selection",
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        r'asm sideeffect "svc 0"',
        r"\(i64 260, i64 1, ptr",
        r"\(i64 260, i64 4, ptr",
      ],
      "forbidden_patterns": [],
    }

  if target in POWERPC64_TARGETS:
    return {
      "provider": (
        "measured Time Base or Linux MONOTONIC/MONOTONIC_RAW libc, direct "
        "versioned vDSO, SC, or SCV"
      ),
      "native_primitive": "mfspr TB or clock id 1/4 through libc, vDSO, sc, or scv 0",
      "ordering": "unordered local read from the independently selected timeline",
      "required_patterns": [
        "powerpc64.*ticks_after_selection",
        r'"mfspr \$\{0\}, 268"',
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        r'asm sideeffect "sc"',
        r'asm sideeffect "sc"[^\n]*\(i64 246, i64 4, ptr',
        r"scv 0",
        r"scv 0[^\n]*\(i64 246, i64 4, ptr",
      ],
      "forbidden_patterns": [r'"sync"'],
    }

  if target == "wasm32-wasip2":
    return {
      "provider": "WASIp2 host monotonic clock",
      "native_primitive": "wasi:clocks/monotonic-clock@0.2.4.now",
      "ordering": "component host-call serialization",
      "required_patterns": ["wasip27imports4wasi6clocks15monotonic_clock3now"],
      "forbidden_patterns": ["wasi_snapshot_preview1"],
    }

  if "wasip" in target:
    return {
      "provider": "WASI host monotonic clock",
      "native_primitive": "wasi_snapshot_preview1.clock_time_get(MONOTONIC=1)",
      "ordering": "host-call serialization",
      "required_patterns": [
        r"fallback6wasip114clock_time_get[^\n]*\(i32 noundef 1,",
      ],
      "forbidden_patterns": [],
    }

  if target in ("wasm32-unknown-unknown", "wasm32v1-none"):
    return {
      "provider": "measured host-selected JavaScript monotonic clock",
      "native_primitive": "globalThis.performance.now() or process.hrtime.bigint()",
      "ordering": "host-call serialization",
      "required_patterns": ["tach4arch4wasm15wall_now_millis"],
      "forbidden_patterns": ["tach4arch4wasm15performance_now"],
    }

  if target == "wasm32-unknown-emscripten":
    return {
      "provider": "measured local Emscripten JavaScript wall clock",
      "native_primitive": "guarded performance.now() or process.hrtime.bigint() import",
      "ordering": "unordered local host call",
      "required_patterns": [*EMSCRIPTEN_LOCAL_WALL_IMPORTS],
      "forbidden_patterns": [
        "tach_emscripten_performance_epoch_now",
        "tach_emscripten_get_now_millis",
        clock_gettime(1),
      ],
    }

  return {
    "provider": "POSIX monotonic clock",
    "native_primitive": "clock_gettime(CLOCK_MONOTONIC)",
    "ordering": "call-boundary serialization",
    "required_patterns": [clock_gettime(1)],
    "forbidden_patterns": [],
  }


def ordered_instant_route(target: str, mode: str) -> dict:
  if target == "aarch64-pc-windows-msvc":
    return {
      "provider": "ordered Windows performance counter",
      "native_primitive": "DMB ISHLD + ISB + QueryPerformanceCounter",
      "ordering": "load-completion and instruction barriers before the QPC read",
      "required_patterns": [r"\bdmb ishld\\0Aisb\b", "@QueryPerformanceCounter"],
      "forbidden_patterns": ["llvm.x86.rdtsc", r"\brdtscp\b", "cntvct_el0"],
    }

  if target in ("x86_64-pc-windows-msvc", "i686-pc-windows-msvc"):
    return {
      "provider": "measured exact x86 barrier + Windows performance counter",
      "native_primitive": (
        "CPUID, Intel LFENCE, RDTSCP, AMD MFENCE, or SERIALIZE followed by "
        "QueryPerformanceCounter"
      ),
      "ordering": "runtime-selected complete barrier + QPC compound provider",
      "required_patterns": [
        # LLVM can preserve the cold helper or inline it into this exact
        # bench-internal wrapper when composing the optimized implementation IR.
        r"(?:qpc_ticks_ordered_after_selection|windows_ticks_ordered_after_selection)",
        r'asm sideeffect inteldialect "lfence"',
        r'asm sideeffect inteldialect "mfence"',
        r'asm sideeffect inteldialect "rdtscp\\0Alfence"',
        r'asm sideeffect inteldialect "serialize"',
        r"\\0Acpuid\\0A",
        "@QueryPerformanceCounter",
      ],
      "forbidden_patterns": ["cntvct_el0", r"fence[^\n]*seq_cst"],
    }

  if target == "x86_64-apple-darwin":
    return {
      "provider": "measured ordered XNU Mach system or commpage provider",
      "native_primitive": "mach_absolute_time or commpage seqlock + LFENCE/RDTSC/LFENCE",
      "ordering": "XNU's LFENCE + RDTSC + LFENCE absolute-time protocol",
      "required_patterns": [
        "apple_x86_64.*ticks_after_selection",
        "@mach_absolute_time",
        r"\\0Alfence\\0Ardtsc\\0Alfence\\0A",
      ],
      "forbidden_patterns": [r"\brdtscp\b"],
    }

  if target == "x86_64-unknown-freebsd":
    return {
      "provider": (
        "independently measured ordered TSC or exact barrier + FreeBSD libc/raw clock"
      ),
      "native_primitive": (
        "LFENCE + RDTSC, RDTSCP, or CPUID + RDTSC; "
        "Intel CPUID/LFENCE, AMD MFENCE/RDTSCP, or unknown-vendor CPUID + "
        "clock_gettime(CLOCK_MONOTONIC=4)/syscall 232"
      ),
      "ordering": "runtime-selected exact barrier + wall-read compound provider",
      "required_patterns": [
        "freebsd_x86_64.*ticks_ordered_after_selection",
        r"\blfence\\0Ardtsc\b",
        r"\brdtscp\b",
        r'asm sideeffect inteldialect "lfence"',
        r'asm sideeffect inteldialect "mfence"',
        (
          r'asm sideeffect inteldialect "mov rsi, rbx\\0Axor eax, eax'
          r'\\0Acpuid\\0Amov rbx, rsi"'
        ),
        clock_gettime(4),
      ],
      "forbidden_patterns": [],
    }

  if target in LINUX_X86_WALL_TARGETS:
    syscall_patterns = (
      [
        r'asm sideeffect inteldialect "syscall"',
        r"\(i64 228, i32 1,",
        r"\(i64 228, i32 4,",
      ]
      if not target.startswith("i686")
      else [
        r'asm sideeffect alignstack inteldialect "push ebx.*int 0x80',
        r"\(i32 1, i32 265,",
        r"\(i32 4, i32 265,",
        r"\(i32 1, i32 403,",
        r"\(i32 4, i32 403,",
      ]
    )
    return {
      "provider": "measured exact ordered Linux x86 compound provider",
      "native_primitive": (
        "ordered TSC or CPUID/LFENCE/MFENCE/RDTSCP+LFENCE followed by "
        "MONOTONIC/MONOTONIC_RAW libc, a direct versioned vDSO export, or an "
        "exact raw syscall ABI"
      ),
      "ordering": "runtime-selected exact barrier + wall-read compound path",
      "required_patterns": [
        "linux_x86_wall.*ticks_ordered_after_selection",
        r"\blfence\\0Ardtsc\b",
        r"\brdtscp\\0Alfence\b",
        r'asm sideeffect inteldialect "mfence"',
        r"\\0Acpuid\\0A",
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        *syscall_patterns,
      ],
      "forbidden_patterns": [r"fence[^\n]*seq_cst"],
    }

  if target in ("aarch64-unknown-linux-gnu", "aarch64-linux-android"):
    return {
      "provider": "measured exact ordered Linux aarch64 provider",
      "native_primitive": (
        "ISB+CNTVCT, CNTVCTSS, or MONOTONIC/MONOTONIC_RAW through libc, the "
        "direct versioned vDSO export, or a raw syscall"
      ),
      "ordering": "independently selected complete ordered wall path",
      "required_patterns": [
        "linux_aarch64_wall.*ticks_ordered_after_selection",
        r"\bisb sy\b",
        "cntvct_el0",
        "S3_3_C14_C0_6",
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        r'asm sideeffect "svc 0"',
        r"\(i64 1, ptr [^\n]*, i64 113\)",
        r"\(i64 4, ptr [^\n]*, i64 113\)",
      ],
      "forbidden_patterns": [],
    }

  if target.startswith(("x86_64", "i686")):
    return {
      "provider": "runtime-selected ordered architectural monotonic counter",
      "native_primitive": "LFENCE + RDTSC, RDTSCP, or CPUID + RDTSC",
      "ordering": "capability-gated candidates selected by measured read cost",
      "required_patterns": [r"\blfence\\0Ardtsc\b", r"\brdtscp\b", r"cpuid\\0A[^\"]*rdtsc\b"],
      "forbidden_patterns": [],
    }

  if target in (
    "aarch64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "aarch64-linux-android",
  ):
    return {
      "provider": "runtime-selected ordered architectural monotonic counter",
      "native_primitive": "ISB SY + CNTVCT_EL0 or FEAT_ECV CNTVCTSS_EL0",
      "ordering": "capability-gated candidates selected by measured read cost",
      "required_patterns": [r"\bisb sy\b", "cntvct_el0", "S3_3_C14_C0_6"],
      "forbidden_patterns": [],
    }

  if target.startswith("aarch64"):
    return {
      "provider": "ordered architectural monotonic counter",
      "native_primitive": "ISB SY + CNTVCT_EL0",
      "ordering": "instruction synchronization barrier",
      "required_patterns": [r"\bisb sy\b", "cntvct_el0"],
      "forbidden_patterns": [],
    }

  if target.startswith("riscv64"):
    return {
      "provider": (
        "independently measured ordered TIME CSR or Linux MONOTONIC/MONOTONIC_RAW "
        "libc, direct versioned vDSO, or raw clock"
      ),
      "native_primitive": (
        "fence r, i + rdtime/libc/vDSO/raw clock, or bare precise-ECALL raw "
        "clock syscall"
      ),
      "ordering": (
        "ratified Zicsr read-before-input relation or precise ECALL trap ordering, "
        "selected by measured complete-path cost"
      ),
      "required_patterns": [
        "riscv64.*ticks_ordered_after_selection",
        r"\bfence r, i\b",
        r"rdtime\b",
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        r'asm sideeffect "ecall"',
        r"\(i64 1, ptr [^\n]*, i64 113\)",
        r"\(i64 4, ptr [^\n]*, i64 113\)",
        r"switch i8 [^\n]*[\s\S]{0,240}i8 7, label",
        r"switch i8 [^\n]*[\s\S]{0,280}i8 8, label",
      ],
      "forbidden_patterns": [],
    }

  if target.startswith("loongarch64"):
    return {
      "provider": (
        "measured ordered StableCounter or Linux MONOTONIC/MONOTONIC_RAW libc, "
        "direct versioned vDSO, or raw clock"
      ),
      "native_primitive": (
        "getpid exception boundary + rdtime.d/libc/vDSO clock, or direct "
        "clock_gettime syscall"
      ),
      "ordering": "synchronous syscall exception return before the adjacent counter read",
      "required_patterns": [
        "loongarch64.*ticks_ordered_after_selection",
        r"\bsyscall 0\\0Ardtime\.d\b",
        r'asm sideeffect "syscall 0"',
        r"\(i64 0, i64 172\)",
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        r"\(i64 1, ptr [^\n]*, i64 113\)",
        r"\(i64 4, ptr [^\n]*, i64 113\)",
      ],
      "forbidden_patterns": [],
    }

  if target in POWERPC64_TARGETS:
    return {
      "provider": (
        "measured ordered Power Time Base or Linux MONOTONIC/MONOTONIC_RAW "
        "libc, direct versioned vDSO, SC, or SCV clock"
      ),
      "native_primitive": (
        "heavyweight sync before the counter/libc/vDSO read, or a "
        "context-synchronizing bare sc/scv 0 syscall"
      ),
      "ordering": "runtime winner among explicit-sync and OS-owned context-sync routes",
      "required_patterns": [
        "powerpc64.*ticks_ordered_after_selection",
        r'asm sideeffect "sync"',
        r'"mfspr \$\{0\}, 268"',
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        r'asm sideeffect "sc"',
        r'asm sideeffect "sc"[^\n]*\(i64 246, i64 4, ptr',
        r"scv 0",
        r"scv 0[^\n]*\(i64 246, i64 4, ptr",
        r"switch i8 [^\n]*[\s\S]{0,300}i8 9, label",
        r"switch i8 [^\n]*[\s\S]{0,340}i8 10, label",
        r"switch i8 [^\n]*[\s\S]{0,380}i8 11, label",
        r"switch i8 [^\n]*[\s\S]{0,420}i8 12, label",
      ],
      "forbidden_patterns": [],
    }

  if target in ("wasm32-unknown-unknown", "wasm32v1-none"):
    return {
      "provider": "worker-comparable host-selected JavaScript wall clock",
      "native_primitive": "epoch-correlated performance.now() or process.hrtime.bigint()",
      "ordering": "module-shared sequentially consistent atomic maximum",
      "required_patterns": [
        "tach4arch4wasm23ordered_wall_now_millis",
        r"atomicrmw umax[^\n]*seq_cst",
      ],
      "forbidden_patterns": ["tach4arch4wasm21performance_epoch_now"],
    }

  if target == "wasm32-unknown-emscripten":
    if mode != "emscripten-pthreads":
      return {
        "provider": "measured local Emscripten JavaScript wall clock",
        "native_primitive": "guarded performance.now() or process.hrtime.bigint() import",
        "ordering": "single-thread host-call serialization",
        "required_patterns": [*EMSCRIPTEN_LOCAL_WALL_IMPORTS],
        "forbidden_patterns": [
          "tach_emscripten_performance_epoch_now",
          "tach_emscripten_get_now_millis",
          r"atomicrmw umax",
          clock_gettime(1),
        ],
      }
    return {
      "provider": "measured worker-comparable Emscripten host clock",
      "native_primitive": (
        "performance.timeOrigin + performance.now() or pthread-synchronized "
        "emscripten_get_now()"
      ),
      "ordering": "module-shared sequentially consistent atomic maximum",
      "required_patterns": [
        *EMSCRIPTEN_ORDERED_PTHREAD_IMPORTS,
        r"atomicrmw umax[^\n]*seq_cst",
        r"cmpxchg",
      ],
      "forbidden_patterns": [clock_gettime(1)],
    }

  if target == "armv7-unknown-linux-gnueabihf":
    return {
      "provider": (
        "independently measured ordered MONOTONIC/MONOTONIC_RAW libc, direct "
        "versioned time32/time64 vDSO, or raw time32/time64 clock"
      ),
      "native_primitive": (
        "DMB ISH + ISB before the read, or bare context-synchronizing SVC syscall"
      ),
      "ordering": "runtime winner among explicit-barrier and OS-owned SVC routes",
      "required_patterns": [
        "linux_clock_wall.*ticks_ordered_after_selection",
        r"\bdmb ish\\0Aisb\b",
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        r'asm sideeffect alignstack "push \{r7\}.*svc 0',
        r"\(i32 263, i32 1, ptr",
        r"\(i32 403, i32 1, ptr",
        r"\(i32 263, i32 4, ptr",
        r"\(i32 403, i32 4, ptr",
        r"switch i8 [^\n]*[\s\S]{0,300}i8 8, label",
        r"switch i8 [^\n]*[\s\S]{0,340}i8 9, label",
        r"switch i8 [^\n]*[\s\S]{0,380}i8 10, label",
        r"switch i8 [^\n]*[\s\S]{0,420}i8 11, label",
      ],
      "forbidden_patterns": [
        r"@syscall\([^,\n]*\b(?:263|403),\s*i32 noundef 1,",
      ],
    }

  if target == "s390x-unknown-linux-gnu":
    return {
      "provider": (
        "independently measured ordered MONOTONIC/MONOTONIC_RAW libc, direct "
        "versioned vDSO, or raw syscall clock"
      ),
      "native_primitive": "BCR 15,0 before the read, or bare serializing SVC syscall 260",
      "ordering": "runtime winner among explicit-barrier and OS-owned SVC routes",
      "required_patterns": [
        "linux_clock_wall.*ticks_ordered_after_selection",
        r'asm sideeffect "bcr 15, 0"',
        clock_gettime(1),
        clock_gettime(4),
        *direct_vdso_hot_patterns(target),
        r'asm sideeffect "svc 0"',
        r"\(i64 260, i64 1, ptr",
        r"\(i64 260, i64 4, ptr",
        r"switch i8 [^\n]*[\s\S]{0,300}i8 8, label",
        r"switch i8 [^\n]*[\s\S]{0,360}i8 10, label",
      ],
      "forbidden_patterns": [],
    }

  route = instant_route(target)
  return {
    **route,
    "provider": route["provider"],
    "ordering": "runtime, kernel, or host call-boundary serialization",
  }


def thread_cpu_route(target: str, mode: str) -> dict:
  if target.endswith("apple-darwin"):
    return {
      "provider": "POSIX thread CPU clock",
      "native_primitive": "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)",
      "failure_fallback": "none: macOS primitive is infallible",
      "required_patterns": [r"@clock_gettime_nsec_np\([^,\n]*\b16\)"],
      "forbidden_patterns": [],
    }

  if target.endswith("pc-windows-msvc"):
    return {
      "provider": "Windows thread times",
      "native_primitive": "GetThreadTimes((HANDLE)-2)",
      "failure_fallback": "Windows performance counter",
      "required_patterns": [
        "windows_thread_cpu_nanos",
        "@QueryPerformanceCounter",
      ],
      "forbidden_patterns": ["llvm.x86.rdtsc", r"\brdtscp\b", "cntvct_el0"],
    }

  if target == "wasm32-wasip2":
    return {
      "provider": "WASIp2 monotonic wall-time fallback",
      "native_primitive": "wasi:clocks/monotonic-clock@0.2.4.now",
      "failure_fallback": "same provider",
      "required_patterns": ["wasip27imports4wasi6clocks15monotonic_clock3now"],
      "forbidden_patterns": ["wasi_snapshot_preview1", "clock_time_get"],
    }

  if "wasip" in target:
    return {
      "provider": "WASI thread CPU clock when the host implements clock id 3",
      "native_primitive": "wasi_snapshot_preview1.clock_time_get(THREAD_CPUTIME_ID=3)",
      "failure_fallback": "wasi_snapshot_preview1.clock_time_get(MONOTONIC=1)",
      "required_patterns": [
        (
          r"thread_cpu9now_nanos14clock_time_get[^\n]*"
          r"\(i32 noundef 3, i64 noundef 1,"
        ),
        r"fallback6wasip114clock_time_get[^\n]*\(i32 noundef 1,",
      ],
      "forbidden_patterns": [
        (
          r"thread_cpu9now_nanos14clock_time_get[^\n]*"
          r"\(i32 noundef 3, i64 noundef 0,"
        ),
      ],
    }

  if target in ("wasm32-unknown-unknown", "wasm32v1-none"):
    return {
      "provider": "Node thread CPU clock or JavaScript wall-clock fallback",
      "native_primitive": "process.threadCpuUsage() candidate",
      "failure_fallback": "measured performance.now() or process.hrtime.bigint(), else frozen unavailable",
      "required_patterns": [
        "tach4arch4wasm21node_thread_cpu_usage",
        "tach4arch4wasm15wall_now_millis",
      ],
      "forbidden_patterns": ["tach4arch4wasm15performance_now"],
    }

  if target == "wasm32-unknown-emscripten":
    return {
      "provider": (
        "Node-hosted Emscripten process.threadCpuUsage() current-thread CPU clock "
        "or guarded local JavaScript wall-time fallback"
      ),
      "native_primitive": "process.threadCpuUsage() user + system microseconds candidate",
      "failure_fallback": "guarded performance.now() or process.hrtime.bigint() import",
      "required_patterns": [
        EMSCRIPTEN_NODE_THREAD_CPU_IMPORT,
        *EMSCRIPTEN_LOCAL_WALL_IMPORTS,
      ],
      "forbidden_patterns": [
        "tach_emscripten_performance_epoch_now",
        "tach_emscripten_get_now_millis",
        clock_gettime(1),
        clock_gettime(3),
      ],
    }

  if target == "x86_64-unknown-freebsd":
    return {
      "provider": "measured FreeBSD current-thread CPU clock entry path",
      "native_primitive": "libc clock_gettime or inline syscall 232, CLOCK_THREAD_CPUTIME_ID=14",
      "failure_fallback": "selected FreeBSD TSC or CLOCK_MONOTONIC wall timeline",
      "required_patterns": [
        r'asm sideeffect inteldialect "syscall".*\(i64 232, i32 14,',
        clock_gettime(14),
        r"=\{r8\},=\{r9\},=\{r10\}",
        "freebsd_x86_64.*ticks_after_selection",
        "llvm.x86.rdtsc",
        clock_gettime(4),
      ],
      "forbidden_patterns": [],
    }

  native_id = 3
  if target in LINUX_X86_WALL_TARGETS:
    fallback = (
      "runtime-selected RDTSC, libc, direct versioned vDSO, or raw-syscall wall timeline"
    )
    fallback_pattern = "linux_x86_wall.*ticks_after_selection"
  elif target in ("armv7-unknown-linux-gnueabihf", "s390x-unknown-linux-gnu"):
    fallback = "runtime-selected libc, direct versioned vDSO, or raw-syscall wall timeline"
    fallback_pattern = "linux_clock_wall.*ticks_after_selection"
  elif target.startswith(("x86_64", "i686")):
    fallback = "RDTSC monotonic counter"
    fallback_pattern = "llvm.x86.rdtsc"
  elif target.startswith("aarch64"):
    fallback = (
      "runtime-selected CNTVCT_EL0, libc, direct versioned vDSO, or raw-syscall "
      "wall timeline"
      if target in ("aarch64-unknown-linux-gnu", "aarch64-linux-android")
      else "CNTVCT_EL0 monotonic counter"
    )
    fallback_pattern = "cntvct_el0"
  elif target.startswith("riscv64"):
    fallback = "runtime-selected rdtime, libc, direct versioned vDSO, or raw-syscall wall timeline"
    fallback_pattern = r'"rdtime \$\{0\}'
  elif target.startswith("loongarch64"):
    fallback = (
      "runtime-selected rdtime.d, libc, direct versioned vDSO, or raw-syscall wall timeline"
    )
    fallback_pattern = r'"rdtime\.d \$\{0\}'
  elif target in POWERPC64_TARGETS:
    fallback = (
      "runtime-selected Power Time Base, libc, direct versioned vDSO, SC, or SCV wall timeline"
    )
    fallback_pattern = r'"mfspr \$\{0\}, 268"'
  else:
    fallback = "clock_gettime(CLOCK_MONOTONIC)"
    fallback_pattern = clock_gettime(1)
  raw_syscall = linux_thread_clock_syscall(target)
  required_patterns = [fallback_pattern]
  forbidden_patterns = []
  if raw_syscall is not None and target.endswith(("linux-gnu", "linux-musl", "linux-android")):
    required_patterns.append(raw_syscall)
    required_patterns.append(clock_gettime(native_id))
  elif target in (
    "i686-unknown-linux-gnu",
    "armv7-unknown-linux-gnueabihf",
    "riscv64gc-unknown-linux-gnu",
  ):
    # These libc helpers remain upstream Rust declarations after LTO, while
    # their raw-syscall candidates and perf brackets are inspectable here.
    required_patterns.append(
      "linux_32_libc_nanos"
      if target in ("i686-unknown-linux-gnu", "armv7-unknown-linux-gnueabihf")
      else "linux_riscv64_libc_nanos"
    )
  else:
    required_patterns.append(clock_gettime(native_id))
  if target == "i686-unknown-linux-gnu":
    required_patterns += [
      r'asm sideeffect alignstack inteldialect "push ebx\\0Amov ebx, .*\\0Aint 0x80',
      r"\(i32 3, i32 265, ptr",
      r"\(i32 3, i32 403, ptr",
    ]
    forbidden_patterns += [
      r"@syscall\([^,\n]*\b(?:265|403),\s*i32 noundef 3,",
    ]
  elif target == "armv7-unknown-linux-gnueabihf":
    required_patterns += [
      r'asm sideeffect alignstack "push \{r7\}.*svc 0',
      r"\(i32 263, i32 3, ptr",
      r"\(i32 403, i32 3, ptr",
    ]
  elif target == "riscv64gc-unknown-linux-gnu":
    required_patterns += [
      r'asm sideeffect "ecall"',
      r"\(i64 113, i64 3, ptr",
    ]
  elif target == "s390x-unknown-linux-gnu":
    required_patterns += [
      r'asm sideeffect "svc 0"',
      r"\(i64 260, i64 3, ptr",
    ]
  if target == "armv7-unknown-linux-gnueabihf":
    forbidden_patterns += [
      r"@syscall\([^,\n]*\b(?:263|403),\s*i32 noundef 3,",
    ]
  elif target == "riscv64gc-unknown-linux-gnu":
    forbidden_patterns += [
      r"@syscall\([^,\n]*\b113,\s*i32 noundef signext 3,",
    ]
  if target in PERF_READ_THREAD_CPU_TARGETS:
    if mode == "default":
      required_patterns += perf_read_entry_patterns(target)
    else:
      forbidden_patterns.append("thread_cpu_linux_inline")
  if target in PERF_MMAP_THREAD_CPU_TARGETS and mode == "default":
    if target.startswith(("x86_64", "i686")):
      required_patterns += [
        r'asm sideeffect.*cpuid',
        r'asm sideeffect.*lfence\\0Ardtsc\\0Alfence',
        r'asm sideeffect.*mfence\\0Ardtsc\\0Amfence',
        r'asm sideeffect.*rdtscp\\0Alfence',
        r'asm sideeffect.*serialize\\0Ardtsc\\0Aserialize',
      ]
    elif target.startswith("aarch64"):
      required_patterns += [
        r'asm sideeffect "isb sy\\0Amrs .*cntvct_el0\\0Aisb"',
        r'asm sideeffect "mrs .*S3_3_C14_C0_6\\0Aisb"',
      ]
    elif target == "armv7-unknown-linux-gnueabihf":
      required_patterns.append(
        r'asm sideeffect "isb sy\\0Amrrc p15, 1, .*c14\\0Aisb sy"'
      )
    else:
      required_patterns.append(r'asm sideeffect.*fence r, i\\0Ardtime.*fence i, r')
  measured_failure_fallback = (
    f"measured second-best remaining perf mmap/read/POSIX route; POSIX failure uses {fallback}"
    if target in PERF_READ_THREAD_CPU_TARGETS and mode == "default"
    else fallback
  )
  return {
    "provider": (
      "runtime-measured perf task-clock mmap, persistent perf read, or POSIX thread CPU clock"
      if target in PERF_MMAP_THREAD_CPU_TARGETS and mode == "default"
      else "runtime-measured persistent perf task-clock read or POSIX thread CPU clock"
      if target in PERF_READ_THREAD_CPU_TARGETS and mode == "default"
      else "POSIX thread CPU clock"
    ),
    "native_primitive": (
      "adaptive libc or direct syscall CLOCK_THREAD_CPUTIME_ID entry path"
      if raw_syscall is not None
      and target.endswith(("linux-gnu", "linux-musl", "linux-android"))
      else "adaptive time32/time64 direct syscall or libc CLOCK_THREAD_CPUTIME_ID"
      if target in ("i686-unknown-linux-gnu", "armv7-unknown-linux-gnueabihf")
      else "adaptive direct syscall or libc CLOCK_THREAD_CPUTIME_ID"
      if target == "riscv64gc-unknown-linux-gnu"
      else "clock_gettime(CLOCK_THREAD_CPUTIME_ID)"
    ),
    "failure_fallback": measured_failure_fallback,
    "required_patterns": required_patterns,
    "forbidden_patterns": forbidden_patterns,
  }


def route_specs(target: str, mode: str) -> dict[str, dict]:
  instant = instant_route(target)
  ordered = ordered_instant_route(target, mode)
  thread_cpu = thread_cpu_route(target, mode)
  return {
    "instant_now": {"root": "tach_probe_instant_now", "spec": instant},
    "instant_now_elapsed": {
      "root": "tach_probe_instant_now_elapsed",
      "spec": instant,
      "phase_roots": {
        "now": "tach_probe_instant_now",
        "elapsed": "tach_probe_instant_elapsed",
      },
    },
    "ordered_instant_now": {"root": "tach_probe_ordered_instant_now", "spec": ordered},
    "ordered_instant_now_elapsed": {
      "root": "tach_probe_ordered_instant_now_elapsed",
      "spec": ordered,
      "phase_roots": {
        "now": "tach_probe_ordered_instant_now",
        "elapsed": "tach_probe_ordered_instant_elapsed",
      },
    },
    "thread_cpu_instant_now": {"root": "tach_probe_thread_cpu_now", "spec": thread_cpu},
    "thread_cpu_instant_now_elapsed": {
      "root": "tach_probe_thread_cpu_now_elapsed",
      "spec": thread_cpu,
      "phase_roots": {
        "now": "tach_probe_thread_cpu_now",
        "elapsed": "tach_probe_thread_cpu_elapsed",
      },
    },
  }


def validate_route_patterns(
  target: str,
  mode: str,
  route_name: str,
  spec: dict,
  closure: str,
) -> None:
  required = spec["required_patterns"]
  forbidden = spec["forbidden_patterns"]
  missing = [pattern for pattern in required if re.search(pattern, closure) is None]
  unexpected = [pattern for pattern in forbidden if re.search(pattern, closure) is not None]
  if missing or unexpected:
    detail = []
    if missing:
      detail.append(f"missing {missing}")
    if unexpected:
      detail.append(f"unexpected {unexpected}")
    raise RuntimeError(
      f"{target} {mode} {route_name} route proof failed: {'; '.join(detail)}"
    )


def validate_codegen(
  target: str,
  mode: str,
  path: Path,
  implementation_path: Path,
) -> dict:
  ir = normalize_tach_ir_symbols(
    "\n".join((path.read_text(), implementation_path.read_text()))
  )
  validated = {}
  for route_name, route in route_specs(target, mode).items():
    root = route["root"]
    spec = route["spec"]
    closure = reachable_ir(ir, root)
    phase_roots = route.get("phase_roots")
    if phase_roots is None:
      validate_route_patterns(target, mode, route_name, spec, closure)
      validated[route_name] = {
        **spec,
        "root": root,
        "llvm_ir": str(path),
        "implementation_llvm_ir": str(implementation_path),
      }
      continue

    resolved_phases = require_composed_phase_roots(ir, route_name, root, phase_roots)
    phases = {}
    for phase, phase_root in resolved_phases.items():
      phase_closure = reachable_ir(ir, phase_root)
      validate_route_patterns(target, mode, f"{route_name}:{phase}", spec, phase_closure)
      phases[phase] = {"root": phase_root, "llvm_ir": str(path)}
    validated[route_name] = {
      **spec,
      "root": root,
      "llvm_ir": str(path),
      "implementation_llvm_ir": str(implementation_path),
      "phases": phases,
    }
  return validated


def validate_vdso_resolver_codegen(target: str, path: Path) -> dict:
  ir = path.read_text()
  definitions = llvm_functions(ir)
  spec = vdso_resolver_spec(target)
  validated = {}
  for route_name, detector in spec["detectors"].items():
    roots = sorted(
      name
      for name in definitions
      if detector in name and "select_thread_owned_process_provider_slow" not in name
    )
    if not roots:
      raise RuntimeError(f"{target} vDSO resolver is missing detector {detector}")
    closure = "\n".join(reachable_ir_with_globals(ir, root) for root in roots)
    missing = [
      pattern
      for pattern in spec["required_patterns"]
      if re.search(pattern, closure) is None
    ]
    if missing:
      raise RuntimeError(
        f"{target} {route_name} vDSO resolver proof failed: missing {missing}"
      )
    validated[route_name] = {
      "provider": "direct versioned Linux vDSO clock resolver",
      "native_primitive": f'{spec["symbol"]}@{spec["version"]}',
      "independent_time64_primitive": spec["time64_symbol"],
      "detector_match": detector,
      "roots": roots,
      "required_patterns": spec["required_patterns"],
      "llvm_ir": str(path),
    }
  return validated


def source_fingerprint() -> str:
  digest = hashlib.sha256()
  sources = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", *sorted((ROOT / "src").rglob("*.rs"))]
  for source in sources:
    digest.update(source.relative_to(ROOT).as_posix().encode())
    digest.update(b"\0")
    digest.update(source.read_bytes())
    digest.update(b"\0")
  return digest.hexdigest()


def runtime_evidence_class(target: str) -> str:
  if target in BENCHMARKED_TARGETS:
    return "measured_external_runtime"
  if target in PERF_READ_THREAD_CPU_TARGETS or target in {
    "x86_64-unknown-freebsd",
    "wasm32-unknown-unknown",
    "wasm32-unknown-emscripten",
  }:
    return "runtime_self_selecting_codegen_proven"
  if target in {"wasm32-wasip2", "wasm32v1-none"}:
    return "fallback_only_source_codegen_proven"
  return "unique_provider_source_codegen_proven"


def git_output(*args: str) -> str:
  result = subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True)
  return result.stdout.strip()


def main() -> None:
  args = parse_args()
  proof_status = git_output(
    "status",
    "--short",
    "--",
    "Cargo.toml",
    "Cargo.lock",
    "src",
    "benches/verify-target-providers.py",
  )
  if proof_status:
    raise RuntimeError(f"provider proof inputs differ from HEAD:\n{proof_status}")
  output = args.output_dir.resolve()
  if output == ROOT or ROOT in output.parents and output.name != "provider-proof":
    raise RuntimeError("output directory must be a dedicated provider-proof directory")
  if output.exists():
    shutil.rmtree(output)
  output.mkdir(parents=True)
  ensure_targets(args.toolchain, args.install_targets)
  probe = make_probe(output, args.toolchain)

  artifacts = {}
  failures = []
  with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as executor:
    futures = {
      executor.submit(check_target, target, args.toolchain, output, probe): target
      for target in TARGETS
    }
    for future in concurrent.futures.as_completed(futures):
      target = futures[future]
      try:
        artifacts[target] = future.result()
        print(f"PASS compile/codegen {target}", flush=True)
      except Exception as error:
        failures.append(f"{target}: {error}")
        print(f"FAIL {target}: {error}", file=sys.stderr, flush=True)
  if failures:
    raise RuntimeError("\n".join(failures))

  results = []
  for target in TARGETS:
    modes = {
      mode: validate_codegen(
        target,
        mode,
        artifacts[target][mode],
        artifacts[target][f"{mode}-implementation"],
      )
      for mode in target_modes(target)
    }
    vdso_resolver_routes = (
      validate_vdso_resolver_codegen(target, artifacts[target]["vdso-resolver"])
      if target in LINUX_VDSO_TARGETS
      else {}
    )
    checked_modes = list(target_modes(target))
    dependency_contracts = {
      "default": (
        "thread-cpu-inline may explicitly link std on supported Linux/Android "
        "perf-mmap targets"
      ),
      "no-default": "strict no_std dependency",
    }
    if target == "wasm32-unknown-emscripten":
      dependency_contracts["emscripten-pthreads"] = (
        "no_std source with an explicit Cargo feature plus Emscripten pthread/"
        "Wasm-atomics compiler flags"
      )
    results.append(
      {
        "target": target,
        "covered_by_six_platform_runtime_campaign": target in BENCHMARKED_TARGETS,
        "evidence_class": runtime_evidence_class(target),
        "runtime_speed_evidence": (
          "six-platform native speed campaign"
          if target in BENCHMARKED_TARGETS
          else None
        ),
        "external_empirical_artifact_available": target in BENCHMARKED_TARGETS,
        "release_required_hosted_runtime_cell": (
          target in RELEASE_REQUIRED_HOSTED_SPEED_TARGETS
          and target not in BENCHMARKED_TARGETS
        ),
        "warning_strict_crate_checks": checked_modes,
        "warning_strict_external_api_checks": checked_modes,
        "external_probe_source": "#![no_std]",
        "dependency_contracts": dependency_contracts,
        "strict_no_default_dependency_no_std_asserted": True,
        "codegen_routes": modes,
        "optimized_vdso_resolver_routes": vdso_resolver_routes,
      }
    )

  configuration_count = sum(len(target_modes(target)) for target in TARGETS)
  vdso_resolver_route_count = sum(
    len(vdso_resolver_spec(target)["detectors"]) for target in LINUX_VDSO_TARGETS
  )
  rustc = subprocess.check_output(["rustc", f"+{args.toolchain}", "-Vv"], text=True)
  report = {
    "schema": "tach-target-provider-proof-v3",
    "passed": True,
    "source": {
      "git_head": git_output("rev-parse", "HEAD"),
      "tree_sha256": source_fingerprint(),
      "status": proof_status,
    },
    "proof_script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    "rustc": rustc.strip(),
    "counts": {
      "targets": len(TARGETS),
      "feature_configurations": configuration_count,
      "warning_strict_crate_checks": configuration_count,
      "warning_strict_external_api_checks": configuration_count,
      "strict_no_default_dependency_no_std_checks": len(TARGETS),
      "optimized_instant_now_route_checks": configuration_count,
      "optimized_instant_now_elapsed_route_checks": configuration_count,
      "optimized_ordered_instant_now_route_checks": configuration_count,
      "optimized_ordered_instant_now_elapsed_route_checks": configuration_count,
      "optimized_thread_cpu_instant_now_route_checks": configuration_count,
      "optimized_thread_cpu_instant_now_elapsed_route_checks": configuration_count,
      "optimized_now_elapsed_phase_closure_checks": configuration_count * 3 * 2,
      "optimized_clock_route_checks": configuration_count * 6,
      "optimized_vdso_resolver_artifact_checks": len(LINUX_VDSO_TARGETS),
      "optimized_vdso_resolver_route_checks": vdso_resolver_route_count,
      "targets_with_runtime_speed_evidence": len(BENCHMARKED_TARGETS),
      "targets_without_external_runtime_artifacts": len(TARGETS) - len(BENCHMARKED_TARGETS),
    },
    "runtime_performance_coverage": {
      "measured_targets": sorted(BENCHMARKED_TARGETS),
      "runtime_self_selecting_codegen_proven_targets": sorted(
        target
        for target in TARGETS
        if runtime_evidence_class(target) == "runtime_self_selecting_codegen_proven"
      ),
      "unique_provider_source_codegen_proven_targets": sorted(
        target
        for target in TARGETS
        if runtime_evidence_class(target) == "unique_provider_source_codegen_proven"
      ),
      "fallback_only_source_codegen_proven_targets": sorted(
        target
        for target in TARGETS
        if runtime_evidence_class(target) == "fallback_only_source_codegen_proven"
      ),
      "release_required_hosted_cells_missing_from_six_platform_campaign": sorted(
        RELEASE_REQUIRED_HOSTED_SPEED_TARGETS - BENCHMARKED_TARGETS
      ),
      "release_hosted_runtime_coverage_complete": (
        RELEASE_REQUIRED_HOSTED_SPEED_TARGETS <= BENCHMARKED_TARGETS
      ),
      "invariant": (
        "external measurement, deployment-host runtime self-selection, unique-provider source "
        "proof, and fallback-only source proof remain distinct evidence classes; codegen proof "
        "never masquerades as an external latency measurement"
      ),
    },
    "public_elapsed_route_scope": {
      "included": [
        "Instant::now() + Instant::elapsed()",
        "OrderedInstant::now() + OrderedInstant::elapsed()",
        "ThreadCpuInstant::now() + ThreadCpuInstant::elapsed()",
      ],
      "excluded": {
        "OrderedInstant::elapsed_unordered()": ORDERED_UNORDERED_ELAPSED_EXCLUSION,
      },
    },
    "targets": results,
    "limitations": [
      (
        "Cross-compilation and reachable LLVM IR prove API availability and expected primitive "
        "routing, not runtime availability, instruction latency, or relative performance on "
        "unbenchmarked hardware."
      ),
      (
        "For each Linux/Android vDSO target, a separate optimized tach-library closure proves "
        "that wall-provider detection reaches AT_SYSINFO_EHDR resolution of the architecture's "
        "exact versioned clock symbol and publishes the direct function pointer used by both "
        "public wall-clock hot routes. Kernel export availability remains a runtime eligibility "
        "condition."
      ),
      (
        "Linux x86, x86_64, Arm, aarch64, riscv64, LoongArch64, and powerpc64 builds contain "
        "their eligible architectural, libc, direct versioned vDSO, and raw-syscall candidates; "
        "tach chooses by measured runtime cost, which native benchmark evidence must establish "
        "per environment."
      ),
      (
        "Linux-kernel x86 and x86_64 contain direct TSC plus CLOCK_MONOTONIC and "
        "CLOCK_MONOTONIC_RAW libc, direct versioned vDSO, and raw-syscall routes. "
        "TSC eligibility requires invariant-TSC CPUID metadata, an enabled PR_GET_TSC mode, "
        "and Linux retaining tsc in its available clocksource list (current tsc is accepted "
        "when Android exposes only that node). Runtime measurements require the branched TSC "
        "path to win materially for that Instant contract. Instant and OrderedInstant select "
        "independently and retain separate conversion scales."
      ),
      (
        "FreeBSD x86_64 contains direct TSC and exact barrier + libc/raw CLOCK_MONOTONIC "
        "compound routes. Runtime selection requires the kernel's active TSC timecounter to "
        "be invariant, SMP-safe, enabled, and in its proven-safe quality tier. Intel compares "
        "CPUID with LFENCE for each OS mechanism, AMD compares MFENCE with RDTSCP, and unknown "
        "vendors retain CPUID; the selected full branched path must win materially."
      ),
      (
        "Linux RISC-V executes rdtime only when the all-online-CPU hwprobe intersection reports "
        "Zicntr and a nonzero TIME-CSR frequency; older kernels select among libc, direct "
        "versioned vDSO, and raw-syscall routes without probing the CSR. RISC-V, LoongArch, "
        "and powerpc64 independently measure CLOCK_MONOTONIC and CLOCK_MONOTONIC_RAW through "
        "each eligible libc, vDSO, and raw ABI; LoongArch and powerpc64 also measure their "
        "architectural counters, legacy syscall, and, on Power, HWCAP2-gated SCV paths. Ordered "
        "RISC-V ECALL and Power SC/SCV syscall candidates compete both with and without a "
        "redundant explicit pre-barrier."
      ),
      (
        "Linux armv7 exposes no ELF HWCAP that guarantees a standalone EL0 CNTVCT read, so "
        "Instant and OrderedInstant independently select among CLOCK_MONOTONIC/"
        "CLOCK_MONOTONIC_RAW libc/vDSO and time32/time64 syscalls. ThreadCpuInstant may use "
        "AArch32 CNTVCT only while perf's seqlocked cap_user_time metadata authorizes and "
        "converts that exact task-clock mmap read; it compares the complete path against the "
        "selected native thread-clock entry route. AArch32 CNTVCTSS remains ineligible because "
        "there is no FEAT_ECV HWCAP. s390x independently selects the wall time domains through "
        "its libc/vDSO or native raw-syscall path. Ordered Arm and s390 raw syscalls compete "
        "both with an explicit pre-barrier and with the ISA-owned SVC ordering boundary alone."
      ),
      (
        "RISC-V uses either the ratified memory-read before CSR-input FENCE relation or precise "
        "ECALL trap ordering; LoongArch Linux uses a synchronous syscall exception return "
        "immediately before RDTIME. These routes require native-hardware benchmark evidence."
      ),
      (
        "WASIp1 thread CPU clock id 3 is host-dependent and intentionally falls back to "
        "monotonic host time when unavailable; WASIp2 has no thread CPU clock and routes "
        "directly to its component-model monotonic clock."
      ),
      (
        "JavaScript-hosted WebAssembly uses process.threadCpuUsage() current-thread user + "
        "system time when Node exposes it and otherwise falls back to a module-init tournament "
        "between performance.now() and process.hrtime.bigint(); Date.now() is limited to one-time "
        "epoch correlation and an unavailable host freezes honestly. Node-hosted Emscripten "
        "uses the same process.threadCpuUsage() contract when available and otherwise exposes "
        "its guarded performance wall clock honestly as a fallback domain."
      ),
      (
        "Stable rustc does not expose its accepted wasm32 Emscripten +atomics codegen option "
        "through cfg(target_feature), so pthread builds use the explicit emscripten-pthreads "
        "Cargo feature together with the required Emscripten pthread/Wasm-atomics flags. The "
        "ordinary target is separately proved to retain its faster single-thread local route."
      ),
    ],
  }
  report_path = output / "report.json"
  report_path.write_text(json.dumps(report, indent=2) + "\n")
  print(
    f"PASS {len(TARGETS)}/{len(TARGETS)} targets, "
    f"{configuration_count * 2} warning-strict API checks, "
    f"{configuration_count * 6} optimized public clock routes, "
    f"{configuration_count * 3 * 2} now-plus-elapsed phase closures, "
    f"{vdso_resolver_route_count} optimized vDSO resolver routes; report: {report_path}"
  )


if __name__ == "__main__":
  main()
