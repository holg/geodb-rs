//! The upstream dataset has no city populations. The converters used to store
//! each city's source row id as its "population"; make sure that never returns.

use geodb_core::prelude::*;

fn assert_no_city_population(db: &DefaultGeoDb, source: &str) {
    let cities = db.cities().count();
    let with_population: Vec<String> = db
        .cities()
        .filter(|(city, _, _)| city.population().is_some())
        .take(5)
        .map(|(city, _, _)| format!("{} = {:?}", city.name(), city.population()))
        .collect();
    assert!(cities > 100_000, "{source}: only {cities} cities");
    assert!(
        with_population.is_empty(),
        "{source}: cities with a population although the dataset has none: {with_population:?}"
    );

    // Country populations are real upstream data and must stay.
    let de = db.find_country_by_iso2("DE").expect("Germany");
    #[cfg(not(feature = "legacy_model"))]
    let pop = de.population().unwrap_or(0);
    #[cfg(feature = "legacy_model")]
    let pop = de.population() as u64;
    assert!(pop > 80_000_000, "{source}: Germany population {pop}");
}

#[test]
fn built_from_source_has_no_city_population() {
    let db = DefaultGeoDb::load().expect("build from bundled JSON");
    assert_no_city_population(&db, "built from JSON");
}

#[test]
fn bundled_binary_has_no_city_population() {
    // data/geodb.<model>.comp.blobs.bin (what WASM, FFI and Python embed).
    let path = DefaultGeoDb::default_bin_path();
    if !path.exists() {
        eprintln!("no bundled binary for this feature set at {path:?}; skipping");
        return;
    }
    let db = DefaultGeoDb::load_from_path(&path, None).expect("load bundled binary");
    assert_no_city_population(&db, &path.display().to_string());
}
