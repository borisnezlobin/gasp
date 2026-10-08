"""Builds the tour's whale art.

    python3 build_whales.py swim <kit>/previews/ink-frames/behaviors
    python3 build_whales.py glide

`swim` writes, for the light and the dark theme, from the humpback kit's
ink renders:

- swim-<theme>.png: the kit's cruise loop, every other frame, cropped to
  the box all the frames share and laid side by side as one strip.
- still-<theme>.png: the strip's first frame on its own, for the empty
  new tab, which shouldn't decode the whole strip.

The dark theme's whale is the light one's tones swapped, so it reads as
chalk on the dark paper instead of vanishing into it.

`glide` writes glide-<theme>.png from the website's gliding humpback
(site/src/assets/whale-swim-<theme>.png): a strip of frames of it seen
through moving water, the way the website's and the iPhone's water bends
it. Two octaves of smooth value noise push each pixel about; the noise
drifts round a circle, so the last frame leads back into the first. The
page's wash over the whale and the fade into the deep are drawn in, so
the app only lays the frames over the page.
"""
import glob
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageOps
from scipy.ndimage import map_coordinates

SWIM_WIDTH = 440

GLIDE_WIDTH = 760
GLIDE_FRAMES = 32
GLIDE_SECONDS = 6.0
# The app shows the whale about this many points wide; the water's sizes
# below are in points, as the iPhone's shader has them.
GLIDE_POINTS = 500
WATER_STRENGTH = 5.0
WATER_FREQUENCY = (0.008, 0.05)
# How fast the iPhone's water drifts, in noise cells a second.
WATER_DRIFT = 0.33
# How much of the page lies over the whale.
WHALE_DEPTH = 0.65
# Where the fade into the deep starts and ends, in whale widths from its
# top.
FADE = (0.22, 0.47)
PAD = 10
GLIDE_COLOURS = 32

HERE = Path(__file__).resolve().parent
SITE_ASSETS = HERE.parents[3] / "site" / "src" / "assets"


def chalk(image):
    """Swaps the ink's tones, keeping its alpha."""
    rgb = image.convert("RGB")
    alpha = image.getchannel("A")
    swapped = ImageOps.invert(rgb)
    swapped = swapped.point(lambda v: int(40 + v * 0.78))
    swapped.putalpha(alpha)
    return swapped


def union_box(images):
    boxes = [image.getchannel("A").point(lambda a: 255 if a > 8 else 0).getbbox() for image in images]
    return (
        min(b[0] for b in boxes),
        min(b[1] for b in boxes),
        max(b[2] for b in boxes),
        max(b[3] for b in boxes),
    )


def swim_strip(frames_dir):
    paths = sorted(glob.glob(f"{frames_dir}/cruise_loop/*.png"))[::2]
    frames = [Image.open(path).convert("RGBA") for path in paths]
    left, top, right, bottom = union_box(frames)
    pad = 6
    box = (left - pad, top - pad, right + pad, bottom + pad)
    scale = SWIM_WIDTH / (box[2] - box[0])
    height = round((box[3] - box[1]) * scale)
    cropped = [frame.crop(box).resize((SWIM_WIDTH, height), Image.LANCZOS) for frame in frames]
    strip = Image.new("RGBA", (SWIM_WIDTH * len(cropped), height))
    for index, frame in enumerate(cropped):
        strip.paste(frame, (index * SWIM_WIDTH, 0))
    return strip, len(cropped)


def build_swim(frames_dir):
    strip, count = swim_strip(frames_dir)
    strip.save(HERE / "swim-light.png", optimize=True)
    chalk(strip).save(HERE / "swim-dark.png", optimize=True)
    still = strip.crop((0, 0, SWIM_WIDTH, strip.height))
    still.save(HERE / "still-light.png", optimize=True)
    chalk(still).save(HERE / "still-dark.png", optimize=True)
    print(f"swim: {count} frames of {SWIM_WIDTH}x{strip.height}")


def cell_hash(x, y):
    sx = np.mod(x * 123.34, 1.0)
    sy = np.mod(y * 456.21, 1.0)
    spread = sx * (sx + 45.32) + sy * (sy + 45.32)
    return np.mod((sx + spread) * (sy + spread), 1.0)


