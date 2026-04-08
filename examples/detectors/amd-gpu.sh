#!/usr/bin/env bash
# AMD GPU pressure detector for proc-siding.
#
# Reads gpu_busy_percent from sysfs and scans /proc/<pid>/fd for /dev/dri/
# device files to identify contributing processes.  Outputs TSV lines of
# <value>\t<entity> suitable for proc-siding's detector_cmd.
#
# Baseline PID filtering: on the first run, captures PIDs that already have
# DRI fds open (compositor, desktop infrastructure) into a state file and
# excludes them on subsequent runs.  This prevents persistent desktop
# processes from appearing as pressure contributors.
#
# Usage:
#   detector_cmd = "examples/detectors/amd-gpu.sh --exclude-unit ollama.service"
#
# Options:
#   --exclude-unit <unit>   Exclude PIDs belonging to a systemd unit's cgroup.
#   --state-dir <dir>       Directory for baseline PID state file.
#                           Default: /run/proc-siding
#
# Requirements:
#   - Linux with AMD GPU (amdgpu driver)
#   - Root or CAP_SYS_PTRACE (for /proc/<pid>/fd scanning)
#   - /sys/class/drm/card*/device/gpu_busy_percent must exist

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

BASELINE_FILE="${STATE_DIR}/amd-baseline-pids"

# Collect excluded PIDs from the systemd unit's cgroup.
excluded_pids() {
  if [[ -n "$EXCLUDE_UNIT" ]]; then
    local cgroup="/sys/fs/cgroup/system.slice/${EXCLUDE_UNIT}/cgroup.procs"
    if [[ -r "$cgroup" ]]; then
      cat "$cgroup"
    fi
  fi
}

# Find all PIDs with /dev/dri/ device fds open.  Returns "pid comm" lines.
all_dri_users() {
  for pid_dir in /proc/[0-9]*/fd; do
    local pid
    pid="$(echo "$pid_dir" | grep -oP '/proc/\K[0-9]+')"
    # Check if any fd symlink points to /dev/dri/.
    if find "$pid_dir" -maxdepth 1 -type l -exec readlink {} + 2>/dev/null \
       | grep -q '/dev/dri/'; then
      local comm
      comm="$(cat "/proc/${pid}/comm" 2>/dev/null || echo "pid:${pid}")"
      echo "${pid} ${comm}"
    fi
  done
}

# Read gpu_busy_percent from the first AMD card in sysfs.
read_busy_percent() {
  for card in /sys/class/drm/card*; do
    # Skip renderD* nodes.
    [[ "$(basename "$card")" == card* ]] || continue
    local busy_path="${card}/device/gpu_busy_percent"
    if [[ -r "$busy_path" ]]; then
      cat "$busy_path"
      return
    fi
  done
  echo "0"
}

# ── Main ─────────────────────────────────────────────────────────────────────

mkdir -p "$STATE_DIR"

# Capture all current DRI users.
mapfile -t dri_users < <(all_dri_users)

# On first run, capture baseline PIDs (compositor, desktop infrastructure).
if [[ ! -f "$BASELINE_FILE" ]]; then
  printf '%s\n' "${dri_users[@]}" | awk '{print $1}' > "$BASELINE_FILE"
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
for entry in "${dri_users[@]}"; do
  pid="${entry%% *}"
  name="${entry#* }"
  if [[ -z "${exclude[$pid]+x}" ]]; then
    contributors+=("$name")
  fi
done

# If no external contributors have the device open, report zero pressure
# regardless of sysfs utilization (the owned workload may be using the GPU).
if [[ ${#contributors[@]} -eq 0 ]]; then
  exit 0
fi

busy=$(read_busy_percent)

# Output one TSV line per contributor, each carrying the full busy percent.
# proc-siding sums these, so we divide evenly (attribution is approximate;
# Linux does not report per-process GPU utilization for AMD).
count=${#contributors[@]}
per_contributor=$(echo "scale=2; $busy / $count" | bc)

for name in "${contributors[@]}"; do
  printf '%s\t%s\n' "$per_contributor" "$name"
done
