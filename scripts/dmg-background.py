"""Draws the background of Gasp's install window: the whale leaps from Gasp to Applications.

    python3 scripts/dmg-background.py

Needs Pillow and numpy, and macOS's tiffutil. Writes background.png, background@2x.png and
the combined background.tiff to apps/desktop/assets/dmg/. The icon spots and the window
size must match scripts/dmg-settings.py.
"""
import math
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

REPO_ROOT = Path(__file__).resolve().parent.parent
WHALE_ART = REPO_ROOT / "site/src/assets/whale-swim-light.png"
OUT_DIR = REPO_ROOT / "apps/desktop/assets/dmg"

WIDTH, HEIGHT = 660, 420
SUPERSAMPLE = 4
GASP_SPOT = (170, 190)
APPLICATIONS_SPOT = (490, 190)
ICON_HALF = 64

PAPER = (246, 246, 247)
WHALE_INK = (59, 59, 64)
ACCENT = (200, 40, 60)
NOTE_LINE = (220, 220, 223)
LABEL_LINE = (118, 118, 121)
GRAIN_STRENGTH = 4

LABEL_LINE_TOP = 256
LABEL_LINE_HEIGHT = 28
NOTE_LEFT = 62
NOTE_RIGHT = 598
NOTE_LINE_HEIGHT = 10
NOTE_LINES = [(304, 500), (330, 436), (356, 522), (382, 268)]
CARET_SIZE = (3, 18)
CARET_GAP = 12

ARC_START = (GASP_SPOT[0] + ICON_HALF + 2, GASP_SPOT[1] - ICON_HALF)
ARC_CONTROL = (330, -40)
ARC_END = (APPLICATIONS_SPOT[0] - ICON_HALF, APPLICATIONS_SPOT[1] - ICON_HALF)
TRAIL_SPAN = (0.02, 0.41)
TRAIL_WIDTH = (0.4, 2.6)
TRAIL_STEPS = 96
WHALE_FLUKE_AT = 0.40
WHALE_SNOUT_AT = 0.97
WHALE_FLUKE_TIP = (11, 214)
WHALE_SNOUT_TIP = (988, 105)


def arc_point(t: float) -> tuple[float, float]:
    u = 1 - t
    x = u * u * ARC_START[0] + 2 * u * t * ARC_CONTROL[0] + t * t * ARC_END[0]
    y = u * u * ARC_START[1] + 2 * u * t * ARC_CONTROL[1] + t * t * ARC_END[1]
    return x, y


def arc_heading(t: float) -> float:
    dx = 2 * (1 - t) * (ARC_CONTROL[0] - ARC_START[0]) + 2 * t * (ARC_END[0] - ARC_CONTROL[0])
    dy = 2 * (1 - t) * (ARC_CONTROL[1] - ARC_START[1]) + 2 * t * (ARC_END[1] - ARC_CONTROL[1])
    return math.atan2(dy, dx)


def scaled(value: float) -> float:
    return value * SUPERSAMPLE


def rounded_bar(draw: ImageDraw.ImageDraw, box: tuple[float, float, float, float], colour) -> None:
    left, top, right, bottom = box
    radius = min(right - left, bottom - top) / 2
    draw.rounded_rectangle([scaled(left), scaled(top), scaled(right), scaled(bottom)], radius=scaled(radius), fill=colour)


def draw_note(draw: ImageDraw.ImageDraw) -> None:
    rounded_bar(draw, (NOTE_LEFT, LABEL_LINE_TOP, NOTE_RIGHT, LABEL_LINE_TOP + LABEL_LINE_HEIGHT), LABEL_LINE)
    for top, length in NOTE_LINES:
        rounded_bar(draw, (NOTE_LEFT, top, NOTE_LEFT + length, top + NOTE_LINE_HEIGHT), NOTE_LINE)
    last_top, last_length = NOTE_LINES[-1]
    caret_left = NOTE_LEFT + last_length + CARET_GAP
    caret_top = last_top + NOTE_LINE_HEIGHT / 2 - CARET_SIZE[1] / 2
    rounded_bar(draw, (caret_left, caret_top, caret_left + CARET_SIZE[0], caret_top + CARET_SIZE[1]), ACCENT)


def trail_width(progress: float) -> float:
    """Thin where the leap leaves Gasp, swelling into the whale's tail."""
    thin, thick = TRAIL_WIDTH
    return thin + (thick - thin) * progress


def trail_edge(t: float, width: float, side: int) -> tuple[float, float]:
    x, y = arc_point(t)
    normal = arc_heading(t) + side * math.pi / 2
    return scaled(x + math.cos(normal) * width / 2), scaled(y + math.sin(normal) * width / 2)


