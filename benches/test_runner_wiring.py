#!/usr/bin/env python3

from __future__ import annotations

from pathlib import Path
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
        for filename in ("run-speed-local.sh", "run-speed-aws.sh"):
            with self.subTest(runner=filename):
                source = self.source(filename)
                self.assertIn("compose-speed.py", source)
                self.assertNotIn("compose-supplemental-speed.py", source)
                self.assertIn("--collector-bundle", source)

        source = self.source("run-speed-freebsd-aws.sh")
        self.assertIn("compose-supplemental-speed.py", source)
        self.assertNotIn('compose-speed.py"', source)
        self.assertIn("--artifact speed-supplemental-freebsd-x86_64.json", source)
        self.assertIn("--collector-bundle", source)

    def test_aws_rejects_unsupported_alias_before_any_aws_call(self) -> None:
        source = self.source("run-speed-aws.sh")
        guard_start = source.index("  amd|c7a)")
        guard_end = source.index("  *)", guard_start)
        guard = source[guard_start:guard_end]

        self.assertIn("no canonical primary artifact", guard)
        self.assertIn("exit 2", guard)
        self.assertLess(guard_start, source.index("aws_ ec2 describe-instances"))

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
                '$SCP "$TARBALL" "ec2-user@$IP:/tmp/src.tgz"',
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

    def test_lambda_fails_closed_before_any_cloud_or_composer_path(self) -> None:
        source = self.source("run-speed-lambda.sh")

        self.assertIn(
            "Lambda speed runner cannot currently produce retained release evidence",
            source,
        )
        self.assertIn("Lambda host-observation/source-seal protocol is required", source)
        self.assertIn("exit 2", source)
        self.assertNotIn("aws lambda", source)
        self.assertNotIn("cargo lambda", source)
        self.assertNotIn("compose-speed.py", source)
        self.assertNotIn("git archive", source)
        self.assertNotRegex(
            source,
            r"(?m)^\s*(?:aws|cargo|curl|wget|ssh|scp)\b",
        )


if __name__ == "__main__":
    unittest.main()
