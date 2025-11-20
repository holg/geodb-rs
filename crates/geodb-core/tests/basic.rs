use geodb_core::prelude::*;

#[test]
fn load_filtered_us_and_basic_queries_work() {
    // Load only the United States to keep tests fast and deterministic
    let db = GeoDb::<DefaultBackend>::load_filtered_by_iso2(&["US"]).expect("load filtered DB");

    // Basic stats
    let stats = db.stats();
    assert_eq!(
        stats.countries, 1,
        "filtered load should have exactly one country"
    );
    assert!(
        stats.states >= 1,
        "US should have at least one state/region"
    );
    assert!(stats.cities >= 1, "US should have at least one city");

    // Lookups by ISO codes (case-insensitive)
    let us_by_iso2 = db.find_country_by_iso2("us").expect("find US by iso2");
    assert_eq!(us_by_iso2.iso2(), "US");

    // try ISO3 via generic code search; in the source dataset US has ISO3 = "USA"
    let us_by_iso3 = db
        .find_country_by_code("usa")
        .expect("find US by iso3 (usa)");
    assert_eq!(us_by_iso3.iso2(), "US");

    // iter_cities should yield cities that belong to a state and the US
    let mut saw_city = false;
    for (city, state, country) in db.cities() {
        assert!(!city.name().is_empty());
        assert!(!state.name().is_empty());
        assert_eq!(country.iso2(), "US");
        saw_city = true;
        // We don't break here to also test iterator stability, but ensure at least one
    }
    assert!(saw_city, "expected at least one city when filtering US");
}

#[test]
fn load_from_path_and_multi_filter() {
    // Build explicit path to the bundled dataset
    let dir = GeoDb::<DefaultBackend>::default_data_dir();
    let filename = GeoDb::<DefaultBackend>::default_dataset_filename();
    let path = dir.join(filename);

    // Filter to two countries and ensure counts line up with the filter length
    let db = GeoDb::<DefaultBackend>::load_from_path(&path, Some(&["DE", "FR"]))
        .expect("load filtered DB (DE, FR)");

    assert_eq!(db.stats().countries, 2);

    // Ensure both are present regardless of case
    assert!(db.find_country_by_code("de").is_some());
    assert!(db.find_country_by_code("fr").is_some());

    // Sanity: each country should have a name and ISO2
    for c in db.countries() {
        assert!(!c.name().is_empty());
        assert_eq!(c.iso2().len(), 2);
    }
}
