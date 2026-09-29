#!/bin/bash
# Idle check: opens a vault, lets it settle, then watches it untouched.
# Fails if it draws any frame, uses more CPU than the limit while idle, or
# holds more memory than the budget with the vault open.
#
#   scripts/idle-cpu.sh BINARY VAULT [SECONDS] [LIMIT_PERCENT] [MEMORY_MB]
#
# Run it under a display (on CI: xvfb-run with openbox). GPUI 0.2.2 wakes
# once per display refresh while a window is visible, to ask whether it
# needs drawing; that's what the CPU left at idle is, and why the limit
# isn't zero (see PLAN.md, Verification). Memory is the app's anonymous
# resident memory, as its benches report it; the grammar checker's own
# process isn't counted (it closes after a minute without work). Linux
# only: it reads /proc.
set -euo pipefail
binary=$1
vault=$2
seconds=${3:-10}
limit=${4:-1.5}
memory_limit=${5:-100}

home=$(mktemp -d)
log=$(mktemp)
trap 'kill "$pid" 2>/dev/null || true; rm -rf "$home" "$log"' EXIT

HOME=$home XDG_CONFIG_HOME=$home/.config XDG_DATA_HOME=$home/.local/share \
    XDG_CACHE_HOME=$home/.cache EDITOR_TRACE_STARTUP=all \
    "$binary" "$vault" >"$log" 2>&1 &
pid=$!
# Startup, the index and the background loads finish well within this.
sleep 12

ticks() { awk '{print $14 + $15}' "/proc/$pid/stat"; }
frames() { grep -c "workspace-render" "$log" || true; }
cpu_before=$(ticks)
frames_before=$(frames)
sleep "$seconds"
cpu_after=$(ticks)
frames_after=$(frames)
memory=$(awk '/^RssAnon:/ { printf "%.1f", $2 / 1024 }' "/proc/$pid/status")

hz=$(getconf CLK_TCK)
percent=$(awk -v t=$((cpu_after - cpu_before)) -v hz="$hz" -v s="$seconds" \
    'BEGIN { printf "%.2f", t * 100 / hz / s }')
drawn=$((frames_after - frames_before))
echo "idle for ${seconds}s: ${percent}% CPU, ${drawn} frames drawn, ${memory} MB"

status=0
if [ "$drawn" -ne 0 ]; then
    echo "an idle window drew frames" >&2
    status=1
fi
if awk -v p="$percent" -v l="$limit" 'BEGIN { exit !(p > l) }'; then
    echo "idle CPU is over ${limit}%" >&2
    status=1
fi
if awk -v m="$memory" -v l="$memory_limit" 'BEGIN { exit !(m > l) }'; then
    echo "memory with the vault open is over ${memory_limit} MB" >&2
    status=1
fi
exit $status
