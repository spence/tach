#!/usr/bin/env bash
# Run a source-sealed Criterion campaign on the official FreeBSD/amd64 EC2
# image, collect its retained bundle, and compose one supplemental speed cell.
set -euo pipefail

instance_type="${1:-c7i.large}"
region=us-east-2
profile=tach
key_name="tach-speed-freebsd-$$-$(date +%s 2>/dev/null || echo 0)"
key_path="$(mktemp -t tach-speed-freebsd-key.XXXXXX)"
security_group=sg-05e99abafa54936d3
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
source_revision="$(bash "$repo_root/benches/require-clean-benchmark-source.sh")"
result_dir="$(mktemp -d -t tach-speed-freebsd.XXXXXX)"
bundle_dir="$result_dir/collector.bundle"
composed_output="$result_dir/speed-supplemental-freebsd-x86_64.json"
tarball="$(mktemp -t tach-speed-freebsd-src.XXXXXX)"
source_dir="$(mktemp -d -t tach-speed-freebsd-source.XXXXXX)"
remote_runner="$(mktemp -t tach-speed-freebsd-runner.XXXXXX)"
instance_id=""
key_created=0

aws_() { aws "$@" --region "$region" --profile "$profile"; }

cleanup() {
  if [ -n "${instance_id:-}" ]; then
    echo "terminating $instance_id"
    if aws_ ec2 terminate-instances --instance-ids "$instance_id" >/dev/null 2>&1; then
      aws_ ec2 wait instance-terminated --instance-ids "$instance_id" >/dev/null 2>&1 || true
    fi
  fi
  if [ "$key_created" = 1 ]; then
    aws_ ec2 delete-key-pair --key-name "$key_name" >/dev/null 2>&1 || true
  fi
  rm -f "$key_path" "$tarball" "$remote_runner" 2>/dev/null || true
  rm -rf "$source_dir" 2>/dev/null || true
}
trap cleanup EXIT

# Freeze the exact commit that passed the clean-source gate before any cloud
# action. The same Git archive supplies both remote execution and local
# supplemental composition.
git -C "$repo_root" --no-replace-objects archive --format=tar "$source_revision" | gzip -n > "$tarball"
tar -xzf "$tarball" -C "$source_dir"

orphans="$(aws_ ec2 describe-instances \
  --filters "Name=tag:Name,Values=tach-bench-*" \
    "Name=instance-state-name,Values=running,pending" \
  --query 'Reservations[].Instances[].InstanceId' --output text)"
if [ -n "$orphans" ]; then
  echo "ABORT: orphan tach-bench-* instances still alive: $orphans" >&2
  exit 1
fi

# 782442783595 publishes the FreeBSD Project's EC2 images. Pin the release
# image name so a community image can never enter the benchmark provenance.
ami_id="$(aws_ ec2 describe-images \
  --owners 782442783595 \
  --filters 'Name=architecture,Values=x86_64' 'Name=state,Values=available' \
    'Name=name,Values=FreeBSD 15.0-RELEASE-amd64 cloud-init UFS' \
  --query 'Images[0].ImageId' --output text)"
if [ -z "$ami_id" ] || [ "$ami_id" = None ]; then
  echo "official FreeBSD 15.0 cloud-init AMI was not found" >&2
  exit 1
fi

echo "creating ephemeral keypair $key_name"
aws_ ec2 create-key-pair --key-name "$key_name" --query KeyMaterial --output text > "$key_path"
key_created=1
chmod 600 "$key_path"

echo "launching $instance_type (FreeBSD 15.0, ami $ami_id)"
instance_id="$(aws_ ec2 run-instances \
  --image-id "$ami_id" \
  --instance-type "$instance_type" \
  --key-name "$key_name" \
  --security-group-ids "$security_group" \
  --instance-initiated-shutdown-behavior terminate \
  --user-data $'#!/bin/sh\nshutdown -p +30\n' \
  --tag-specifications \
    'ResourceType=instance,Tags=[{Key=Name,Value=tach-bench-speed-freebsd}]' \
  --query 'Instances[0].InstanceId' --output text)"

aws_ ec2 wait instance-running --instance-ids "$instance_id"
ip="$(aws_ ec2 describe-instances --instance-ids "$instance_id" \
  --query 'Reservations[0].Instances[0].PublicIpAddress' --output text)"
echo "instance $instance_id at $ip"

ssh_options=(-o StrictHostKeyChecking=no -o ConnectTimeout=10 -i "$key_path")
for _ in $(seq 1 60); do
  if ssh "${ssh_options[@]}" "ec2-user@$ip" true 2>/dev/null; then
    break
  fi
  sleep 5
done
ssh "${ssh_options[@]}" "ec2-user@$ip" true

scp "${ssh_options[@]}" "$tarball" "ec2-user@$ip:/tmp/tach-src.tgz"

cat > "$remote_runner" <<'REMOTE_EOF'
#!/bin/sh
set -eu
SOURCE_REVISION="$1"
RUNNER="$2"
sudo env ASSUME_ALWAYS_YES=yes pkg bootstrap -f
sudo pkg install -y curl python311
rm -rf "$HOME/tach"
mkdir -p "$HOME/tach"
tar -xzf /tmp/tach-src.tgz -C "$HOME/tach"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
  sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
cd "$HOME/tach"
target_dir="$HOME/tach/.tach-speed-target"
if [ -e "$target_dir" ]; then
  echo "fresh benchmark target already exists: $target_dir" >&2
  exit 1
fi
cargo test --locked --release --tests --features bench-internal
export CARGO_TARGET_DIR="$target_dir"
export TACH_BENCH_EVIDENCE=1
export TACH_BENCH_SOURCE_REVISION="$SOURCE_REVISION"
export TACH_BENCH_RUNNER="$RUNNER"
python3.11 benches/seal-speed-source.py "$target_dir/criterion" -- \
  cargo bench --locked --bench instant --features bench-internal -- \
    --warm-up-time 1 --measurement-time 3
python3.11 benches/collect-speed-bundle.py "$target_dir/criterion" "$HOME/tach/collector.bundle"
REMOTE_EOF
chmod +x "$remote_runner"
scp "${ssh_options[@]}" "$remote_runner" "ec2-user@$ip:/tmp/run-tach-speed.sh"
ssh "${ssh_options[@]}" "ec2-user@$ip" \
  "sh /tmp/run-tach-speed.sh '$source_revision' 'aws-freebsd'"

scp -r "${ssh_options[@]}" "ec2-user@$ip:tach/collector.bundle" "$bundle_dir"
python3 "$source_dir/benches/compose-supplemental-speed.py" \
  --artifact speed-supplemental-freebsd-x86_64.json \
  --output "$composed_output" \
  --source-revision "$source_revision" \
  --collector-bundle "$bundle_dir" \
  --instant-profile runtime_tournament \
  --ordered-profile runtime_tournament \
  --thread-cpu-profile fixed_native
echo "wrote $composed_output with retained collector bundle $bundle_dir"

aws_ ec2 terminate-instances --instance-ids "$instance_id" >/dev/null
aws_ ec2 wait instance-terminated --instance-ids "$instance_id"
instance_id=""
