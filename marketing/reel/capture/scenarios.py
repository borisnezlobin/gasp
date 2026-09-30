"""The takes. Each function drives the real app through one shot.

Coordinates are screen pixels on the 2560x1600 capture display, with the
app at a scale factor of 2. Every take starts from fresh copies of the
showcase notes (see Take.reset_notes). A scenario sets up, then calls
take.start_recording(); take.py stops the recording and quits the app.
"""
import json
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent

# Where things are, at scale 2.
SYNC_ICON = (2514, 1576)
TEXT_LEFT = 560


def settle(take, seconds: float = 1.6) -> None:
    """Lets the app finish starting (fonts, the file watcher) before a shot."""
    take.wait(seconds)


def cold_start(take) -> None:
    """Black screen, then the app launched and its caret blinking. Uncut."""
    take.reset_notes()
    take.start_recording(paced=False)
    take.wait(0.9)
    take.launch("Surfacing.md")
    take.wait(2.6)


def typing(take) -> None:
    """Fast typing at the end of the essay; `## ` and `**` hide once the
    caret leaves them."""
    take.reset_notes()
    take.launch("Surfacing.md")
    settle(take)
    take.key("ctrl+End")
    take.start_recording()
    take.wait(0.5)
    take.type("## Blows\n", per_char=0.075, seed=3)
    take.type("A blow can rise **ten metres** and hang there after the whale has gone.",
              per_char=0.062, seed=4)
    take.wait(1.2)


def math(take) -> None:
    """Kepler's third law typed with the math snippets; the preview renders
    on every key and the formula sets inline when the caret leaves it."""
    take.reset_notes()
    take.launch("Orbits.md")
    settle(take)
    take.key("ctrl+End")
    take.start_recording()
    take.wait(0.4)
    take.type("The period is ", per_char=0.06, seed=5)
    take.type("mkT = 2pi sqacb/GM", per_char=0.085, seed=6)
    take.wait(0.25)
    for _ in range(3):
        take.key("Tab")
        take.wait(0.12)
    take.type(", so a wider orbit takes longer.", per_char=0.06, seed=7)
    take.wait(1.0)


def table(take) -> None:
    """Tab through a grid table, add a column from the palette, drag a row."""
    take.reset_notes()
    take.launch("Dive Log.md")
    settle(take)
    take.start_recording(cursor=True)
    take.wait(0.5)
    take.key("Tab")
    take.wait(0.35)
    take.key("Tab")
    take.wait(0.35)
    take.key("ctrl+p")
    take.wait(0.25)
    take.type("insert column right", per_char=0.035, seed=8)
    take.wait(0.2)
    take.key("Return")
    take.wait(0.3)
    take.type("Blows", per_char=0.07, seed=9)
    take.wait(0.3)
    take.key("Down")
    take.type("3", per_char=0.07)
    take.key("Down")
    take.type("5", per_char=0.07)
    take.key("Down")
    take.type("2", per_char=0.07)
    take.key("Down")
    take.type("4", per_char=0.07)
    take.wait(0.3)
    # Drag the blue whale's row to the top by its handle.
    take.move(900, 798)
    take.wait(0.35)
    take.drag(536, 798, 536, 520, seconds=0.55)
    take.wait(0.3)
    take.move(1900, 1200)
    take.wait(1.0)


def search(take) -> None:
    """Full-text search across the whole vault as the query is typed."""
    take.reset_notes()
    take.launch("Surfacing.md")
    settle(take, 2.5)
    take.start_recording()
    take.wait(0.5)
    take.key("ctrl+shift+f")
    # The panel reads every note's text when it first opens.
    take.wait(1.5)
    take.type("echo", per_char=0.12, seed=10)
    take.wait(0.8)
    take.key("Down")
    take.wait(0.18)
    take.key("Down")
    take.wait(0.18)
    take.key("Down")
    take.wait(1.0)


