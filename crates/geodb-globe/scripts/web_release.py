#!/usr/bin/env python3
"""Build the globe web demo for a server that sends compressed files.

Builds mini.html twice — a WebGPU build and a WebGL2 build (mini + the
`webgl` feature) — and a small loader in index.html that asks for a WebGPU
adapter first and loads only the build that will run: browsers without
WebGPU (older iPads, Firefox on macOS/Linux) get the WebGL2 build, the
others never download it. (--flex builds flex.html alone, which has both
backends.) Then swaps in the data files with uncompressed
payloads (assets/mini-raw, from `make_mini_assets`) and writes a `.br`
(brotli -q 11) and `.gz` (gzip -9) next to every file. Brotli on the raw
columns is about 10% smaller than the gzip inside the files.

    python3 crates/geodb-globe/scripts/web_release.py            # -> crates/geodb-globe/dist-web/
    python3 crates/geodb-globe/scripts/web_release.py --serve 8770

Serve the precompressed files, e.g. nginx `brotli_static on; gzip_static on;`
or Caddy `file_server { precompressed br gzip }`. A server that sends the
plain files would send the raw data uncompressed (2.7 MB): use the
`.br`/`.gz` files or the gzipped assets/mini data there.

--pages DIR writes a copy for static hosts that send files as they are
(GitHub Pages): no .br/.gz, and the data files with gzip inside their own
format again (assets/mini; coast10m.bin gzipped the same way), so they
are small without Content-Encoding.

    python3 crates/geodb-globe/scripts/web_release.py --pages site/globe

--serve runs a small local server that does the same (Accept-Encoding: br,
gzip or identity), for testing. --host 0.0.0.0 serves the intranet too;
there browsers need HTTPS (a secure context) for WebGPU, the 5 µs timer
and brotli, so add --tls: a self-signed certificate for localhost, this
host's name and its current IP addresses is made once in .tls/ (accept
it once per browser, or trust it). It also sends COOP/COEP headers: the page
is then cross-origin isolated and its timer precise to 5 µs (send them from
the real server too for per-query benchmark samples).
"""

import argparse
import gzip
import os
import re
import http.server
import shutil
import socket
import ssl
import subprocess
import sys
from pathlib import Path

CRATE = Path(__file__).resolve().parent.parent
RAW = CRATE / "assets" / "mini-raw"
GZIPPED = CRATE / "assets" / "mini"
OUT = CRATE / "dist-web"
TYPES = {
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript",
    ".wasm": "application/wasm",
    ".globe": "application/octet-stream",
    ".bin": "application/octet-stream",
    ".coords": "application/octet-stream",
    ".meta": "application/octet-stream",
    ".names": "application/octet-stream",
    ".fold": "application/octet-stream",
    ".foldhan": "application/octet-stream",
    ".foldhangul": "application/octet-stream",
    ".webp": "image/webp",
}
# Already compressed: served as they are.
NO_PRECOMPRESS = {".webp"}
DETAIL = CRATE / "assets" / "detail"


# The web builds are size-tuned: opt-level s with fat LTO and one codegen unit
# is 12% smaller than the default release profile (brotli) and faster on
# the CPU queries (opt-level z: 5% smaller still, but 26% slower).
SIZE_PROFILE = {
    "CARGO_PROFILE_RELEASE_OPT_LEVEL": "s",
    "CARGO_PROFILE_RELEASE_LTO": "fat",
    "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "1",
    "CARGO_PROFILE_RELEASE_PANIC": "abort",
}


def build(page: str, dist: Path | None = None) -> Path:
    dist = dist or CRATE / ("dist-flex" if page == "flex.html" else "dist-mini")
    subprocess.run(["trunk", "build", page, "--release", "--dist", str(dist)], cwd=CRATE, check=True,
                   env={**os.environ, **SIZE_PROFILE})
    return dist


LOADER = """<script type="module">
// Load the build that will run here: WebGPU where an adapter exists, else
// the WebGL2 build (no GPU compute, same app). Each browser fetches one.
// ?webgl2 (or ?gl) forces the WebGL2 build, to compare it on this device.
const forced = new URLSearchParams(location.search);
const webgl2 = forced.has("webgl2") || forced.has("gl");
const webgpu = !webgl2 && await (async () => {
    try { return !!(navigator.gpu && await navigator.gpu.requestAdapter()); } catch { return false; }
})();
const build = webgpu
    ? { name: "webgpu", js: "./%(gpu_js)s", wasm: "./%(gpu_wasm)s" }
    : { name: "webgl2", js: "./%(gl_js)s", wasm: "./%(gl_wasm)s" };
window.__GEODB_BUILD = build.name;
window.__GEODB_FILES = { js: build.js, wasm: build.wasm };
const { default: init } = await import(build.js);
await init({ module_or_path: build.wasm });
</script>"""


