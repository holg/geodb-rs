#!/usr/bin/env python3
"""Build the board's screen for the web: geodb-fw-core in wasm on the firmware image.

    python3 crates/geodb-fw-web/build.py                       # -> crates/geodb-fw-web/dist/
    python3 crates/geodb-fw-web/build.py --serve 8792 --host 0.0.0.0 --tls

dist/ holds index.html, board.wasm (no wasm-bindgen, no JS glue) and geodb.fw
(firmware/stm32f769i-disco/geodb.fw, from
`cargo run --release -p geodb-fw-core --features std --example make_image`),
each with a .br and .gz next to it, and geodb-stm32f769i-disco.zip: everything
a board fresh from the box needs (bootloader, program, the same geodb.fw, a
manifest with addresses and SHA-256, flash.sh for probe-rs), built from this
tree (--no-bundle skips it). --serve, --host and --tls are those of
geodb-globe/scripts/web_release.py (WebGPU is not needed here, but a phone
on the LAN still wants https for brotli).
"""

import argparse
import datetime
import gzip
import hashlib
import json
import struct
import http.server
import os
import shutil
import ssl
import subprocess
import sys
from pathlib import Path

CRATE = Path(__file__).resolve().parent
REPO = CRATE.parent.parent
OUT = CRATE / "dist"
IMAGE = REPO / "firmware" / "stm32f769i-disco" / "geodb.fw"
sys.path.insert(0, str(REPO / "crates" / "geodb-globe" / "scripts"))
import web_release  # noqa: E402  (the server and the certificate)

FIRMWARE = REPO / "firmware" / "stm32f769i-disco"
BOOTLOADER = REPO / "firmware" / "bootloader"
BUNDLE = "geodb-stm32f769i-disco.zip"
CHIP = "STM32F769NIHx"
# Where each file goes in the internal flash (firmware/stm32f769i-disco/README.md).
LAYOUT = [
    ("bootloader.bin", 0x0800_0000, "the A/B bootloader (sector 0)"),
    ("app-a.bin", 0x0804_0000, "the program, slot A (sector 5): the board starts this"),
    ("geodb.fw", 0x080C_0000, "the city image (sectors 7-11)"),
]
# Not flashed at first: the program for slot B, the first update over Ethernet.
UPDATE = ("app-b.bin", 0x0808_0000, "the program, slot B: the first update (geodb-board ota app-b.bin)")

FLASH_SH = """#!/usr/bin/env bash
# geodb on the STM32F769I-DISCO: writes the bootloader, the program (slot A) and the city image with
# probe-rs (https://probe.rs) over the board's ST-LINK (the USB port next to the LCD).
# The whole INTERNAL flash is erased first (the factory demo is gone: see README.txt for a backup);
# the QSPI flash and the option bytes are not touched.
set -euo pipefail
cd "$(dirname "$0")"
CHIP=%(chip)s
SPEED=500  # kHz: the DISCO's ST-LINK V2-1 is happier slow
shasum -a 256 -c SHA256SUMS
probe-rs erase --chip $CHIP --speed $SPEED --connect-under-reset
%(downloads)s
probe-rs reset --chip $CHIP --speed $SPEED
echo "flashed. The board starts slot A; the LCD shows the globe, the bottom line its network address."
"""

README = """geodb on the STM32F769I-DISCO
%(rule)s

%(cities)s cities in the chip's flash, queried in place, on a globe on the 4" LCD; touch to turn and zoom.
Built from geodb-rs %(commit)s on %(date)s (manifest.json has the details).

Flash a board (macOS or Linux, with probe-rs: https://probe.rs/docs/getting-started/installation/):

    ./flash.sh

It checks the files (SHA256SUMS), erases the internal flash and writes:

%(table)s

A backup of the factory demo first, if you want it back later (needs stlink: brew install stlink):

    st-flash --connect-under-reset read factory-flash-2MB.bin 0x08000000 0x200000
    st-flash --connect-under-reset write factory-flash-2MB.bin 0x08000000   # to restore

Afterwards, with the board on Ethernet (DHCP), updates go over the network with geodb-board
(https://github.com/holg/geodb-rs, crates/geodb-board):

    geodb-board ota app-b.bin --host IP     # the program for the slot that is not running

Without probe-rs, any tool that writes a raw binary at an address works (STM32CubeProgrammer,
st-flash write FILE ADDRESS): the addresses are above. Cities: countries-states-cities-database
(dr5hn, CC-BY-4.0).
"""

