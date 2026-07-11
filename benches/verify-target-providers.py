#!/usr/bin/env python3
"""Compile every supported target and verify ThreadCpuInstant's codegen route."""

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

LINUX_INLINE_TARGETS = {
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
  "x86_64-unknown-linux-musl",
}

PROBE_SOURCE = """#![no_std]

use tach::{Instant, OrderedInstant, ThreadCpuInstant};

#[unsafe(no_mangle)]
pub extern "C" fn tach_probe_thread_cpu_elapsed() -> u64 {
  let start = ThreadCpuInstant::now();
  start.elapsed().as_nanos() as u64
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
default = ["tach/thread-cpu-inline"]

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
  return [] if mode == "default" else ["--no-default-features"]


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
  env["RUSTFLAGS"] = "-D warnings"
  artifacts = {}
  for mode in ("default", "no-default"):
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
      env,
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
      env,
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
      env,
    )
    candidates = list(
      (codegen_target / target / "release/deps").glob("tach_architecture_probe-*.ll")
    )
    if len(candidates) != 1:
      raise RuntimeError(f"{target} {mode}: expected one LLVM IR file, found {len(candidates)}")
    artifacts[mode] = candidates[0]
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


def reachable_ir(ir: str) -> str:
  definitions = llvm_functions(ir)
  root = "tach_probe_thread_cpu_elapsed"
  if root not in definitions:
    raise RuntimeError(f"optimized probe is missing {root}")
  symbol = re.compile(r'@(?:"([^"]+)"|([-A-Za-z0-9_.$]+))')
  pending = [root]
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
      if called in definitions and called not in visited:
        pending.append(called)
  return "\n".join(bodies)


def clock_gettime(clock_id: int) -> str:
  return rf"@clock_gettime\([^,\n]*\b{clock_id},"


def route(target: str, mode: str) -> tuple[str, str, str, list[str], list[str]]:
  if target in LINUX_INLINE_TARGETS and mode == "default":
    counter = "cntvct_el0" if target.startswith("aarch64") else "llvm.x86.rdtsc"
    provider = "measured Linux perf-mmap task clock or POSIX thread CPU clock"
    primitive = f"perf mmap seqlock + {counter}; clock_gettime(CLOCK_THREAD_CPUTIME_ID) candidate"
    fallback = "architectural monotonic counter"
    required = [
      "thread_cpu12linux_inline",
      counter,
      "load volatile",
      r'fence syncscope\("singlethread"\) acquire',
      clock_gettime(3),
    ]
    return provider, primitive, fallback, required, []

  if target.endswith("apple-darwin"):
    provider = "POSIX thread CPU clock"
    primitive = "clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)"
    fallback = "none: macOS primitive is infallible"
    return provider, primitive, fallback, [r"@clock_gettime_nsec_np\([^,\n]*\b16\)"], []

  if target.endswith("pc-windows-msvc"):
    counter = "cntvct_el0" if target.startswith("aarch64") else "llvm.x86.rdtsc"
    provider = "Windows thread times"
    primitive = "GetThreadTimes(GetCurrentThread())"
    fallback = "architectural monotonic counter"
    return provider, primitive, fallback, ["@GetCurrentThread", "@GetThreadTimes", counter], []

  if "wasip" in target:
    provider = "WASI thread CPU clock when the host implements clock id 3"
    primitive = "wasi_snapshot_preview1.clock_time_get(THREAD_CPUTIME_ID=3)"
    fallback = "wasi_snapshot_preview1.clock_time_get(MONOTONIC=1)"
    required = [
      r"thread_cpu9now_nanos14clock_time_get[^\n]*\(i32 noundef 3,",
      r"fallback4wasi14clock_time_get[^\n]*\(i32 noundef 1,",
    ]
    return provider, primitive, fallback, required, []

  if target in ("wasm32-unknown-unknown", "wasm32v1-none"):
    provider = "Performance.now wall-time fallback"
    primitive = "globalThis.performance.now()"
    fallback = "same provider"
    return provider, primitive, fallback, ["tach4arch4wasm15performance_now"], []

  if target == "wasm32-unknown-emscripten":
    provider = "monotonic wall-time fallback"
    primitive = "clock_gettime(CLOCK_MONOTONIC)"
    fallback = "same provider"
    return provider, primitive, fallback, [clock_gettime(1)], [clock_gettime(3)]

  if target == "x86_64-unknown-freebsd":
    native_id = 14
  else:
    native_id = 3
  provider = "POSIX thread CPU clock"
  primitive = "clock_gettime(CLOCK_THREAD_CPUTIME_ID)"
  if target.startswith(("x86_64", "i686")):
    fallback = "RDTSC monotonic counter"
    fallback_pattern = "llvm.x86.rdtsc"
  elif target.startswith("aarch64"):
    fallback = "CNTVCT_EL0 monotonic counter"
    fallback_pattern = "cntvct_el0"
  elif target.startswith("riscv64"):
    fallback = "rdtime monotonic counter"
    fallback_pattern = r'"rdtime \$\{0\}'
  elif target.startswith("loongarch64"):
    fallback = "rdtime.d monotonic counter"
    fallback_pattern = r'"rdtime\.d \$\{0\}'
  else:
    fallback = "clock_gettime(CLOCK_MONOTONIC)"
    fallback_pattern = clock_gettime(1)
  forbidden = ["thread_cpu12linux_inline"]
  return provider, primitive, fallback, [clock_gettime(native_id), fallback_pattern], forbidden


def validate_codegen(target: str, mode: str, path: Path) -> dict:
  closure = reachable_ir(path.read_text())
  provider, primitive, fallback, required, forbidden = route(target, mode)
  missing = [pattern for pattern in required if re.search(pattern, closure) is None]
  unexpected = [pattern for pattern in forbidden if re.search(pattern, closure) is not None]
  if missing or unexpected:
    detail = []
    if missing:
      detail.append(f"missing {missing}")
    if unexpected:
      detail.append(f"unexpected {unexpected}")
    raise RuntimeError(f"{target} {mode} provider proof failed: {'; '.join(detail)}")
  return {
    "provider": provider,
    "native_primitive": primitive,
    "failure_fallback": fallback,
    "llvm_ir": str(path),
    "required_patterns": required,
    "forbidden_patterns": forbidden,
  }


def source_fingerprint() -> str:
  digest = hashlib.sha256()
  sources = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", *sorted((ROOT / "src").rglob("*.rs"))]
  for source in sources:
    digest.update(source.relative_to(ROOT).as_posix().encode())
    digest.update(b"\0")
    digest.update(source.read_bytes())
    digest.update(b"\0")
  return digest.hexdigest()


def git_output(*args: str) -> str:
  result = subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True)
  return result.stdout.strip()


def main() -> None:
  args = parse_args()
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
      mode: validate_codegen(target, mode, artifacts[target][mode])
      for mode in ("default", "no-default")
    }
    results.append(
      {
        "target": target,
        "covered_by_six_platform_runtime_campaign": target in BENCHMARKED_TARGETS,
        "warning_strict_crate_checks": ["default", "no-default"],
        "warning_strict_external_no_std_api_checks": ["default", "no-default"],
        "codegen_routes": modes,
      }
    )

  rustc = subprocess.check_output(["rustc", f"+{args.toolchain}", "-Vv"], text=True)
  report = {
    "schema": "tach-target-provider-proof-v1",
    "passed": True,
    "source": {
      "git_head": git_output("rev-parse", "HEAD"),
      "tree_sha256": source_fingerprint(),
      "status": git_output("status", "--short", "--", "Cargo.toml", "Cargo.lock", "src"),
    },
    "rustc": rustc.strip(),
    "counts": {
      "targets": len(TARGETS),
      "feature_configurations": len(TARGETS) * 2,
      "warning_strict_crate_checks": len(TARGETS) * 2,
      "warning_strict_external_no_std_api_checks": len(TARGETS) * 2,
      "optimized_provider_codegen_checks": len(TARGETS) * 2,
    },
    "targets": results,
    "limitations": [
      (
        "Cross-compilation and LLVM IR prove API availability and provider routing, not "
        "runtime availability or relative latency on unbenchmarked hardware."
      ),
      (
        "Linux x86_64 and aarch64 default builds contain both candidates; tach chooses by "
        "measured runtime cost, which the benchmark campaign must establish per environment."
      ),
      (
        "WASI thread CPU clock id 3 is host-dependent and intentionally falls back to "
        "monotonic host time when unavailable."
      ),
      (
        "WebAssembly unknown/none and Emscripten have no portable current-thread CPU clock, "
        "so ThreadCpuInstant uses the fastest tach wall source for those targets."
      ),
    ],
  }
  report_path = output / "report.json"
  report_path.write_text(json.dumps(report, indent=2) + "\n")
  print(
    f"PASS {len(TARGETS)}/23 targets, 92 warning-strict API checks, "
    f"46 optimized provider routes; report: {report_path}"
  )


if __name__ == "__main__":
  main()
