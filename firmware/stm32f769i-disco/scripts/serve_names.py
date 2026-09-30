#!/usr/bin/env python3
"""Answers the board's "who is here?" questions over the ST-LINK's virtual COM port.

The board holds about a third of the city names in flash. For the rest it sends

    ?LAT,LON\n        (degrees x 1e5, integers)

and this script replies with the nearest city of the full dataset (within 2 km)

    =LAT,LON|Name|State, Country\n      (ASCII; "=LAT,LON||" when unknown)

Usage: serve_names.py [/dev/cu.usbmodemXXXX] [--baud 115200]
Needs pyserial (pip install pyserial); unidecode is used when present.
"""
import glob
import gzip
import json
import math
import sys
import unicodedata
from pathlib import Path

import serial

DATA = Path(__file__).resolve().parents[3] / "crates/geodb-core/data/countries+states+cities.json.gz"

try:
    from unidecode import unidecode
except ImportError:  # Latin scripts only: strip the accents, drop the rest
    def unidecode(s):
        s = unicodedata.normalize("NFKD", s)
        return "".join(c for c in s if c.isascii())


def ascii_of(s, n):
    return "".join(c for c in unidecode(s or "") if " " <= c < "\x7f").strip()[:n]


def load():
    grid = {}
    count = 0
    for country in json.load(gzip.open(DATA)):
        for state in country.get("states") or []:
            for city in state.get("cities") or []:
                try:
                    lat, lon = float(city["latitude"]), float(city["longitude"])
                except (TypeError, ValueError, KeyError):
                    continue
                key = (round(lat * 50), round(lon * 50))  # 0.02 deg cells
                detail = f'{ascii_of(state["name"], 18)}, {ascii_of(country["name"], 18)}'
                grid.setdefault(key, []).append((lat, lon, ascii_of(city["name"], 23), detail))
                count += 1
    print(f"{count} cities loaded", flush=True)
    return grid


def km(lat1, lon1, lat2, lon2):
    p = math.pi / 180
    a = math.sin((lat2 - lat1) * p / 2) ** 2 + math.cos(lat1 * p) * math.cos(lat2 * p) * math.sin((lon2 - lon1) * p / 2) ** 2
    return 12742 * math.asin(math.sqrt(a))


def lookup(grid, lat, lon):
    best = None
    ky, kx = round(lat * 50), round(lon * 50)
    for dy in (-1, 0, 1):
        for dx in (-1, 0, 1):
            for c in grid.get((ky + dy, kx + dx), ()):
                d = km(lat, lon, c[0], c[1])
                if d < 2.0 and (best is None or d < best[0]):
                    best = (d, c)
    return best[1] if best else None


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    baud = int(sys.argv[sys.argv.index("--baud") + 1]) if "--baud" in sys.argv else 115200
    args = [a for a in args if a != str(baud)]
    port = args[0] if args else (glob.glob("/dev/cu.usbmodem*") + glob.glob("/dev/ttyACM*"))[0]
    grid = load()
    ser = serial.Serial(port, baud, timeout=1)
    print(f"serving on {port} at {baud} baud", flush=True)
    while True:
        line = ser.readline().decode("ascii", "replace").strip()
        if not line.startswith("?"):
            continue
        try:
            la, lo = (int(v) for v in line[1:].split(","))
        except ValueError:
            continue
        found = lookup(grid, la / 1e5, lo / 1e5)
        reply = f"={la},{lo}|{found[2]}|{found[3]}\n" if found else f"={la},{lo}||\n"
        ser.write(reply.encode("ascii"))
        print(f"{la / 1e5:.4f},{lo / 1e5:.4f} -> {found[2] if found else '-'}", flush=True)


if __name__ == "__main__":
    main()
