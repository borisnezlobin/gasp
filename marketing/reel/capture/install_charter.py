"""Installs Charter for the capture display.

The app's default text and interface font is Charter, which macOS ships.
Linux doesn't, so the footage would show a fallback serif. XCharter (CTAN,
under Bitstream's free Charter licence) is Bitstream Charter with more
glyphs; this renames its family to "Charter" so the app finds it by the
name its theme asks for.

    python install_charter.py <dir with XCharter-*.otf> <fonts dir>

Fetch the files from https://mirrors.ctan.org/fonts/xcharter/opentype/.
Run fc-cache afterwards.
"""
import sys
from pathlib import Path

from fontTools.ttLib import TTFont

STYLES = {
    "Roman": ("Regular", 400),
    "Bold": ("Bold", 700),
    "Italic": ("Italic", 400),
    "BoldItalic": ("Bold Italic", 700),
}


def main() -> None:
    source, target = Path(sys.argv[1]), Path(sys.argv[2])
    target.mkdir(parents=True, exist_ok=True)
    for suffix, (style, _weight) in STYLES.items():
        font = TTFont(source / f"XCharter-{suffix}.otf")
        names = font["name"]
        full = "Charter" if style == "Regular" else f"Charter {style}"
        postscript = "Charter-" + style.replace(" ", "")
        for record in list(names.names):
            if record.nameID in (16, 17, 21, 22):
                names.removeNames(nameID=record.nameID)
        for platform in ((3, 1, 0x409), (1, 0, 0)):
            names.setName("Charter", 1, *platform)
            names.setName(style, 2, *platform)
            names.setName(full, 4, *platform)
            names.setName(postscript, 6, *platform)
        font.save(target / f"Charter-{style.replace(' ', '')}.otf")
        print("installed", full)


if __name__ == "__main__":
    main()
