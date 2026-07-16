#!/usr/bin/env bash
# Provision EC2, run a source-sealed Criterion campaign, collect its retained
# bundle, compose the selected primary or no-default supplemental cell, pull it
# back, then terminate.
#
# Usage: benches/run-speed-aws.sh <cell-name> <instance-type> [--use-docker-alpine] [--no-default]
#   e.g. benches/run-speed-aws.sh c7g    c7g.large
#        benches/run-speed-aws.sh inteln c7i.large
#        benches/run-speed-aws.sh intelm c7i.large --use-docker-alpine
#
# Requires: aws CLI profile "tach". Self-terminates on exit (trap) and on shutdown.
# Output: a fresh /tmp/tach-speed-<cell>.* directory containing the selected
# source-bound JSON cell and its retained collector.bundle.
set -euo pipefail

CELL="${1:?usage: run-speed-aws.sh <cell-name> <instance-type> [--use-docker-alpine] [--no-default]}"
INSTANCE_TYPE="${2:?need instance type (e.g. c7g.large)}"
USE_ALPINE=0
BUILD_MODE=default
for option in "${@:3}"; do
  case "$option" in
    --use-docker-alpine) USE_ALPINE=1 ;;
    --no-default) BUILD_MODE=no-default ;;
    *)
      echo "unknown option: $option" >&2
      exit 2
      ;;
  esac
done

case "$CELL" in
  c7g|graviton)
    default_artifact_id="speed-1-c7g.json"
    no_default_artifact_id="speed-supplemental-linux-aarch64-no-default.json"
    runner="aws-c7g"
    expected_mode=gnu
    ;;
  inteln|c7i|gnu)
    default_artifact_id="speed-2-inteln.json"
    no_default_artifact_id="speed-supplemental-linux-x86_64-no-default.json"
    runner="aws-inteln"
    expected_mode=gnu
    ;;
  intelm|musl)
    default_artifact_id="speed-3-intelm.json"
    no_default_artifact_id="speed-supplemental-linux-musl-x86_64-no-default.json"
    runner="aws-intelm"
    expected_mode=musl
    ;;
  amd|c7a)
    # Sanctioned flip-probe cell (OBJ-SIMPLIFY-TIMERS §5.2): the same x86_64-gnu
    # measured code as the frozen `inteln` cell, run on AMD Zen4 to check for a
    # same-target selection flip. It mints no canonical primary cell; it retains
    # the collector bundle and its Rust-emitted selection, tagged with an honest
    # runner. Genuinely unknown aliases still fail fast in the `*)` arm below.
    default_artifact_id="speed-probe-amd-c7a.json"
    no_default_artifact_id="speed-probe-amd-c7a-no-default.json"
    runner="aws-c7a"
    expected_mode=gnu
    is_flip_probe=1
    ;;
  c8g)
    # Sanctioned flip-probe cell: the same aarch64-gnu measured code as the
    # frozen `c7g` cell, run on Graviton 4 to check for a same-target flip.
    default_artifact_id="speed-probe-c8g.json"
    no_default_artifact_id="speed-probe-c8g-no-default.json"
    runner="aws-c8g"
    expected_mode=gnu
    is_flip_probe=1
    ;;
  *)
    echo "unknown campaign cell '$CELL'; add a canonical primary artifact before launching" >&2
    exit 2
    ;;
esac
if [ "$BUILD_MODE" = no-default ]; then
  artifact_id="$no_default_artifact_id"
else
  artifact_id="$default_artifact_id"
fi
if [ "$expected_mode" = musl ] && [ "$USE_ALPINE" != 1 ]; then
  echo "$CELL requires --use-docker-alpine for its musl primary identity" >&2
  exit 2
fi
if [ "$expected_mode" = gnu ] && [ "$USE_ALPINE" = 1 ]; then
  echo "$CELL has a GNU primary identity and cannot use the Alpine runner" >&2
  exit 2
fi

