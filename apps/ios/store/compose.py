#!/usr/bin/env python3
"""Lays out the App Store screenshots: a caption over each raw screenshot,
on the paper and with the red caret of Gasp's icon.

    python apps/ios/store/compose.py

Reads target/store/raw (from apps/ios/scripts/store-screenshots.sh) and
writes 1320 x 2868 PNGs, Apple's 6.9" iPhone size, to target/store/final.
Needs Pillow and numpy.
"""
import pathlib

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

REPO = pathlib.Path(__file__).resolve().parents[3]
RAW = REPO / "target/store/raw"
FINAL = REPO / "target/store/final"
CHARTER = "/System/Library/Fonts/Supplemental/Charter.ttc"
CHARTER_BOLD_INDEX = 3

WIDTH, HEIGHT = 1320, 2868
SIDE_MARGIN = 120
CAPTION_TOP = 150
CAPTION_SIZE = 100
CAPTION_LEADING = 1.14
CAPTION_LINES = 2
SHOT_WIDTH = 1060
SHOT_GAP = 90
SHOT_RADIUS = 132
GRAIN = 4

LIGHT = {"paper": (248, 246, 241), "ink": (24, 24, 27)}
DARK = {"paper": (21, 20, 18), "ink": (247, 245, 241)}
CARET = (200, 40, 60)

SLIDES = [
    ("1-note", "Notes that look finished as you write", LIGHT),
    ("2-typing", "Markdown buttons above the keyboard", LIGHT),
    ("3-dark", "Easy on the eyes at night", DARK),
    ("4-files", "Every note is a plain file you keep", LIGHT),
    ("5-tabs", "Keep several notes open in tabs", LIGHT),
]


def caption_font():
    return ImageFont.truetype(CHARTER, CAPTION_SIZE, index=CHARTER_BOLD_INDEX)


def wrapped(text, font, width):
    """One line if it fits, else the two lines closest in width."""
    if font.getlength(text) <= width:
        return [text]
    words = text.split()
    splits = [(" ".join(words[:cut]), " ".join(words[cut:])) for cut in range(1, len(words))]
    fitting = [pair for pair in splits if font.getlength(pair[0]) <= width and font.getlength(pair[1]) <= width]
    if not fitting:
        return [text, "", ""]
    return list(min(fitting, key=lambda pair: abs(font.getlength(pair[0]) - font.getlength(pair[1]))))


def paper(colour):
    rng = np.random.default_rng(11)
    grain = rng.normal(0.0, GRAIN, (HEIGHT, WIDTH, 1))
    base = np.ones((HEIGHT, WIDTH, 3)) * np.array(colour)
    return Image.fromarray(np.clip(base + grain, 0, 255).astype(np.uint8), "RGB").convert("RGBA")


def draw_caption(canvas, text, theme):
    font = caption_font()
    draw = ImageDraw.Draw(canvas)
    line_height = int(CAPTION_SIZE * CAPTION_LEADING)
    lines = wrapped(text, font, WIDTH - 2 * SIDE_MARGIN)
    if len(lines) > CAPTION_LINES:
        raise SystemExit(f"Caption is more than {CAPTION_LINES} lines: {text}")
    y = CAPTION_TOP
    for line in lines:
        draw.text((SIDE_MARGIN, y), line, font=font, fill=theme["ink"])
        y += line_height
    last_width = font.getlength(lines[-1])
    caret_left = SIDE_MARGIN + last_width + CAPTION_SIZE * 0.14
    caret_top = y - line_height + CAPTION_SIZE * 0.12
    caret = (caret_left, caret_top, caret_left + CAPTION_SIZE * 0.09, caret_top + CAPTION_SIZE * 0.92)
    draw.rounded_rectangle(caret, radius=CAPTION_SIZE * 0.045, fill=CARET)


def rounded_mask(size, radius):
    scale = 4
    big = Image.new("L", (size[0] * scale, size[1] * scale), 0)
    ImageDraw.Draw(big).rounded_rectangle((0, 0, big.width - 1, big.height - 1), radius=radius * scale, fill=255)
    return big.resize(size, Image.LANCZOS)


def place_screenshot(canvas, shot_path, top):
    shot = Image.open(shot_path).convert("RGBA")
    height = int(shot.height * SHOT_WIDTH / shot.width)
    shot = shot.resize((SHOT_WIDTH, height), Image.LANCZOS)
    mask = rounded_mask(shot.size, SHOT_RADIUS)
    left = (WIDTH - SHOT_WIDTH) // 2
    shadow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    shadow_mask = Image.new("L", canvas.size, 0)
    shadow_mask.paste(mask, (left, top + 28))
    shadow.putalpha(shadow_mask.filter(ImageFilter.GaussianBlur(44)).point(lambda value: int(value * 0.22)))
    canvas.alpha_composite(shadow)
    shot.putalpha(mask)
    canvas.alpha_composite(shot, (left, top))


def main():
    FINAL.mkdir(parents=True, exist_ok=True)
    shot_top = CAPTION_TOP + int(CAPTION_SIZE * CAPTION_LEADING) * CAPTION_LINES + SHOT_GAP
    for name, caption, theme in SLIDES:
        canvas = paper(theme["paper"])
        draw_caption(canvas, caption, theme)
        place_screenshot(canvas, RAW / f"{name}.png", shot_top)
        out = FINAL / f"{name}.png"
        canvas.convert("RGB").save(out)
        print(f"wrote {out.relative_to(REPO)}")


if __name__ == "__main__":
    main()
