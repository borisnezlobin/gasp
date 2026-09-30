"""Lists a take's steps on its nominal clock, for choosing in and out points.

    python events.py <frames-dir> <take> ...
"""
import json
import sys
from pathlib import Path

for name in sys.argv[2:]:
    meta = json.loads((Path(sys.argv[1]) / f"{name}.json").read_text())
    print(f"== {name}  paced={meta.get('paced')}  anchors={len(meta.get('anchors', []))}")
    line = []
    for event in meta["events"]:
        what = event["what"]
        if what == "char":
            line.append(event["char"])
            continue
        if line:
            print(f"   typed {''.join(line)!r}")
            line = []
        detail = event.get("keys") or event.get("label") or ""
        if "x" in event:
            detail = f"{event['x']},{event['y']}"
        print(f"{event.get('nominal', 0):7.3f}  {what} {detail}")
    if line:
        print(f"   typed {''.join(line)!r}")