REGION="us-east-2"
PROFILE="tach"
KEY_NAME="tach-speed-$$-$(date +%s 2>/dev/null || echo 0)"
KEY_PATH="$(mktemp -t tach-speed-key.XXXXXX)"
SG_ID="sg-05e99abafa54936d3"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SOURCE_REVISION="$(bash "$REPO_ROOT/benches/require-clean-benchmark-source.sh")"
RESULT_DIR="$(mktemp -d -t "tach-speed-${CELL}.XXXXXX")"
BUNDLE_DIR="$RESULT_DIR/${artifact_id%.json}.collector.bundle"
BUNDLE_ARCHIVE="$RESULT_DIR/collector.bundle.tgz"
COMPOSED_OUT="$RESULT_DIR/$artifact_id"
TARBALL="$(mktemp -t tach-speed-src.XXXXXX)"
SOURCE_DIR="$(mktemp -d -t "tach-speed-${CELL}-source.XXXXXX")"
REMOTE="$(mktemp -t tach-speed-remote.XXXXXX)"
IID=""
KEY_CREATED=0

case "$INSTANCE_TYPE" in
  c7g*|c8g*|t4g*|c6g*|m7g*|m6g*|r7g*|r8g*) ARCH=arm64 ;;
  *) ARCH=x86_64 ;;
esac
AMI_PATTERN="al2023-ami-2023.*-kernel-6.12-${ARCH}"

aws_() { aws "$@" --region "$REGION" --profile "$PROFILE"; }

cleanup() {
  if [ -n "${IID:-}" ]; then
    echo "terminating $IID"
    if aws_ ec2 terminate-instances --instance-ids "$IID" >/dev/null 2>&1; then
      aws_ ec2 wait instance-terminated --instance-ids "$IID" >/dev/null 2>&1 || true
    fi
  fi
  if [ "$KEY_CREATED" = 1 ]; then
    aws_ ec2 delete-key-pair --key-name "$KEY_NAME" >/dev/null 2>&1 || true
  fi
  rm -f "$KEY_PATH" "$TARBALL" "$REMOTE" 2>/dev/null || true
  rm -rf "$SOURCE_DIR" 2>/dev/null || true
}
trap cleanup EXIT

# Freeze the exact commit that passed the clean-source gate before any cloud
# action. The same Git archive is unpacked locally for composition and shipped
# to the instance for the benchmark itself.
git -C "$REPO_ROOT" --no-replace-objects archive --format=tar "$SOURCE_REVISION" | gzip -n > "$TARBALL"
tar -xzf "$TARBALL" -C "$SOURCE_DIR"

# Orphan guard: refuse to launch while prior bench instances are still alive.
ORPHANS=$(aws_ ec2 describe-instances \
  --filters "Name=tag:Name,Values=tach-bench-*" "Name=instance-state-name,Values=running,pending" \
  --query 'Reservations[].Instances[].InstanceId' --output text)
if [ -n "$ORPHANS" ]; then
  echo "ABORT: orphan tach-bench-* instances still alive: $ORPHANS" >&2
  exit 1
fi

AMI_ID=$(aws_ ec2 describe-images --owners amazon \
  --filters "Name=name,Values=${AMI_PATTERN}" "Name=state,Values=available" \
  --query 'reverse(sort_by(Images,&CreationDate))[0].ImageId' --output text)

echo "creating ephemeral keypair $KEY_NAME"
aws_ ec2 create-key-pair --key-name "$KEY_NAME" --query 'KeyMaterial' --output text > "$KEY_PATH"
KEY_CREATED=1
chmod 600 "$KEY_PATH"

echo "launching $INSTANCE_TYPE ($ARCH, ami $AMI_ID) — self-terminates on exit"
IID=$(aws_ ec2 run-instances \
  --image-id "$AMI_ID" \
  --instance-type "$INSTANCE_TYPE" \
  --key-name "$KEY_NAME" \
  --security-group-ids "$SG_ID" \
  --instance-initiated-shutdown-behavior terminate \
  --user-data $'#!/bin/bash\nshutdown -h +60\n' \
  --tag-specifications "ResourceType=instance,Tags=[{Key=Name,Value=tach-bench-speed-${CELL}}]" \
  --query 'Instances[0].InstanceId' --output text)
echo "instance $IID"

aws_ ec2 wait instance-running --instance-ids "$IID"
IP=$(aws_ ec2 describe-instances --instance-ids "$IID" \
  --query 'Reservations[0].Instances[0].PublicIpAddress' --output text)
echo "ip $IP"

SSH="ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4 -i $KEY_PATH ec2-user@$IP"
SCP="scp -o StrictHostKeyChecking=no -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4 -i $KEY_PATH"

