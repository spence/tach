#!/usr/bin/env python3

from __future__ import annotations

from pathlib import Path
import subprocess
import unittest


BENCHES_DIR = Path(__file__).resolve().parent
RUNNERS = (
    "run-speed-local.sh",
    "run-speed-aws.sh",
    "run-speed-freebsd-aws.sh",
)
SNAPSHOT_RUNNERS = (
    "run-speed-local.sh",
    "run-speed-aws.sh",
    "run-speed-freebsd-aws.sh",
    "run-speed-lambda.sh",
    "run-speed-host-runtime.sh",
    "run-speed-native-supplemental.sh",
    "run-runtime-smoke.sh",
)


class SealedRunnerWiringTests(unittest.TestCase):
    def source(self, filename: str) -> str:
        return (BENCHES_DIR / filename).read_text(encoding="utf-8")

    def test_every_nonworkflow_runner_uses_fresh_sealed_evidence(self) -> None:
        for filename in RUNNERS:
            with self.subTest(runner=filename):
                source = self.source(filename)

                self.assertIn("CARGO_TARGET_DIR=\"$target_dir\"", source)
                self.assertTrue(
                    "mktemp -d -t tach-speed" in source
                    or "if [ -e \"$target_dir\" ]" in source,
                    "benchmark target is not demonstrably fresh",
                )
                self.assertIn("TACH_BENCH_EVIDENCE", source)
                self.assertIn("TACH_BENCH_SOURCE_REVISION", source)
                self.assertIn("TACH_BENCH_RUNNER", source)

                sealer = source.index("seal-speed-source.py")
                collector = source.index("collect-speed-bundle.py")
                self.assertLess(sealer, collector)
                self.assertNotIn("extract_speed.py", source)
                self.assertNotIn("clocks-out.json", source)

    def test_primary_and_supplemental_runners_use_their_correct_composers(self) -> None:
        source = self.source("run-speed-local.sh")
        self.assertIn("compose-speed.py", source)
        self.assertIn("compose-supplemental-speed.py", source)
        self.assertIn("speed-supplemental-macos-aarch64-no-default.json", source)
        self.assertIn("--no-default-features --features bench-internal", source)
        self.assertIn("--collector-bundle", source)

        source = self.source("run-speed-aws.sh")
        self.assertIn("compose-speed.py", source)
        self.assertIn("compose-supplemental-speed.py", source)
        self.assertIn("speed-supplemental-linux-x86_64-no-default.json", source)
        self.assertIn("speed-supplemental-linux-aarch64-no-default.json", source)
        self.assertIn("speed-supplemental-linux-musl-x86_64-no-default.json", source)
        self.assertIn("--thread-cpu-profile runtime_tournament", source)
        self.assertIn("--collector-bundle", source)

        source = self.source("run-speed-lambda.sh")
        self.assertIn("compose-speed.py", source)
        self.assertIn("compose-supplemental-speed.py", source)
        self.assertIn("speed-supplemental-lambda-aarch64.json", source)
        self.assertIn("--instant-profile runtime_tournament", source)
        self.assertIn("--collector-bundle", source)

        source = self.source("run-speed-host-runtime.sh")
        for artifact in (
            "speed-supplemental-wasm-node.json",
            "speed-supplemental-wasm-node-no-default.json",
            "speed-supplemental-browser-negative.json",
            "speed-supplemental-browser-negative-no-default.json",
            "speed-supplemental-emscripten-node.json",
            "speed-supplemental-emscripten-node-no-default.json",
            "speed-supplemental-emscripten-pthreads.json",
            "speed-supplemental-wasi-p1-node.json",
            "speed-supplemental-wasi-p1-node-no-default.json",
            "speed-supplemental-wasi-p1-wasmtime.json",
            "speed-supplemental-wasi-p1-wasmtime-no-default.json",
            "speed-supplemental-wasi-p2-wasmtime.json",
            "speed-supplemental-wasi-p2-wasmtime-no-default.json",
        ):
            self.assertIn(artifact, source)
        self.assertIn("wasm-bindgen", source)
        self.assertIn("tach-host-runtime-emscripten", source)
        self.assertIn("tach-host-runtime-wasip1", source)
        self.assertIn('require("node:wasi")', source)
        self.assertIn('wasmtime run "$runtime"', source)
        self.assertIn("collect-host-speed-bundle.py", source)
        self.assertIn("compose-supplemental-speed.py", source)
        self.assertIn('thread_cpu_profile="availability_fallback"', source)
        self.assertIn('thread_cpu_profile="fallback_only"', source)
        self.assertIn('cargo_args+=(--no-default-features)', source)
        self.assertIn("emscripten-host,emscripten-pthreads", source)
        self.assertIn('pthread_toolchain="nightly-2026-06-02"', source)
        self.assertIn("-Z build-std=std,panic_abort", source)
        self.assertIn("-C panic=abort", source)

        source = self.source("run-runtime-smoke.sh")
        for artifact in (
            "speed-supplemental-wasip1-threads-smoke.json",
            "speed-supplemental-wasip1-threads-no-default-smoke.json",
            "speed-supplemental-wasm32v1-none-smoke.json",
            "speed-supplemental-wasm32v1-none-no-default-smoke.json",
        ):
            self.assertIn(artifact, source)
        self.assertIn('feature_args+=(--no-default-features)', source)
        self.assertIn("compose-supplemental-speed.py", source)

        source = self.source("run-speed-freebsd-aws.sh")
        self.assertIn("compose-supplemental-speed.py", source)
        self.assertNotIn('compose-speed.py"', source)
        self.assertIn("speed-supplemental-freebsd-x86_64.json", source)
        self.assertIn("speed-supplemental-freebsd-x86_64-no-default.json", source)
        self.assertIn('build_mode="${2:-default}"', source)
        self.assertIn("--no-default-features --features bench-internal", source)
        self.assertIn("--collector-bundle", source)
        self.assertIn("--thread-cpu-profile runtime_tournament", source)

        source = self.source("run-speed-native-supplemental.sh")
        self.assertIn("SUPPLEMENTAL_SPEED_CELLS", source)
        self.assertIn("run-speed-criterion.sh", source)
        self.assertIn("compose-supplemental-speed.py", source)
        self.assertIn('host_triple" != "$target', source)
        self.assertIn("--thread-cpu-profile runtime_tournament", source)

    def test_hosted_criterion_workflow_retains_source_sealed_bundles(self) -> None:
        source = (BENCHES_DIR.parent / ".github/workflows/bench-speed-windows.yml").read_text(
            encoding="utf-8"
        )
        self.assertEqual(source.count("seal-speed-source.py"), 8)
        self.assertEqual(source.count("collect-speed-bundle.py"), 8)
        self.assertEqual(source.count("--collector-bundle"), 8)
        self.assertEqual(source.count("TACH_BENCH_EVIDENCE:"), 7)
        self.assertEqual(source.count("TACH_BENCH_SOURCE_REVISION:"), 7)
        self.assertEqual(source.count("TACH_BENCH_RUNNER:"), 10)
        self.assertEqual(source.count("compose-supplemental-speed.py"), 7)
        self.assertIn("python benches/compose-speed.py", source)
        for artifact in (
            "speed-4-windows.json",
            "speed-supplemental-windows-i686.json",
            "speed-supplemental-windows-aarch64.json",
            "speed-supplemental-linux-i686.json",
            "speed-supplemental-linux-i686-no-default.json",
            "speed-supplemental-linux-riscv64gc.json",
            "speed-supplemental-linux-riscv64gc-no-default.json",
            "speed-supplemental-macos-x86_64.json",
            "speed-supplemental-windows-x86_64-no-default.json",
            "speed-supplemental-windows-i686-no-default.json",
            "speed-supplemental-windows-aarch64-no-default.json",
            "speed-supplemental-macos-x86_64-no-default.json",
            "speed-supplemental-macos-aarch64-no-default.json",
        ):
            self.assertIn(artifact, source)
        self.assertNotIn("extract_speed.py", source)
        self.assertNotIn("clocks.json", source)
        self.assertNotIn("cp -R", source)

    def test_aws_correctness_gate_retains_failure_and_prevents_sealing(self) -> None:
        source = self.source("run-speed-aws.sh")
        functions = []
        lines = source.splitlines()
        for index, line in enumerate(lines):
            if line.lstrip() != "run_logged_gate() {":
                continue
            indent = line[:len(line) - len(line.lstrip())]
            for end in range(index + 1, len(lines)):
                if lines[end] == f"{indent}}}":
                    functions.append("\n".join(lines[index:end + 1]))
                    break

        self.assertEqual(len(functions), 2)
        gate_call = (
            "run_logged_gate cargo-test cargo test --locked --release "
            "--tests --features bench-internal"
        )
        self.assertEqual(source.count(gate_call), 2)
        self.assertNotIn(f"{gate_call} >/dev/null", source)
        for function in functions:
            with self.subTest(function=function):
                completed = subprocess.run(
                    ["sh", "-c", f"""
set -eu
{function}
run_logged_gate cargo-test sh -c \
  'printf "retained stdout\\n"; printf "retained stderr\\n" >&2; exit 101' \
  diagnostic 'one spaced' one spaced "quote'arg"
printf "SEAL_RAN\\n"
"""],
                    check=False,
                    capture_output=True,
                    text=True,
                )

                self.assertEqual(completed.returncode, 101)
                self.assertIn("retained stdout", completed.stdout)
                self.assertIn("retained stderr", completed.stderr)
                self.assertIn("gate cargo-test command:", completed.stdout)
                self.assertIn(
                    " <diagnostic> <one spaced> <one> <spaced> "
                    "<quote'arg> ===",
                    completed.stdout,
                )
                self.assertIn("gate cargo-test status: 101", completed.stdout)
                self.assertNotIn("SEAL_RAN", completed.stdout)

    def test_alpine_remote_payload_has_no_literal_single_quote(self) -> None:
        source = self.source("run-speed-aws.sh")
        marker = "-w /work alpine:3.20 sh -c '"
        payload_start = source.index(marker) + len(marker)
        payload_end = source.index("\n  '\n", payload_start)

        self.assertNotIn("'", source[payload_start:payload_end])

    def test_aws_rejects_unsupported_alias_before_any_aws_call(self) -> None:
        source = self.source("run-speed-aws.sh")
        guard_start = source.index("  amd|c7a)")
        guard_end = source.index("  *)", guard_start)
        guard = source[guard_start:guard_end]

        self.assertIn("no canonical primary artifact", guard)
        self.assertIn("exit 2", guard)
        self.assertLess(guard_start, source.index("aws_ ec2 describe-instances"))

    def test_aws_transfers_the_collector_as_one_archive(self) -> None:
        source = self.source("run-speed-aws.sh")

        collector = source.rindex("collect-speed-bundle.py")
        archive = source.index(
            'tar -czf "$HOME/tach/collector.bundle.tgz"',
            collector,
        )
        transfer = source.index(
            'retry_scp "ec2-user@$IP:tach/collector.bundle.tgz" "$BUNDLE_ARCHIVE"'
        )
        extract = source.index('tar -xzf "$BUNDLE_ARCHIVE" -C "$RESULT_DIR"')

        self.assertLess(collector, archive)
        self.assertLess(archive, transfer)
        self.assertLess(transfer, extract)
        self.assertNotIn('$SCP -r "ec2-user@$IP:tach/collector.bundle"', source)

    def test_cloud_runners_emit_artifact_specific_bundle_names(self) -> None:
        aws = self.source("run-speed-aws.sh")
        self.assertIn(
            'BUNDLE_DIR="$RESULT_DIR/${artifact_id%.json}.collector.bundle"',
            aws,
        )
        self.assertIn('mv "$RESULT_DIR/collector.bundle" "$BUNDLE_DIR"', aws)

        freebsd = self.source("run-speed-freebsd-aws.sh")
        self.assertIn(
            'bundle_dir="$result_dir/${artifact%.json}.collector.bundle"',
            freebsd,
        )

    def test_aws_waits_for_stable_cloud_init_before_source_transfer(self) -> None:
        source = self.source("run-speed-aws.sh")

        readiness = source.index("cloud-init status --wait")
        readiness_guard = source.index('if [ "$ssh_ready" != 1 ]', readiness)
        transfer = source.index(
            'retry_scp "$TARBALL" "ec2-user@$IP:/tmp/src.tgz"'
        )

        self.assertLess(readiness, readiness_guard)
        self.assertLess(readiness_guard, transfer)
        self.assertIn('ssh_ready=1', source[readiness:readiness_guard])
        self.assertIn('exit 1', source[readiness_guard:transfer])

    def test_aws_retries_transports_without_restarting_the_benchmark(self) -> None:
        source = self.source("run-speed-aws.sh")
        retry_functions = source[source.index("retry_scp() {"):source.index("ssh_ready=0")]
        remote_run = source.index('$SSH "sh /tmp/remote-speed.sh')

        self.assertEqual(source.count("retry_scp "), 3)
        self.assertEqual(source.count("retry_ssh "), 1)
        self.assertEqual(retry_functions.count("for _ in $(seq 1 12)"), 2)
        self.assertNotIn("retry_ssh", source[remote_run:])

    def test_alpine_collector_is_returned_to_the_host_user_before_archiving(self) -> None:
        source = self.source("run-speed-aws.sh")
        collect = source.index(
            'collect-speed-bundle.py "$target_dir/criterion" /work/collector.bundle'
        )
        handoff = source.index(
            'sudo chown -R "$(id -u):$(id -g)" "$HOME/tach/collector.bundle"'
        )
        archive = source.index('tar -czf "$HOME/tach/collector.bundle.tgz"')

        self.assertLess(collect, handoff)
        self.assertLess(handoff, archive)

    def test_every_runner_uses_an_immutable_checked_revision_snapshot(self) -> None:
        expectations = {
            "run-speed-local.sh": (
                'git -C "$repo_root" --no-replace-objects archive --format=tar "$source_revision"',
                'tar -xf - -C "$source_dir"',
                'cd "$source_dir"',
                'python3 benches/compose-speed.py',
            ),
            "run-speed-aws.sh": (
                'git -C "$REPO_ROOT" --no-replace-objects archive --format=tar "$SOURCE_REVISION"',
                'tar -xzf "$TARBALL" -C "$SOURCE_DIR"',
                'retry_scp "$TARBALL" "ec2-user@$IP:/tmp/src.tgz"',
                'tar -xzf /tmp/src.tgz -C tach',
                'python3 "$SOURCE_DIR/benches/compose-speed.py"',
            ),
            "run-speed-freebsd-aws.sh": (
                'git -C "$repo_root" --no-replace-objects archive --format=tar "$source_revision"',
                'tar -xzf "$tarball" -C "$source_dir"',
                'scp "${ssh_options[@]}" "$tarball" "ec2-user@$ip:/tmp/tach-src.tgz"',
                'tar -xzf /tmp/tach-src.tgz -C "$HOME/tach"',
                'python3 "$source_dir/benches/compose-supplemental-speed.py"',
            ),
            "run-speed-lambda.sh": (
                'git -C "$repo_root" --no-replace-objects archive --format=tar "$source_revision"',
                'tar -xf - -C "$source_dir"',
                'cd "$source_dir/benches/lambda-speed"',
                'python3 "$source_dir/benches/compose-speed.py"',
            ),
            "run-speed-host-runtime.sh": (
                'git -C "$repo_root" --no-replace-objects archive --format=tar "$source_revision"',
                'tar -xf - -C "$source_dir"',
                'cargo_command=(cargo +1.95 build)',
                '"${cargo_command[@]}" --locked --release --manifest-path "$manifest"',
                'python3 "$source_dir/benches/compose-supplemental-speed.py"',
            ),
            "run-runtime-smoke.sh": (
                'git -C "$repo_root" --no-replace-objects archive --format=tar "$source_revision"',
                'tar -xf - -C "$source_dir"',
                'cargo +1.95 build --locked --release',
                'python3 "$source_dir/benches/compose-supplemental-speed.py"',
            ),
            "run-speed-native-supplemental.sh": (
                'git -C "$repo_root" --no-replace-objects archive --format=tar "$source_revision"',
                'tar -xf - -C "$source_dir"',
                'run-speed-criterion.sh',
                'python3 "$source_dir/benches/compose-supplemental-speed.py"',
            ),
        }

        self.assertEqual(set(expectations), set(SNAPSHOT_RUNNERS))
        for filename in SNAPSHOT_RUNNERS:
            with self.subTest(runner=filename):
                source = self.source(filename)
                self.assertIn("require-clean-benchmark-source.sh", source)
                self.assertIn("--no-replace-objects archive", source)
                self.assertRegex(
                    source,
                    r"(?m)^git -C \"\$[A-Za-z_]+\" --no-replace-objects "
                    r"archive --format=tar",
                )
                self.assertNotRegex(
                    source,
                    r"(?m)^git -C \"\$[A-Za-z_]+\" archive --format=tar",
                )
                for expected in expectations[filename]:
                    self.assertIn(expected, source)

                # A remotely shipped tree must be the checked commit's Git
                # archive, never a mutable checkout packed by `tar`.
                if filename in {"run-speed-aws.sh", "run-speed-freebsd-aws.sh"}:
                    self.assertNotIn("tar --exclude=target", source)
                    self.assertNotIn("--exclude=.git", source)
                    self.assertLess(
                        source.index("git -C"),
                        source.index("aws_ ec2 describe-instances"),
                    )

    def test_lambda_retains_runtime_attested_host_observation(self) -> None:
        source = self.source("run-speed-lambda.sh")

        self.assertIn("require-clean-benchmark-source.sh", source)
        self.assertIn("TACH_BENCH_SOURCE_REVISION", source)
        self.assertIn("TACH_BENCH_INVOCATION_ID", source)
        self.assertIn("TACH_BENCH_RUNNER", source)
        self.assertIn("cargo lambda build --locked --release", source)
        self.assertIn("build_arch_args=(--arm64)", source)
        self.assertIn("build_arch_args=(--x86-64)", source)
        self.assertIn('"$host_dir/run-$run.json"', source)
        self.assertIn('"$host_dir/invoke-$run.json"', source)
        self.assertIn("runtime-attestation.json", source)
        collector = source.index("collect-host-speed-bundle.py")
        composer = source.index("compose-speed.py")
        self.assertLess(collector, composer)
        self.assertIn("wait_until_deleted", source)
        self.assertIn("trap cleanup EXIT", source)
        self.assertNotIn("benches/speed-5-lambda.json", source)

    def test_browser_runner_waits_for_the_navigation_execution_context(self) -> None:
        source = self.source("run-browser-host-runtime.mjs")

        self.assertIn("evaluateAfterNavigation", source)
        self.assertIn("Cannot find default execution context", source)
        self.assertIn("const deadline = Date.now() + 30_000", source)
        self.assertIn("Date.now() >= deadline", source)
        self.assertIn("setTimeout(resolveRetry, 25)", source)
        self.assertIn("const withTimeout =", source)
        self.assertEqual(source.count("clearTimeout(timeout)"), 2)
        self.assertNotIn("await Promise.race", source)


if __name__ == "__main__":
    unittest.main()
