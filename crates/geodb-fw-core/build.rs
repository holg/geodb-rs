//! A build id for the crate: a hash of its source files. The board reports it (`!info`), the mirror
//! window compares it with its own, so a stale mirror (or a stale board) is noticed instead of showing a
//! picture that differs from the other one.

use std::path::Path;

fn walk(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, files);
        } else if p.extension().is_some_and(|e| e == "rs") {
            files.push(p);
        }
    }
}

fn main() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    println!("cargo:rerun-if-changed=src");
    let mut files = Vec::new();
    walk(&src, &mut files);
    // FNV-1a over the names and the contents
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for f in &files {
        let name = f
            .strip_prefix(&src)
            .unwrap_or(f)
            .to_string_lossy()
            .into_owned();
        for b in name.bytes().chain(std::fs::read(f).unwrap_or_default()) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    println!(
        "cargo:rustc-env=GEODB_FW_CORE_BUILD={:08x}",
        (h ^ (h >> 32)) as u32
    );
}