def build_webgl_variant() -> Path:
    """mini.html with the WebGL2 backend compiled in, built to dist-mini-gl."""
    page = CRATE / ".mini-gl.html"
    html = (CRATE / "mini.html").read_text()
    html = html.replace('data-cargo-features="mini"', 'data-cargo-features="mini,webgl"')
    page.write_text(html)
    try:
        return build(page.name, CRATE / "dist-mini-gl")
    finally:
        page.unlink(missing_ok=True)


def glue_pair(dist: Path) -> tuple[str, str]:
    wasm = next(dist.glob("*_bg.wasm")).name
    return wasm.removesuffix("_bg.wasm") + ".js", wasm


def release(page: str) -> None:
    if not shutil.which("brotli"):
        sys.exit("needs the brotli command (brew install brotli)")
    if not (RAW / "cities.globe").exists():
        sys.exit("run: cargo run --release -p geodb-globe --example make_mini_assets")
    dist = build(page)
    if OUT.exists():
        shutil.rmtree(OUT)
    shutil.copytree(dist, OUT)
    if page == "mini.html":
        # Second build with WebGL2, and the loader that picks one.
        gl = build_webgl_variant()
        gpu_js, gpu_wasm = glue_pair(dist)
        gl_js, gl_wasm = glue_pair(gl)
        for name in (gl_js, gl_wasm):
            shutil.copy(gl / name, OUT / name)
        index = OUT / "index.html"
        html = index.read_text()
        html = re.sub(r'<link rel="(modulepreload|preload)"[^>]*>\s*', "", html)
        html = re.sub(r'<script type="module">.*?</script>', lambda _: LOADER % {
            "gpu_js": gpu_js, "gpu_wasm": gpu_wasm, "gl_js": gl_js, "gl_wasm": gl_wasm,
        }, html, count=1, flags=re.S)
        index.write_text(html)
        print(f"builds: webgpu {gpu_wasm}, webgl2 {gl_wasm}")
    for raw in RAW.iterdir():
        shutil.copy(raw, OUT / raw.name)
    # Earth detail layers (scripts/fetch_detail.py), when made.
    for f in [DETAIL / "coast10m.bin", *sorted(DETAIL.glob("earth-*.webp"))]:
        if f.exists():
            shutil.copy(f, OUT / f.name)

    rows = []
    for f in sorted(p for p in OUT.iterdir() if p.suffix in TYPES and p.suffix not in NO_PRECOMPRESS):
        data = f.read_bytes()
        (f.parent / (f.name + ".gz")).write_bytes(gzip.compress(data, 9, mtime=0))
        # Window 2^24: browsers accept it, and it helps the large layers.
        subprocess.run(["brotli", "-q", "11", "-w", "24", "-f", "-o", str(f) + ".br", str(f)], check=True)
        rows.append((f.name, len(data), (f.parent / (f.name + ".gz")).stat().st_size,
                     (f.parent / (f.name + ".br")).stat().st_size))

    w = max(len(r[0]) for r in rows)
    print(f"\n{OUT}\n{'file':<{w}} {'plain':>10} {'gzip':>10} {'brotli':>10}")
    for name, *sizes in rows:
        print(f"{name:<{w}} " + " ".join(f"{s:>10,}" for s in sizes))
    totals = [sum(r[i] for r in rows) for i in (1, 2, 3)]
    print(f"{'total':<{w}} " + " ".join(f"{s:>10,}" for s in totals))


def gzip_inside(data: bytes) -> bytes:
    """Packed coastlines (`GDBC`, 5-byte header) with a gzip payload; the
    reader detects it (the same as `single_file::gzip_inside`)."""
    if data.startswith(b"GDBC") and data[5:7] != b"\x1f\x8b":
        return data[:5] + gzip.compress(data[5:], 9, mtime=0)
    return data


def pages(dest: Path) -> None:
    """dist-web for a static host without precompression."""
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    for f in OUT.iterdir():
        if f.is_file() and f.suffix not in (".br", ".gz"):
            shutil.copy(f, dest / f.name)
    for f in GZIPPED.iterdir():
        shutil.copy(f, dest / f.name)
    coast = dest / "coast10m.bin"
    if coast.exists():
        coast.write_bytes(gzip_inside(coast.read_bytes()))
    total = sum(f.stat().st_size for f in dest.iterdir())
    print(f"\n{dest}: {total / 1e6:.1f} MB for a static host (data gzipped inside)")
    for f in sorted(dest.iterdir()):
        print(f"  {f.name:<40} {f.stat().st_size:>12,}")


