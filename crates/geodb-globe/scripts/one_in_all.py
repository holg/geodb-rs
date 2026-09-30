#!/usr/bin/env python3
"""Bundle the globe web demo into one self-contained HTML file.

The result opens straight from disk (file:///…/one-in-all.html): no server,
no fetch, no module imports. The wasm-bindgen glue is inlined as a module
script, and the wasm and data files are base64 blocks that the app reads
through `window.__GEODB_EMBEDDED` before it would fetch.

    python3 crates/geodb-globe/scripts/one_in_all.py            # mini: geodb-mini, WebGPU (~2.3 MB)
    python3 crates/geodb-globe/scripts/one_in_all.py --flex     # + geodb-float and WebGL2 (~16 MB)

Writes crates/geodb-globe/dist-one/one-in-all.html (or one-in-all-flex.html).
"""

import argparse
import base64
import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import web_release  # noqa: E402  (SIZE_PROFILE)

CRATE = Path(__file__).resolve().parent.parent
# Fixed width, so writing the real size does not change the size.
SIZE_PLACEHOLDER = "__GEODB_FILE_BYTES_000000000"


def build(page: str, dist: Path) -> None:
    subprocess.run(
        ["trunk", "build", page, "--release", "--dist", str(dist)],
        cwd=CRATE,
        check=True,
        env={**os.environ, **web_release.SIZE_PROFILE},
    )


def glue_script(js: str) -> str:
    """The wasm-bindgen glue as plain module code: no exports."""
    js = re.sub(r"^export \{[^}]*\};?\s*$", "", js, flags=re.M)
    js = re.sub(r"^export (function|class|const|let|async function) ", r"\1 ", js, flags=re.M)
    if re.search(r"^\s*(import|export)\b", js, flags=re.M):
        sys.exit("the glue has imports or exports left; cannot inline it")
    # A literal "</script>" would end the inline script early.
    return js.replace("</script>", "<\\/script>")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--flex", action="store_true", help="both datasets and WebGL2")
    ap.add_argument("--no-build", action="store_true", help="reuse the last trunk build")
    ap.add_argument("--with-names", action="store_true", help="also embed the names layer (~11 MB)")
    args = ap.parse_args()

    page, dist = ("flex.html", CRATE / "dist-flex") if args.flex else ("mini.html", CRATE / "dist-mini")
    if not args.no_build:
        build(page, dist)

    html = (dist / "index.html").read_text()
    wasm = next(dist.glob("*_bg.wasm"))
    glue = next(p for p in dist.glob("*.js"))
    # Data and the optional layers (loaded on demand, but inline too); the
    # names layer (~11 MB) only when asked for.
    kinds = (".globe", ".bin", ".webp", ".coords", ".meta", ".fold", ".foldhan", ".foldhangul") + ((".names",) if args.with_names else ())
    data = sorted(p for p in dist.iterdir() if p.suffix in kinds)

    # Drop trunk's loader, preloads and copied-file links.
    html = re.sub(r'<link rel="(modulepreload|preload)"[^>]*>\s*', "", html)
    html = re.sub(r'<script type="module">.*?</script>\s*', "", html, flags=re.S)

    # Same layout as the in-page "Download as one HTML file": the wasm and
    # glue as app.wasm / app.js, the page skeleton as page.html (so the
    # file can be downloaded again from itself).
    skeleton = html
    blocks = [
        f'<script type="application/octet-stream" id="embed:{name}">'
        f"{base64.b64encode(data).decode()}</script>"
        for name, data in [
            ("app.wasm", wasm.read_bytes()),
            ("app.js", glue.read_bytes()),
            ("page.html", skeleton.encode()),
        ] + [(p.name, p.read_bytes()) for p in data]
    ]
    names = ["app.wasm", "app.js", "page.html"] + [p.name for p in data]
    loader = f"""<script type="module">
{glue_script(glue.read_text())}
const bytes = (id) => Uint8Array.from(atob(document.getElementById("embed:" + id).textContent), (c) => c.charCodeAt(0));
window.__GEODB_FILE_BYTES = Number("{SIZE_PLACEHOLDER}".replace(/\\D/g, ""));
window.__GEODB_BUILD = "{'flex' if args.flex else 'webgpu'}";
window.__GEODB_FILES = {{ js: "app.js", wasm: "app.wasm" }};
window.__GEODB_EMBEDDED = {{ {", ".join(f'"{n}": bytes("{n}")' for n in names)} }};
await __wbg_init({{ module_or_path: window.__GEODB_EMBEDDED["app.wasm"] }});
</script>"""
    html = html.replace("</body>", "\n".join(blocks) + "\n" + loader + "\n</body>")

    size = len(html.encode())
    html = html.replace(SIZE_PLACEHOLDER, f"__GEODB_FILE_BYTES_{size:09d}")
    assert len(html.encode()) == size

    out_dir = CRATE / "dist-one"
    out_dir.mkdir(exist_ok=True)
    out = out_dir / ("one-in-all-flex.html" if args.flex else "one-in-all.html")
    out.write_text(html)
    print(f"{out} ({size / 1e6:.2f} MB): wasm {wasm.stat().st_size} B + "
          + " + ".join(f"{p.name} {p.stat().st_size} B" for p in data))


if __name__ == "__main__":
    main()
