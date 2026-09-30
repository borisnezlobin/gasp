#!/bin/bash
# Renders both cuts from recorded takes. See README.md for the whole flow.
#
#   marketing/reel/render.sh <work-dir> <out-dir> [hero|vertical ...]
#
# <work-dir> must already hold frames/ (capture/unpack.py), measure.json
# and bench-corpus.log (capture/measure.py). Writes gasp-reel-hero.mp4 and
# gasp-reel-vertical.mp4 to <out-dir>.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
work="$(cd "$1" && pwd)"
out="$2"
shift 2
formats=("$@")
[ ${#formats[@]} -eq 0 ] && formats=(hero vertical)
build="$work/build"
mkdir -p "$build" "$out"

TAKES=(cold_start_10 typing_2 math_2 table_2 search_4 accent_1 keymap_1 snippet_1 sync_2 agent_2 tagline_2)

cp "$here/compose/stage.html" "$here/compose/reel.js" "$build/"
cp "$repo/apps/desktop/assets/icon/whale-breach.png" "$build/"
[ -f "$build/fonts/BricolageGrotesque.ttf" ] || "$here/compose/fetch_fonts.sh" "$build/fonts"
python3 "$here/compose/prep.py" "$work/frames" "$work/measure.json" "$build/data.json" "${TAKES[@]}"
python3 "$here/audio/synth.py" peaks "$build/peaks.json"

for format in "${formats[@]}"; do
  node "$here/compose/render.js" "$build" "$format" cues "$build/cues-$format.json"
  python3 "$here/audio/synth.py" "$build/cues-$format.json" "$build/audio-$format.wav"
  # SKIP_PICTURE=1 reuses the last picture, to remix the sound only.
  [ -n "${SKIP_PICTURE:-}" ] || node "$here/compose/render.js" "$build" "$format" video "$build/picture-$format.mkv" "${WORKERS:-3}"
  # Loudness for the web: two-pass loudnorm to -14 LUFS, peaks under -1 dBTP after AAC encoding.
  stats=$(ffmpeg -hide_banner -i "$build/audio-$format.wav" \
    -af loudnorm=I=-14:TP=-2:LRA=9:print_format=json -f null - 2>&1 | sed -n '/^{/,/^}/p')
  mi=$(echo "$stats" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["input_i"], d["input_tp"], d["input_lra"], d["input_thresh"], d["target_offset"])')
  read -r ii itp ilra ith off <<< "$mi"
  ffmpeg -loglevel error -y -i "$build/picture-$format.mkv" -i "$build/audio-$format.wav" \
    -af "loudnorm=I=-14:TP=-2:LRA=9:measured_I=$ii:measured_TP=$itp:measured_LRA=$ilra:measured_thresh=$ith:offset=$off:linear=true,aresample=48000" \
    -c:v libx264 -preset slow -crf 16 -profile:v high -pix_fmt yuv420p -r 60 -movflags +faststart \
    -c:a aac -b:a 256k -ar 48000 -shortest "$out/gasp-reel-$format.mp4"
  echo "wrote $out/gasp-reel-$format.mp4"
done