retry_scp() {
  source_path="$1"
  destination_path="$2"
  for _ in $(seq 1 12); do
    if $SCP "$source_path" "$destination_path"; then
      return 0
    fi
    sleep 5
  done
  echo "secure copy to or from instance $IID never succeeded" >&2
  return 1
}

retry_ssh() {
  for _ in $(seq 1 12); do
    if $SSH "$@"; then
      return 0
    fi
    sleep 5
  done
  echo "remote setup command on instance $IID never succeeded" >&2
  return 1
}

ssh_ready=0
for _ in $(seq 1 40); do
  if $SSH 'cloud-init status --wait >/dev/null 2>&1'; then
    ssh_ready=1
    break
  fi
  sleep 5
done
if [ "$ssh_ready" != 1 ]; then
  echo "instance $IID never reached stable SSH readiness" >&2
  exit 1
fi

# Ship the frozen Git archive. The remote runner writes only a sealed collector
# bundle, never a caller-shaped clocks JSON.
retry_scp "$TARBALL" "ec2-user@$IP:/tmp/src.tgz"
retry_ssh 'rm -rf tach && mkdir -p tach && tar -xzf /tmp/src.tgz -C tach'

# Keep nested quoting in one remote script. The source seal runs the benchmark
# command and only writes after that command succeeds.
cat > "$REMOTE" <<'REMOTE_EOF'
#!/bin/sh
set -eu
MODE="$1"
SOURCE_REVISION="$2"
RUNNER="$3"
BUILD_MODE="$4"
cd "$HOME/tach"

# The EC2 cells exercise both sides of tach's Linux provider policy: make the
# perf task-clock user page available, then audit the capability-selected mmap
# path against CLOCK_THREAD_CPUTIME_ID. Lambda remains unmodified and covers
# the fleet-policy-denied fallback separately.
sudo sysctl -w kernel.perf_event_paranoid=-1
if [ -e /proc/sys/kernel/perf_user_access ]; then
  sudo sysctl -w kernel.perf_user_access=1
fi

if [ "$MODE" = musl ]; then
  sudo dnf install -y docker >/dev/null 2>&1
  sudo systemctl start docker
  sudo docker run --rm --security-opt seccomp=unconfined \
    -e TACH_BENCH_EVIDENCE=1 \
    -e TACH_BENCH_SOURCE_REVISION="$SOURCE_REVISION" \
    -e TACH_BENCH_RUNNER="$RUNNER" \
    -e BUILD_MODE="$BUILD_MODE" \
    -v "$HOME/tach:/work" -w /work alpine:3.20 sh -c '
    set -eu
    apk add --no-cache build-base curl python3 >/dev/null 2>&1
    curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null 2>&1
    . "$HOME/.cargo/env"
    target_dir=/work/.tach-speed-target
    if [ -e "$target_dir" ]; then
      echo "fresh benchmark target already exists: $target_dir" >&2
      exit 1
    fi
    run_logged_gate() {
      gate_name="$1"
      shift
      printf "=== gate %s command:" "$gate_name"
      for gate_arg in "$@"; do
        printf " <%s>" "$gate_arg"
      done
      printf " ===\n"
      if "$@"; then
        gate_status=0
      else
        gate_status=$?
      fi
      printf "=== gate %s status: %s ===\n" "$gate_name" "$gate_status"
      return "$gate_status"
    }
    if [ "$BUILD_MODE" = no-default ]; then
      run_logged_gate cargo-test cargo test --locked --release --tests \
        --no-default-features --features bench-internal
    else
      run_logged_gate cargo-test cargo test --locked --release --tests --features bench-internal
    fi
    export CARGO_TARGET_DIR="$target_dir"
    if [ "$BUILD_MODE" = no-default ]; then
      python3 benches/seal-speed-source.py "$target_dir/criterion" -- \
        cargo bench --locked --bench instant --no-default-features --features bench-internal -- \
          --warm-up-time 1 --measurement-time 3
    else
      python3 benches/seal-speed-source.py "$target_dir/criterion" -- \
        cargo bench --locked --bench instant --features bench-internal -- \
          --warm-up-time 1 --measurement-time 3
    fi
    python3 benches/collect-speed-bundle.py "$target_dir/criterion" /work/collector.bundle
  '
  sudo chown -R "$(id -u):$(id -g)" "$HOME/tach/collector.bundle"
