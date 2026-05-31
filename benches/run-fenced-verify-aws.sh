#!/usr/bin/env bash
# Provision an EC2 instance, run the fenced-verify study, pull JSON back, terminate.
#
# Finds the boundary where FencedInstant goes backward cross-thread: runs the
# synchronization-order test under pinned placements (adversarial cross-socket
# pair + full-span + oversubscribed-2x) across tach / tach_fenced / tach_synced /
# std. Bare `tach` is the positive control — it MUST show violations under a
# placement, or that placement was inert (result inconclusive, not a pass).
#
# Aimed at multi-socket / NUMA metal (m7i.metal-48xl, m7a.metal-48xl, c6a.metal)
# — the topologies single-socket cells can't exercise and where the TSC could
# genuinely lag across sockets.
#
# Usage: benches/run-fenced-verify-aws.sh <cell-name> <instance-type> [duration-secs]
#   e.g. benches/run-fenced-verify-aws.sh intel-2s-m7i m7i.metal-48xl 300
#
# Requires: aws CLI profile "tach", SSH key ~/.ssh/tach-bench.pem, SG with SSH.
# Metal instances are ~$8-11/hr. Self-terminates on exit (trap) and on shutdown.
set -euo pipefail

CELL="${1:?usage: run-fenced-verify-aws.sh <cell-name> <instance-type> [duration-secs]}"
INSTANCE_TYPE="${2:?need instance type (e.g. m7i.metal-48xl)}"
DURATION="${3:-300}"

REGION="us-east-2"
PROFILE="tach"
KEY_NAME="tach-bench"
KEY_PATH="$HOME/.ssh/tach-bench.pem"
SG_ID="sg-05e99abafa54936d3"

case "$INSTANCE_TYPE" in
  c7g*|c8g*|t4g*|c6g*|m7g*|m6g*|r7g*|r8g*) ARCH=arm64 ;;
  *) ARCH=x86_64 ;;
esac
AMI_SSM="/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-6.12-${ARCH}"

aws_() { aws "$@" --region "$REGION" --profile "$PROFILE"; }

AMI_ID=$(aws_ ssm get-parameters --names "$AMI_SSM" --query 'Parameters[0].Value' --output text)
echo "launching $INSTANCE_TYPE ($ARCH, ami $AMI_ID) — metal is ~\$8-11/hr, self-terminates on exit"
IID=$(aws_ ec2 run-instances \
  --image-id "$AMI_ID" \
  --instance-type "$INSTANCE_TYPE" \
  --key-name "$KEY_NAME" \
  --security-group-ids "$SG_ID" \
  --instance-initiated-shutdown-behavior terminate \
  --query 'Instances[0].InstanceId' --output text)
echo "instance $IID"

cleanup() {
  if [ -n "${IID:-}" ]; then
    echo "terminating $IID"
    aws_ ec2 terminate-instances --instance-ids "$IID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

aws_ ec2 wait instance-running --instance-ids "$IID"
IP=$(aws_ ec2 describe-instances --instance-ids "$IID" \
  --query 'Reservations[0].Instances[0].PublicIpAddress' --output text)
echo "ip $IP"

SSH="ssh -o StrictHostKeyChecking=no -o ConnectTimeout=10 -i $KEY_PATH ec2-user@$IP"
SCP="scp -o StrictHostKeyChecking=no -i $KEY_PATH"
for _ in $(seq 1 40); do $SSH true 2>/dev/null && break || sleep 5; done

# Ship source (exclude heavy/irrelevant paths), install toolchain + gcc.
TARBALL=/tmp/tach-fv-src.tgz
tar --exclude=target --exclude=.git --exclude='benches/*.png' --exclude='benches/*.svg' \
  -czf "$TARBALL" .
$SCP "$TARBALL" "ec2-user@$IP:/tmp/src.tgz"
$SSH 'mkdir -p tach && tar -xzf /tmp/src.tgz -C tach'
$SSH 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal'
$SSH 'sudo dnf install -y gcc >/dev/null 2>&1'

echo "=== remote topology ==="
$SSH 'lscpu | grep -E "^(Architecture|Model name|Socket|Core|NUMA node\(s\)|CPU\(s\)):"'

# Adversarial cross-domain pin pair: first CPU of the first socket + first CPU of
# the last socket. Falls back to first/last NUMA node, then "0,1" (single-socket;
# the pair is then inert by design and only full-span/oversubscribed are decisive).
PIN=$($SSH "lscpu -p=CPU,SOCKET,NODE | awk -F, '
  !/^#/ {
    if (!(\$2 in sk)) { sk[\$2]=\$1; sord[++ns]=\$2 }
    if (!(\$3 in nd)) { nd[\$3]=\$1; nord[++nn]=\$3 }
  }
  END {
    if (ns>=2) print sk[sord[1]] \",\" sk[sord[ns]];
    else if (nn>=2) print nd[nord[1]] \",\" nd[nord[nn]];
    else print \"0,1\";
  }'")
echo "adversarial pin pair: $PIN"

OUT="benches/fenced-verify-${CELL}.json"
$SSH "cd tach && source \$HOME/.cargo/env && cargo build --release --bench skew --features bench-internal 2>&1 | tail -2"
$SSH "cd tach && source \$HOME/.cargo/env && BIN=\$(find target/release/deps -name 'skew-*' -type f -perm -u+x | head -1) && \"\$BIN\" --mode fenced-verify --cell '$CELL' --pin '$PIN' --duration '$DURATION' --output '$OUT'"
$SCP "ec2-user@$IP:tach/${OUT}" "${OUT}"
echo "pulled ${OUT}"

# Verdict per placement.
python3 - "${OUT}" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print(f"\n=== {d['cell']} ({d['target_triple']}) — {d['host']['cpu_model']} — {d['duration_secs_per_run']}s/run ===")
for p in d["placements"]:
    r = p["results"]
    def v(k): return r[k]["total_violations"]
    if v("tach") == 0:
        verdict = "INCONCLUSIVE (control inert — placement didn't exercise cross-domain reads)"
    elif v("tach_fenced") == 0:
        verdict = "Fenced SUFFICIENT (control fired, fenced held at 0)"
    else:
        verdict = f"BOUNDARY: Fenced went backward (max {r['tach_fenced']['max_violation_ns']}ns) -> Synced REQUIRED"
    sync = "synced=0" if v("tach_synced") == 0 else f"synced={v('tach_synced')}!"
    print(f"  {p['placement']:<18} cores~{p['pinned_cores'][:2]} "
          f"tach={v('tach')} fenced={v('tach_fenced')} {sync} std={v('std')}  -> {verdict}")
PY
