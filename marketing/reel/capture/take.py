"""Records one take of the real app on a virtual display.

    python take.py <scenario> <vault> <out-dir> [--name NAME]

Starts Xvfb and openbox if the display isn't up, starts ffmpeg grabbing the
screen at 60 fps, launches `gasp` on the vault (or a note in it), plays the
scenario's steps with xdotool, and stops. Nothing is sped up or edited here:
the file is the screen as it was.

It writes <out>/<name>.mkv (lossless H.264), <out>/<name>.json (every step
with its time in seconds from the first recorded frame, and the app's own
trace lines) and <out>/<name>.log (the app's stderr).

Scenarios live in scenarios.py. Environment: GASP (the binary),
DISPLAY_NUM (default 99), SCREEN (default 2560x1600).
"""
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

import scenarios

HERE = Path(__file__).resolve().parent
DISPLAY = ":" + os.environ.get("DISPLAY_NUM", "99")
SCREEN = os.environ.get("SCREEN", "2560x1600")
GASP = os.environ.get("GASP", "/tmp/target-app/release/gasp")
SCALE = os.environ.get("SCALE", "2")
# Short, so the app's MCP socket path fits in a sockaddr_un.
RUNTIME_DIR = "/tmp/gasp-rt"
RECORDER_CPUS = os.environ.get("RECORDER_CPUS", "3")
GRAB = os.environ.get("GRAB", str(HERE / "grab"))
# How long a paced step waits for the app's frame.
SETTLE = float(os.environ.get("SETTLE", "0.5"))
GRAB_FLAGS = os.environ.get("GRAB_FLAGS", "").split()
APP_CPUS = os.environ.get("APP_CPUS", "0-2")


def display_env() -> dict:
    env = dict(os.environ)
    env["DISPLAY"] = DISPLAY
    env.pop("WAYLAND_DISPLAY", None)
    return env


