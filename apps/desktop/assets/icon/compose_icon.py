"""Composes the app icon from an ink render of the humpback.

    python compose_icon.py <whale.png> <out.png> [angle] [scale] [centre_x] [centre_y] [--small | --ios]

The Mac icon is a rounded tile with a shadow on a clear canvas. With --ios it's
the same tile filling an opaque square, since iOS rounds the corners itself and
App Store Connect rejects icons with transparency.
"""
import sys

import numpy as np
from PIL import Image, ImageChops, ImageDraw, ImageFilter

CANVAS = 1024
TILE_INSET = 100
TILE_SIZE = CANVAS - 2 * TILE_INSET
SUPERELLIPSE_EXPONENT = 5.0
SUPERSAMPLE = 4

PAPER = (248, 246, 241)
INK = (11, 11, 13)
ACCENT = (192, 43, 74)

# The dark versions, from the site's dark theme: the paper goes near black
# and the whale's ink swaps, a light body with dark grooves and fins.
DARK_PAPER = (28, 27, 25)
DARK_INK = (235, 231, 224)
DARK_ACCENT = (240, 97, 111)
DARK_BODY = (200, 198, 192)
DARK_GROOVES = (38, 37, 42)
SHADOW = INK

# The head close-up: the whale rises into the tile from the bottom-left
# corner, so only its head, grooves and front fin show.
HEAD_ANGLE = 40
HEAD_SCALE = 1.8
HEAD_TIP = (0.62, 0.3)
# Where the snout's tip is in the render, as fractions of its size.
RENDER_TIP = (0.997, 0.36)
TEXT_LINE_ALPHA = 34
GRAIN_STRENGTH = 5

TEXT_LINE_LEFT = 196
TEXT_LINE_HEIGHT = 18
TEXT_LINE_GAP = 58
TEXT_LINE_LENGTHS = [520, 600, 452, 580, 380]
TEXT_TOP = 560
SURFACE_OFFSET = -TEXT_LINE_HEIGHT // 2
SUBMERGED_OPACITY = 0.38
FOAM_HEIGHT = 7
RIPPLE_ALPHA = 150


def tile_mask(size: int) -> Image.Image:
    big = size * SUPERSAMPLE
    coordinates = np.linspace(-1.0, 1.0, big)
    x, y = np.meshgrid(coordinates, coordinates)
    inside = np.abs(x) ** SUPERELLIPSE_EXPONENT + np.abs(y) ** SUPERELLIPSE_EXPONENT <= 1.0
    mask = Image.fromarray((inside * 255).astype(np.uint8))
    return mask.resize((size, size), Image.LANCZOS)


def paper_tile(size: int) -> Image.Image:
    rng = np.random.default_rng(7)
    grain = rng.normal(0.0, GRAIN_STRENGTH, (size, size, 1))
    base = np.ones((size, size, 3)) * np.array(PAPER)
    return Image.fromarray(np.clip(base + grain, 0, 255).astype(np.uint8), "RGB").convert("RGBA")


