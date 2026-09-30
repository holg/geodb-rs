#!/usr/bin/env python3
"""The earth picture of the firmware image: NASA Blue Marble at 256 x 128,
RGB565 little-endian (64 KB), from the 512 x 256 picture the web demo starts
with. Written to assets/earth-256x128.rgb565 (in git).

    python3 crates/geodb-fw-core/scripts/make_texture.py
"""

from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent.parent
SRC = HERE.parent / "geodb-globe" / "assets" / "mini" / "earth-tiny.webp"
OUT = HERE / "assets" / "earth-256x128.rgb565"

im = Image.open(SRC).convert("RGB").resize((256, 128), Image.Resampling.LANCZOS)
out = bytearray()
for r, g, b in im.getdata():
    v = ((r & 0xF8) << 8) | ((g & 0xFC) << 3) | (b >> 3)
    out += v.to_bytes(2, "little")
OUT.write_bytes(out)
print(f"{OUT}: {len(out):,} bytes")
