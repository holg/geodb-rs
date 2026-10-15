//! The program is linked for one of two flash slots (see ../bootloader): `GEODB_SLOT=a` (default)
//! or `GEODB_SLOT=b`. The slot decides the FLASH origin; the city image lives outside both slots.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=GEODB_SLOT");
    let slot = std::env::var("GEODB_SLOT").unwrap_or_else(|_| "a".into());
    let origin = match slot.as_str() {
        "a" | "A" => 0x0804_0000u32,
        "b" | "B" => 0x0808_0000u32,
        other => panic!("GEODB_SLOT is a or b, not {other}"),
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(
        out.join("memory.x"),
        format!(
            "MEMORY\n{{\n  FLASH : ORIGIN = {origin:#x}, LENGTH = 256K\n  RAM   : ORIGIN = 0x20000000, LENGTH = 512K\n}}\n"
        ),
    )
    .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!(
        "cargo:rustc-link-search={}",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    );
    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rustc-link-arg-bins=-Tethbuf.x");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
    println!("cargo:rustc-link-arg-bins=-Tdefmt.x");
    println!("cargo:rerun-if-changed=ethbuf.x");
}
