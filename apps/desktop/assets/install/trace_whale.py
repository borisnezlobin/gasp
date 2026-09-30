"""Traces the icon's whale into the two single-colour SVGs the install window draws.

    python trace_whale.py

Reads ../icon/whale-breach.png and writes whale-body.svg (the whole
silhouette, drawn in ink) and whale-light.svg (the throat pleats, fin and
fluke, drawn in paper over it). GPUI draws an SVG in one colour and can
rotate it, which it can't do to a bitmap, hence two layers. Needs Pillow,
numpy and potracer (`pip install potracer`).
"""
from pathlib import Path

import numpy as np
import potrace
from PIL import Image

HERE = Path(__file__).resolve().parent
SOURCE = HERE.parent / "icon" / "whale-breach.png"
TRACE_WIDTH = 1200
OPAQUE = 128
LIGHT = 150
SPECKLE = 6


def masks() -> tuple[np.ndarray, np.ndarray, tuple[int, int]]:
    whale = Image.open(SOURCE).convert("RGBA")
    whale = whale.crop(whale.getbbox())
    height = round(whale.height * TRACE_WIDTH / whale.width)
    whale = whale.resize((TRACE_WIDTH, height), Image.LANCZOS)
    pixels = np.asarray(whale).astype(np.float32)
    alpha = pixels[..., 3]
    luminance = pixels[..., :3] @ np.array([0.299, 0.587, 0.114])
    body = alpha > OPAQUE
    light = body & (luminance > LIGHT)
    return body, light, (TRACE_WIDTH, height)


def svg_path(mask: np.ndarray) -> str:
    # potracer traces the False pixels.
    traced = potrace.Bitmap(~mask).trace(turdsize=SPECKLE, alphamax=1.0, opticurve=True, opttolerance=0.2)
    parts = []
    for curve in traced:
        start = curve.start_point
        parts.append(f"M{start.x:.1f},{start.y:.1f}")
        for segment in curve.segments:
            end = segment.end_point
            if segment.is_corner:
                parts.append(f"L{segment.c.x:.1f},{segment.c.y:.1f}L{end.x:.1f},{end.y:.1f}")
            else:
                parts.append(
                    f"C{segment.c1.x:.1f},{segment.c1.y:.1f} {segment.c2.x:.1f},{segment.c2.y:.1f} {end.x:.1f},{end.y:.1f}"
                )
        parts.append("Z")
    return "".join(parts)


def write(name: str, mask: np.ndarray, size: tuple[int, int]) -> None:
    # The drawing is square, with the whale across its middle: GPUI skips
    # a sprite whose unturned bounds miss the clip, and a square's reach
    # covers the whale at any angle.
    width, height = size
    top = -(width - height) / 2
    svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 {top:.1f} {width} {width}" width="{width}" height="{width}">'
        f'<path fill="#000" fill-rule="evenodd" d="{svg_path(mask)}"/></svg>\n'
    )
    (HERE / name).write_text(svg)
    print(f"{name}: {len(svg) // 1024} KB")


if __name__ == "__main__":
    body, light, size = masks()
    write("whale-body.svg", body, size)
    write("whale-light.svg", light, size)
