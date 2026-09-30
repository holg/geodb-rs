#!/usr/bin/env python3
"""Fetch and prepare the optional earth detail layers of the globe demo.

- earth-16k.webp / earth-8k.webp / earth-4k.webp: NASA Blue Marble Next
  Generation, July 2004, with topography and bathymetry (public domain;
  credit NASA Earth Observatory), resized from 21600 x 10800.
- coast10m.bin: Natural Earth 1:10m land and lakes (public domain), packed
  like coast.bin, raw (served with brotli), for a sharper re-bake.

    python3 crates/geodb-globe/scripts/fetch_detail.py

Sources are cached in assets/detail/src/; everything in assets/detail/ is
generated (not in git). web_release.py copies the outputs next to the page.
"""

import subprocess
import sys
import urllib.request
from pathlib import Path

CRATE = Path(__file__).resolve().parent.parent
OUT = CRATE / "assets" / "detail"
SRC = OUT / "src"
BLUE_MARBLE = (
    "https://eoimages.gsfc.nasa.gov/images/imagerecords/73000/73751/"
    "world.topo.bathy.200407.3x21600x10800.jpg"
)
NE = "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/"


def fetch(url: str, dest: Path) -> Path:
    if dest.exists():
        return dest
    print(f"downloading {url}")
    part = dest.with_suffix(dest.suffix + ".part")
    with urllib.request.urlopen(url) as r, open(part, "wb") as f:
        while chunk := r.read(1 << 20):
            f.write(chunk)
    part.rename(dest)
    return dest


def imagery(src: Path) -> None:
    from PIL import Image

    Image.MAX_IMAGE_PIXELS = None  # 233 Mpx source
    img = Image.open(src).convert("RGB")
    # WebP is at most 16383 px wide: 16380 x 8190 keeps the 2:1 ratio.
    for width, name in ((16380, "earth-16k.webp"), (8192, "earth-8k.webp"), (4096, "earth-4k.webp")):
        out = OUT / name
        img.resize((width, width // 2), Image.Resampling.LANCZOS).save(
            out, "WEBP", quality=82, method=6
        )
        print(f"{out.name}: {out.stat().st_size:,} bytes")


def main() -> None:
    SRC.mkdir(parents=True, exist_ok=True)
    imagery(fetch(BLUE_MARBLE, SRC / "blue-marble-200407-21600.jpg"))
    for name in ("ne_10m_land.geojson", "ne_10m_lakes.geojson"):
        fetch(NE + name, SRC / name)
    # The packing is the Rust one (same format and reader as coast.bin).
    subprocess.run(
        ["cargo", "run", "--release", "-q", "-p", "geodb-globe", "--example", "make_mini_assets",
         "--", "--detail"],
        check=True,
    )


if __name__ == "__main__":
    sys.exit(main())
