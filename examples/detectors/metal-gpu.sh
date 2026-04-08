#!/usr/bin/env bash
# Metal GPU pressure detector for proc-siding (macOS).
#
# Uses metalps (https://github.com/LoganBarnett/metalps) to query
# per-process GPU utilization on Apple Silicon or AMD GPUs under macOS.
# Outputs TSV lines of <value>\t<entity> suitable for proc-siding's
# detector_cmd.
#
# Unlike the Linux scripts, metalps reports per-process utilization
# directly, so no /proc scanning or baseline PID logic is needed.
#
# Usage:
#   detector_cmd = "examples/detectors/metal-gpu.sh --exclude-pattern ollama"
#
# Options:
#   --exclude-pattern <pat>   Exclude processes matching this pattern
#                             (passed to pgrep -f).  Also excludes their
#                             child process trees.
#   --interval-ms <ms>        Sampling interval for metalps.
#                             Default: 1000
#
# Requirements:
#   - macOS with Metal-capable GPU
#   - metalps on PATH (https://github.com/LoganBarnett/metalps)
#   - jq on PATH

set -euo pipefail

EXCLUDE_PATTERN=""
INTERVAL_MS=1000

while [[ $# -gt 0 ]]; do
  case "$1" in
    --exclude-pattern)
      EXCLUDE_PATTERN="$2"
      shift 2
      ;;
    --interval-ms)
      INTERVAL_MS="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
done

# Collect PIDs to exclude: the pattern match and all their descendants.
excluded_pids() {
  if [[ -z "$EXCLUDE_PATTERN" ]]; then
    return
  fi

  # Collect root PIDs matching the pattern.
  local roots
  roots="$(pgrep -f "$EXCLUDE_PATTERN" 2>/dev/null || true)"
  if [[ -z "$roots" ]]; then
    return
  fi

  # Walk children recursively.
  local all="$roots"
  local frontier="$roots"
  while [[ -n "$frontier" ]]; do
    local next=""
    while IFS= read -r pid; do
      local children
      children="$(pgrep -P "$pid" 2>/dev/null || true)"
      if [[ -n "$children" ]]; then
        next="${next}${next:+$'\n'}${children}"
      fi
    done <<< "$frontier"
    frontier="$next"
    if [[ -n "$next" ]]; then
      all="${all}${all:+$'\n'}${next}"
    fi
  done

  echo "$all"
}

# ── Main ─────────────────────────────────────────────────────────────────────

if ! command -v metalps >/dev/null 2>&1; then
  echo "metalps not found; install from https://github.com/LoganBarnett/metalps" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq not found; install jq" >&2
  exit 1
fi

# Build a jq set of excluded PIDs for efficient lookup.
declare -A exclude_set
while IFS= read -r pid; do
  [[ -n "$pid" ]] && exclude_set["$pid"]=1
done < <(excluded_pids)

# Run metalps and parse the JSON output.
# metalps --json outputs an array of objects with at least:
#   { "pid": 1234, "command": "foo", "gpu_percent": 12.5 }
json="$(metalps --json --interval-ms "$INTERVAL_MS" 2>/dev/null)"

# Filter excluded PIDs, drop zero-utilization processes, and emit TSV.
echo "$json" | jq -r '.[] | select(.gpu_percent > 0) | "\(.pid)\t\(.gpu_percent)\t\(.command)"' \
  | while IFS=$'\t' read -r pid gpu_pct command; do
      if [[ -z "${exclude_set[$pid]+x}" ]]; then
        printf '%s\t%s\n' "$gpu_pct" "$command"
      fi
    done