def ensure_display() -> None:
    probe = subprocess.run(["xdotool", "getdisplaygeometry"], env=display_env(), capture_output=True)
    if probe.returncode == 0:
        return
    subprocess.Popen(
        ["Xvfb", DISPLAY, "-screen", "0", f"{SCREEN}x24", "-br", "-nolisten", "tcp", "-dpi", "96"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    for _ in range(100):
        time.sleep(0.05)
        if subprocess.run(["xdotool", "getdisplaygeometry"], env=display_env(), capture_output=True).returncode == 0:
            break
    subprocess.Popen(
        ["openbox", "--config-file", str(HERE / "openbox-rc.xml")],
        env=display_env(),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    time.sleep(0.8)


class Take:
    def __init__(self, vault: Path, out: Path, name: str):
        self.vault = vault
        self.out = out
        self.name = name
        self.events: list = []
        self.app = None
        self.app_log = None
        self.home = out / "home"
        self.start_wall = None
        self.recording = False
        self.paced = False
        self.nominal = 0.0
        self.anchors: list = []

    # ---- recording -------------------------------------------------------

    def start_recording(self, cursor: bool = False, paced: bool = True) -> None:
        """Starts grab.c on the display (see grab.c for why not x11grab)."""
        self.video = self.out / f"{self.name}.grab"
        self.grab_log = open(self.out / f"{self.name}.grab.log", "w")
        args = [GRAB, "record", str(self.video)] + (["--cursor"] if cursor else []) + GRAB_FLAGS
        # The recorder gets its own core, so it neither misses ticks nor
        # slows the app down more than it must.
        self.recorder = subprocess.Popen(["taskset", "-c", RECORDER_CPUS, *args], env=display_env(),
                                         stderr=self.grab_log)
        time.sleep(0.3)
        self.recording = True
        self.paced = paced
        self.nominal = 0.0
        self.anchors = []
        self.anchor()

    def stop_recording(self) -> None:
        self.anchor()
        self.recorder.send_signal(signal.SIGINT)
        self.recorder.wait(timeout=120)
        self.recording = False
        self.grab_log.close()
        text = (self.out / f"{self.name}.grab.log").read_text()
        match = re.search(r"start ([0-9.]+)", text)
        self.start_wall = float(match.group(1)) if match else None
        missed = re.search(r"missed (\d+)", text)
        self.missed = int(missed.group(1)) if missed else None

    # ---- the app ---------------------------------------------------------

    def launch(self, target: str | None = None, trace: str = "all", extra_env: dict | None = None) -> None:
        env = display_env()
        env["HOME"] = str(self.home)
        env["XDG_CONFIG_HOME"] = str(self.home / ".config")
        env["XDG_DATA_HOME"] = str(self.home / ".local/share")
        env["XDG_STATE_HOME"] = str(self.home / ".local/state")
        env["XDG_CACHE_HOME"] = str(self.home / ".cache")
        env["XDG_RUNTIME_DIR"] = RUNTIME_DIR
        os.makedirs(RUNTIME_DIR, mode=0o700, exist_ok=True)
        env["GPUI_X11_SCALE_FACTOR"] = SCALE
        env["XCURSOR_SIZE"] = "48"
        env["EDITOR_TRACE_STARTUP"] = trace
        env.update(extra_env or {})
        path = str(self.vault / target) if target else str(self.vault)
        self.app_log = open(self.out / f"{self.name}.log", "w")
        self.mark("exec")
        self.app = subprocess.Popen(["taskset", "-c", APP_CPUS, GASP, path], env=env, stdout=self.app_log, stderr=subprocess.STDOUT)

    def quit_app(self) -> None:
        if self.app and self.app.poll() is None:
            self.app.terminate()
            try:
                self.app.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.app.kill()
        if self.app_log:
            self.app_log.close()

    # ---- steps -----------------------------------------------------------
    #
    # Paced takes. Here the app renders in software, about three frames a
    # second at 2560x1600, so keys typed at human speed would land several
    # to a frame. In a paced take every step (a key, a pointer move) is sent
    # alone and the take waits SETTLE seconds for the app's frame, while a
    # nominal clock advances by the step's human duration. The take keeps
    # (nominal, wall) anchors; the compositor plays the recording back on
    # the nominal clock, so each key's own frame shows from the moment the
    # key lands. Nothing is drawn that the app didn't draw. The real-time
    # take (cold_start) isn't paced.

    def mark(self, what: str, **data) -> None:
        self.events.append({"wall": time.time(), "nominal": round(self.nominal, 4), "what": what, **data})

    def anchor(self) -> None:
        if self.recording:
            self.anchors.append((round(self.nominal, 4), time.time()))

    def step(self, nominal_seconds: float) -> None:
        """Waits for the app's frame after a step and advances the clocks."""
        if self.paced:
            time.sleep(SETTLE)
            self.nominal += 1 / 60
            self.anchor()
            self.nominal += max(0.0, nominal_seconds - 1 / 60)
            self.anchor()
        else:
            time.sleep(nominal_seconds)
            self.nominal += nominal_seconds
            self.anchor()

    def xdo(self, *args: str) -> None:
        subprocess.run(["xdotool", *args], env=display_env(), check=False)

    def key(self, combo: str, label: str | None = None, after: float = 0.1) -> None:
        self.anchor()
        self.mark("key", keys=combo, label=label)
        self.xdo("key", "--clearmodifiers", combo)
        self.step(after)

    def type(self, text: str, per_char: float = 0.07, jitter: float = 0.025, seed: int = 1) -> None:
        import random

        rnd = random.Random(seed)
        for char in text:
            self.anchor()
            self.mark("char", char=char)
            if char == "\n":
                self.xdo("key", "Return")
            elif char == "\t":
                self.xdo("key", "Tab")
            else:
                self.xdo("type", "--delay", "0", char)
            self.step(max(0.03, per_char + rnd.uniform(-jitter, jitter)))

    def wait(self, seconds: float) -> None:
        """Real time: whatever happens on screen meanwhile plays as it was."""
        self.anchor()
        time.sleep(seconds)
        self.nominal += seconds
        self.anchor()

    def move(self, x: int, y: int, after: float = 0.05) -> None:
        self.anchor()
        self.mark("move", x=x, y=y)
        self.xdo("mousemove", str(x), str(y))
        self.step(after)

    def click(self, x: int, y: int, after: float = 0.1) -> None:
        self.anchor()
        self.mark("click", x=x, y=y)
        self.xdo("mousemove", str(x), str(y), "click", "1")
        self.step(after)

    def drag(self, x0: int, y0: int, x1: int, y1: int, seconds: float = 0.5, steps: int = 20) -> None:
        self.move(x0, y0, after=0.08)
        self.anchor()
        self.mark("press", x=x0, y=y0)
        self.xdo("mousedown", "1")
        self.step(0.05)
        for index in range(1, steps + 1):
            u = index / steps
            ease = u * u * (3 - 2 * u)
            self.move(int(x0 + (x1 - x0) * ease), int(y0 + (y1 - y0) * ease), after=seconds / steps)
        self.anchor()
        self.mark("release", x=x1, y=y1)
        self.xdo("mouseup", "1")
        self.step(0.1)

    def run(self, argv: list, stdin: str | None = None, label: str = "run") -> str:
        self.mark(label + "-start")
        result = subprocess.run(argv, input=stdin, capture_output=True, text=True)
        self.mark(label + "-end", stdout=result.stdout[-4000:], stderr=result.stderr[-2000:])
        return result.stdout

    def write_file(self, path: Path, text: str, label: str) -> None:
        self.mark("write", label=label, path=str(path))
        path.write_text(text)

    # ---- the vault ---------------------------------------------------------

    def reset_notes(self) -> None:
        """Fresh copies of the showcase notes and the vault's settings."""
        import make_vault

        for note in (HERE / "notes").glob("*.md"):
            shutil.copy(note, self.vault / note.name)
        (self.vault / ".gasp" / "settings.toml").write_text(make_vault.SETTINGS)
        (self.vault / ".gasp" / "theme.toml").write_text(make_vault.THEME)

    def write_setting(self, key: str, value: str) -> None:
        """Changes an [appearance] setting in the vault's settings file,
        which the app picks up while running."""
        import make_vault

        path = self.vault / ".gasp" / "settings.toml"
        text = make_vault.SETTINGS.replace('theme = "light"', f'{key} = "{value}"')
        self.write_file(path, text, f"{key}={value}")

    def write_settings(self, replace: dict, extra: str = "") -> None:
        """The vault's settings with some lines changed and more added."""
        import make_vault

        text = make_vault.SETTINGS
        for old, new in replace.items():
            text = text.replace(old, new)
        (self.vault / ".gasp" / "settings.toml").write_text(text + "\n" + extra)

    def commit_vault(self, message: str) -> None:
        for args in (["add", "-A"], ["commit", "-q", "--allow-empty", "-m", message], ["push", "-q"]):
            subprocess.run(["git", *args], cwd=self.vault, capture_output=True)

    def mcp_prepare(self, calls: Path, transcript: Path) -> subprocess.Popen:
        """Starts mcp_call.py: `gasp mcp <vault>`, initialized, waiting for
        mcp_go before it sends its tools/call requests."""
        env = dict(os.environ)
        env["HOME"] = str(self.home)
        env["XDG_RUNTIME_DIR"] = RUNTIME_DIR
        env["GASP"] = GASP
        process = subprocess.Popen(
            [sys.executable, str(HERE / "mcp_call.py"), str(self.vault), str(calls), str(transcript), "--wait"],
            env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        assert process.stdout.readline().strip() == "ready"
        return process

    def mcp_go(self, process: subprocess.Popen) -> None:
        # Real time: the calls and the app's response play as they happened.
        self.anchor()
        began = time.time()
        self.mark("mcp-start")
        process.stdin.write("go\n")
        process.stdin.flush()
        process.wait(timeout=60)
        self.nominal += time.time() - began
        self.anchor()
        self.mark("mcp-end")

    # ---- output ------------------------------------------------------------

    def save(self) -> None:
        trace = []
        log = self.out / f"{self.name}.log"
        if log.exists():
            trace = [line for line in log.read_text().splitlines() if line.startswith("startup")]
        for event in self.events:
            if self.start_wall is not None:
                event["t"] = round(event["wall"] - self.start_wall, 4)
        (self.out / f"{self.name}.json").write_text(
            json.dumps({"name": self.name, "start_wall": self.start_wall, "screen": SCREEN,
                        "missed_ticks": getattr(self, "missed", None),
                        "paced": self.paced, "settle": SETTLE,
                        "anchors": [(n, round(w - self.start_wall, 4)) for n, w in self.anchors]
                        if self.start_wall else [],
                        "events": self.events, "trace": trace}, indent=1)
        )


def main() -> None:
    scenario, vault, out = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
    name = sys.argv[sys.argv.index("--name") + 1] if "--name" in sys.argv else scenario
    out.mkdir(parents=True, exist_ok=True)
    ensure_display()
    take = Take(vault, out, name)
    home_seed = out / "home-seed"
    if take.home.exists():
        shutil.rmtree(take.home)
    if home_seed.exists():
        shutil.copytree(home_seed, take.home)
    take.home.mkdir(parents=True, exist_ok=True)
    try:
        # A scenario sets up, then calls take.start_recording() itself.
        getattr(scenarios, scenario)(take)
    finally:
        if take.recording:
            take.stop_recording()
        take.quit_app()
        take.save()
    print(f"{take.video} start={take.start_wall}")


if __name__ == "__main__":
    main()
