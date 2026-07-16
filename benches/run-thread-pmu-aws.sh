#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
region="${AWS_REGION:-us-east-2}"
profile="${AWS_PROFILE:-tach}"
instance_type="${INSTANCE_TYPE:-c7g.large}"
security_group="${TACH_AWS_SECURITY_GROUP_ID:-sg-05e99abafa54936d3}"
output="${OUTPUT:-/tmp/tach-thread-pmu-${instance_type//./-}.txt}"
case "$instance_type" in
  c7g*|c8g*|t4g*|c6g*|m7g*|m6g*|r7g*|r8g*) probe_arch=arm64; probe_file=aarch64-thread-pmu.c ;;
  *) probe_arch=x86_64; probe_file=x86-thread-pmu.c ;;
esac
key_name="tach-thread-pmu-$$-$(date +%s)"
key_path="$(mktemp -t tach-thread-pmu-key.XXXXXX).pem"
instance_id=

aws_ec2() {
  aws ec2 "$@" --region "$region" --profile "$profile"
}

cleanup() {
  if [[ -n "$instance_id" ]]; then
    aws_ec2 terminate-instances --instance-ids "$instance_id" >/dev/null 2>&1 || true
    aws_ec2 wait instance-terminated --instance-ids "$instance_id" >/dev/null 2>&1 || true
  fi
  aws_ec2 delete-key-pair --key-name "$key_name" >/dev/null 2>&1 || true
  rm -f "$key_path"
}
trap cleanup EXIT

orphans="$(aws_ec2 describe-instances \
  --filters 'Name=tag:Name,Values=tach-thread-pmu-probe' \
            'Name=instance-state-name,Values=pending,running,stopping,stopped' \
  --query 'Reservations[].Instances[].InstanceId' --output text)"
if [[ -n "$orphans" ]]; then
  echo "refusing to launch with an existing probe instance: $orphans" >&2
  exit 1
fi

ami="$(aws_ec2 describe-images --owners amazon \
  --filters "Name=name,Values=al2023-ami-2023.*-kernel-6.12-${probe_arch}" \
            'Name=state,Values=available' \
  --query 'reverse(sort_by(Images,&CreationDate))[0].ImageId' --output text)"

aws_ec2 create-key-pair --key-name "$key_name" --query KeyMaterial --output text >"$key_path"
chmod 600 "$key_path"

instance_id="$(aws_ec2 run-instances \
  --image-id "$ami" \
  --instance-type "$instance_type" \
  --key-name "$key_name" \
  --security-group-ids "$security_group" \
  --instance-initiated-shutdown-behavior terminate \
  --user-data $'#!/bin/bash\nshutdown -h +20\n' \
  --tag-specifications \
    'ResourceType=instance,Tags=[{Key=Name,Value=tach-thread-pmu-probe}]' \
  --query 'Instances[0].InstanceId' --output text)"
echo "launched $instance_id ($instance_type, $ami)"

aws_ec2 wait instance-running --instance-ids "$instance_id"
ip="$(aws_ec2 describe-instances --instance-ids "$instance_id" \
  --query 'Reservations[0].Instances[0].PublicIpAddress' --output text)"

ssh_args=(-o StrictHostKeyChecking=no -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4 -i "$key_path")
for _ in $(seq 1 120); do
  if ssh "${ssh_args[@]}" "ec2-user@$ip" true 2>/dev/null; then
    break
  fi
  sleep 3
done

scp "${ssh_args[@]}" \
  "$repo_root/benches/probes/${probe_file}" \
  "ec2-user@$ip:/tmp/probe.c"
ssh "${ssh_args[@]}" "ec2-user@$ip" \
  'set -e
   sudo sysctl -w kernel.perf_event_paranoid=-1
   sudo sysctl -w kernel.perf_user_access=1 2>/dev/null || true
   sudo dnf install -y gcc >/dev/null
   gcc -O3 -std=gnu11 -Wall -Wextra -Werror /tmp/probe.c -o /tmp/probe
   uname -a
   grep -m1 -E "CPU implementer|CPU part|model name" /proc/cpuinfo || true
   /tmp/probe' | tee "$output"

aws_ec2 terminate-instances --instance-ids "$instance_id" >/dev/null
aws_ec2 wait instance-terminated --instance-ids "$instance_id"
echo "terminated $instance_id; evidence: $output"
instance_id=