# Small and fast enough: the screen is integer texture sampling and a few
# thousand haversines a frame.
PROFILE = {
    "CARGO_PROFILE_RELEASE_OPT_LEVEL": "s",
    "CARGO_PROFILE_RELEASE_LTO": "fat",
    "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "1",
    "CARGO_PROFILE_RELEASE_PANIC": "abort",
}


def build() -> None:
    if not IMAGE.exists():
        sys.exit(f"no {IMAGE}: cargo run --release -p geodb-fw-core --features std --example make_image")
    subprocess.run(["cargo", "build", "--release", "-p", "geodb-fw-web", "--target", "wasm32-unknown-unknown"],
                   cwd=REPO, check=True, env={**os.environ, **PROFILE})
    wasm = REPO / "target" / "wasm32-unknown-unknown" / "release" / "geodb_fw_web.wasm"
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir()
    if shutil.which("wasm-opt"):
        subprocess.run(["wasm-opt", "-Os", "--strip-debug", "--strip-producers", str(wasm),
                        "-o", str(OUT / "board.wasm")], check=True)
    else:
        shutil.copy(wasm, OUT / "board.wasm")
    shutil.copy(CRATE / "web" / "index.html", OUT / "index.html")
    shutil.copy(IMAGE, OUT / "geodb.fw")
    # the Blue Marble picture of the board before the vector coast (the "before" renderer of the page)
    shutil.copy(CRATE / "web" / "earth-256x128.rgb565", OUT / "earth-256x128.rgb565")
    # the elevation pictures for the relief (hill shading, contour lines): geodb-fw-core/scripts/make_elev.py
    for elev in sorted((REPO / "crates" / "geodb-fw-core" / "assets").glob("elev-*.u8")):
        shutil.copy(elev, OUT / elev.name)

    rows = []
    for f in sorted(OUT.iterdir()):
        data = f.read_bytes()
        (OUT / (f.name + ".gz")).write_bytes(gzip.compress(data, 9, mtime=0))
        sizes = [len(data), (OUT / (f.name + ".gz")).stat().st_size]
        if shutil.which("brotli"):
            subprocess.run(["brotli", "-q", "11", "-w", "24", "-f", "-o", str(f) + ".br", str(f)], check=True)
            sizes.append((OUT / (f.name + ".br")).stat().st_size)
        rows.append((f.name, sizes))
    print(f"\n{OUT}\n{'file':<12} {'plain':>10} {'gzip':>10} {'brotli':>10}")
    for name, sizes in rows:
        print(f"{name:<12} " + " ".join(f"{s:>10,}" for s in sizes))


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git(*args: str) -> str:
    return subprocess.run(["git", *args], cwd=REPO, capture_output=True, text=True).stdout.strip()


