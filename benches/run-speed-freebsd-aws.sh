#!/usr/bin/env bash
# Run the complete speed campaign on an official FreeBSD/amd64 EC2 image and
# return one supplemental, contract-validated evidence cell.
set -euo pipefail

instance_type="${1:-c7i.large}"
region=us-east-2
profile=tach
key_name="tach-speed-freebsd-$$-$(date +%s 2>/dev/null || echo 0)"
key_path="$(mktemp -t tach-speed-freebsd-key.XXXXXX).pem"
rustc_path="$(mktemp -t tach-speed-freebsd-rustc.XXXXXX)"
security_group=sg-05e99abafa54936d3
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
source_revision="$(bash "$repo_root/benches/require-clean-benchmark-source.sh")"
instance_id=""

aws_() { aws "$@" --region "$region" --profile "$profile"; }

cleanup() {
  if [ -n "${instance_id:-}" ]; then
    echo "terminating $instance_id"
    if aws_ ec2 terminate-instances --instance-ids "$instance_id" >/dev/null 2>&1; then
      aws_ ec2 wait instance-terminated --instance-ids "$instance_id" >/dev/null 2>&1 || true
    fi
  fi
  aws_ ec2 delete-key-pair --key-name "$key_name" >/dev/null 2>&1 || true
  rm -f "$key_path" "$rustc_path" 2>/dev/null || true
}
trap cleanup EXIT

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

tarball="$(mktemp -t tach-speed-freebsd-src.XXXXXX).tgz"
remote_runner="$(mktemp -t tach-speed-freebsd-runner.XXXXXX)"
trap 'rm -f "$tarball" "$remote_runner" 2>/dev/null || true; cleanup' EXIT
tar --exclude=target --exclude=.git --exclude='benches/*.png' \
  --exclude='benches/*.svg' -czf "$tarball" -C "$repo_root" .
scp "${ssh_options[@]}" "$tarball" "ec2-user@$ip:/tmp/tach-src.tgz"

cat > "$remote_runner" <<'REMOTE_EOF'
#!/bin/sh
set -eu
sudo env ASSUME_ALWAYS_YES=yes pkg bootstrap -f
sudo pkg install -y curl python311
rm -rf "$HOME/tach"
mkdir -p "$HOME/tach"
tar -xzf /tmp/tach-src.tgz -C "$HOME/tach"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
  sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
cd "$HOME/tach"
cargo test --locked --release --tests --features bench-internal
cargo bench --locked --bench instant --features bench-internal -- \
  --warm-up-time 1 --measurement-time 3
python3.11 benches/extract_speed.py target/criterion > clocks-out.json
rustc --version > rustc-version.txt
uname -a > kernel.txt
sysctl -a | grep -E 'kern.timecounter|machdep.tsc|hw.model' > machine.txt
REMOTE_EOF
chmod +x "$remote_runner"
scp "${ssh_options[@]}" "$remote_runner" "ec2-user@$ip:/tmp/run-tach-speed.sh"
ssh "${ssh_options[@]}" "ec2-user@$ip" sh /tmp/run-tach-speed.sh

raw_output=/tmp/speed-clocks-freebsd.json
composed_output=/tmp/speed-freebsd.json
scp "${ssh_options[@]}" "ec2-user@$ip:tach/clocks-out.json" "$raw_output"
scp "${ssh_options[@]}" "ec2-user@$ip:tach/rustc-version.txt" "$rustc_path"
scp "${ssh_options[@]}" "ec2-user@$ip:tach/kernel.txt" /tmp/speed-freebsd-kernel.txt
scp "${ssh_options[@]}" "ec2-user@$ip:tach/machine.txt" /tmp/speed-freebsd-machine.txt

python3 "$repo_root/benches/compose-speed.py" "$raw_output" "$composed_output" \
  --title 'AWS FreeBSD' \
  --instance "$instance_type + FreeBSD 15.0" \
  --triple x86_64-unknown-freebsd \
  --order 7 \
  --source-revision "$source_revision" \
  --rustc-version "$(cat "$rustc_path")" \
  --harness criterion \
  --cargo-profile bench

echo "validated supplemental cell -> $composed_output"
aws_ ec2 terminate-instances --instance-ids "$instance_id" >/dev/null
aws_ ec2 wait instance-terminated --instance-ids "$instance_id"
instance_id=""