else
  echo "=== setup system packages ==="
  sudo dnf install -y gcc python3
  echo "=== setup Rust toolchain ==="
  curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  . "$HOME/.cargo/env"
  target_dir="$HOME/tach/.tach-speed-target"
  if [ -e "$target_dir" ]; then
    echo "fresh benchmark target already exists: $target_dir" >&2
    exit 1
  fi
  run_logged_gate() {
    gate_name="$1"
    shift
    printf "=== gate %s command:" "$gate_name"
    for gate_arg in "$@"; do
      printf " <%s>" "$gate_arg"
    done
    printf " ===\n"
    if "$@"; then
      gate_status=0
    else
      gate_status=$?
    fi
    printf "=== gate %s status: %s ===\n" "$gate_name" "$gate_status"
    return "$gate_status"
  }
  if [ "$BUILD_MODE" = no-default ]; then
    run_logged_gate cargo-test cargo test --locked --release --tests \
      --no-default-features --features bench-internal
  else
    run_logged_gate cargo-test cargo test --locked --release --tests --features bench-internal
  fi
  export CARGO_TARGET_DIR="$target_dir"
  export TACH_BENCH_EVIDENCE=1
  export TACH_BENCH_SOURCE_REVISION="$SOURCE_REVISION"
  export TACH_BENCH_RUNNER="$RUNNER"
  if [ "$BUILD_MODE" = no-default ]; then
    python3 benches/seal-speed-source.py "$target_dir/criterion" -- \
      cargo bench --locked --bench instant --no-default-features --features bench-internal -- \
        --warm-up-time 1 --measurement-time 3
  else
    python3 benches/seal-speed-source.py "$target_dir/criterion" -- \
      cargo bench --locked --bench instant --features bench-internal -- \
        --warm-up-time 1 --measurement-time 3
  fi
  python3 benches/collect-speed-bundle.py "$target_dir/criterion" "$HOME/tach/collector.bundle"
fi
tar -czf "$HOME/tach/collector.bundle.tgz" -C "$HOME/tach" collector.bundle
REMOTE_EOF
retry_scp "$REMOTE" "ec2-user@$IP:/tmp/remote-speed.sh"

MODE=gnu
[ "$USE_ALPINE" = 1 ] && MODE=musl
echo "=== running sealed speed bench on instance (mode=$MODE) ==="
$SSH "sh /tmp/remote-speed.sh '$MODE' '$SOURCE_REVISION' '$runner' '$BUILD_MODE'"

retry_scp "ec2-user@$IP:tach/collector.bundle.tgz" "$BUNDLE_ARCHIVE"
tar -xzf "$BUNDLE_ARCHIVE" -C "$RESULT_DIR"
rm -f "$BUNDLE_ARCHIVE"
mv "$RESULT_DIR/collector.bundle" "$BUNDLE_DIR"
if [ "${is_flip_probe:-0}" = 1 ]; then
  # A flip probe has no canonical primary or supplemental artifact identity; its
  # selection lives in the retained collector bundle and is extracted locally.
  echo "flip-probe: retained collector bundle $BUNDLE_DIR (source $SOURCE_REVISION, runner $runner); extract selection locally"
elif [ "$BUILD_MODE" = no-default ]; then
  python3 "$SOURCE_DIR/benches/compose-supplemental-speed.py" \
    --artifact "$artifact_id" \
    --output "$COMPOSED_OUT" \
    --source-revision "$SOURCE_REVISION" \
    --collector-bundle "$BUNDLE_DIR" \
    --instant-profile runtime_tournament \
    --ordered-profile runtime_tournament \
    --thread-cpu-profile runtime_tournament
  echo "wrote $COMPOSED_OUT with retained collector bundle $BUNDLE_DIR"
else
  python3 "$SOURCE_DIR/benches/compose-speed.py" "$COMPOSED_OUT" \
    --collector-bundle "$BUNDLE_DIR"
  echo "wrote $COMPOSED_OUT with retained collector bundle $BUNDLE_DIR"
fi

# Post-run termination is synchronous; the trap remains the failure backstop.
aws_ ec2 terminate-instances --instance-ids "$IID" >/dev/null
aws_ ec2 wait instance-terminated --instance-ids "$IID"
echo "terminated $IID"
IID=""
