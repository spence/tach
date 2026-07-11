#!/usr/bin/env bash
# Provision EC2, run the criterion speed bench, extract per-clock medians, pull
# back, terminate. Models benches/run-ordered-verify-aws.sh (ephemeral keypair,
# describe-images AMI, terminate-on-shutdown, trap cleanup) but runs the SPEED
# bench with the measured thread-CPU tier enabled and pulls a tiny clocks JSON.
#
# Usage: benches/run-speed-aws.sh <cell-name> <instance-type> [--use-docker-alpine]
#   e.g. benches/run-speed-aws.sh c7g    c7g.large
#        benches/run-speed-aws.sh musl   c7i.large --use-docker-alpine
#
# Requires: aws CLI profile "tach". Self-terminates on exit (trap) and on shutdown.
# Output: validated /tmp/speed-<cell>.json plus its raw extracted clocks.
# Thread-CPU entries include the actual provider, read-cost class, direct perf
# baseline when selected, and the complete runtime-selector evidence.
set -euo pipefail

CELL="${1:?usage: run-speed-aws.sh <cell-name> <instance-type> [--use-docker-alpine]}"
INSTANCE_TYPE="${2:?need instance type (e.g. c7g.large)}"
USE_ALPINE=0
[ "${3:-}" = "--use-docker-alpine" ] && USE_ALPINE=1

REGION="us-east-2"
PROFILE="tach"
KEY_NAME="tach-speed-$$-$(date +%s 2>/dev/null || echo 0)"
KEY_PATH="$(mktemp -t tach-speed-key.XXXXXX).pem"
RUSTC_PATH="$(mktemp -t tach-speed-rustc.XXXXXX)"
SG_ID="sg-05e99abafa54936d3"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SOURCE_REVISION="$(bash "$REPO_ROOT/benches/require-clean-benchmark-source.sh")"
IID=""

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
  aws_ ec2 delete-key-pair --key-name "$KEY_NAME" >/dev/null 2>&1 || true
  rm -f "$KEY_PATH" "$RUSTC_PATH" 2>/dev/null || true
}
trap cleanup EXIT

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

# Ship source (incl. benches/extract_speed.py).
TARBALL=/tmp/tach-speed-src.tgz
tar --exclude=target --exclude=.git --exclude='benches/*.png' --exclude='benches/*.svg' -czf "$TARBALL" .
$SCP "$TARBALL" "ec2-user@$IP:/tmp/src.tgz"
$SSH 'rm -rf tach && mkdir -p tach && tar -xzf /tmp/src.tgz -C tach'

# Build a remote runner so all the nested quoting lives in one heredoc, not in ssh
# argument strings. It writes clocks-out.json next to the source.
REMOTE=/tmp/remote-speed.sh
cat > "$REMOTE" <<'REMOTE_EOF'
#!/bin/sh
set -e
MODE="$1"
cd "$HOME/tach"
TEST='cargo test --release --tests --features bench-internal,thread-cpu-inline'
BENCH='cargo bench --bench instant --features bench-internal,thread-cpu-inline -- --warm-up-time 1 --measurement-time 3'
# The inline Linux tier is an opt-in machine capability. The benchmark owns
# enabling it on this disposable host; the extracted provider still records a
# syscall fallback if the PMU/mmap path is unavailable or loses at selection.
sudo sysctl -w kernel.perf_event_paranoid=-1 >/dev/null
if [ "$MODE" = musl ]; then
  sudo dnf install -y docker >/dev/null 2>&1
  sudo systemctl start docker
  sudo docker run --rm --security-opt seccomp=unconfined \
    -v "$HOME/tach:/work" -w /work alpine:3.20 sh -c '
    apk add --no-cache build-base curl python3 >/dev/null 2>&1
    curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null 2>&1
    . "$HOME/.cargo/env"
    cargo test --release --tests --features bench-internal,thread-cpu-inline >/dev/null 2>&1
    cargo bench --bench instant --features bench-internal,thread-cpu-inline -- --warm-up-time 1 --measurement-time 3 >/dev/null 2>&1
    python3 benches/extract_speed.py target/criterion > /work/clocks-out.json
    rustc --version > /work/rustc-version.txt
  '
else
  curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null 2>&1
  sudo dnf install -y gcc python3 >/dev/null 2>&1
  . "$HOME/.cargo/env"
  sh -c "$TEST" >/dev/null 2>&1
  sh -c "$BENCH" >/dev/null 2>&1
  python3 benches/extract_speed.py target/criterion > "$HOME/tach/clocks-out.json"
  rustc --version > "$HOME/tach/rustc-version.txt"
fi
REMOTE_EOF
$SCP "$REMOTE" "ec2-user@$IP:/tmp/remote-speed.sh"

MODE=gnu; [ "$USE_ALPINE" = "1" ] && MODE=musl
echo "=== running speed bench on instance (mode=$MODE) ==="
$SSH "sh /tmp/remote-speed.sh $MODE"

LOCAL_OUT="/tmp/speed-clocks-${CELL}.json"
$SCP "ec2-user@$IP:tach/clocks-out.json" "$LOCAL_OUT"
$SCP "ec2-user@$IP:tach/rustc-version.txt" "$RUSTC_PATH"
echo "pulled clocks -> $LOCAL_OUT"
cat "$LOCAL_OUT"

case "$CELL" in
  c7g|graviton)
    TITLE="AWS Graviton 3"; ORDER=1; TRIPLE="aarch64-unknown-linux-gnu" ;;
  inteln|c7i|gnu)
    TITLE="AWS Intel"; ORDER=2; TRIPLE="x86_64-unknown-linux-gnu" ;;
  intelm|musl)
    TITLE="AWS Intel (musl)"; ORDER=3; TRIPLE="x86_64-unknown-linux-musl" ;;
  *)
    echo "unknown campaign cell '$CELL'; add its title/order/triple mapping" >&2
    exit 1 ;;
esac
COMPOSED_OUT="/tmp/speed-${CELL}.json"
INSTANCE_LABEL="$INSTANCE_TYPE"
if [ "$USE_ALPINE" = 1 ]; then
  INSTANCE_LABEL="$INSTANCE_LABEL + Alpine"
fi
python3 "$REPO_ROOT/benches/compose-speed.py" "$LOCAL_OUT" "$COMPOSED_OUT" \
  --title "$TITLE" --instance "$INSTANCE_LABEL" \
  --triple "$TRIPLE" --order "$ORDER" \
  --source-revision "$SOURCE_REVISION" --rustc-version "$(cat "$RUSTC_PATH")" \
  --harness criterion --cargo-profile bench
echo "validated cell -> $COMPOSED_OUT"

# Post-run termination is synchronous; the trap remains the failure backstop.
aws_ ec2 terminate-instances --instance-ids "$IID" >/dev/null
aws_ ec2 wait instance-terminated --instance-ids "$IID"
echo "terminated $IID"
IID=""