def accent(take) -> None:
    """The accent colour changed in Settings, one swatch per beat."""
    take.reset_notes()
    take.launch("Surfacing.md")
    settle(take)
    take.key("ctrl+comma")
    take.wait(0.5)
    take.click(420, 524)  # Appearance
    take.wait(0.5)
    take.move(1800, 1400)
    take.start_recording(cursor=True)
    take.wait(0.5)
    for x in (1720, 1780, 1840, 1900, 1660, 1960):
        take.click(x, 870)
        take.wait(0.4838)
    take.wait(0.8)


def theme(take) -> None:
    """Light to dark and back from the theme picker, with the note behind."""
    take.reset_notes()
    take.launch("Surfacing.md")
    settle(take)
    take.start_recording()
    take.wait(0.4)
    take.write_setting("theme", "dark")
    take.wait(1.0)
    take.write_setting("theme", "light")
    take.wait(1.0)


def keymap(take) -> None:
    """A shortcut added in Settings, then used."""
    take.reset_notes()
    take.launch("Surfacing.md")
    settle(take)
    take.key("ctrl+End")
    take.key("ctrl+comma")
    take.wait(0.5)
    take.click(464, 644)  # Keyboard shortcuts
    take.wait(0.5)
    take.move(1800, 1400)
    take.start_recording(cursor=True)
    take.wait(0.5)
    take.click(2160, 1297)  # + beside "Insert callout"
    take.wait(0.45)
    take.key("ctrl+shift+j", label="Ctrl Shift J")
    take.wait(0.8)
    take.key("Escape")
    take.wait(0.5)
    take.key("ctrl+shift+j", label="Ctrl Shift J")
    take.wait(0.4)
    take.type("Blows are loudest at dawn.", per_char=0.06, seed=12)
    take.wait(1.0)


def snippet(take) -> None:
    """Snippets expanding as they're typed: mk opens math, @a is alpha,
    sr squares it."""
    take.reset_notes()
    take.launch("Orbits.md")
    settle(take)
    take.key("ctrl+End")
    take.start_recording()
    take.wait(0.4)
    take.type("mk@asr+@bsr=@gsr", per_char=0.11, seed=13)
    take.wait(0.3)
    take.key("Tab")
    take.wait(1.0)


def sync(take) -> None:
    """An edit, Sync now, and the sync popover saying what went out."""
    take.reset_notes()
    take.commit_vault("Before the sync take")
    take.launch("Surfacing.md")
    settle(take, 2.0)
    take.key("ctrl+End")
    take.type("x")
    take.key("BackSpace")
    take.wait(2.5)
    take.key("ctrl+s")
    take.wait(3.0)
    take.start_recording(cursor=True)
    take.wait(0.4)
    take.type("Heard from the ferry at 6:40.", per_char=0.055, seed=14)
    # The note saves itself a second after the last key.
    take.wait(1.4)
    take.key("ctrl+s")
    take.wait(0.5)
    take.click(*SYNC_ICON)
    take.move(2000, 1200)
    take.wait(2.2)


def agent(take) -> None:
    """An agent patches the open note over MCP while it's being edited.
    The server is started and initialized first; its calls go the moment
    the last key's frame is up, while the note still has unsaved edits, so
    the app's bridge applies them in the open editor."""
    take.reset_notes()
    take.launch("Launch.md")
    settle(take, 2.0)
    take.key("ctrl+End")
    calls = HERE / "agent_calls.json"
    transcript = take.out / f"{take.name}.mcp.json"
    agent_process = take.mcp_prepare(calls, transcript)
    take.start_recording()
    take.wait(0.5)
    take.type("Out today.", per_char=0.07, seed=15)
    take.mcp_go(agent_process)
    take.wait(1.5)


def tagline(take) -> None:
    """The tagline typed in Gasp, bold, the asterisks hiding once the caret
    moves on. Large text and no inline title, set in the vault's settings."""
    take.reset_notes()
    take.write_settings({"base-font-size = 17": "base-font-size = 44"},
                        "[editor]\nshow-inline-title = false\n")
    take.launch("Tagline.md")
    settle(take)
    take.start_recording()
    take.wait(0.5)
    take.type("**Faster than you can gasp.**", per_char=0.07, seed=16)
    take.wait(0.35)
    take.key("Return")
    take.wait(1.6)
