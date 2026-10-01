#!/usr/bin/env python3
"""The board's elevation picture: ETOPO 2022 (NOAA NCEI, bedrock + land, 60 arc-second grid) reduced to
8 bits a cell, north up, longitude -180..180.

    python3 scripts/make_elev.py            # downloads the 5 arc-minute subset (37 MB), writes assets/elev-WxH.u8

The height is coded non-linearly (see `relief::elev_m` in src/relief.rs):
    0..64     sea floor, -6000 m .. 0 m in 93 m steps
    64..255   land, 0 .. 6500 m, finer at low heights (v = 64 + 191 * sqrt(h / 6500))
Needs numpy and scipy; the download goes through the THREDDS subset service (horizStride=5).
"""
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
import scipy.io
from scipy.ndimage import gaussian_filter, zoom

HERE = Path(__file__).resolve().parent.parent
URL = ("https://www.ngdc.noaa.gov/thredds/ncss/grid/global/ETOPO2022/60s/60s_bed_elev_netcdf/"
       "ETOPO_2022_v1_60s_N90W180_bed.nc?var=z&horizStride=5&accept=netcdf3")


def encode(h: np.ndarray) -> np.ndarray:
    h = np.asarray(h, dtype=np.float64)
    sea = 64.0 + np.clip(h, -6000.0, 0.0) * 64.0 / 6000.0
    land = 64.0 + 191.0 * np.sqrt(np.clip(h, 0.0, 6500.0) / 6500.0)
    return np.rint(np.where(h < 0, sea, land)).clip(0, 255).astype(np.uint8)


def main() -> None:
    sizes = [(512, 256), (1024, 512)]
    cache = Path(tempfile.gettempdir()) / "etopo2022_5arcmin.nc"
    if not cache.exists():
        print("downloading", URL)
        subprocess.run(["curl", "-sf", "-o", str(cache), URL], check=True)
    z = scipy.io.netcdf_file(str(cache), mmap=False).variables["z"][:].astype(np.float32)
    z = z[::-1]  # north up
    (HERE / "assets").mkdir(exist_ok=True)
    for w, h in sizes:
        # smooth to the target cell (a box of about the cell size), then resample
        fy, fx = z.shape[0] / h, z.shape[1] / w
        small = zoom(gaussian_filter(z, (fy / 2.0, fx / 2.0)), (h / z.shape[0], w / z.shape[1]), order=1)
        out = HERE / "assets" / f"elev-{w}x{h}.u8"
        out.write_bytes(encode(small).tobytes())
        print(out, out.stat().st_size, "bytes; heights", int(small.min()), "..", int(small.max()), "m")


if __name__ == "__main__":
    sys.exit(main())
