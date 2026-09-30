fn main() {
    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!(
        "cargo:rustc-link-search={}",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    );
    println!("cargo:rustc-link-arg-bins=-Tethbuf.x");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
    println!("cargo:rustc-link-arg-bins=-Tdefmt.x");
    println!("cargo:rerun-if-changed=geodb.fw");
    println!("cargo:rerun-if-changed=ethbuf.x");
}