def text_lines(size: int) -> Image.Image:
    layer = Image.new("RGBA", (size * SUPERSAMPLE, size * SUPERSAMPLE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    for index, length in enumerate(TEXT_LINE_LENGTHS):
        top = (TEXT_TOP - TILE_INSET + index * TEXT_LINE_GAP) * SUPERSAMPLE
        left = (TEXT_LINE_LEFT - TILE_INSET) * SUPERSAMPLE
        box = (left, top, left + length * SUPERSAMPLE, top + TEXT_LINE_HEIGHT * SUPERSAMPLE)
        draw.rounded_rectangle(box, radius=TEXT_LINE_HEIGHT * SUPERSAMPLE // 2, fill=(*INK, TEXT_LINE_ALPHA))
    return layer.resize((size, size), Image.LANCZOS)


def caret(size: int) -> Image.Image:
    layer = Image.new("RGBA", (size * SUPERSAMPLE, size * SUPERSAMPLE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    last = 0
    left = (TEXT_LINE_LEFT - TILE_INSET + TEXT_LINE_LENGTHS[last] + 22) * SUPERSAMPLE
    top = (TEXT_TOP - TILE_INSET + last * TEXT_LINE_GAP - 12) * SUPERSAMPLE
    width = max(9, TEXT_LINE_HEIGHT // 2)
    box = (left, top, left + width * SUPERSAMPLE, top + (TEXT_LINE_HEIGHT + 24) * SUPERSAMPLE)
    draw.rounded_rectangle(box, radius=width // 2 * SUPERSAMPLE, fill=(*ACCENT, 255))
    return layer.resize((size, size), Image.LANCZOS)


def use_dark_theme() -> None:
    """Near-black paper, light lines and the softer red of the dark theme."""
    global PAPER, INK, ACCENT, SHADOW
    PAPER, INK, ACCENT, SHADOW = DARK_PAPER, DARK_INK, DARK_ACCENT, (0, 0, 0)


def swapped_ink(whale: Image.Image) -> Image.Image:
    """The whale with its ink swapped for a dark page: the dark body turns
    light and the white grooves and fins turn dark, as the site's dark
    whale does."""
    pixels = np.array(whale, dtype=np.float32)
    luminance = pixels[..., :3] @ np.array([0.299, 0.587, 0.114])
    body, white = np.percentile(luminance[pixels[..., 3] > 200], [5, 99])
    light = np.clip((luminance - body) / max(white - body, 1.0), 0.0, 1.0)[..., None]
    pixels[..., :3] = np.array(DARK_BODY) * (1.0 - light) + np.array(DARK_GROOVES) * light
    return Image.fromarray(pixels.astype(np.uint8), "RGBA")


def whale_render(path: str) -> Image.Image:
    whale = Image.open(path).convert("RGBA")
    whale = whale.crop(whale.getbbox())
    return swapped_ink(whale) if PAPER == DARK_PAPER else whale


def placed_head(path: str, size: int) -> Image.Image:
    """The whale large and turned up, with its snout's tip at HEAD_TIP."""
    whale = whale_render(path)
    width = int(size * HEAD_SCALE)
    whale = whale.resize((width, int(whale.height * width / whale.width)), Image.LANCZOS)
    tip = np.array([RENDER_TIP[0] * whale.width, RENDER_TIP[1] * whale.height]) - np.array(whale.size) / 2
    turn = np.radians(HEAD_ANGLE)
    turned_tip = np.array([
        tip[0] * np.cos(turn) + tip[1] * np.sin(turn),
        -tip[0] * np.sin(turn) + tip[1] * np.cos(turn),
    ])
    whale = whale.rotate(HEAD_ANGLE, resample=Image.BICUBIC, expand=True)
    tip_in_whale = turned_tip + np.array(whale.size) / 2
    target = np.array(HEAD_TIP) * size
    x, y = (target - tip_in_whale).astype(int)
    layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    layer.alpha_composite(whale, (max(x, 0), max(y, 0)), (max(-x, 0), max(-y, 0)))
    return layer


def placed_whale(path: str, size: int, angle: float, scale: float, centre: tuple[float, float]) -> Image.Image:
    whale = whale_render(path)
    width = int(size * scale)
    whale = whale.resize((width, int(whale.height * width / whale.width)), Image.LANCZOS)
    whale = whale.rotate(angle, resample=Image.BICUBIC, expand=True)
    layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    x = int(centre[0] * size - whale.width / 2)
    y = int(centre[1] * size - whale.height / 2)
    layer.alpha_composite(whale, (max(x, 0), max(y, 0)), (max(-x, 0), max(-y, 0)))
    return layer


def submerge(whale: Image.Image) -> Image.Image:
    surface = TEXT_TOP - TILE_INSET - SURFACE_OFFSET
    alpha = np.array(whale.getchannel("A"), dtype=np.float32)
    alpha[surface:, :] *= SUBMERGED_OPACITY
    submerged = whale.copy()
    submerged.putalpha(Image.fromarray(alpha.astype(np.uint8)))
    return submerged


def surface_row() -> int:
    return TEXT_TOP - TILE_INSET - SURFACE_OFFSET


def foam(whale: Image.Image) -> Image.Image:
    alpha = np.array(whale.getchannel("A"), dtype=np.float32)
    band = np.zeros_like(alpha)
    top = surface_row()
    band[top:top + FOAM_HEIGHT, :] = alpha[top - 1, :][None, :]
    layer = Image.new("RGBA", whale.size, (*PAPER, 0))
    layer.putalpha(Image.fromarray(band.astype(np.uint8)).filter(ImageFilter.GaussianBlur(1.2)))
    return layer


def ripples(whale: Image.Image) -> Image.Image:
    row = np.array(whale.getchannel("A"))[surface_row() - 1, :]
    columns = np.nonzero(row > 128)[0]
    layer = Image.new("RGBA", (whale.width * SUPERSAMPLE, whale.height * SUPERSAMPLE), (0, 0, 0, 0))
    if len(columns) == 0:
        return layer.resize(whale.size, Image.LANCZOS)
    draw = ImageDraw.Draw(layer)
    y = surface_row() * SUPERSAMPLE
    for edge, direction in ((columns[0], -1), (columns[-1], 1)):
        for step, (length, lift) in enumerate(((46, 4), (28, 16))):
            start = (edge + direction * (14 + step * 26)) * SUPERSAMPLE
            end = start + direction * length * SUPERSAMPLE
            left, right = sorted((start, end))
            box = (left, y - (lift + 10) * SUPERSAMPLE, right, y - lift * SUPERSAMPLE + 10 * SUPERSAMPLE)
            draw.arc(box, 200, 340, fill=(*INK, RIPPLE_ALPHA), width=5 * SUPERSAMPLE)
    return layer.resize(whale.size, Image.LANCZOS)


def whale_shadow(whale: Image.Image) -> Image.Image:
    alpha = whale.getchannel("A").filter(ImageFilter.GaussianBlur(14))
    shadow = Image.new("RGBA", whale.size, (*SHADOW, 0))
    shadow.putalpha(alpha.point(lambda value: value * 0.16))
    return ImageChops.offset(shadow, 0, 10)


def drop_shadow(mask: Image.Image) -> Image.Image:
    canvas_mask = Image.new("L", (CANVAS, CANVAS), 0)
    canvas_mask.paste(mask, (TILE_INSET, TILE_INSET + 12))
    blurred = canvas_mask.filter(ImageFilter.GaussianBlur(22)).point(lambda value: value * 0.35)
    shadow = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    shadow.putalpha(blurred)
    return shadow


def head_artwork(whale_path: str) -> Image.Image:
    """The tile's paper with the whale's head rising into it."""
    tile = paper_tile(TILE_SIZE)
    whale = placed_head(whale_path, TILE_SIZE)
    tile.alpha_composite(whale_shadow(whale))
    tile.alpha_composite(whale)
    return tile


def artwork(whale_path: str, angle: float, scale: float, centre: tuple[float, float]) -> Image.Image:
    """The tile's paper, whale and lines, square and unmasked."""
    tile = paper_tile(TILE_SIZE)
    whale = submerge(placed_whale(whale_path, TILE_SIZE, angle, scale, centre))
    tile.alpha_composite(whale_shadow(whale))
    tile.alpha_composite(whale)
    tile.alpha_composite(foam(whale))
    tile.alpha_composite(text_lines(TILE_SIZE))
    tile.alpha_composite(caret(TILE_SIZE))
    return tile


def compose(whale_path: str, out_path: str, angle: float, scale: float, centre: tuple[float, float]) -> None:
    mask = tile_mask(TILE_SIZE)
    tile = artwork(whale_path, angle, scale, centre)
    tile.putalpha(ImageChops.multiply(tile.getchannel("A"), mask))
    icon = drop_shadow(mask)
    icon.alpha_composite(tile, (TILE_INSET, TILE_INSET))
    icon.save(out_path)


def compose_full_bleed(whale_path: str, out_path: str, angle: float, scale: float, centre: tuple[float, float]) -> None:
    tile = head_artwork(whale_path) if HEAD else artwork(whale_path, angle, scale, centre)
    tile.resize((CANVAS, CANVAS), Image.LANCZOS).convert("RGB").save(out_path)


HEAD = False


def use_small_size_layout() -> None:
    """Fewer, heavier marks for the 16 and 32 pixel icons, where thin lines vanish."""
    global TEXT_LINE_HEIGHT, TEXT_LINE_GAP, TEXT_LINE_LENGTHS, TEXT_LINE_ALPHA, TEXT_TOP, SURFACE_OFFSET, GRAIN_STRENGTH
    TEXT_LINE_HEIGHT = 40
    TEXT_LINE_GAP = 96
    TEXT_LINE_LENGTHS = [470, 600, 420]
    TEXT_LINE_ALPHA = 70
    TEXT_TOP = 570
    SURFACE_OFFSET = -TEXT_LINE_HEIGHT // 2
    GRAIN_STRENGTH = 0


if __name__ == "__main__":
    arguments = sys.argv[1:]
    if "--small" in arguments:
        arguments.remove("--small")
        use_small_size_layout()
    full_bleed = "--ios" in arguments
    if full_bleed:
        arguments.remove("--ios")
    if "--dark" in arguments:
        arguments.remove("--dark")
        use_dark_theme()
    if "--head" in arguments:
        arguments.remove("--head")
        HEAD = True
    (compose_full_bleed if full_bleed else compose)(
        arguments[0],
        arguments[1],
        float(arguments[2]) if len(arguments) > 2 else 34.0,
        float(arguments[3]) if len(arguments) > 3 else 1.12,
        (float(arguments[4]) if len(arguments) > 4 else 0.46, float(arguments[5]) if len(arguments) > 5 else 0.5),
    )
