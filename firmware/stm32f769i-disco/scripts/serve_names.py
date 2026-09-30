#!/usr/bin/env python3
"""Answers the board's "who is here?" questions over the ST-LINK's virtual COM port.

The board holds about a third of the city names in flash. For the rest it sends

    ?LAT,LON\n        (degrees x 1e5, integers)

and this script replies with the nearest city of the full dataset (within 2 km)

    =LAT,LON|Name|State, Country\n      (ASCII; "=LAT,LON||" when unknown)

Usage: serve_names.py             UDP on port 7878: the board (Ethernet, DHCP) broadcasts
                                  its questions, this answers the sender
       serve_names.py --serial [/dev/cu.usbmodemXXXX]   the ST-LINK serial port (pyserial)
unidecode is used when present.
"""
import glob
import gzip
import json
import math
import socket
import sys
import unicodedata
from pathlib import Path


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


def answer(grid, line):
    """The reply line for one question line, or None when it is not a question."""
    line = line.strip()
    if not line.startswith("?"):
        return None
    try:
        la, lo = (int(v) for v in line[1:].split(","))
    except ValueError:
        return None
    found = lookup(grid, la / 1e5, lo / 1e5)
    print(f"{la / 1e5:.4f},{lo / 1e5:.4f} -> {found[2] if found else '-'}", flush=True)
    return f"={la},{lo}|{found[2]}|{found[3]}\n" if found else f"={la},{lo}||\n"


def serve_udp(grid, port=7878):
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(("", port))
    print(f"serving UDP on port {port} (the board broadcasts its questions)", flush=True)
    while True:
        data, peer = sock.recvfrom(512)
        reply = answer(grid, data.decode("ascii", "replace"))
        if reply:
            sock.sendto(reply.encode("ascii"), peer)


def serve_serial(grid, port, baud):
    import serial

    ser = serial.Serial(port, baud, timeout=1)
    print(f"serving on {port} at {baud} baud", flush=True)
    while True:
        reply = answer(grid, ser.readline().decode("ascii", "replace"))
        if reply:
            ser.write(reply.encode("ascii"))


def main():
    grid = load()
    if "--serial" in sys.argv:
        rest = [a for a in sys.argv[sys.argv.index("--serial") + 1:] if not a.startswith("--")]
        port = rest[0] if rest else (glob.glob("/dev/cu.usbmodem*") + glob.glob("/dev/ttyACM*"))[0]
        serve_serial(grid, port, 115200)
    else:
        serve_udp(grid)


if __name__ == "__main__":
    main()
