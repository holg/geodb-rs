#!/usr/bin/env python3
"""The picture the globe starts with: NASA Blue Marble at 512 x 256 px, lossy
WebP, about 11 KB (a whole-world Scrapbook picture of the System 7 era was
12 KB). It replaces the 61 KB packed coastlines at start: the page is the app
plus this. Written to assets/mini/earth-tiny.webp (in git).

    python3 crates/geodb-globe/scripts/make_tiny_earth.py

Needs assets/detail/earth-4k.webp (scripts/fetch_detail.py makes it).
"""

import sys
from pathlib import Path

from PIL import Image

CRATE = Path(__file__).resolve().parent.parent
SRC = CRATE / "assets" / "detail" / "earth-4k.webp"
OUT = CRATE / "assets" / "mini" / "earth-tiny.webp"
WIDTH, QUALITY = 512, 55


def main() -> None:
    if not SRC.exists():
        sys.exit(f"{SRC} missing: run crates/geodb-globe/scripts/fetch_detail.py first")
    im = Image.open(SRC).convert("RGB").resize((WIDTH, WIDTH // 2), Image.Resampling.LANCZOS)
    im.save(OUT, "WEBP", quality=QUALITY, method=6)
    print(f"{OUT}: {OUT.stat().st_size:,} bytes ({WIDTH} x {WIDTH // 2})")


if __name__ == "__main__":
    main()
