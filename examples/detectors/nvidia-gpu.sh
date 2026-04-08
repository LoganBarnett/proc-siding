#!/usr/bin/env bash
# NVIDIA GPU pressure detector for proc-siding.
#
# Queries nvidia-smi for GPU utilization and scans /proc/<pid>/fd for
# /dev/nvidia* device files to identify contributing processes.  Outputs
# TSV lines of <value>\t<entity> suitable for proc-siding's detector_cmd.
#
# Baseline PID filtering: on the first run, captures PIDs that already have
# NVIDIA device fds open (compositor, desktop infrastructure) into a state
# file and excludes them on subsequent runs.
#
# Usage:
#   detector_cmd = "examples/detectors/nvidia-gpu.sh --exclude-unit ollama.service"
#
# Options:
#   --exclude-unit <unit>   Exclude PIDs belonging to a systemd unit's cgroup.
#   --state-dir <dir>       Directory for baseline PID state file.
#                           Default: /run/proc-siding
#
# Requirements:
#   - Linux with NVIDIA GPU (nvidia driver)
#   - nvidia-smi on PATH
#   - Root or CAP_SYS_PTRACE (for /proc/<pid>/fd scanning)

set -euo pipefail

STATE_DIR="/run/proc-siding"
EXCLUDE_UNIT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --exclude-unit)
      EXCLUDE_UNIT="$2"
      shift 2
      ;;
    --state-dir)
      STATE_DIR="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
done

BASELINE_FILE="${STATE_DIR}/nvidia-baseline-pids"

# Collect excluded PIDs from the systemd unit's cgroup.
excluded_pids() {
  if [[ -n "$EXCLUDE_UNIT" ]]; then
    local cgroup="/sys/fs/cgroup/system.slice/${EXCLUDE_UNIT}/cgroup.procs"
    if [[ -r "$cgroup" ]]; then
      cat "$cgroup"
    fi
  fi
}

# Find all PIDs with /dev/nvidia* device fds open.  Returns "pid comm" lines.
all_nvidia_users() {
  for pid_dir in /proc/[0-9]*/fd; do
    local pid
    pid="$(echo "$pid_dir" | grep -oP '/proc/\K[0-9]+')"
    # Check if any fd symlink points to /dev/nvidia*.
    if find "$pid_dir" -maxdepth 1 -type l -exec readlink {} + 2>/dev/null \
       | grep -q '/dev/nvidia'; then
      local comm
      comm="$(cat "/proc/${pid}/comm" 2>/dev/null || echo "pid:${pid}")"
      echo "${pid} ${comm}"
    fi
  done
}

# Read GPU utilization via nvidia-smi.
read_gpu_utilization() {
  if command -v nvidia-smi >/dev/null 2>&1; then
    nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits \
      | head -1 \
      | tr -d ' '
  else
    echo "0"
  fi
}

# ── Main ─────────────────────────────────────────────────────────────────────

mkdir -p "$STATE_DIR"

# Capture all current NVIDIA device users.
mapfile -t nvidia_users < <(all_nvidia_users)

# On first run, capture baseline PIDs (compositor, desktop infrastructure).
if [[ ! -f "$BASELINE_FILE" ]]; then
  printf '%s\n' "${nvidia_users[@]}" | awk '{print $1}' > "$BASELINE_FILE"
fi

# Build exclusion set: baseline PIDs + systemd unit PIDs.
declare -A exclude
while IFS= read -r pid; do
  [[ -n "$pid" ]] && exclude["$pid"]=1
done < "$BASELINE_FILE"

while IFS= read -r pid; do
  [[ -n "$pid" ]] && exclude["$pid"]=1
done < <(excluded_pids)

# Filter to external contributors only.
contributors=()
for entry in "${nvidia_users[@]}"; do
  pid="${entry%% *}"
  name="${entry#* }"
  if [[ -z "${exclude[$pid]+x}" ]]; then
    contributors+=("$name")
  fi
done

# If no external contributors have the device open, report zero pressure
# regardless of nvidia-smi utilization (the owned workload may be using it).
if [[ ${#contributors[@]} -eq 0 ]]; then
  exit 0
fi

busy=$(read_gpu_utilization)

# Output one TSV line per contributor, dividing utilization evenly.
# Attribution is approximate; nvidia-smi does not report per-process
# utilization by default.
count=${#contributors[@]}
per_contributor=$(echo "scale=2; $busy / $count" | bc)

for name in "${contributors[@]}"; do
  printf '%s\t%s\n' "$per_contributor" "$name"
done
