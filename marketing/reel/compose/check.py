"""Checks a rendered cut: stills at every cut and inside every shot, a
contact sheet of them, and how far each cut is from the beat grid.

    python check.py <video.mp4> <cuts.json> <out-dir> [bpm]
"""
import json
import subprocess
import sys
from pathlib import Path

video, cuts_path, out = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
beat = 60 / float(sys.argv[4]) if len(sys.argv) > 4 else 60 / 124
out.mkdir(parents=True, exist_ok=True)
cuts = json.loads(cuts_path.read_text())
times = []
for t0, t1, kind, take in cuts:
    offset = (t0 / beat) - round(t0 / beat)
    print(f"{t0:7.3f}  beat {t0 / beat:6.2f}  off {offset * beat * 1000:+6.1f} ms  {kind} {take}")
    times += [t0 + 1 / 60, (t0 + t1) / 2]
stills = []
for t in times:
    path = out / f"t{t:07.3f}.png"
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-ss", f"{t:.3f}", "-i", video, "-frames:v", "1", str(path)],
                   check=True)
    stills.append(str(path))
here = Path(__file__).resolve().parent
portrait = "vertical" in video
subprocess.run([sys.executable, str(here / "sheet.py"), str(out / "contact.png"), "10" if portrait else "6",
                "200" if portrait else "320", "--label", *stills], check=True)
print(out / "contact.png")
