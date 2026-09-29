#!/bin/bash
# Performance budgets: launches, types, scrolls and opens notes, and fails
# when a measure is over its budget (PLAN.md, Verification).
#
#   scripts/perf-budgets.sh BINARY VAULT
#
# On Linux run it under a display (on CI: xvfb-run with openbox). On macOS
# the benches draw into windows that are never shown, so it runs with the
# screen locked. The app's own folders and the copy of the vault go in a
# temporary HOME.
set -euo pipefail
binary=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")

# Budgets, in milliseconds and megabytes.
LAUNCH_MS=300          # main to the first frame, median of 5 warm launches
KEY_P50_MS=8           # keystroke to painted, typical: one 120 Hz frame
KEY_P95_MS=16          # keystroke to painted, slow ones: one 60 Hz frame
SCROLL_P95_MS=16       # a page scrolled: layout plus paint
OPEN_P95_MS=50         # opening a note, or switching tabs, to its frame
OPEN_MEMORY_MB=300     # after opening the 30 longest notes, 9 tabs open

home=$(mktemp -d)
trap 'rm -rf "$home"' EXIT
# The app saves its window and tabs in the vault it opens, so it opens a
# copy.
vault=$home/vault
cp -R "$2" "$vault"
export HOME=$home XDG_CONFIG_HOME=$home/.config XDG_DATA_HOME=$home/.local/share \
    XDG_CACHE_HOME=$home/.cache
hidden=()
if [ "$(uname)" = Darwin ]; then
    hidden=(--hidden)
fi

status=0
# check NAME VALUE LIMIT UNIT
check() {
    if awk -v value="$2" -v limit="$3" 'BEGIN { exit !(value > limit) }'; then
        echo "over budget: $1 is $2 $4 (budget $3 $4)" >&2
        status=1
    else
        echo "ok: $1 is $2 $4 (budget $3 $4)"
    fi
}
# field REPORT LINE_PREFIX KEY: the number after KEY= on the report's line.
field() {
    grep -m1 "^$2" <<<"$1" | sed -E "s/.*$3=([0-9.]+)ms.*/\1/"
}

launches=()
for run in 0 1 2 3 4 5; do
    first=$(EDITOR_TRACE_STARTUP=quit "$binary" "$vault" 2>&1 >/dev/null |
        awk '/first-frame/ { print $4 }')
    # The first launch warms the disk cache and the font cache.
    if [ "$run" -gt 0 ]; then
        launches+=("$first")
    fi
done
launch=$(printf '%s\n' "${launches[@]}" | sort -n | sed -n 3p)
check "launch to first frame" "$launch" "$LAUNCH_MS" ms

for mode in "" --in-math --in-table --in-code; do
    report=$("$binary" --bench-layout "$vault" ${mode:+"$mode"} "${hidden[@]}")
    typing=$(sed -n '/-- typing/,/-- scrolling/p' <<<"$report")
    scrolling=$(sed -n '/-- scrolling/,$p' <<<"$report")
    name="typing${mode:+ $mode}"
    check "$name p50" "$(field "$typing" input-to-paint p50)" "$KEY_P50_MS" ms
    check "$name p95" "$(field "$typing" input-to-paint p95)" "$KEY_P95_MS" ms
    scroll=$(awk -v l="$(field "$scrolling" layout p95)" -v p="$(field "$scrolling" paint p95)" \
        'BEGIN { print l + p }')
    check "scrolling${mode:+ $mode} p95" "$scroll" "$SCROLL_P95_MS" ms
done

report=$("$binary" --bench-open "$vault" "${hidden[@]}")
check "open p95" "$(field "$report" "open to first frame" p95)" "$OPEN_P95_MS" ms
check "tab switch p95" "$(field "$report" "tab switch" p95)" "$OPEN_P95_MS" ms
memory=$(grep -m1 '^memory:' <<<"$report" | awk '{ print $2 }')
check "memory after opening" "$memory" "$OPEN_MEMORY_MB" MB

exit $status
