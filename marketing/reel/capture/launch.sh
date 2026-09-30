#!/bin/bash
# Launches the app on the capture display with a throwaway home, for
# lining up a scenario by hand:  launch.sh <state-dir> <vault-or-note>
# It runs in the background and logs to <state-dir>/app.log.
set -euo pipefail
state="$1"
target="$2"
mkdir -p "$state/home"
export DISPLAY=":${DISPLAY_NUM:-99}"
unset WAYLAND_DISPLAY
export HOME="$state/home"
export XDG_CONFIG_HOME="$HOME/.config" XDG_DATA_HOME="$HOME/.local/share"
export XDG_STATE_HOME="$HOME/.local/state" XDG_CACHE_HOME="$HOME/.cache"
mkdir -p /tmp/gasp-rt && chmod 700 /tmp/gasp-rt
export XDG_RUNTIME_DIR=/tmp/gasp-rt
export GPUI_X11_SCALE_FACTOR="${SCALE:-2}"
export EDITOR_TRACE_STARTUP="${EDITOR_TRACE_STARTUP:-all}"
nohup "${GASP:-/tmp/target-app/release/gasp}" "$target" > "$state/app.log" 2>&1 &
echo $! > "$state/app.pid"