def draw_trail(draw: ImageDraw.ImageDraw) -> None:
    start, end = TRAIL_SPAN
    progresses = [i / TRAIL_STEPS for i in range(TRAIL_STEPS + 1)]
    points = [(start + (end - start) * p, trail_width(p)) for p in progresses]
    left_edge = [trail_edge(t, width, -1) for t, width in points]
    right_edge = [trail_edge(t, width, 1) for t, width in points]
    draw.polygon(left_edge + right_edge[::-1], fill=WHALE_INK)


def centred_on(art: Image.Image, point: tuple[float, float]) -> Image.Image:
    x, y = point
    half_width = max(x, art.width - x)
    half_height = max(y, art.height - y)
    padded = Image.new("RGBA", (round(2 * half_width), round(2 * half_height)), (0, 0, 0, 0))
    padded.alpha_composite(art, (round(half_width - x), round(half_height - y)))
    return padded


def whale_sprite(fluke: tuple[float, float], snout: tuple[float, float]) -> Image.Image:
    art = Image.open(WHALE_ART).convert("RGBA")
    art_dx, art_dy = WHALE_SNOUT_TIP[0] - WHALE_FLUKE_TIP[0], WHALE_SNOUT_TIP[1] - WHALE_FLUKE_TIP[1]
    path_dx, path_dy = snout[0] - fluke[0], snout[1] - fluke[1]
    scale = scaled(math.hypot(path_dx, path_dy)) / math.hypot(art_dx, art_dy)
    axis_middle = ((WHALE_FLUKE_TIP[0] + WHALE_SNOUT_TIP[0]) / 2, (WHALE_FLUKE_TIP[1] + WHALE_SNOUT_TIP[1]) / 2)
    art = centred_on(art, axis_middle)
    art = art.resize((round(art.width * scale), round(art.height * scale)), Image.LANCZOS)
    turn = math.atan2(path_dy, path_dx) - math.atan2(art_dy, art_dx)
    return art.rotate(-math.degrees(turn), resample=Image.BICUBIC, expand=True)


def place_whale(canvas: Image.Image) -> None:
    fluke, snout = arc_point(WHALE_FLUKE_AT), arc_point(WHALE_SNOUT_AT)
    sprite = whale_sprite(fluke, snout)
    middle_x, middle_y = scaled((fluke[0] + snout[0]) / 2), scaled((fluke[1] + snout[1]) / 2)
    canvas.alpha_composite(sprite, (round(middle_x - sprite.width / 2), round(middle_y - sprite.height / 2)))


def render_master() -> Image.Image:
    canvas = Image.new("RGBA", (round(scaled(WIDTH)), round(scaled(HEIGHT))), PAPER + (255,))
    draw = ImageDraw.Draw(canvas)
    draw_note(draw)
    draw_trail(draw)
    place_whale(canvas)
    return canvas.convert("RGB")


def add_grain(image: Image.Image, seed: int) -> Image.Image:
    pixels = np.asarray(image).astype(np.int16)
    is_paper = np.all(pixels == PAPER, axis=2, keepdims=True)
    noise = np.random.default_rng(seed).integers(-GRAIN_STRENGTH, GRAIN_STRENGTH + 1, pixels.shape[:2])
    grained = pixels + np.where(is_paper, noise[..., None], 0)
    return Image.fromarray(np.clip(grained, 0, 255).astype(np.uint8))


def relative_luminance(colour) -> float:
    def linear(channel: int) -> float:
        value = channel / 255
        return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4

    red, green, blue = (linear(channel) for channel in colour)
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue


def contrast(first, second) -> float:
    lighter, darker = sorted((relative_luminance(first), relative_luminance(second)), reverse=True)
    return (lighter + 0.05) / (darker + 0.05)


def write_outputs(master: Image.Image) -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for scale, name, seed in [(1, "background.png", 1), (2, "background@2x.png", 2)]:
        image = master.resize((WIDTH * scale, HEIGHT * scale), Image.LANCZOS)
        add_grain(image, seed).save(OUT_DIR / name, dpi=(72 * scale, 72 * scale), optimize=True)
    subprocess.run(
        ["tiffutil", "-cathidpicheck", OUT_DIR / "background.png", OUT_DIR / "background@2x.png",
         "-out", OUT_DIR / "background.tiff"],
        check=True,
        capture_output=True,
    )


def main() -> None:
    write_outputs(render_master())
    print(f"Label line {LABEL_LINE}: {contrast(LABEL_LINE, (0, 0, 0)):.2f}:1 with black labels, "
          f"{contrast(LABEL_LINE, (255, 255, 255)):.2f}:1 with white labels")
    print(f"Wrote {OUT_DIR.relative_to(REPO_ROOT)}/background.png, background@2x.png and background.tiff")


if __name__ == "__main__":
    main()