def value_noise(x, y):
    cell_x, cell_y = np.floor(x), np.floor(y)
    ease_x = (x - cell_x) ** 2 * (3 - 2 * (x - cell_x))
    ease_y = (y - cell_y) ** 2 * (3 - 2 * (y - cell_y))
    bottom = cell_hash(cell_x, cell_y) * (1 - ease_x) + cell_hash(cell_x + 1, cell_y) * ease_x
    top = cell_hash(cell_x, cell_y + 1) * (1 - ease_x) + cell_hash(cell_x + 1, cell_y + 1) * ease_x
    return bottom * (1 - ease_y) + top * ease_y


def fractal_noise(x, y):
    return value_noise(x, y) * 0.65 + value_noise(x * 2.03 + 17.1, y * 2.03 + 17.1) * 0.35


def drift(x, y, turn):
    """The water's bend at points `x`, `y`, `turn` of the way round the
    loop, each axis from -1 to 1."""
    radius = WATER_DRIFT * GLIDE_SECONDS / (2 * np.pi)
    angle = 2 * np.pi * turn
    across_x, across_y = x * WATER_FREQUENCY[0], y * WATER_FREQUENCY[1]
    across = fractal_noise(across_x + radius * np.cos(angle), across_y + radius * np.sin(angle)) - 0.5
    down = fractal_noise(
        across_x * 1.27 + 7.3 + radius * np.cos(angle + 2.1),
        across_y * 1.27 + 2.1 + radius * np.sin(angle + 2.1),
    ) - 0.5
    return across * 2, down * 2


def premultiplied(image):
    pixels = np.asarray(image).astype(np.float32) / 255
    pixels[..., :3] *= pixels[..., 3:4]
    return pixels


def depth_mask(height, width, scale):
    """The page's wash over the whale and its fade into the deep."""
    whale_width = (width - PAD * 2) / scale
    rows = (np.arange(height) - PAD) / scale / whale_width
    fade = 1 - np.clip((rows - FADE[0]) / (FADE[1] - FADE[0]), 0, 1)
    return (1 - WHALE_DEPTH) * fade[:, None]


def glide_frames(whale):
    height = round(whale.height * GLIDE_WIDTH / whale.width)
    whale = whale.resize((GLIDE_WIDTH, height), Image.LANCZOS)
    source = np.zeros((height + PAD * 2, GLIDE_WIDTH + PAD * 2, 4), np.float32)
    source[PAD:PAD + height, PAD:PAD + GLIDE_WIDTH] = premultiplied(whale)
    rows, columns = np.mgrid[0:source.shape[0], 0:source.shape[1]].astype(np.float32)
    scale = GLIDE_WIDTH / GLIDE_POINTS
    mask = depth_mask(source.shape[0], source.shape[1], scale)
    frames = []
    for index in range(GLIDE_FRAMES):
        across, down = drift(columns / scale, rows / scale, index / GLIDE_FRAMES)
        from_rows = rows + down * WATER_STRENGTH * scale
        from_columns = columns + across * WATER_STRENGTH * scale
        bent = np.stack(
            [map_coordinates(source[..., channel], [from_rows, from_columns], order=1) for channel in range(4)],
            axis=-1,
        )
        bent *= mask[..., None]
        alpha = bent[..., 3:4]
        colour = np.where(alpha > 1e-4, bent[..., :3] / np.maximum(alpha, 1e-4), 0)
        frame = np.concatenate([colour, alpha], axis=-1)
        frames.append(Image.fromarray((np.clip(frame, 0, 1) * 255).round().astype(np.uint8), "RGBA"))
    return frames


def glide_strip(theme):
    whale = Image.open(SITE_ASSETS / f"whale-swim-{theme}.png").convert("RGBA")
    frames = glide_frames(whale)
    width, height = frames[0].size
    strip = Image.new("RGBA", (width * len(frames), height))
    for index, frame in enumerate(frames):
        strip.paste(frame, (index * width, 0))
    return strip.quantize(GLIDE_COLOURS, method=Image.Quantize.FASTOCTREE, dither=Image.Dither.NONE)


def build_glide():
    for theme in ("light", "dark"):
        strip = glide_strip(theme)
        path = HERE / f"glide-{theme}.png"
        strip.save(path, optimize=True)
        print(f"glide-{theme}: {GLIDE_FRAMES} frames, {strip.width}x{strip.height}, {path.stat().st_size // 1024} KB")


BUILDS = {"swim": lambda args: build_swim(args[0]), "glide": lambda args: build_glide()}

if __name__ == "__main__":
    BUILDS[sys.argv[1]](sys.argv[2:])
