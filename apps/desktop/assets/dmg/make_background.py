"""Draws the disk image window's background: the icon's page, the whale
under the text lines and a line saying to open the app beside it.

    python make_background.py [--font Charter.otf] [--preview preview.png]

Writes background.png (1x) and background@2x.png beside this script;
scripts/package-macos.sh joins them with tiffutil. The layout constants
below must match the icon positions in scripts/package-macos.sh. With
--preview it also draws the icons and labels roughly where Finder will,
to judge the layout without a Mac. Needs Pillow and numpy.
"""
import argparse
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).resolve().parent
WHALE = HERE.parent / "icon" / "whale-breach.png"
ICON = HERE.parent / "icon" / "AppIcon-1024.png"

# The window's content, in points. Keep in step with package-macos.sh.
WIDTH, HEIGHT = 660, 368
ICON_SIZE = 128
APP_CENTER = (196, 138)
APPLICATIONS_CENTER = (548, 138)

PAPER = (248, 246, 241)
INK = (11, 11, 13)
WHALE_INK = (58, 57, 62)
ACCENT = (192, 43, 74)
GRAIN = 3.2

# The water: text lines, the first of them the surface.
LINE_LEFT = 56
LINE_TOP = 214
LINE_GAP = 24
LINE_THICKNESS = 8
LINE_ALPHA = 34
# The first line is the app's own: it stops at the gap, and the prompt
# carries on after it.
LINE_LENGTHS = [0, 520, 468, 540, 330]
# The surface parts round the app, as if it just came up through it,
# less on each line down as the wake closes.
PART_HALF_WIDTHS = [70, 52, 30, 0, 0]
# Letters thrown up either side of the app as it broke through.
SPRAY = [("g", -80, -22), ("a", -64, -42), ("s", 60, -46), ("p", 78, -30)]
SPRAY_SIZE = 13
SPRAY_ALPHA = 96

# The whale under the lines: the icon's pose, faded like the icon's
# submerged half, reaching down from the app's icon.
WHALE_ANGLE = 38
WHALE_LENGTH = 300
WHALE_OFFSET = (-8, 88)
WHALE_ALPHA = 0.1

# What to do, as the words of the first line, with the caret after them.
PROMPT = "Double-click to open"
PROMPT_SIZE = 17
# Room between the gap's edge and the prompt.
PROMPT_INDENT = 12
PROMPT_ALPHA = 205
CARET_WIDTH = 2.5
CARET_GAP = 6

FONT_CANDIDATES = [
    "/System/Library/Fonts/Supplemental/Charter.ttc",
    "/Library/Fonts/Charter.ttc",
    "/usr/local/share/fonts/charter/Charter-Regular.otf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
]


def paper(scale: int) -> Image.Image:
    rng = np.random.default_rng(11)
    size = (HEIGHT * scale, WIDTH * scale, 1)
    grain = rng.normal(0.0, GRAIN, size)
    base = np.ones(size[:2] + (3,)) * np.array(PAPER)
    return Image.fromarray(np.clip(base + grain, 0, 255).astype(np.uint8), "RGB").convert("RGBA")


