"""Gathers what the compositor needs from the unpacked takes.

    python prep.py <frames-dir> <measure.json> <out data.json> <take> ...

For every take: its tick-to-frame table, the (nominal, real) anchors, its
events, trace lines, the MCP transcript if it has one, and where the caret
is in each changed frame (found by its colour, the accent red), so the
virtual camera can follow the caret instead of guessing.
"""
import json
import re
import sys
from pathlib import Path

import numpy as np
from PIL import Image

ACCENT = np.array([192, 43, 74])


def find_caret(path: Path):
    """The caret: a thin vertical run of the accent red. Returns (x, top,
    bottom) in screen pixels, or None."""
    image = np.asarray(Image.open(path).convert("RGB"), dtype=np.int16)
    distance = np.abs(image - ACCENT).sum(axis=2)
    red = distance < 60
    columns = red.sum(axis=0)
    candidates = np.nonzero(columns >= 24)[0]
    best = None
    for x in candidates:
        rows = np.nonzero(red[:, x])[0]
        top, bottom = int(rows.min()), int(rows.max())
        if bottom - top > 140 or len(rows) < 0.8 * (bottom - top + 1):
            continue
        # Thin: the colour stops within a few pixels either side.
        left = x
        while left > 0 and red[top:bottom + 1, left - 1].mean() > 0.8:
            left -= 1
        right = x
        while right < red.shape[1] - 1 and red[top:bottom + 1, right + 1].mean() > 0.8:
            right += 1
        if right - left > 8:
            continue
        best = (float((left + right) / 2), top, bottom)
        break
    return best


def main() -> None:
    frames, measure_path, out = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    data = {"takes": {}, "measure": json.loads(measure_path.read_text())}
    data["measure"].setdefault("bench_corpus_log", str(measure_path.parent / "bench-corpus.log"))
    data["measure"].setdefault("vault_notes", 5208)
    data["measure"].pop("bench_layout", None)
    for name in sys.argv[4:]:
        meta = json.loads((frames / f"{name}.json").read_text())
        cached = frames / f"{name}.carets.json"
        if cached.exists() and cached.stat().st_mtime > (frames / f"{name}.json").stat().st_mtime:
            carets = json.loads(cached.read_text())
        else:
            carets = [find_caret(frames / name / f"{index:05d}.png") for index in range(meta["frames"])]
            cached.write_text(json.dumps(carets))
        start = meta["start_wall"]
        events = []
        for event in meta["events"]:
            event = dict(event)
            event["t"] = round(event.pop("wall") - start, 4)
            events.append(event)
        take = {
            "frames": meta["frames"],
            "ticks": meta["ticks"],
            "anchors": meta.get("anchors") or [[0, 0]],
            "paced": meta.get("paced", False),
            "events": events,
            "trace": meta.get("trace", []),
            "carets": carets,
        }
        transcript = frames.parent / "takes" / f"{name}.mcp.json"
        if transcript.exists():
            take["mcp"] = [dict(line, t=round(line["wall"] - start, 4)) for line in json.loads(transcript.read_text())]
        searches = [float(m.group(1)) for m in
                    (re.search(r"search-query .* took ([0-9.]+)", line) for line in take["trace"]) if m]
        if searches:
            take["search_ms"] = searches
        first = [line for line in take["trace"] if "first-frame" in line]
        if first:
            take["first_frame"] = first[0]
        data["takes"][name] = take
        found = sum(1 for caret in carets if caret)
        print(f"{name}: {meta['frames']} frames, caret in {found}")
    data["measure"].update(numbers(frames, data))
    print(json.dumps({k: v for k, v in data["measure"].items() if k != "cold_start_runs"}, indent=1))
    out.write_text(json.dumps(data))


def numbers(frames: Path, data: dict) -> dict:
    """The numbers the reel shows, each from a measurement."""
    measure = data["measure"]
    # Cold start: from exec to the first recorded frame with the note on
    # screen (the frames before it are black, or an empty window).
    cold = data["takes"]["cold_start_10"]
    meta = json.loads((frames / "cold_start_10.json").read_text())
    exec_t = next(e["t"] for e in cold["events"] if e["what"] == "exec")
    visible = None
    for tick, frame in enumerate(cold["ticks"]):
        if frame < 0 or tick / 60 < exec_t:
            continue
        image = np.asarray(Image.open(frames / "cold_start_10" / f"{frame:05d}.png").convert("L"))
        if image.mean() > 128:
            visible = tick / 60 - exec_t
            break
    # Keystrokes: the layout bench's median input-to-paint in the corpus
    # joined into one 5,026-line note.
    bench = Path(measure["bench_corpus_log"]).read_text()
    keystroke = float(re.search(r"-- typing.*?input-to-paint: n=\d+ p50=([0-9.]+)ms", bench, re.S).group(1))
    lines = int(re.search(r"note: (\d+) lines", bench).group(1))
    # Search: the app's own timing of the last query in the search take.
    search = data["takes"]["search_4"]["search_ms"][-1]
    notes = measure["vault_notes"]
    return {
        "cold_visible_s": round(visible, 3),
        "cold_trace": cold.get("first_frame"),
        "keystroke_ms": f"{keystroke:.1f}",
        "search_ms": f"{search:.0f}",
        "notes": f"{notes:,}",
        # Short enough for two lines at a size a phone can read; the
        # README has the full method.
        "footnote": (
            f"Measured on a 4-core Linux VM with software rendering, {notes:,} notes. Launch shot in real time; "
            f"other shots recorded one key per frame. Median of {len(measure['cold_start_runs'])} launches: "
            f"{measure['cold_start_median_ms']:.0f} ms."
        ),
    }


if __name__ == "__main__":
    main()
