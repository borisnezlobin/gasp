"""Contact sheets, for checking frames by eye.

    python sheet.py <out.png> <columns> <tile-width> [--crop x,y,w,h] [--label] <image> ...

Labels each tile with its file name when --label is given.
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw


def main() -> None:
    args = sys.argv[1:]
    out, columns, width = args[0], int(args[1]), int(args[2])
    rest = args[3:]
    crop = None
    label = False
    if "--crop" in rest:
        at = rest.index("--crop")
        crop = tuple(int(v) for v in rest[at + 1].split(","))
        del rest[at:at + 2]
    if "--label" in rest:
        rest.remove("--label")
        label = True
    tiles = []
    for path in rest:
        image = Image.open(path).convert("RGB")
        if crop:
            x, y, w, h = crop
            image = image.crop((x, y, x + w, y + h))
        image = image.resize((width, round(image.height * width / image.width)), Image.LANCZOS)
        if label:
            draw = ImageDraw.Draw(image)
            draw.rectangle((0, 0, width, 18), fill=(0, 0, 0))
            draw.text((4, 3), Path(path).name, fill=(255, 255, 0))
        tiles.append(image)
    rows = (len(tiles) + columns - 1) // columns
    height = max(tile.height for tile in tiles)
    sheet = Image.new("RGB", (columns * (width + 6), rows * (height + 6)), (40, 40, 40))
    for index, tile in enumerate(tiles):
        sheet.paste(tile, ((index % columns) * (width + 6), (index // columns) * (height + 6)))
    sheet.save(out)


if __name__ == "__main__":
    main()