def submerged_whale(scale: int) -> Image.Image:
    whale = Image.open(WHALE).convert("RGBA")
    whale = whale.crop(whale.getbbox())
    length = WHALE_LENGTH * scale
    whale = whale.resize((length, round(whale.height * length / whale.width)), Image.LANCZOS)
    whale = whale.rotate(WHALE_ANGLE, resample=Image.BICUBIC, expand=True)
    shade = Image.new("RGBA", whale.size, (*WHALE_INK, 0))
    shade.putalpha(whale.getchannel("A").point(lambda value: int(value * WHALE_ALPHA)))
    layer = Image.new("RGBA", (WIDTH * scale, HEIGHT * scale), (0, 0, 0, 0))
    centre_x = (APP_CENTER[0] + WHALE_OFFSET[0]) * scale
    centre_y = (APP_CENTER[1] + WHALE_OFFSET[1]) * scale
    layer.alpha_composite(shade, (int(centre_x - shade.width / 2), int(centre_y - shade.height / 2)))
    # Only what's under the surface shows; the icon is the part above.
    surface = (LINE_TOP - LINE_THICKNESS // 2) * scale
    alpha = np.array(layer.getchannel("A"))
    alpha[:surface, :] = 0
    fade = np.clip((np.arange(alpha.shape[0]) - surface) / (12 * scale), 0, 1)[:, None]
    layer.putalpha(Image.fromarray((alpha * fade).astype(np.uint8)))
    return layer


def lines(scale: int) -> Image.Image:
    layer = Image.new("RGBA", (WIDTH * scale, HEIGHT * scale), (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    radius = LINE_THICKNESS * scale // 2
    fill = (*INK, LINE_ALPHA)
    for index, (length, half) in enumerate(zip(LINE_LENGTHS, PART_HALF_WIDTHS)):
        top = (LINE_TOP + index * LINE_GAP - LINE_THICKNESS / 2) * scale
        bottom = top + LINE_THICKNESS * scale
        left = LINE_LEFT * scale
        right = (LINE_LEFT + length) * scale if length else (APP_CENTER[0] - half) * scale
        gap = ((APP_CENTER[0] - half) * scale, (APP_CENTER[0] + half) * scale)
        pieces = [(left, min(right, gap[0])), (gap[1], right)] if half else [(left, right)]
        for start, end in pieces:
            if end - start > 2 * radius:
                draw.rounded_rectangle((start, top, end, bottom), radius=radius, fill=fill)
    return layer


def spray(scale: int, font_path: str) -> Image.Image:
    layer = Image.new("RGBA", (WIDTH * scale, HEIGHT * scale), (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    font = ImageFont.truetype(font_path, SPRAY_SIZE * scale)
    for letter, dx, dy in SPRAY:
        at = ((APP_CENTER[0] + dx) * scale, (LINE_TOP + dy) * scale)
        draw.text(at, letter, font=font, fill=(*INK, SPRAY_ALPHA), anchor="mm")
    return layer


def prompt(scale: int, font_path: str) -> Image.Image:
    """The prompt as the first line's words after the gap, caret after."""
    layer = Image.new("RGBA", (WIDTH * scale, HEIGHT * scale), (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    font = ImageFont.truetype(font_path, PROMPT_SIZE * scale)
    left = (APP_CENTER[0] + PART_HALF_WIDTHS[0] + PROMPT_INDENT) * scale
    middle = LINE_TOP * scale
    draw.text((left, middle), PROMPT, font=font, fill=(*INK, PROMPT_ALPHA), anchor="lm")
    text_right = left + draw.textlength(PROMPT, font=font)
    ascent, _ = font.getmetrics()
    caret_left = text_right + CARET_GAP * scale
    half_height = ascent * 0.56
    draw.rounded_rectangle(
        (caret_left, middle - half_height, caret_left + CARET_WIDTH * scale, middle + half_height),
        radius=CARET_WIDTH * scale / 2,
        fill=(*ACCENT, 255),
    )
    return layer


def background(scale: int, font_path: str) -> Image.Image:
    art = paper(scale)
    art.alpha_composite(submerged_whale(scale))
    art.alpha_composite(lines(scale))
    art.alpha_composite(spray(scale, font_path))
    art.alpha_composite(prompt(scale, font_path))
    return art.convert("RGB")


def preview(art: Image.Image, font_path: str) -> Image.Image:
    """The 2x art with the icons and labels about where Finder draws them."""
    shown = art.convert("RGBA")
    scale = shown.width // WIDTH
    icon = Image.open(ICON).convert("RGBA").resize((ICON_SIZE * scale,) * 2, Image.LANCZOS)
    folder = Image.new("RGBA", icon.size, (0, 0, 0, 0))
    ImageDraw.Draw(folder).rounded_rectangle(
        (16 * scale, 30 * scale, 112 * scale, 104 * scale), radius=8 * scale, fill=(92, 158, 230, 255)
    )
    label_font = ImageFont.truetype(font_path, 13 * scale)
    draw = ImageDraw.Draw(shown)
    for image, centre, label in ((icon, APP_CENTER, "Gasp"), (folder, APPLICATIONS_CENTER, "Applications")):
        x, y = centre[0] * scale - image.width // 2, centre[1] * scale - image.height // 2
        shown.alpha_composite(image, (x, y))
        width = draw.textlength(label, font=label_font)
        draw.text((centre[0] * scale - width / 2, (centre[1] + ICON_SIZE / 2 + 4) * scale), label, font=label_font, fill=(0, 0, 0, 255))
    return shown


def find_font(chosen: str | None) -> str:
    for candidate in ([chosen] if chosen else []) + FONT_CANDIDATES:
        if candidate and Path(candidate).exists():
            return candidate
    raise SystemExit("no serif font found; pass --font")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--font")
    parser.add_argument("--preview")
    arguments = parser.parse_args()
    font_path = find_font(arguments.font)
    background(1, font_path).save(HERE / "background.png", optimize=True)
    double = background(2, font_path)
    double.save(HERE / "background@2x.png", optimize=True)
    if arguments.preview:
        preview(double, font_path).save(arguments.preview)


if __name__ == "__main__":
    main()
