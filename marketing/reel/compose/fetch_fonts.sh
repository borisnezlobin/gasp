#!/bin/bash
# Downloads the reel's fonts (both SIL OFL, from the Google Fonts repository)
# into <dir>:  fetch_fonts.sh <dir>
#   Bricolage Grotesque, the display face, variable in opsz, wdth and wght.
#   JetBrains Mono, for the agent's terminal pane only.
set -euo pipefail
dir="$1"
mkdir -p "$dir"
base="https://raw.githubusercontent.com/google/fonts/main/ofl"
curl -sSfL -o "$dir/BricolageGrotesque.ttf" "$base/bricolagegrotesque/BricolageGrotesque%5Bopsz,wdth,wght%5D.ttf"
curl -sSfL -o "$dir/JetBrainsMono.ttf" "$base/jetbrainsmono/JetBrainsMono%5Bwght%5D.ttf"
echo "fonts in $dir"
