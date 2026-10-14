// The watch globe (src/watch_globe.rs) reads the board's image, firmware/stm32f769i-disco/geodb.fw, which is
// git-ignored (geodb-fw-core's make_image writes it; GeoDB-Apps/scripts/build_spm_package.sh makes it before the
// watchOS builds). Its host test only builds where the image is, so a fresh checkout still passes the tests.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(geodb_fw_image)");
    let image = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../firmware/stm32f769i-disco/geodb.fw");
    println!("cargo::rerun-if-changed={}", image.display());
    if image.exists() {
        println!("cargo::rustc-cfg=geodb_fw_image");
    }
}
