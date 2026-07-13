#!/usr/bin/env bash
# Provision EC2, run a source-sealed Criterion campaign, collect its retained
# bundle, compose the canonical primary cell, pull it back, then terminate.
#
# Usage: benches/run-speed-aws.sh <cell-name> <instance-type> [--use-docker-alpine]
#   e.g. benches/run-speed-aws.sh c7g    c7g.large
#        benches/run-speed-aws.sh inteln c7i.large
#        benches/run-speed-aws.sh intelm c7i.large --use-docker-alpine
#
# Requires: aws CLI profile "tach". Self-terminates on exit (trap) and on shutdown.
# Output: a fresh /tmp/tach-speed-<cell>.* directory containing the canonical
# primary JSON cell and its retained collector.bundle.
set -euo pipefail

CELL="${1:?usage: run-speed-aws.sh <cell-name> <instance-type> [--use-docker-alpine]}"
INSTANCE_TYPE="${2:?need instance type (e.g. c7g.large)}"
USE_ALPINE=0
[ "${3:-}" = "--use-docker-alpine" ] && USE_ALPINE=1

case "$CELL" in
  c7g|graviton)
    artifact_id="speed-1-c7g.json"
    runner="aws-c7g"
    expected_mode=gnu
    ;;
  inteln|c7i|gnu)
    artifact_id="speed-2-inteln.json"
    runner="aws-inteln"
    expected_mode=gnu
    ;;
  intelm|musl)
    artifact_id="speed-3-intelm.json"
    runner="aws-intelm"
    expected_mode=musl
    ;;
  amd|c7a)
    echo "no canonical primary artifact is declared for the AMD EC2 alias; refusing to launch" >&2
    exit 2
    ;;
  *)
    echo "unknown campaign cell '$CELL'; add a canonical primary artifact before launching" >&2
    exit 2
    ;;
esac
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
BUNDLE_DIR="$RESULT_DIR/collector.bundle"
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
  --user-data $'#!/bin/bash\nshutdown -h +30\n' \
  --tag-specifications "ResourceType=instance,Tags=[{Key=Name,Value=tach-bench-speed-${CELL}}]" \
  --query 'Instances[0].InstanceId' --output text)
echo "instance $IID"

aws_ ec2 wait instance-running --instance-ids "$IID"
IP=$(aws_ ec2 describe-instances --instance-ids "$IID" \
  --query 'Reservations[0].Instances[0].PublicIpAddress' --output text)
echo "ip $IP"

SSH="ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 -i $KEY_PATH ec2-user@$IP"
SCP="scp -o StrictHostKeyChecking=no -i $KEY_PATH"
for _ in $(seq 1 40); do $SSH true 2>/dev/null && break || sleep 5; done

# Ship the frozen Git archive. The remote runner writes only a sealed collector
# bundle, never a caller-shaped clocks JSON.
$SCP "$TARBALL" "ec2-user@$IP:/tmp/src.tgz"
$SSH 'rm -rf tach && mkdir -p tach && tar -xzf /tmp/src.tgz -C tach'

# Keep nested quoting in one remote script. The source seal runs the benchmark
# command and only writes after that command succeeds.
cat > "$REMOTE" <<'REMOTE_EOF'
#!/bin/sh
set -eu
MODE="$1"
SOURCE_REVISION="$2"
RUNNER="$3"
cd "$HOME/tach"

# The EC2 cells exercise both sides of tach's Linux provider policy: make the
# perf task-clock user page available, then let tach's measured selector decide
# whether it is actually faster than CLOCK_THREAD_CPUTIME_ID. Lambda remains
# unmodified and covers the fleet-policy-denied fallback separately.
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
    cargo test --locked --release --tests --features bench-internal >/dev/null 2>&1
    export CARGO_TARGET_DIR="$target_dir"
    python3 benches/seal-speed-source.py "$target_dir/criterion" -- \
      cargo bench --locked --bench instant --features bench-internal -- \
        --warm-up-time 1 --measurement-time 3
    python3 benches/collect-speed-bundle.py "$target_dir/criterion" /work/collector.bundle
  '
else
  curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null 2>&1
  sudo dnf install -y gcc python3 >/dev/null 2>&1
  . "$HOME/.cargo/env"
  target_dir="$HOME/tach/.tach-speed-target"
  if [ -e "$target_dir" ]; then
    echo "fresh benchmark target already exists: $target_dir" >&2
    exit 1
  fi
  cargo test --locked --release --tests --features bench-internal >/dev/null 2>&1
  export CARGO_TARGET_DIR="$target_dir"
  export TACH_BENCH_EVIDENCE=1
  export TACH_BENCH_SOURCE_REVISION="$SOURCE_REVISION"
  export TACH_BENCH_RUNNER="$RUNNER"
  python3 benches/seal-speed-source.py "$target_dir/criterion" -- \
    cargo bench --locked --bench instant --features bench-internal -- \
      --warm-up-time 1 --measurement-time 3
  python3 benches/collect-speed-bundle.py "$target_dir/criterion" "$HOME/tach/collector.bundle"
fi
REMOTE_EOF
$SCP "$REMOTE" "ec2-user@$IP:/tmp/remote-speed.sh"

MODE=gnu
[ "$USE_ALPINE" = 1 ] && MODE=musl
echo "=== running sealed speed bench on instance (mode=$MODE) ==="
$SSH "sh /tmp/remote-speed.sh '$MODE' '$SOURCE_REVISION' '$runner'"

$SCP -r "ec2-user@$IP:tach/collector.bundle" "$BUNDLE_DIR"
python3 "$SOURCE_DIR/benches/compose-speed.py" "$COMPOSED_OUT" \
  --collector-bundle "$BUNDLE_DIR"
echo "wrote $COMPOSED_OUT with retained collector bundle $BUNDLE_DIR"

# Post-run termination is synchronous; the trap remains the failure backstop.
aws_ ec2 terminate-instances --instance-ids "$IID" >/dev/null
aws_ ec2 wait instance-terminated --instance-ids "$IID"
echo "terminated $IID"
IID=""
