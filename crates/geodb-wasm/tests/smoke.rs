use wasm_bindgen_test::*;

// Import the wasm functions from this crate
use geodb_wasm::{get_country_count, get_country_name};

#[wasm_bindgen_test]
fn can_get_country_count() {
    // Ensure module is initialized (defensive; start() should run automatically)
    #[cfg(target_arch = "wasm32")]
    geodb_wasm::start();

    let count = get_country_count();
    assert!(count > 0, "expected at least one country, got {count}");
}

#[wasm_bindgen_test]
fn can_lookup_country_name() {
    #[cfg(target_arch = "wasm32")]
    geodb_wasm::start();

    let name = get_country_name("US");
    assert!(name.is_some());
}
