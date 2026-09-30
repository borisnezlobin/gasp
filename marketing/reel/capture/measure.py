"""Measures the numbers the reel shows, on this machine.

    python measure.py <vault> <out.json> <corpus-dir> [runs]

Cold start: launches `gasp <vault>/Surfacing.md` with EDITOR_TRACE_STARTUP=quit
`runs` times on the capture display and takes, for each, the wall-clock time
from just before exec to the app's `first-frame` line (which the app logs
once the first frame has been presented, in Unix milliseconds). The first
run is a warm-up and isn't counted.

Keystrokes: `gasp --bench-layout <corpus-dir> --keystrokes 300`, which
joins the repository's corpus (fixtures/corpus) into one long note, types
into its middle and reports input-to-paint percentiles. The log is written
beside out.json as bench-corpus.log; prep.py reads the median from it.

Search: the `search-query` spans from a recorded search take are read by
the timeline instead (they're per keystroke, from the real run).
"""
import json
import os
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

import take


def env_for(home: Path, trace: str) -> dict:
    env = take.display_env()
    env["HOME"] = str(home)
    for key, sub in (("XDG_CONFIG_HOME", ".config"), ("XDG_DATA_HOME", ".local/share"),
                     ("XDG_STATE_HOME", ".local/state"), ("XDG_CACHE_HOME", ".cache")):
        env[key] = str(home / sub)
    env["XDG_RUNTIME_DIR"] = take.RUNTIME_DIR
    env["GPUI_X11_SCALE_FACTOR"] = take.SCALE
    env["EDITOR_TRACE_STARTUP"] = trace
    return env


def cold_starts(vault: Path, home: Path, runs: int) -> list:
    results = []
    for run in range(runs + 1):
        started = time.time() * 1000
        # The log goes to a file: a pipe would be held open by the app's
        # helper processes after it quits.
        log = home / "cold-start.log"
        with open(log, "w") as handle:
            subprocess.run([take.GASP, str(vault / "Surfacing.md")], env=env_for(home, "quit"),
                           stdout=handle, stderr=subprocess.STDOUT, timeout=60)
        text = log.read_text()
        match = re.search(r"first-frame\s+at ([0-9.]+).*unix (\d+)", text)
        if not match:
            print(text[-2000:])
            continue
        since_main = float(match.group(1))
        wall = int(match.group(2)) - started
        if run > 0:
            results.append({"exec_to_first_frame_ms": round(wall, 1), "main_to_first_frame_ms": since_main})
        time.sleep(0.5)
    return results


def keystrokes(corpus: Path, log: Path, home: Path) -> str:
    with open(log, "w") as handle:
        subprocess.run([take.GASP, "--bench-layout", str(corpus), "--keystrokes", "300"],
                       env=env_for(home, "0"), stdout=handle, stderr=subprocess.STDOUT, timeout=900)
    return log.read_text()


def main() -> None:
    vault, out, corpus = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    runs = int(sys.argv[4]) if len(sys.argv) > 4 else 10
    take.ensure_display()
    home = out.parent / "measure-home"
    home.mkdir(parents=True, exist_ok=True)
    os.makedirs(take.RUNTIME_DIR, mode=0o700, exist_ok=True)
    starts = cold_starts(vault, home, runs)
    walls = [run["exec_to_first_frame_ms"] for run in starts]
    bench = keystrokes(corpus, out.parent / "bench-corpus.log", home)
    notes = sum(1 for path in vault.rglob("*.md") if ".git" not in path.parts)
    result = {
        "machine": subprocess.run(["uname", "-srm"], capture_output=True, text=True).stdout.strip(),
        "cpus": os.cpu_count(),
        "cold_start_runs": starts,
        "cold_start_median_ms": statistics.median(walls) if walls else None,
        "cold_start_min_ms": min(walls) if walls else None,
        "bench_layout": bench,
        "bench_corpus_log": str(out.parent / "bench-corpus.log"),
        "vault_notes": notes,
    }
    out.write_text(json.dumps(result, indent=1))
    print(json.dumps({k: v for k, v in result.items() if k != "bench_layout"}, indent=1))
    print(bench)


if __name__ == "__main__":
    main()
