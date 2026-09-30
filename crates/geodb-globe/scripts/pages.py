#!/usr/bin/env python3
"""Build the GitHub Pages site (https://holg.github.io/geodb-rs/) and,
with --deploy, push it to the gh-pages branch.

    python3 crates/geodb-globe/scripts/pages.py            # -> crates/geodb-globe/dist-pages/
    python3 crates/geodb-globe/scripts/pages.py --deploy   # + commit and push gh-pages

Layout: index.html (pages/index.html), globe/ (web_release.py --pages:
both builds and the loader, data gzipped inside, the Earth detail layers
when scripts/fetch_detail.py made them), one-in-all.html, blog/.

Pages sends files as they are (no .br) and without COOP/COEP, so the
timer is the coarse one there; everything else runs as on a full server.
The build needs the local toolchain (trunk, brotli); CI cannot build the
crate without the scopekit deploy key.
"""

import argparse
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

CRATE = Path(__file__).resolve().parent.parent
REPO = CRATE.parent.parent
OUT = CRATE / "dist-pages"


def run(*cmd: str, cwd: Path = REPO) -> str:
    return subprocess.run(cmd, cwd=cwd, check=True, text=True, capture_output=True).stdout


def build(no_build: bool) -> None:
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir()
    release = [sys.executable, str(CRATE / "scripts" / "web_release.py"), "--pages", str(OUT / "globe")]
    subprocess.run(release + (["--no-build"] if no_build else []), check=True)
    if not no_build:
        subprocess.run([sys.executable, str(CRATE / "scripts" / "one_in_all.py")], check=True)
    shutil.copy(CRATE / "dist-one" / "one-in-all.html", OUT / "one-in-all.html")
    shutil.copy(CRATE / "pages" / "index.html", OUT / "index.html")
    blog = OUT / "blog"
    blog.mkdir()
    for name in ("index.html", "blog-styles.css", "geodb_android.html"):
        if (REPO / "blog" / name).exists():
            shutil.copy(REPO / "blog" / name, blog / name)
    (OUT / ".nojekyll").write_text("")  # serve files as they are
    total = sum(f.stat().st_size for f in OUT.rglob("*") if f.is_file())
    print(f"{OUT}: {total / 1e6:.1f} MB")


def deploy() -> None:
    """Replaces the gh-pages branch content with dist-pages (one commit)."""
    head = run("git", "rev-parse", "--short", "HEAD").strip()
    with tempfile.TemporaryDirectory() as tmp:
        wt = Path(tmp) / "gh-pages"
        exists = run("git", "ls-remote", "--heads", "origin", "gh-pages").strip() != ""
        if exists:
            run("git", "fetch", "-q", "origin", "gh-pages")
            run("git", "worktree", "add", "-q", "-B", "gh-pages", str(wt), "origin/gh-pages")
        else:
            run("git", "worktree", "add", "-q", "--detach", str(wt))
            run("git", "checkout", "-q", "--orphan", "gh-pages", cwd=wt)
        try:
            for p in wt.iterdir():
                if p.name != ".git":
                    shutil.rmtree(p) if p.is_dir() else p.unlink()
            shutil.copytree(OUT, wt, dirs_exist_ok=True)
            run("git", "add", "-A", cwd=wt)
            if run("git", "status", "--porcelain", cwd=wt).strip():
                run("git", "commit", "-q", "-m", f"Deploy the globe site from {head}", cwd=wt)
                run("git", "push", "-q", "origin", "gh-pages", cwd=wt)
                print(f"pushed gh-pages (from {head})")
            else:
                print("gh-pages is up to date")
        finally:
            run("git", "worktree", "remove", "--force", str(wt))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--no-build", action="store_true", help="reuse dist-web and dist-one")
    ap.add_argument("--deploy", action="store_true", help="push the site to the gh-pages branch")
    args = ap.parse_args()
    build(args.no_build)
    if args.deploy:
        deploy()


if __name__ == "__main__":
    main()
