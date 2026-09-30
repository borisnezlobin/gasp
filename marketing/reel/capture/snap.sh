#!/bin/bash
# Grabs one frame of the capture display: snap.sh out.png [scale]
# (for lining up scenario coordinates, not for the reel).
set -euo pipefail
out="$1"
scale="${2:-1}"
ffmpeg -loglevel error -y -f x11grab -video_size "${SCREEN:-2560x1600}" -i ":${DISPLAY_NUM:-99}.0" \
  -frames:v 1 -vf "scale=iw*${scale}:-1" "$out"
