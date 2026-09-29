"""Builds the tour's whale art from the humpback kit's ink renders.

    python3 build_whales.py <kit>/previews/ink-frames/behaviors ../icon/whale-breach.png

It writes, for the light and the dark theme:

- swim-<theme>.png: the kit's cruise loop, every other frame, cropped to
  the box all the frames share and laid side by side as one strip.
- breach-<theme>.png: the icon's whale, turned nose up as it leaves the
  water.
- still-<theme>.png: the strip's first frame on its own, for the empty
  new tab, which shouldn't decode the whole strip.

The dark theme's whale is the light one's tones swapped, so it reads as
chalk on the dark paper instead of vanishing into it.
"""
import glob
import sys

from PIL import Image, ImageOps

SWIM_WIDTH = 440
BREACH_WIDTH = 900
BREACH_ANGLE = 38

frames_dir, breach_path = sys.argv[1:3]


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


def swim_strip():
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


def breach():
    whale = Image.open(breach_path).convert("RGBA")
    turned = whale.rotate(BREACH_ANGLE, resample=Image.BICUBIC, expand=True)
    turned = turned.crop(turned.getchannel("A").getbbox())
    scale = BREACH_WIDTH / turned.width
    return turned.resize((BREACH_WIDTH, round(turned.height * scale)), Image.LANCZOS)


strip, count = swim_strip()
strip.save("swim-light.png", optimize=True)
chalk(strip).save("swim-dark.png", optimize=True)
still = strip.crop((0, 0, SWIM_WIDTH, strip.height))
still.save("still-light.png", optimize=True)
chalk(still).save("still-dark.png", optimize=True)
hero = breach()
hero.save("breach-light.png", optimize=True)
chalk(hero).save("breach-dark.png", optimize=True)
print(f"swim: {count} frames of {SWIM_WIDTH}x{strip.height}; breach: {hero.width}x{hero.height}")