class Precompressed(http.server.SimpleHTTPRequestHandler):
    """Serves `x.br` / `x.gz` for `x` when the client accepts them."""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(OUT), **kwargs)

    def do_HEAD(self):
        self.send_file(head=True)

    def do_GET(self):
        self.send_file(head=False)

    def send_file(self, head: bool) -> None:
        """Like nginx with br/gzip_static: the precompressed file when the
        client accepts it, an ETag, 304 for a matching If-None-Match (the
        browser revalidates every load), HEAD with the encoded Content-Length."""
        path = self.path.split("?")[0].lstrip("/") or "index.html"
        f = OUT / path
        if not f.is_file() or f.suffix not in TYPES:
            return super().do_HEAD() if head else super().do_GET()
        accept = self.headers.get("Accept-Encoding", "")
        chosen, encoding = f, None
        for enc, ext in (("br", ".br"), ("gzip", ".gz")):
            if enc in accept and (OUT / (path + ext)).is_file():
                chosen, encoding = OUT / (path + ext), enc
                break
        st = chosen.stat()
        etag = f'"{st.st_mtime_ns:x}-{st.st_size:x}"'
        if encoding:
            etag = etag[:-1] + f'-{encoding}"'
        if self.headers.get("If-None-Match") == etag:
            self.send_response(304)
            self.send_header("ETag", etag)
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            return
        body = chosen.read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", TYPES[f.suffix])
        if encoding:
            self.send_header("Content-Encoding", encoding)
        self.send_header("Vary", "Accept-Encoding")
        self.send_header("ETag", etag)
        self.send_header("Cache-Control", "no-cache")
        # Cross-origin isolation: performance.now() gets 5 µs instead of
        # 100 µs, so the query benchmark can time single queries.
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if not head:
            self.wfile.write(body)


def local_addresses() -> list[str]:
    """IPv4 addresses of this machine (for the certificate and the URLs)."""
    out = subprocess.run(["ifconfig"], capture_output=True, text=True).stdout if shutil.which("ifconfig") else ""
    addrs = [line.split()[1] for line in out.splitlines() if line.strip().startswith("inet ")]
    return [a for a in addrs if a != "127.0.0.1"]


def certificate() -> tuple[Path, Path]:
    """A self-signed certificate for localhost, this host and its addresses."""
    tls = CRATE / ".tls"
    cert, key = tls / "cert.pem", tls / "key.pem"
    if cert.exists() and key.exists():
        return cert, key
    tls.mkdir(exist_ok=True)
    host = socket.gethostname()
    names = ["localhost", host] + ([host.removesuffix(".local") + ".local"] if not host.endswith(".local") else [])
    san = ",".join([f"DNS:{n}" for n in dict.fromkeys(names)] + ["IP:127.0.0.1"] + [f"IP:{a}" for a in local_addresses()])
    subprocess.run(
        ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "825",
         "-keyout", str(key), "-out", str(cert), "-subj", f"/CN={host}", "-addext", f"subjectAltName={san}"],
        check=True, capture_output=True,
    )
    print(f"made {cert} for {san}")
    return cert, key


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--flex", action="store_true", help="build flex.html instead of mini.html")
    ap.add_argument("--serve", type=int, metavar="PORT", help="serve dist-web with precompression")
    ap.add_argument("--host", default="127.0.0.1", help="address to listen on (0.0.0.0: intranet too)")
    ap.add_argument("--tls", action="store_true", help="HTTPS with a self-signed certificate (.tls/)")
    ap.add_argument("--no-build", action="store_true", help="only serve the last release")
    ap.add_argument("--pages", type=Path, metavar="DIR", help="also write a copy for a static host (GitHub Pages)")
    args = ap.parse_args()
    if not args.no_build:
        release("flex.html" if args.flex else "mini.html")
    if args.pages:
        pages(args.pages)
    if args.serve:
        server = http.server.ThreadingHTTPServer((args.host, args.serve), Precompressed)
        scheme = "http"
        if args.tls:
            cert, key = certificate()
            ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            ctx.load_cert_chain(cert, key)
            server.socket = ctx.wrap_socket(server.socket, server_side=True)
            scheme = "https"
        hosts = ["127.0.0.1"] if args.host == "127.0.0.1" else ["localhost", socket.gethostname()] + local_addresses()
        print(f"\nserving {OUT} (br, gzip or plain) on:")
        for h in hosts:
            print(f"  {scheme}://{h}:{args.serve}/")
        if args.host != "127.0.0.1" and not args.tls:
            print("  note: plain http off localhost is not a secure context (no WebGPU, no brotli); add --tls")
        server.serve_forever()


if __name__ == "__main__":
    main()