def bundle() -> None:
    """The flash bundle: built here from this tree, so the program and the image fit together."""
    subprocess.run(["cargo", "build", "--release"], cwd=BOOTLOADER, check=True)
    subprocess.run([str(FIRMWARE / "build-ota.sh")], check=True)
    work = OUT / ".bundle"
    if work.exists():
        shutil.rmtree(work)
    work.mkdir()
    elf = BOOTLOADER / "target" / "thumbv7em-none-eabihf" / "release" / "geodb-bootloader"
    subprocess.run(["rust-objcopy", "-O", "binary", str(elf), str(work / "bootloader.bin")], check=True)
    for name in ("app-a.bin", "app-b.bin"):
        shutil.copy(FIRMWARE / name, work / name)
    shutil.copy(IMAGE, work / "geodb.fw")

    head = (work / "geodb.fw").read_bytes()[:64]
    version, cities = struct.unpack_from("<H", head, 4)[0], struct.unpack_from("<I", head, 8)[0]
    commit = git("rev-parse", "--short", "HEAD")
    dirty = bool(git("status", "--porcelain", "--", "firmware", "crates/geodb-fw-core"))
    if dirty:
        commit += " + uncommitted changes in firmware/ or geodb-fw-core"
    date = datetime.date.today().isoformat()
    files = []
    for name, at, what in LAYOUT + [UPDATE]:
        f = work / name
        files.append({"file": name, "address": f"0x{at:08X}", "size": f.stat().st_size,
                      "sha256": sha256(f), "flash_at_first": name != UPDATE[0], "what": what})
    end = {e["file"]: int(e["address"], 16) + e["size"] for e in files}
    assert end["bootloader.bin"] <= 0x0800_8000 and end["app-a.bin"] <= 0x0808_0000, "a file overruns its sectors"
    assert end["app-b.bin"] <= 0x080C_0000 and end["geodb.fw"] <= 0x0820_0000, "a file overruns its sectors"
    manifest = {"board": "STM32F769I-DISCO", "chip": CHIP, "commit": commit, "date": date,
                "image": {"format": "GDFW", "version": version, "cities": cities}, "files": files}
    (work / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (work / "SHA256SUMS").write_text("".join(f"{e['sha256']}  {e['file']}\n" for e in files))
    downloads = "\n".join(
        f"probe-rs download --chip $CHIP --speed $SPEED --binary-format bin --base-address {e['address']} {e['file']}"
        for e in files if e["flash_at_first"])
    (work / "flash.sh").write_text(FLASH_SH % {"chip": CHIP, "downloads": downloads})
    (work / "flash.sh").chmod(0o755)
    table = "\n".join(f"    {e['address']}  {e['file']:<15} {e['size']:>9,} bytes  {e['what']}"
                      for e in files if e["flash_at_first"])
    title = f"geodb on the STM32F769I-DISCO"
    (work / "README.txt").write_text(README % {"rule": "=" * len(title), "cities": f"{cities:,}",
                                               "commit": commit, "date": date, "table": table})
    # one folder in the zip, the script stays executable
    stem = BUNDLE.removesuffix(".zip")
    if (work / stem).exists():
        shutil.rmtree(work / stem)
    staged = work / stem
    staged.mkdir()
    for f in list(work.iterdir()):
        if f.is_file():
            shutil.move(str(f), staged / f.name)
    (OUT / BUNDLE).unlink(missing_ok=True)
    subprocess.run(["zip", "-q", "-X", "-r", str(OUT / BUNDLE), stem], cwd=work, check=True)
    shutil.rmtree(work)
    print(f"\n{OUT / BUNDLE}: image v{version}, {cities:,} cities, {commit}")
    for e in files:
        print(f"  {e['address']}  {e['file']:<15} {e['size']:>9,}  {e['sha256'][:16]}…")


def pages(dest: Path) -> None:
    """dist for a static host that sends files as they are (no .br/.gz)."""
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    for f in OUT.iterdir():
        if f.suffix not in (".br", ".gz"):
            shutil.copy(f, dest / f.name)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--no-build", action="store_true", help="only serve the last build")
    ap.add_argument("--no-bundle", action="store_true", help="skip the flash bundle (bootloader, program, image)")
    ap.add_argument("--pages", type=Path, metavar="DIR", help="also write a copy for a static host")
    ap.add_argument("--serve", type=int, metavar="PORT", help="serve dist with precompression")
    ap.add_argument("--host", default="127.0.0.1", help="address to listen on (0.0.0.0: intranet too)")
    ap.add_argument("--tls", action="store_true", help="HTTPS with web_release.py's self-signed certificate")
    args = ap.parse_args()
    if not args.no_build:
        build()
        if not args.no_bundle:
            bundle()
    if args.pages:
        pages(args.pages)
    if args.serve:
        web_release.OUT = OUT
        web_release.TYPES[".fw"] = "application/octet-stream"
        server = http.server.ThreadingHTTPServer((args.host, args.serve), web_release.Precompressed)
        scheme = "http"
        if args.tls:
            cert, key = web_release.certificate()
            ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            ctx.load_cert_chain(cert, key)
            server.socket = ctx.wrap_socket(server.socket, server_side=True)
            scheme = "https"
        hosts = ["127.0.0.1"] if args.host == "127.0.0.1" else ["localhost"] + web_release.local_addresses()
        print(f"\nserving {OUT} on:")
        for h in hosts:
            print(f"  {scheme}://{h}:{args.serve}/")
        server.serve_forever()


if __name__ == "__main__":
    main()
