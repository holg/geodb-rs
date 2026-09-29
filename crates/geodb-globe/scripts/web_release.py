#!/usr/bin/env python3
"""Build the globe web demo for a server that sends compressed files.

Builds mini.html (or flex.html), swaps in the data files with uncompressed
payloads (assets/mini-raw, from `make_mini_assets`) and writes a `.br`
(brotli -q 11) and `.gz` (gzip -9) next to every file. Brotli on the raw
columns is about 10% smaller than the gzip inside the files.

    python3 crates/geodb-globe/scripts/web_release.py            # -> crates/geodb-globe/dist-web/
    python3 crates/geodb-globe/scripts/web_release.py --serve 8770

Serve the precompressed files, e.g. nginx `brotli_static on; gzip_static on;`
or Caddy `file_server { precompressed br gzip }`. A server that sends the
plain files would send the raw data uncompressed (2.7 MB): use the
`.br`/`.gz` files or the gzipped assets/mini data there.

--serve runs a small local server that does the same (Accept-Encoding: br,
gzip or identity), for testing. It also sends COOP/COEP headers: the page
is then cross-origin isolated and its timer precise to 5 µs (send them from
the real server too for per-query benchmark samples).
"""

import argparse
import gzip
import http.server
import shutil
import subprocess
import sys
from pathlib import Path

CRATE = Path(__file__).resolve().parent.parent
RAW = CRATE / "assets" / "mini-raw"
OUT = CRATE / "dist-web"
TYPES = {
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript",
    ".wasm": "application/wasm",
    ".globe": "application/octet-stream",
    ".bin": "application/octet-stream",
}


def build(page: str) -> Path:
    dist = CRATE / ("dist-flex" if page == "flex.html" else "dist-mini")
    subprocess.run(["trunk", "build", page, "--release", "--dist", str(dist)], cwd=CRATE, check=True)
    return dist


def release(page: str) -> None:
    if not shutil.which("brotli"):
        sys.exit("needs the brotli command (brew install brotli)")
    if not (RAW / "cities.globe").exists():
        sys.exit("run: cargo run --release -p geodb-globe --example make_mini_assets")
    dist = build(page)
    if OUT.exists():
        shutil.rmtree(OUT)
    shutil.copytree(dist, OUT)
    for raw in RAW.iterdir():
        shutil.copy(raw, OUT / raw.name)

    rows = []
    for f in sorted(p for p in OUT.iterdir() if p.suffix in TYPES):
        data = f.read_bytes()
        (f.parent / (f.name + ".gz")).write_bytes(gzip.compress(data, 9, mtime=0))
        subprocess.run(["brotli", "-q", "11", "-f", "-o", str(f) + ".br", str(f)], check=True)
        rows.append((f.name, len(data), (f.parent / (f.name + ".gz")).stat().st_size,
                     (f.parent / (f.name + ".br")).stat().st_size))

    w = max(len(r[0]) for r in rows)
    print(f"\n{OUT}\n{'file':<{w}} {'plain':>10} {'gzip':>10} {'brotli':>10}")
    for name, *sizes in rows:
        print(f"{name:<{w}} " + " ".join(f"{s:>10,}" for s in sizes))
    totals = [sum(r[i] for r in rows) for i in (1, 2, 3)]
    print(f"{'total':<{w}} " + " ".join(f"{s:>10,}" for s in totals))


class Precompressed(http.server.SimpleHTTPRequestHandler):
    """Serves `x.br` / `x.gz` for `x` when the client accepts them."""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(OUT), **kwargs)

    def do_GET(self):
        path = self.path.split("?")[0].lstrip("/") or "index.html"
        f = OUT / path
        if not f.is_file() or f.suffix not in TYPES:
            return super().do_GET()
        accept = self.headers.get("Accept-Encoding", "")
        for enc, ext in (("br", ".br"), ("gzip", ".gz")):
            if enc in accept and (OUT / (path + ext)).is_file():
                body, encoding = (OUT / (path + ext)).read_bytes(), enc
                break
        else:
            body, encoding = f.read_bytes(), None
        self.send_response(200)
        self.send_header("Content-Type", TYPES[f.suffix])
        if encoding:
            self.send_header("Content-Encoding", encoding)
        self.send_header("Vary", "Accept-Encoding")
        # Cross-origin isolation: performance.now() gets 5 µs instead of
        # 100 µs, so the query benchmark can time single queries.
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--flex", action="store_true", help="build flex.html instead of mini.html")
    ap.add_argument("--serve", type=int, metavar="PORT", help="serve dist-web with precompression")
    ap.add_argument("--no-build", action="store_true", help="only serve the last release")
    args = ap.parse_args()
    if not args.no_build:
        release("flex.html" if args.flex else "mini.html")
    if args.serve:
        print(f"\nserving {OUT} on http://127.0.0.1:{args.serve}/ (br, gzip or plain)")
        http.server.ThreadingHTTPServer(("127.0.0.1", args.serve), Precompressed).serve_forever()


if __name__ == "__main__":
    main()
