#!/usr/bin/env python3
"""Fetch and prepare the optional earth detail layers of the globe demo.

- earth-4k.webp, earth-8k-{row}-{col}.webp (2 x 1 tiles),
  earth-16k-{row}-{col}.webp (4 x 2 tiles of 4095 px): NASA Blue Marble
  Next Generation, July 2004, with topography and bathymetry (public
  domain; credit NASA Earth Observatory), resized from 21600 x 10800. The
  app decodes one tile at a time (at most 4096 px, 64 MB decoded), so a
  phone never holds a decoded 16K image (536 MB).
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
    # 16380 x 8190 keeps the 2:1 ratio and cuts into 4095 px tiles (the
    # tile grid is IMAGERY in src/mini_app.rs).
    for width, stem, (cols, rows) in ((16380, "earth-16k", (4, 2)), (8192, "earth-8k", (2, 1)),
                                      (4096, "earth-4k", (1, 1))):
        for old in OUT.glob(f"{stem}*.webp"):
            old.unlink()
        full = img.resize((width, width // 2), Image.Resampling.LANCZOS)
        tw, th = width // cols, width // 2 // rows
        total = 0
        for r in range(rows):
            for c in range(cols):
                name = f"{stem}.webp" if cols * rows == 1 else f"{stem}-{r}-{c}.webp"
                out = OUT / name
                full.crop((c * tw, r * th, (c + 1) * tw, (r + 1) * th)).save(
                    out, "WEBP", quality=82, method=6
                )
                total += out.stat().st_size
        print(f"{stem}: {cols * rows} file(s), {total:,} bytes")


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
