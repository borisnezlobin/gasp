#!/bin/bash
# Records and unpacks takes:  record_all.sh <vault> <work-dir> <scenario>[:take-name] ...
# Needs GRAB (the built grab.c) and GASP (the release binary) in the
# environment, or their defaults.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
vault="$1"; work="$2"; shift 2
for spec in "$@"; do
  scenario="${spec%%:*}"
  name="${spec#*:}"
  python3 "$here/take.py" "$scenario" "$vault" "$work/takes" --name "$name"
  tail -1 "$work/takes/$name.grab.log"
  python3 "$here/unpack.py" "$work/takes" "$work/frames" "$name"
done
