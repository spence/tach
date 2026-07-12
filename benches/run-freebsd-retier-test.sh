#!/usr/bin/env bash
# Run FreeBSD-local AT_TIMEKEEP retiering regressions and retain their output.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
region=${AWS_REGION:-us-east-2}
profile=${AWS_PROFILE:-tach}
instance_type=${INSTANCE_TYPE:-c7i.large}
security_group=${TACH_AWS_SECURITY_GROUP_ID:-sg-05e99abafa54936d3}
output=${OUTPUT:-/tmp/tach-freebsd-retier-$(date +%Y%m%d-%H%M%S).log}
key_name="tach-freebsd-retier-${RANDOM}-$(date +%s)"
key_path=$(mktemp -t tach-freebsd-retier-key.XXXXXX)
tarball=$(mktemp -t tach-freebsd-retier-src.XXXXXX.tgz)
instance_id=""

aws_ec2() {
  aws ec2 "$@" --region "$region" --profile "$profile"
}

cleanup() {
  if [[ -n "$instance_id" ]]; then
    aws_ec2 terminate-instances --instance-ids "$instance_id" >/dev/null 2>&1 || true
    aws_ec2 wait instance-terminated --instance-ids "$instance_id" >/dev/null 2>&1 || true
  fi
  aws_ec2 delete-key-pair --key-name "$key_name" >/dev/null 2>&1 || true
  rm -f "$key_path" "$tarball"
}
trap cleanup EXIT

orphans=$(aws_ec2 describe-instances \
  --filters 'Name=tag:Name,Values=tach-freebsd-retier-test' \
            'Name=instance-state-name,Values=pending,running,stopping,stopped' \
  --query 'Reservations[].Instances[].InstanceId' --output text)
if [[ -n "$orphans" && "$orphans" != "None" ]]; then
  echo "refusing to launch with existing retier test instance: $orphans" >&2
  exit 1
fi

ami=$(aws_ec2 describe-images --owners 782442783595 \
  --filters 'Name=architecture,Values=x86_64' \
            'Name=state,Values=available' \
            'Name=name,Values=FreeBSD 15.0-RELEASE-amd64 cloud-init UFS' \
  --query 'Images[0].ImageId' --output text)
if [[ -z "$ami" || "$ami" == "None" ]]; then
  echo 'official FreeBSD 15.0 cloud image was not found' >&2
  exit 1
fi

aws_ec2 create-key-pair --key-name "$key_name" --query KeyMaterial --output text >"$key_path"
chmod 600 "$key_path"
instance_id=$(aws_ec2 run-instances \
  --image-id "$ami" \
  --instance-type "$instance_type" \
  --key-name "$key_name" \
  --security-group-ids "$security_group" \
  --instance-initiated-shutdown-behavior terminate \
  --user-data $'#!/bin/sh\nshutdown -p +25\n' \
  --tag-specifications 'ResourceType=instance,Tags=[{Key=Name,Value=tach-freebsd-retier-test}]' \
  --query 'Instances[0].InstanceId' --output text)
echo "launched $instance_id ($instance_type)"

aws_ec2 wait instance-running --instance-ids "$instance_id"
ip=$(aws_ec2 describe-instances --instance-ids "$instance_id" \
  --query 'Reservations[0].Instances[0].PublicIpAddress' --output text)
ssh_args=(-o StrictHostKeyChecking=no -o ConnectTimeout=10 -i "$key_path")
for _ in $(seq 1 60); do
  if ssh "${ssh_args[@]}" "ec2-user@$ip" true 2>/dev/null; then
    break
  fi
  sleep 5
done
ssh "${ssh_args[@]}" "ec2-user@$ip" true

tar --exclude=target --exclude=.git --exclude='benches/__pycache__' \
  --exclude='benches/*.png' --exclude='benches/*.svg' \
  -czf "$tarball" -C "$repo_root" .
scp "${ssh_args[@]}" "$tarball" "ec2-user@$ip:/tmp/tach-src.tgz"

ssh "${ssh_args[@]}" "ec2-user@$ip" 'sh -s' <<'REMOTE' | tee "$output"
set -eu
sudo env ASSUME_ALWAYS_YES=yes pkg bootstrap -f
sudo pkg install -y curl
rm -rf "$HOME/tach-retier"
mkdir -p "$HOME/tach-retier"
tar -xzf /tmp/tach-src.tgz -C "$HOME/tach-retier"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
rustup toolchain install 1.95.0 --profile minimal
cd "$HOME/tach-retier"
cargo +1.95.0 test --locked --lib timekeep_retirement
cargo +1.95.0 test --locked --lib timekeep_retirement --no-default-features
printf '%s\n' 'FREEBSD_RETIER_NATIVE_PASS'
REMOTE

if ! rg -qx 'FREEBSD_RETIER_NATIVE_PASS' "$output"; then
  echo "FreeBSD retier test did not emit a pass marker; retained log: $output" >&2
  exit 1
fi
printf 'PASS: FreeBSD AT_TIMEKEEP retiering tests; log: %s\n' "$output"
