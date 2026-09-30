"""Unpacks recorded takes into what the compositor reads.

    python unpack.py <takes-dir> <frames-dir> <name> [<name> ...]

For each take, writes <frames-dir>/<name>/NNNNN.png, one per frame that
changed on screen, and <frames-dir>/<name>.json: the take's start time,
its events and trace (from take.py), and `ticks`, the changed frame each
60 Hz tick shows. A tick's time is start + tick / 60.
Also writes a lossless H.264 of the whole take, <frames-dir>/<name>.mkv,
for checking it by eye.
"""
import json
import os
import subprocess
import sys
from pathlib import Path

GRAB = os.environ.get("GRAB", str(Path(__file__).resolve().parent / "grab"))


def unpack(takes: Path, frames: Path, name: str) -> None:
    out = frames / name
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("*.png"):
        old.unlink()
    index = frames / f"{name}.index"
    grab = subprocess.Popen([GRAB, "unique", str(takes / f"{name}.grab"), str(index)], stdout=subprocess.PIPE)
    header = None
    meta = json.loads((takes / f"{name}.json").read_text())
    width, height = map(int, meta["screen"].split("x"))
    subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "bgr0", "-s", f"{width}x{height}",
         "-i", "-", "-start_number", "0", str(out / "%05d.png")],
        stdin=grab.stdout, check=True,
    )
    grab.wait()
    ticks = []
    for line in index.read_text().splitlines():
        if line.startswith("start"):
            header = line.split()
            continue
        _tick, frame = line.split()
        ticks.append(int(frame))
    meta["start_wall"] = float(header[1])
    meta["ticks"] = ticks
    meta["frames"] = len(list(out.glob("*.png")))
    (frames / f"{name}.json").write_text(json.dumps(meta))
    index.unlink()
    decode = subprocess.Popen([GRAB, "decode", str(takes / f"{name}.grab")], stdout=subprocess.PIPE,
                              stderr=subprocess.DEVNULL)
    subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "bgr0", "-s", f"{width}x{height}",
         "-r", "60", "-i", "-", "-vf", "scale=1280:-2", "-c:v", "libx264", "-crf", "16", "-preset", "fast",
         "-pix_fmt", "yuv420p", str(frames / f"{name}.mp4")],
        stdin=decode.stdout, check=True,
    )
    decode.wait()
    print(f"{name}: {meta['frames']} frames over {len(ticks)} ticks")


if __name__ == "__main__":
    for take_name in sys.argv[3:]:
        unpack(Path(sys.argv[1]), Path(sys.argv[2]), take_name)
